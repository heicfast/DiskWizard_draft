//! Device + volume enumeration — the `list_devices` inventory.
//!
//! Strategy (research: `artifacts/MOM-modules/DeviceManager.md`,
//! `Device.mo` roles — "Enum disk begin/end"): enumerate mounted
//! volumes (letters), join each to its physical disk via
//! `IOCTL_STORAGE_GET_DEVICE_NUMBER`, enrich disks via
//! `IOCTL_STORAGE_QUERY_PROPERTY` (bus type → USB/SD classification,
//! product strings), detect SSD vs HDD (seek-penalty query) and TRIM
//! (StorageDeviceTrimProperty), then lay the core's partition tables
//! over the physical disks to expose lost partitions.
//!
//! Output shape = the frozen IPC `DeviceInventory` (the UI contract):
//! `{localDisks, externalDevices, diskImages}` rows with
//! id/name/letter/kind/fileSystem/used/total/health/bitLocker/trim/
//! isSystem/diskIndex.

use serde::{Deserialize, Serialize};

use dw_core::blockdev::{BlockDevice, BlockRead, CachedDevice};
use dw_core::partition::{PartitionLayout, sweep_for_lost_partitions};

use crate::blockdev::RawHandleDevice;

/// One inventory row (serde wire format = the UI `StorageDevice`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRow {
    pub id: String,
    pub name: String,
    pub letter: Option<String>,
    pub kind: String,
    pub file_system: String,
    pub used: u64,
    pub total: u64,
    pub health: String,
    pub bit_locker: String,
    pub trim: Option<String>,
    pub is_system: bool,
    pub disk_index: u32,
}

/// The inventory result (wire format = the UI `DeviceInventory`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceInventory {
    pub local_disks: Vec<DeviceRow>,
    pub external_devices: Vec<DeviceRow>,
    pub disk_images: Vec<DeviceRow>,
}

/// A physical disk's hardware facts.
#[derive(Debug, Clone, Default)]
pub struct DiskFacts {
    pub index: u32,
    pub model: String,
    pub serial: String,
    pub bus: String,
    pub total_bytes: u64,
    pub is_ssd: bool,
    pub trim: Option<bool>,
    pub removable: bool,
}

/// Enumerate the full inventory.
///
/// # Errors
/// Never fails whole-list: individual enumeration failures degrade
/// to skipped rows (an unreadable disk doesn't hide the others).
pub fn enumerate_inventory() -> DeviceInventory {
    let mut inv = DeviceInventory::default();
    let letters = mounted_letters();
    // Collect unique disk indexes + per-letter facts.
    let mut disk_facts: Vec<DiskFacts> = Vec::new();
    for letter in &letters {
        let Ok(dev) = RawHandleDevice::open_volume(*letter) else {
            continue;
        };
        let (disk_index, part_no) = dev.device_number().unwrap_or((u32::MAX, 0));
        if disk_index == u32::MAX {
            continue;
        }
        if !disk_facts.iter().any(|d| d.index == disk_index) {
            if let Some(facts) = probe_disk_facts(disk_index) {
                disk_facts.push(facts);
            }
        }
        let vol = volume_facts(*letter);
        let facts = disk_facts
            .iter()
            .find(|d| d.index == disk_index)
            .cloned()
            .unwrap_or_default();
        let kind = classify_kind(&facts, *letter);
        let is_system = is_system_volume(*letter);
        let mut row = DeviceRow {
            id: format!("disk{disk_index}-part{part_no}"),
            name: volume_display_name(*letter, &facts),
            letter: Some(format!("{letter}:")),
            kind: kind.clone(),
            file_system: vol.fs.clone(),
            used: vol.used,
            total: vol.total,
            health: if vol.fs == "RAW" { "raw".into() } else { "ok".into() },
            bit_locker: "none".into(),
            trim: facts.trim.map(|t| if t { "on".into() } else { "off".into() }),
            is_system,
            disk_index,
        };
        // BitLocker-locked: FS reports blank; boot sector carries the
        // FVE signature (probe via the raw volume).
        if vol.fs.is_empty() || vol.fs == "RAW" {
            if crate::bitlocker::volume_is_bitlocker_locked(*letter) {
                row.health = "bitlocker".into();
                row.bit_locker = "locked".into();
                row.file_system = "RAW".into();
            }
        }
        if kind == "usb" || kind == "sdcard" || kind == "portable-ssd" {
            inv.external_devices.push(row);
        } else {
            inv.local_disks.push(row);
        }
    }

    // Lost partitions on each physical disk (quick sweep — the deep
    // scan refines): surfaced as scan targets, health "lost".
    for facts in &disk_facts {
        if let Ok(dev) = RawHandleDevice::open_physical(facts.index) {
            let cached = std::sync::Arc::new(CachedDevice::new(std::sync::Arc::new(dev)));
            if let Ok(layout) = read_layout(&*cached) {
                for hint in sweep_for_lost_partitions(&*cached, &layout)
                    .unwrap_or_default()
                {
                    inv.local_disks.push(DeviceRow {
                        id: format!("disk{}-lost{}", facts.index, hint.info.start_lba),
                        name: format!(
                            "Lost {} ({:.1} GB) on Disk {}",
                            hint.info.fs.wire(),
                            (hint.info.total_sectors * 512) as f64 / 1_073_741_824.0,
                            facts.index
                        ),
                        letter: None,
                        kind: "hdd".into(),
                        file_system: hint.info.fs.wire().to_string(),
                        used: 0,
                        total: hint.info.total_sectors * 512,
                        health: "lost".into(),
                        bit_locker: "none".into(),
                        trim: None,
                        is_system: false,
                        disk_index: facts.index,
                    });
                }
            }
        }
    }
    inv
}

