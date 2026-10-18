//! SMART — the free-tier disk health engine.
//!
//! Multi-transport per the research (`artifacts/DiskHealth/
//! diskhealth--smart-companion.md` — the CrystalDiskInfo CAtaSmart
//! lineage): ATA PASS-THROUGH (SAT) for SATA/USB bridges, NVMe
//! admin-queue passthrough for NVMe, both through
//! `IOCTL_STORAGE_QUERY_PROPERTY` protocol-specific queries and
//! `IOCTL_ATA_PASS_THROUGH`. Output: the standard 22-attribute table
//! + temperature + verdict (Good/Caution/Bad by threshold rules) —
//! the wire shape the UI's Disk Tools dashboard renders.

use serde::{Deserialize, Serialize};

use crate::blockdev::RawHandleDevice;

/// One SMART attribute row (UI `SmartAttribute`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartAttribute {
    pub id: u8,
    pub name: String,
    pub current: u8,
    pub worst: u8,
    pub threshold: u8,
    pub raw_value: String,
    pub status: String,
}

/// The full report (UI `SmartReport`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartReport {
    pub device_id: String,
    pub device_name: String,
    pub model: String,
    pub firmware: String,
    pub serial: String,
    pub health: String,
    pub temperature: i32,
    pub power_on_hours: u64,
    pub power_cycles: u64,
    pub total_writes: u64,
    pub interface: String,
    pub attributes: Vec<SmartAttribute>,
}

/// The named attribute table (the standard 22 + modern SSD additions).
const ATA_ATTR_NAMES: &[(u8, &str)] = &[
    (0x01, "Read Error Rate"),
    (0x02, "Throughput Performance"),
    (0x03, "Spin-Up Time"),
    (0x04, "Start/Stop Count"),
    (0x05, "Reallocated Sectors Count"),
    (0x07, "Seek Error Rate"),
    (0x08, "Seek Time Performance"),
    (0x09, "Power-On Hours"),
    (0x0A, "Spin Retry Count"),
    (0x0B, "Recalibration Retries"),
    (0x0C, "Power Cycle Count"),
    (0xAA, "Available Reserved Space"),
    (0xAB, "Program Fail Count"),
    (0xAC, "Erase Fail Count"),
    (0xAD, "Wear Leveling Count"),
    (0xAE, "Unexpected Power Loss"),
    (0xB7, "SATA Downshift Error Count"),
    (0xB8, "End-to-End Error"),
    (0xBB, "Reported Uncorrectable Errors"),
    (0xBE, "Airflow Temperature"),
    (0xC0, "Power-off Retract Count"),
    (0xC2, "Temperature"),
    (0xC3, "Hardware ECC Recovered"),
    (0xC4, "Reallocation Event Count"),
    (0xC5, "Current Pending Sector Count"),
    (0xC6, "Uncorrectable Sector Count"),
    (0xC7, "UltraDMA CRC Error Count"),
    (0xC9, "Soft Read Error Rate"),
    (0xCA, "Data Address Mark Errors"),
    (0xE7, "SSD Life Left"),
    (0xE9, "NAND Writes (Host)"),
    (0xF1, "Total LBAs Written"),
    (0xF2, "Total LBAs Read"),
];

fn attr_name(id: u8) -> String {
    ATA_ATTR_NAMES
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, n)| (*n).to_string())
        .unwrap_or_else(|| format!("Attribute 0x{id:02X}"))
}

/// Build the report for a physical disk (probes transport by device).
///
/// # Errors
/// String error when no transport yields data.
pub fn report_for_disk(index: u32) -> Result<SmartReport, String> {
    let dev = RawHandleDevice::open_physical(index).map_err(|e| e.to_string())?;
    let (model, serial, bus) = crate::devices::disk_identity(index);
    // NVMe first (protocol-specific query), then ATA PASS-THROUGH.
    if let Some(mut rep) = nvme_health(&dev, &model, &serial) {
        finalize_verdict(&mut rep);
        return Ok(rep);
    }
    if let Some(mut rep) = ata_smart(&dev, &model, &serial) {
        finalize_verdict(&mut rep);
        return Ok(rep);
    }
    Err(format!("no SMART transport for PhysicalDrive{index} ({bus})"))
}

