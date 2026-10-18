//! SMART — the free-tier disk health engine.
//!
//! Multi-transport per the research (`artifacts/DiskHealth/
//! diskhealth--smart-companion.md` — the CrystalDiskInfo CAtaSmart
//! lineage): ATA PASS-THROUGH (SAT) for SATA/USB bridges, NVMe
//! admin-queue passthrough for NVMe, both through
//! `IOCTL_STORAGE_QUERY_PROPERTY` protocol-specific queries and
//! `IOCTL_ATA_PASS_THROUGH`. Output: the standard 22-attribute table
//! plus temperature plus verdict (Good/Caution/Bad by threshold rules)
//! — the wire shape the UI's Disk Tools dashboard renders.

use crate::win_len;
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
    ATA_ATTR_NAMES.iter().find(|(i, _)| *i == id).map_or_else(
        || format!("Attribute 0x{id:02X}"),
        |(_, n)| (*n).to_string(),
    )
}

/// Build the report for a physical disk (probes transport by device).
///
/// # Errors
/// String error when no transport yields data.
pub fn report_for_disk(index: u32) -> Result<SmartReport, String> {
    let dev = RawHandleDevice::open_physical(index).map_err(|e| e.to_string())?;
    let (model, serial, bus) = crate::devices::disk_identity(index);
    // NVMe first (protocol-specific query), then ATA PASS-THROUGH.
    if let Some(mut rep) = nvme_health(&dev, index, &model, &serial) {
        finalize_verdict(&mut rep);
        return Ok(rep);
    }
    if let Some(mut rep) = ata_smart(&dev, index, &model, &serial) {
        finalize_verdict(&mut rep);
        return Ok(rep);
    }
    Err(format!(
        "no SMART transport for PhysicalDrive{index} ({bus})"
    ))
}

/// The distilled ATA SMART READ DATA sector.
struct AtaSummary {
    attributes: Vec<SmartAttribute>,
    temperature: i32,
    hours: u64,
    cycles: u64,
    lbas_written: u64,
    bad_health: bool,
    warn_health: bool,
}

/// Walk the 512-byte SMART READ DATA sector: 2-byte revision, then up
/// to 30 attribute blocks of 12 bytes (id, flags u16, value, worst,
/// raw 48-bit LE, reserved).
fn parse_ata_smart_sector(data: &[u8]) -> AtaSummary {
    let mut s = AtaSummary {
        attributes: Vec::new(),
        temperature: 0,
        hours: 0,
        cycles: 0,
        lbas_written: 0,
        bad_health: false,
        warn_health: false,
    };
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
            0,
            0,
        ]);
        // Vendor-specific thresholds (the CrystalDiskInfo rule set).
        let (threshold, status) = threshold_and_status(id, flags, current, worst, raw);
        if status == "bad" {
            s.bad_health = true;
        } else if status == "warn" {
            s.warn_health = true;
        }
        match id {
            0x09 => s.hours = raw,
            0x0C => s.cycles = raw,
            // Temperature lives in the RAW value's low word (vendors
            // put °C in the low byte or word; the normalized value is
            // NOT the temperature).
            0xC2 => s.temperature = i32::from(u16::try_from(raw & 0xFFFF).unwrap_or(0)),
            0xF1 | 0xE9 => s.lbas_written = s.lbas_written.max(raw),
            _ => {}
        }
        s.attributes.push(SmartAttribute {
            id,
            name: attr_name(id),
            current,
            worst,
            threshold,
            raw_value: format_raw(raw),
            status: status.to_string(),
        });
    }
    s
}

/// ATA SMART READ DATA via `IOCTL_ATA_PASS_THROUGH` (SAT).
fn ata_smart(dev: &RawHandleDevice, index: u32, model: &str, serial: &str) -> Option<SmartReport> {
    use windows::Win32::Storage::IscsiDisc::{
        ATA_FLAGS_DATA_IN, ATA_PASS_THROUGH_EX, IOCTL_ATA_PASS_THROUGH,
    };
    let handle = windows::Win32::Foundation::HANDLE(dev.as_file_handle());
    // The 512-byte SMART data sector follows the struct in the buffer.
    let struct_size = std::mem::size_of::<ATA_PASS_THROUGH_EX>();
    let mut buf = vec![0u8; struct_size + 512];
    let ata = ATA_PASS_THROUGH_EX {
        // The flag constant is 2 (fits u16 by construction; a wider
        // value would be an upstream breaking change we want to hear
        // about, hence the explicit saturate-to-0).
        AtaFlags: u16::try_from(ATA_FLAGS_DATA_IN).unwrap_or(0),
        DataTransferLength: 512,
        TimeOutValue: 10,
        DataBufferOffset: struct_size,
        PreviousTaskFile: [0; 8],
        CurrentTaskFile: [
            0xD0, // SMART READ DATA command features
            0x00,
            0x00,
            0x00,
            0x4F,            // low cylinder
            0xC2,            // high cylinder
            0xB0,            // SMART command
            0xA0 | (1 << 6), // device select: LBA bit
        ],
        ..Default::default()
    };
    buf[..struct_size].copy_from_slice(unsafe {
        std::slice::from_raw_parts((&raw const ata).cast::<u8>(), struct_size)
    });
    let ok = unsafe {
        windows::Win32::System::IO::DeviceIoControl(
            handle,
            IOCTL_ATA_PASS_THROUGH,
            Some(buf.as_mut_ptr() as _),
            win_len(buf.len()),
            Some(buf.as_mut_ptr().cast()),
            win_len(buf.len()),
            None,
            None,
        )
    };
    if ok.is_err() {
        return None;
    }
    let s = parse_ata_smart_sector(&buf[struct_size..]);
    if s.attributes.is_empty() {
        return None;
    }
    let health = if s.bad_health {
        "bad"
    } else if s.warn_health {
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
        temperature: s.temperature,
        power_on_hours: s.hours,
        power_cycles: s.cycles,
        total_writes: s.lbas_written.saturating_mul(512),
        interface: "ATA/SAT".into(),
        attributes: s.attributes,
    })
}