/// (model, serial, bus) for a physical disk — the SMART report's
/// identity block.
#[must_use]
pub fn disk_identity(index: u32) -> (String, String, String) {
    probe_disk_facts(index)
        .map(|f| (f.model, f.serial, f.bus))
        .unwrap_or_else(|| (format!("Disk {index}"), String::new(), String::new()))
}

/// Parse the partition layout of a device (MBR → EBR → GPT chain).
pub fn read_layout(dev: &dyn BlockDevice) -> dw_core::error::CoreResult<PartitionLayout> {
    let sector0 = dev.read_struct_at::<512>(0)?;
    if let Ok((scheme, mut parts)) = dw_core::partition::parse_mbr(&sector0, dev.total_sectors()) {
        if scheme == dw_core::partition::PartitionScheme::Gpt {
            if let Ok((_, gpt_parts)) = dw_core::partition::parse_gpt(dev) {
                return Ok(PartitionLayout {
                    scheme,
                    partitions: gpt_parts,
                });
            }
        }
        // Walk EBR chains for extended containers.
        let extended: Vec<u64> = sector0[0x1BE..0x1FE]
            .chunks_exact(16)
            .filter(|e| (e[4] == 0x05 || e[4] == 0x0F) && e[8..12] != [0, 0, 0, 0])
            .map(|e| {
                u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as u64
            })
            .collect();
        for ebr0 in extended {
            if let Ok(logical) = dw_core::partition::parse_ebr_chain(
                dev,
                ebr0,
                dev.total_sectors(),
                parts.len() as u32,
            ) {
                parts.extend(logical);
            }
        }
        return Ok(PartitionLayout {
            scheme,
            partitions: parts,
        });
    }
    Ok(PartitionLayout::default())
}

/// Volume-level facts from `GetVolumeInformationW` + free space.
pub struct VolumeFacts {
    pub fs: String,
    pub used: u64,
    pub total: u64,
    pub label: String,
}

fn volume_facts(letter: char) -> VolumeFacts {
    let root = format!("{letter}:\\");
    let root_w: Vec<u16> = root.encode_utf16().collect();
    let mut fs_buf = [0u16; 32];
    let mut label_buf = [0u16; 64];
    let mut serial = 0u32;
    let mut max_comp = 0u32;
    let mut flags = 0u32;
    let ok = unsafe {
        windows::Win32::Storage::FileSystem::GetVolumeInformationW(
            windows::core::PCWSTR(root_w.as_ptr()),
            Some(label_buf.as_mut_ptr()),
            Some(label_buf.len() as u32),
            Some(&mut serial),
            Some(&mut max_comp),
            Some(&mut flags),
            Some(fs_buf.as_mut_ptr()),
            Some(fs_buf.len() as u32),
        )
    };
    let fs = if ok.as_bool() {
        String::from_utf16_lossy(&fs_buf)
            .trim_end_matches('\0')
            .to_string()
    } else {
        String::new()
    };
    let label = String::from_utf16_lossy(&label_buf)
        .trim_end_matches('\0')
        .to_string();
    let (total, free) = volume_space(&root);
    VolumeFacts {
        fs,
        used: total.saturating_sub(free),
        total,
        label,
    }
}