/// ATA SMART READ DATA via `IOCTL_ATA_PASS_THROUGH` (SAT).
fn ata_smart(dev: &RawHandleDevice, model: &str, serial: &str) -> Option<SmartReport> {
    use windows::Win32::Storage::Ioctl::{
        ATA_PASS_THROUGH_EX, ATA_FLAGS_DATA_IN, IOCTL_ATA_PASS_THROUGH,
    };
    let handle = windows::Win32::Foundation::HANDLE(dev.as_file_handle());
    // The 512-byte SMART data sector follows the struct in the buffer.
    let struct_size = std::mem::size_of::<ATA_PASS_THROUGH_EX>();
    let mut buf = vec![0u8; struct_size + 512];
    let ata = ATA_PASS_THROUGH_EX {
        AtaFlags: ATA_FLAGS_DATA_IN,
        DataTransferLength: 512,
        TimeOutValue: 10,
        DataBufferOffset: struct_size as u32,
        PreviousTaskFile: [0; 8],
        CurrentTaskFile: [
            0xD0, // SMART READ DATA command features
            0x00, 0x00, 0x00,
            0x4F, // low cylinder
            0xC2, // high cylinder
            0xB0, // SMART command
            0xA0 | (1 << 6) // device select: LBA bit
        ],
        ..Default::default()
    };
    buf[..struct_size].copy_from_slice(unsafe {
        std::slice::from_raw_parts((&ata as *const ATA_PASS_THROUGH_EX).cast::<u8>(), struct_size)
    });
    let ok = unsafe {
        windows::Win32::System::IO::DeviceIoControl(
            handle,
            IOCTL_ATA_PASS_THROUGH,
            Some(buf.as_mut_ptr() as _),
            buf.len() as u32,
            Some(buf.as_mut_ptr() as _),
            buf.len() as u32,
            None,
            None,
        )
    };
    if !ok.as_bool() {
        return None;
    }
    let data = &buf[struct_size..];
    // SMART data: 362 bytes of attributes (30 bytes each × 12... no —
    // 2-byte revision, then 12 × 6-byte value triples, then more).
    // Layout: data[2..362] = attribute blocks of 12 bytes? Standard:
    // 512-byte sector, attributes at offset 2, each 12 bytes, 30 max.
    let mut attributes = Vec::new();
    let mut temperature = 0i32;
    let mut hours = 0u64;
    let mut cycles = 0u64;
    let mut lbas_written = 0u64;
    let mut bad_health = false;
    let mut warn_health = false;
    for i in 0..30usize {
        let off = 2 + i * 12;
        if off + 12 > data.len() {
            break;
        }
        let id = data[off];
        if id == 0 {
            continue;
        }
        let flags = u16::from_le_bytes([data[off + 1], data[off + 2]]);
        let current = data[off + 3];
        let worst = data[off + 4];
        let raw = u64::from_le_bytes([
            data[off + 5],
            data[off + 6],
            data[off + 7],
            data[off + 8],
            data[off + 9],
            data[off + 10],
        ]);
        // Vendor-specific thresholds (the CrystalDiskInfo rule set).
        let (threshold, status) = threshold_and_status(id, flags, current, worst, raw);
        if status == "bad" {
            bad_health = true;
        } else if status == "warn" {
            warn_health = true;
        }
        match id {
            0x09 => hours = raw,
            0x0C => cycles = raw,
            0xC2 => temperature = raw as i32,
            0xF1 | 0xE9 => lbas_written = lbas_written.max(raw),
            _ => {}
        }
        attributes.push(SmartAttribute {
            id,
            name: attr_name(id),
            current,
            worst,
            threshold,
            raw_value: format_raw(raw),
            status: status.to_string(),
        });
    }
    if attributes.is_empty() {
        return None;
    }
    let health = if bad_health {
        "bad"
    } else if warn_health {
        "caution"
    } else {
        "good"
    };
    Some(SmartReport {
        device_id: format!("disk-{index}"),
        device_name: format!("PhysicalDrive{index}"),
        model: model.to_string(),
        firmware: String::new(),
        serial: serial.to_string(),
        health: health.to_string(),
        temperature,
        power_on_hours: hours,
        power_cycles: cycles,
        total_writes: lbas_written.saturating_mul(512),
        interface: "ATA/SAT".into(),
        attributes,
    })
}

