//! Target resolution — device ids on Windows, image files anywhere.
//! Mirrors the app layer's `resolve_target` (commands/real/scan.rs)
//! so the CLI exercises the exact production paths.

use std::sync::Arc;

use dw_core::blockdev::{BlockDevice, ImageFile, SectorSizeMode};

/// A resolved scan target: the block device + its display name + the
/// device-id string used to REOPEN it during recovery.
pub struct Target {
    pub dev: Arc<dyn BlockDevice>,
    pub name: String,
    /// The original target spec (device id or image path) — stored in
    /// the results JSON so `recover` can re-open the same source.
    pub spec: String,
    /// True when the target is a mounted volume (QA-8 live-scan note).
    pub live_volume: bool,
}

/// Resolve `spec` (device id or image path) → open device.
///
/// # Errors
/// Typed string errors matching the app layer's vocabulary.
pub fn resolve(spec: &str) -> Result<Target, String> {
    // Image files: anything with an extension or path separators that
    // exists on disk.
    let looks_like_path = spec.contains('/') || spec.contains('\\') || spec.contains('.');
    if looks_like_path && std::path::Path::new(spec).is_file() {
        let name = std::path::Path::new(spec)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Disk image")
            .to_string();
        let dev = ImageFile::open(std::path::Path::new(spec), SectorSizeMode::Probe)
            .map_err(|e| format!("IMAGE_OPEN_FAILED. {e}"))?;
        return Ok(Target {
            dev: Arc::new(dev),
            name,
            spec: spec.to_string(),
            live_volume: false,
        });
    }

    #[cfg(windows)]
    {
        use dw_core::blockdev::SubDevice;
        use dw_win::volume::{DeviceTarget, parse_device_id};
        let target = parse_device_id(spec).ok_or_else(|| format!("UNKNOWN_DEVICE. {spec}"))?;
        match target {
            DeviceTarget::WholeDisk(n) => {
                let dev = dw_win::blockdev::RawHandleDevice::open_physical(n)
                    .map_err(|e| format!("DEVICE_OPEN_FAILED. {e}"))?;
                Ok(Target {
                    dev: Arc::new(dev),
                    name: format!("Disk {n}"),
                    spec: spec.to_string(),
                    live_volume: false,
                })
            }
            DeviceTarget::Partition { disk: n, part_no } => {
                // Scan the VOLUME (LBA 0 = the partition boot sector),
                // never the whole-disk MBR — slice the partition out of
                // the parsed table (same as the app layer).
                let dev = dw_win::blockdev::RawHandleDevice::open_physical(n)
                    .map_err(|e| format!("DEVICE_OPEN_FAILED. {e}"))?;
                let parent: Arc<dyn BlockDevice> = Arc::new(dev);
                let (start, len) = partition_extent(&parent, part_no)
                    .ok_or_else(|| format!("PARTITION_NOT_FOUND. disk {n} partition {part_no}"))?;
                let sub = Arc::new(SubDevice::new(
                    parent,
                    start,
                    len,
                    format!("disk{n}-part{part_no}"),
                ));
                Ok(Target {
                    dev: sub,
                    name: format!("Disk {n} Partition {part_no}"),
                    spec: spec.to_string(),
                    live_volume: false,
                })
            }
            DeviceTarget::LostPartition { disk: n, start_lba } => {
                let dev = dw_win::blockdev::RawHandleDevice::open_physical(n)
                    .map_err(|e| format!("DEVICE_OPEN_FAILED. {e}"))?;
                let sub = Arc::new(SubDevice::new(
                    Arc::new(dev) as Arc<dyn BlockDevice>,
                    start_lba,
                    u64::MAX / 512, // clamped by SubDevice to the parent
                    format!("lost@{start_lba}"),
                ));
                Ok(Target {
                    dev: sub,
                    name: format!("Lost partition at sector {start_lba}"),
                    spec: spec.to_string(),
                    live_volume: false,
                })
            }
            DeviceTarget::Volume(letter) => {
                let dev = dw_win::blockdev::RawHandleDevice::open_volume(letter)
                    .map_err(|e| format!("DEVICE_OPEN_FAILED. {e}"))?;
                let name = volume_display_name(letter);
                Ok(Target {
                    dev: Arc::new(dev),
                    name,
                    spec: spec.to_string(),
                    live_volume: true,
                })
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = spec;
        Err(format!(
            "NOT_A_FILE. On this host, scan targets are image files (got `{spec}`)"
        ))
    }
}

/// Locate partition `part_no` on an open whole-disk device.
#[cfg(windows)]
fn partition_extent(disk: &Arc<dyn BlockDevice>, part_no: u32) -> Option<(u64, u64)> {
    use dw_core::blockdev::BlockRead;
    if let Ok(layout) = dw_win::devices::read_layout(disk.as_ref())
        && let Some(p) = layout.partitions.iter().find(|p| p.index == part_no)
    {
        return Some((p.start_lba, p.total_sectors));
    }
    // MBR slot fallback (Windows numbers primaries by slot position).
    let sector0 = disk.read_struct_at::<512>(0).ok()?;
    if u16::from_le_bytes([sector0[510], sector0[511]]) != 0xAA55 {
        return None;
    }
    let slot = (part_no.clamp(1, 4) - 1) as usize;
    let entry = &sector0[0x1BE + slot * 16..0x1BE + slot * 16 + 16];
    let ptype = entry[4];
    if ptype == 0x00 || ptype == 0x05 || ptype == 0x0F || ptype == 0xEE {
        return None;
    }
    let start = u64::from(u32::from_le_bytes([
        entry[8], entry[9], entry[10], entry[11],
    ]));
    let len = u64::from(u32::from_le_bytes([
        entry[12], entry[13], entry[14], entry[15],
    ]));
    (len > 0).then_some((start, len))
}

#[cfg(windows)]
fn volume_display_name(letter: char) -> String {
    let root = format!("{letter}:\\");
    let w: Vec<u16> = root.encode_utf16().chain([0]).collect();
    let mut label = [0u16; 64];
    // SAFETY: w is NUL-terminated; label's capacity travels with it.
    #[allow(unsafe_code)]
    let ok = unsafe {
        windows::Win32::Storage::FileSystem::GetVolumeInformationW(
            windows::core::PCWSTR(w.as_ptr()),
            Some(&mut label),
            None,
            None,
            None,
            None,
        )
    };
    if ok.is_ok() {
        let l = String::from_utf16_lossy(&label)
            .trim_end_matches('\0')
            .to_string();
        if !l.is_empty() {
            return format!("{l} ({letter}:)");
        }
    }
    format!("Drive {letter}:")
}