fn volume_space(root: &str) -> (u64, u64) {
    let root_w: Vec<u16> = root.encode_utf16().chain([0]).collect();
    let mut free_to_caller = 0u64;
    let mut total = 0u64;
    let mut free = 0u64;
    let ok = unsafe {
        windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            windows::core::PCWSTR(root_w.as_ptr()),
            Some(&mut free_to_caller),
            Some(&mut total),
            Some(&mut free),
        )
    };
    if ok.as_bool() {
        (total, free)
    } else {
        (0, 0)
    }
}

/// All mounted drive letters (fixed + removable).
fn mounted_letters() -> Vec<char> {
    let mask = unsafe { windows::Win32::Storage::FileSystem::GetLogicalDrives() };
    let mut out = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) != 0 {
            // Uppercase letter bit 0 = A.
            out.push(char::from_u32(u32::from(b'A') + i).expect("ascii"));
        }
    }
    out
}

fn drive_kind(letter: char) -> u32 {
    let root: Vec<u16> = format!("{letter}:\\").encode_utf16().chain([0]).collect();
    unsafe {
        windows::Win32::Storage::FileSystem::GetDriveTypeW(windows::core::PCWSTR(root.as_ptr()))
    }
}

fn is_system_volume(letter: char) -> bool {
    // The system volume hosts the OS directory.
    let root = format!("{letter}:\\Windows");
    std::path::Path::new(&root).exists()
}

/// Probe hardware facts for a physical disk (query-property based).
fn probe_disk_facts(index: u32) -> Option<DiskFacts> {
    use windows::Win32::Storage::Ioctl::{
        IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_PROPERTY_QUERY, STORAGE_PROPERTY_ID,
        STORAGE_QUERY_TYPE, STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_QUERY_Type1,
        STORAGE_PROPERTY_ID_StorageDeviceProperty,
    };
    let dev = RawHandleDevice::open_physical(index).ok()?;
    let handle = windows::Win32::Foundation::HANDLE(dev.as_file_handle());
    let mut desc_buf = vec![0u8; 4096];
    let mut query = STORAGE_PROPERTY_QUERY {
        PropertyId: STORAGE_PROPERTY_ID_StorageDeviceProperty,
        QueryType: STORAGE_QUERY_TYPE(0),
        AdditionalParameters: [0; 1],
    };
    let _ = STORAGE_PROPERTY_QUERY_Type1; // (type alias sanity)
    let _ = STORAGE_PROPERTY_ID(0);
    let ok = unsafe {
        windows::Win32::System::IO::DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(&mut query as *mut _ as _),
            std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            Some(desc_buf.as_mut_ptr() as _),
            desc_buf.len() as u32,
            None,
            None,
        )
    };
    if !ok.as_bool() {
        return None;
    }
    // STORAGE_DEVICE_DESCRIPTOR fixed part is 40 bytes:
    //   u32 Version, u32 Size, u8 DeviceType, u8 DeviceTypeModifier,
    //   BOOLEAN RemovableMedia, BOOLEAN CommandQueueing,
    //   u32 VendorIdOffset, u32 ProductIdOffset, u32 ProductRevisionOffset,
    //   u32 SerialNumberOffset, STORAGE_BUS_TYPE BusType,
    //   u32 RawPropertiesLength, ...
    let desc: STORAGE_DEVICE_DESCRIPTOR = unsafe { std::ptr::read(desc_buf.as_ptr() as *const _) };
    let get_str = |off: u32| -> String {
        if off == 0 {
            return String::new();
        }
        let start = off as usize;
        if start >= desc_buf.len() {
            return String::new();
        }
        let end = desc_buf[start..]
            .iter()
            .position(|&b| b == 0)
            .map(|p| start + p)
            .unwrap_or(desc_buf.len());
        String::from_utf8_lossy(&desc_buf[start..end]).trim().to_string()
    };
    let bus_type = desc.BusType as u32;
    let bus = match bus_type {
        0x07 => "USB",
        0x0A => "MMC",
        0x0B => "Virtual",
        0x0C => "FileBackedVirtual",
        0x03 => "ATAPI",
        0x02 => "ATA",
        0x0D => "SD",
        0x0E => "MMC5",
        _ => "Other",
    }
    .to_string();
    let removable = desc.RemovableMedia.as_bool();
    let total_bytes = u64::from(dev.sector_size()) * dev.total_sectors();
    let is_ssd = probe_seek_penalty(index);
    let trim = probe_trim(index);
    Some(DiskFacts {
        index,
        model: format!("{} {}", get_str(desc.VendorIdOffset), get_str(desc.ProductIdOffset))
            .trim()
            .to_string(),
        serial: get_str(desc.SerialNumberOffset),
        bus,
        total_bytes,
        is_ssd,
        trim,
        removable,
    })
}