/// NVMe SMART/HEALTH log via the protocol-specific storage query.
fn nvme_health(dev: &RawHandleDevice, model: &str, serial: &str) -> Option<SmartReport> {
    use windows::Win32::Storage::Ioctl::{
        IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_PROPERTY_QUERY,
        STORAGE_PROPERTY_ID_StorageAdapterProtocolSpecificProperty,
        STORAGE_PROTOCOL_SPECIFIC_DATA, STORAGE_QUERY_TYPE_PropertyStandardQuery,
        STORAGE_PROTOCOL_TYPE_ProtocolTypeNvme,
    };
    let handle = windows::Win32::Foundation::HANDLE(dev.as_file_handle());
    // Query layout: STORAGE_PROPERTY_QUERY { header, STORAGE_PROTOCOL_SPECIFIC_DATA }.
    let protocol = STORAGE_PROTOCOL_SPECIFIC_DATA {
        ProtocolType: STORAGE_PROTOCOL_TYPE_ProtocolTypeNvme,
        DataType: 2, // NVMeDataTypeLogPage
        ProtocolDataRequestValue: 0x02, // SMART/HEALTH log page
        ProtocolDataRequestSubValue: 0, // controller 0
        ProtocolDataOffset: std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
        ProtocolDataLength: 512,
        FixedProtocolReturnData: 0,
        Reserved: [0; 3],
    };
    let mut buf = vec![0u8; std::mem::size_of::<STORAGE_PROPERTY_QUERY>() + 512 + 128];
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: STORAGE_PROPERTY_ID_StorageAdapterProtocolSpecificProperty,
        QueryType: STORAGE_QUERY_TYPE_PropertyStandardQuery,
        AdditionalParameters: unsafe { std::mem::zeroed() },
    };
    // Overlay: query header + protocol struct (the union member).
    let qs = std::mem::size_of::<STORAGE_PROPERTY_QUERY>();
    let ps = std::mem::size_of::<STORAGE_PROTOCOL_SPECIFIC_DATA>();
    unsafe {
        std::ptr::copy_nonoverlapping(
            (&query as *const STORAGE_PROPERTY_QUERY).cast::<u8>(),
            buf.as_mut_ptr(),
            qs,
        );
        // The AdditionalParameters union member overlaps right after the header.
        std::ptr::copy_nonoverlapping(
            (&protocol as *const STORAGE_PROTOCOL_SPECIFIC_DATA).cast::<u8>(),
            buf.as_mut_ptr().add(qs - ps),
            ps,
        );
    }
    let ok = unsafe {
        windows::Win32::System::IO::DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(buf.as_mut_ptr() as _),
            buf.len() as u32,
            Some(buf.as_mut_ptr() as _),
            buf.len() as u32,
            None,
            None,
        )
    };
    if !ok.as_bool() {
        return None;
    }
    // Health log lands at ProtocolDataOffset from the query start.
    let data_off = qs + 128; // defensive: locate past the protocol header
    let data = buf.get(data_off..data_off + 512)?;
    if data.len() < 64 {
        return None;
    }
    // NVMe SMART/health: byte 0 critical warning, 1-2 composite temp
    // (Kelvin), 32-39 data units written, 48-51 power cycles, 64-71
    // power on hours, 336-339 percentage used.
    let temp_k = u16::from_le_bytes([data[1], data[2]]);
    let temperature = temp_k as i32 - 273;
    let data_units = u128::from_le_bytes({
        let mut b = [0u8; 16];
        b[..8].copy_from_slice(&data[32..40]);
        b
    }) as u64;
    let power_cycles = u64::from_le_bytes({
        let mut b = [0u8; 8];
        b[..4].copy_from_slice(&data[48..52]);
        b
    });
    let hours = u64::from_le_bytes({
        let mut b = [0u8; 8];
        b[..4].copy_from_slice(&data[64..68]);
        b
    });
    let used = data[336];
    let critical = data[0];
    let health = if critical != 0 || used >= 100 {
        "bad"
    } else if used >= 80 {
        "caution"
    } else {
        "good"
    };
    let mut attributes = vec![
        SmartAttribute {
            id: 0xE7,
            name: "SSD Life Left".into(),
            current: 100u8.saturating_sub(used),
            worst: 100,
            threshold: 10,
            raw_value: format!("{} % used", used),
            status: if used >= 100 { "bad".into() } else if used >= 80 { "warn".into() } else { "ok".into() },
        },
        SmartAttribute {
            id: 0xC2,
            name: "Temperature".into(),
            current: 100,
            worst: 100,
            threshold: 70,
            raw_value: format!("{temperature} °C"),
            status: if temperature >= 70 { "bad".into() } else if temperature >= 60 { "warn".into() } else { "ok".into() },
        },
    ];
    attributes.push(SmartAttribute {
        id: 0x09,
        name: "Power-On Hours".into(),
        current: 100,
        worst: 100,
        threshold: 0,
        raw_value: hours.to_string(),
        status: "ok".into(),
    });
    let _ = &mut attributes;
    Some(SmartReport {
        device_id: String::new(),
        device_name: String::new(),
        model: model.to_string(),
        firmware: String::new(),
        serial: serial.to_string(),
        health: health.to_string(),
        temperature,
        power_on_hours: hours,
        power_cycles: power_cycles,
        total_writes: data_units.saturating_mul(512_000),
        interface: "NVMe".into(),
        attributes,
    })
}

/// The CrystalDiskInfo-style threshold rules.
fn threshold_and_status(id: u8, _flags: u16, current: u8, _worst: u8, raw: u64) -> (u8, &'static str) {
    match id {
        0x05 | 0xC5 | 0xC6 | 0xBB | 0xC7 => {
            // Reallocation/pending/uncorrectable: any raw > 0 warns,
            // high counts go bad.
            if raw > 1000 {
                (10, "bad")
            } else if raw > 0 {
                (10, "warn")
            } else {
                (10, "ok")
            }
        }
        0x0A | 0x0B => (10, if raw > 0 { "warn" } else { "ok" }),
        0xC2 => (
            70,
            if current != 0 && current < 40 {
                "warn"
            } else {
                "ok"
            },
        ),
        0xBE => (
            65,
            if current != 0 && current < 45 { "warn" } else { "ok" },
        ),
        0xE7 => (10, if current != 0 && current < 20 { "warn" } else { "ok" }),
        _ => (0, "ok"),
    }
}

fn format_raw(raw: u64) -> String {
    if raw > 1_000_000_000 {
        format!("{:.1} B", raw as f64)
    } else {
        raw.to_string()
    }
}

fn finalize_verdict(rep: &mut SmartReport) {
    if rep.device_id.is_empty() {
        rep.device_id = rep.device_name.clone();
    }
}