/// Query the NVMe SMART/HEALTH log page (512 bytes) through the
/// protocol-specific storage query.
///
/// Request layout (the documented MSDN pattern): an 8-byte
/// `STORAGE_PROPERTY_QUERY` header, the
/// `STORAGE_PROTOCOL_SPECIFIC_DATA` request at offset 8 (where
/// `AdditionalParameters` begins), and the 512-byte log area right
/// after it. `ProtocolDataOffset` counts from the START of the
/// protocol-specific structure.
fn nvme_query_health_log(dev: &RawHandleDevice) -> Option<Vec<u8>> {
    use windows::Win32::System::Ioctl::{
        IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, ProtocolTypeNvme,
        STORAGE_PROPERTY_QUERY, STORAGE_PROTOCOL_SPECIFIC_DATA,
        StorageAdapterProtocolSpecificProperty,
    };
    const HEADER_LEN: usize = 8; // STORAGE_PROPERTY_HEADER (PropertyId + QueryType)
    let ps = std::mem::size_of::<STORAGE_PROTOCOL_SPECIFIC_DATA>();
    let protocol = STORAGE_PROTOCOL_SPECIFIC_DATA {
        ProtocolType: ProtocolTypeNvme,
        DataType: 2,                     // NVMeDataTypeLogPage
        ProtocolDataRequestValue: 0x02,  // SMART/HEALTH log page
        ProtocolDataRequestSubValue: 0,  // controller 0
        ProtocolDataOffset: win_len(ps), // log area follows the protocol struct
        ProtocolDataLength: 512,
        ..Default::default()
    };
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageAdapterProtocolSpecificProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0; 1],
    };
    // header + protocol request + 512-byte log response area.
    let mut buf = vec![0u8; HEADER_LEN + ps + 512];
    // SAFETY: both structs are repr(C) over u32 fields (no padding), and
    // `buf` is sized for header + struct + data, so these fixed-length
    // copies stay in bounds.
    unsafe {
        std::ptr::copy_nonoverlapping(
            (&raw const query).cast::<u8>(),
            buf.as_mut_ptr(),
            HEADER_LEN,
        );
        std::ptr::copy_nonoverlapping(
            (&raw const protocol).cast::<u8>(),
            buf.as_mut_ptr().add(HEADER_LEN),
            ps,
        );
    }
    let handle = windows::Win32::Foundation::HANDLE(dev.as_file_handle());
    let ok = unsafe {
        windows::Win32::System::IO::DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(buf.as_mut_ptr() as _),
            win_len(buf.len()),
            Some(buf.as_mut_ptr().cast()),
            win_len(buf.len()),
            None,
            None,
        )
    };
    if ok.is_err() {
        return None;
    }
    // The health log returns at HEADER_LEN + ProtocolDataOffset.
    let data_off = HEADER_LEN + ps;
    buf.get(data_off..data_off + 512).map(<[u8]>::to_vec)
}

/// The distilled NVMe health log.
struct NvmeSummary {
    temperature: i32,
    data_units_written: u64,
    power_cycles: u64,
    hours: u64,
    media_errors: u64,
    used: u8,
    critical: u8,
    spare: u8,
    spare_threshold: u8,
}

/// Parse the 512-byte NVMe SMART/health log (NVMe 2.x spec):
/// 0 critical warning, 1-2 temperature (K), 3 available spare,
/// 4 spare threshold, 5 percentage used, 16-31 data units read,
/// 32-47 data units written, 48-63 host reads, 64-79 host writes,
/// 80-95 busy time, 96-111 power cycles, 112-127 power-on hours,
/// 128-143 unsafe shutdowns, 144-159 media errors.
fn parse_nvme_health_log(data: &[u8]) -> Option<NvmeSummary> {
    let read128 = |off: usize| -> Option<u64> {
        let mut b = [0u8; 16];
        b.copy_from_slice(data.get(off..off + 16)?);
        // 128-bit counters saturate at u64 (a drive that wrote 2^64
        // data units does not exist; the UI displays "18.4 EB").
        Some(u64::try_from(u128::from_le_bytes(b)).unwrap_or(u64::MAX))
    };
    Some(NvmeSummary {
        temperature: i32::from(u16::from_le_bytes([data[1], data[2]])) - 273,
        data_units_written: read128(32)?,
        power_cycles: read128(96)?,
        hours: read128(112)?,
        media_errors: read128(144)?,
        used: data[5],
        critical: data[0],
        spare: data[3],
        spare_threshold: data[4],
    })
}