fn probe_seek_penalty(index: u32) -> bool {
    // NoSeekPenalty == true → SSD. IOCTL on the physical device.
    use windows::Win32::Storage::Ioctl::{
        IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_PROPERTY_QUERY, DEVICE_SEEK_PENALTY_DESCRIPTOR,
    };
    let Ok(dev) = RawHandleDevice::open_physical(index) else {
        return false;
    };
    let handle = windows::Win32::Foundation::HANDLE(dev.as_file_handle());
    let mut query = STORAGE_PROPERTY_QUERY {
        PropertyId: windows::Win32::Storage::Ioctl::StorageDeviceSeekPenaltyProperty,
        QueryType: windows::Win32::Storage::Ioctl::PropertyStandardQuery,
        AdditionalParameters: [0; 1],
    };
    let mut desc = DEVICE_SEEK_PENALTY_DESCRIPTOR::default();
    let ok = unsafe {
        windows::Win32::System::IO::DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(&mut query as *mut _ as _),
            std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            Some(&mut desc as *mut _ as _),
            std::mem::size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() as u32,
            None,
            None,
        )
    };
    ok.as_bool() && !desc.IncursSeekPenalty.as_bool()
}

fn probe_trim(index: u32) -> Option<bool> {
    use windows::Win32::Storage::Ioctl::{
        IOCTL_STORAGE_QUERY_PROPERTY, DEVICE_TRIM_DESCRIPTOR, STORAGE_PROPERTY_QUERY,
    };
    let dev = RawHandleDevice::open_physical(index).ok()?;
    let handle = windows::Win32::Foundation::HANDLE(dev.as_file_handle());
    let mut query = STORAGE_PROPERTY_QUERY {
        PropertyId: windows::Win32::Storage::Ioctl::StorageDeviceTrimProperty,
        QueryType: windows::Win32::Storage::Ioctl::PropertyStandardQuery,
        AdditionalParameters: [0; 1],
    };
    let mut desc = DEVICE_TRIM_DESCRIPTOR::default();
    let ok = unsafe {
        windows::Win32::System::IO::DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(&mut query as *mut _ as _),
            std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            Some(&mut desc as *mut _ as _),
            std::mem::size_of::<DEVICE_TRIM_DESCRIPTOR>() as u32,
            None,
            None,
        )
    };
    ok.as_bool().then_some(desc.TrimEnabled.as_bool())
}

/// The UI `kind` classification from bus/removable/SSD facts.
fn classify_kind(facts: &DiskFacts, letter: char) -> String {
    let dk = drive_kind(letter);
    if dk == 2 {
        // DRIVE_REMOVABLE
        if facts.bus == "SD" || facts.bus == "MMC" {
            return "sdcard".into();
        }
        if facts.is_ssd {
            return "portable-ssd".into();
        }
        return "usb".into();
    }
    if facts.is_ssd {
        "ssd".into()
    } else {
        "hdd".into()
    }
}

fn volume_display_name(letter: char, facts: &DiskFacts) -> String {
    let root = format!("{letter}:\\");
    let root_w: Vec<u16> = root.encode_utf16().chain([0]).collect();
    let mut label_buf = [0u16; 64];
    let ok = unsafe {
        windows::Win32::Storage::FileSystem::GetVolumeInformationW(
            windows::core::PCWSTR(root_w.as_ptr()),
            Some(label_buf.as_mut_ptr()),
            Some(label_buf.len() as u32),
            None,
            None,
            None,
            None,
            None,
        )
    };
    let label = if ok.as_bool() {
        String::from_utf16_lossy(&label_buf)
            .trim_end_matches('\0')
            .to_string()
    } else {
        String::new()
    };
    if label.is_empty() {
        format!("Local Disk ({letter}:)")
    } else {
        format!("{label} ({letter}:)")
    }
}
