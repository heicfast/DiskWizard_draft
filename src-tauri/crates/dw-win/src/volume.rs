//! Volume helpers — the per-volume facts the app layer joins into
//! inventory rows and scan-target resolution.

/// Resolve a device row id (`diskN-partM`, `vol-X`, `usb-X`,
/// `diskN-lostL`) to (disk index, partition context) for scan
/// targeting.
#[must_use]
pub fn parse_device_id(id: &str) -> Option<DeviceTarget> {
    if let Some(rest) = id.strip_prefix("disk-") {
        // Legacy "disk-0" style.
        let idx: u32 = rest.parse().ok()?;
        return Some(DeviceTarget::WholeDisk(idx));
    }
    if let Some(rest) = id.strip_prefix("disk") {
        let mut it = rest.splitn(2, '-');
        let idx: u32 = it.next()?.parse().ok()?;
        let tail = it.next().unwrap_or("");
        if tail.is_empty() {
            return Some(DeviceTarget::WholeDisk(idx));
        }
        if let Some(lba) = tail.strip_prefix("lost") {
            let start: u64 = lba.parse().ok()?;
            return Some(DeviceTarget::LostPartition {
                disk: idx,
                start_lba: start,
            });
        }
        if let Some(part) = tail.strip_prefix("part") {
            let part_no: u32 = part.parse().ok()?;
            return Some(DeviceTarget::Partition {
                disk: idx,
                part_no,
            });
        }
    }
    // Volume-letter forms: vol-X / usb-X / sd-X / ssd-X.
    if let Some(letter) = id.rsplit(['-', ':']).next().and_then(|l| l.chars().next()) {
        if letter.is_ascii_alphabetic() && id.len() <= 8 {
            return Some(DeviceTarget::Volume(letter.to_ascii_uppercase()));
        }
    }
    None
}

/// A resolved scan target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceTarget {
    WholeDisk(u32),
    Partition { disk: u32, part_no: u32 },
    LostPartition { disk: u32, start_lba: u64 },
    Volume(char),
}