/// NVMe SMART/HEALTH report: query the log page, parse, grade.
fn nvme_health(
    dev: &RawHandleDevice,
    index: u32,
    model: &str,
    serial: &str,
) -> Option<SmartReport> {
    let log = nvme_query_health_log(dev)?;
    let s = parse_nvme_health_log(&log)?;
    let health = if s.critical != 0 || s.used >= 100 || s.spare < s.spare_threshold {
        "bad"
    } else if s.used >= 80 || s.media_errors > 0 || s.temperature >= 60 {
        "caution"
    } else {
        "good"
    };
    let attributes = nvme_attributes(&s);
    Some(SmartReport {
        device_id: format!("disk-{index}"),
        device_name: format!("PhysicalDrive{index}"),
        model: model.to_string(),
        firmware: String::new(),
        serial: serial.to_string(),
        health: health.to_string(),
        temperature: s.temperature,
        power_on_hours: s.hours,
        power_cycles: s.power_cycles,
        total_writes: s.data_units_written.saturating_mul(512_000),
        interface: "NVMe".into(),
        attributes,
    })
}

/// The attribute rows the Disk Tools dashboard renders for an NVMe
/// drive (mapped onto the ATA id/name vocabulary the UI knows).
fn nvme_attributes(s: &NvmeSummary) -> Vec<SmartAttribute> {
    vec![
        SmartAttribute {
            id: 0xE7,
            name: "Percentage Used".into(),
            current: 100u8.saturating_sub(s.used),
            worst: 100,
            threshold: 10,
            raw_value: format!("{} % used", s.used),
            status: if s.used >= 100 {
                "bad".into()
            } else if s.used >= 80 {
                "warn".into()
            } else {
                "ok".into()
            },
        },
        SmartAttribute {
            id: 0xC2,
            name: "Temperature".into(),
            current: 100,
            worst: 100,
            threshold: 70,
            raw_value: format!("{} °C", s.temperature),
            status: if s.temperature >= 70 {
                "bad".into()
            } else if s.temperature >= 60 {
                "warn".into()
            } else {
                "ok".into()
            },
        },
        SmartAttribute {
            id: 0x09,
            name: "Power-On Hours".into(),
            current: 100,
            worst: 100,
            threshold: 0,
            raw_value: s.hours.to_string(),
            status: "ok".into(),
        },
        SmartAttribute {
            id: 0x0C,
            name: "Power Cycle Count".into(),
            current: 100,
            worst: 100,
            threshold: 0,
            raw_value: s.power_cycles.to_string(),
            status: "ok".into(),
        },
        SmartAttribute {
            id: 0xC6,
            name: "Media and Data Integrity Errors".into(),
            current: 100,
            worst: 100,
            threshold: 0,
            raw_value: s.media_errors.to_string(),
            status: if s.media_errors > 0 {
                "warn".into()
            } else {
                "ok".into()
            },
        },
        SmartAttribute {
            id: 0xF1,
            name: "Total LBAs Written".into(),
            current: 100,
            worst: 100,
            threshold: 0,
            raw_value: format_bytes(s.data_units_written.saturating_mul(512_000)),
            status: "ok".into(),
        },
    ]
}

/// The CrystalDiskInfo-style threshold rules.
fn threshold_and_status(
    id: u8,
    _flags: u16,
    _current: u8,
    _worst: u8,
    raw: u64,
) -> (u8, &'static str) {
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
        // Temperature (0xC2): the RAW value carries °C (most drives
        // use the low byte or word); the normalized value is not the
        // temperature.
        0xC2 => (
            60,
            if raw >= 70 {
                "bad"
            } else if raw >= 60 {
                "warn"
            } else {
                "ok"
            },
        ),
        0xBE => (65, if raw >= 70 { "warn" } else { "ok" }),
        0xE7 => (10, if raw >= 90 { "warn" } else { "ok" }),
        _ => (0, "ok"),
    }
}

/// Human-readable byte count (KiB/MiB/GiB/TiB — binary units, the
/// storage-industry standard display).
fn format_bytes(mut bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut unit = 0usize;
    while bytes >= 1024 && unit < UNITS.len() - 1 {
        bytes /= 1024;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{bytes:.1} {}", UNITS[unit])
    }
}

/// Raw attribute value display: human bytes when it looks like a
/// byte/LBA count, plain number otherwise.
fn format_raw(raw: u64) -> String {
    if raw > 1_000_000_000 {
        format_bytes(raw)
    } else {
        raw.to_string()
    }
}

fn finalize_verdict(rep: &mut SmartReport) {
    if rep.device_id.is_empty() {
        rep.device_id = rep.device_name.clone();
    }
}
