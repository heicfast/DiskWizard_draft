//! Mock-core seed data: the device inventory (the cherry-blossom
//! reference page, byte-exact), SMART reports, cloud drives, recovery
//! destinations, and the per-target scan scale tables.
use serde_json::{json, Value};

pub const SDR_DEVICE_ID: &str = "sd-H";

const GB: f64 = 1024.0 * 1024.0 * 1024.0;

fn gb(v: f64) -> u64 {
    (v * GB) as u64
}

/// The folder names the searching label cycles through (mirrors the
/// TS mock's FOLDER_TOUR).
pub const FOLDER_TOUR: [&str; 10] = [
    "Users\\Alex\\Documents",
    "Users\\Alex\\Pictures\\Camera Roll",
    "Users\\Alex\\Videos",
    "$MFT :: deleted record walk",
    "ProgramData\\Packages",
    "Users\\Alex\\Downloads",
    "Windows\\WinSxS",
    "RAW carve :: signature sweep",
    "Users\\Alex\\Music",
    "Users\\Alex\\Desktop",
];

/// Named-field device builder input (12 positional args were a
/// readability liability at call sites — and past clippy's arg budget).
struct DeviceSpec {
    id: &'static str,
    name: &'static str,
    letter: Option<&'static str>,
    kind: &'static str,
    fs: &'static str,
    used: f64,
    total: f64,
    health: &'static str,
    bitlocker: &'static str,
    trim: Option<&'static str>,
    is_system: bool,
    disk_index: u64,
}

fn device(s: DeviceSpec) -> Value {
    json!({
        "id": s.id, "name": s.name, "letter": s.letter, "kind": s.kind,
        "fileSystem": s.fs, "used": gb(s.used), "total": gb(s.total),
        "health": s.health, "bitLocker": s.bitlocker, "trim": s.trim,
        "isSystem": s.is_system, "diskIndex": s.disk_index,
    })
}

/// The full inventory (local + external), mirroring the reference.
fn all_devices() -> Vec<Value> {
    vec![
        device(DeviceSpec {
            id: "disk0-part2",
            name: "Windows (C:)",
            letter: Some("C:"),
            kind: "ssd",
            fs: "NTFS",
            used: 120.5,
            total: 232.3,
            health: "ok",
            bitlocker: "none",
            trim: Some("on"),
            is_system: true,
            disk_index: 0,
        }),
        device(DeviceSpec {
            id: "disk1-part1",
            name: "Data (D:)",
            letter: Some("D:"),
            kind: "hdd",
            fs: "NTFS",
            used: 312.6,
            total: 931.5,
            health: "ok",
            bitlocker: "none",
            trim: None,
            is_system: false,
            disk_index: 1,
        }),
        device(DeviceSpec {
            id: "disk1-part2",
            name: "Projects (E:)",
            letter: Some("E:"),
            kind: "hdd",
            fs: "NTFS",
            used: 78.4,
            total: 500.0,
            health: "ok",
            bitlocker: "none",
            trim: None,
            is_system: false,
            disk_index: 1,
        }),
        device(DeviceSpec {
            id: "disk2-part1",
            name: "Backup (H:)",
            letter: Some("H:"),
            kind: "hdd",
            fs: "NTFS",
            used: 1_402.0,
            total: 2_000.0,
            health: "bitlocker",
            bitlocker: "locked",
            trim: None,
            is_system: false,
            disk_index: 2,
        }),
        device(DeviceSpec {
            id: "usb-F",
            name: "USB Drive (F:)",
            letter: Some("F:"),
            kind: "usb",
            fs: "exFAT",
            used: 14.8,
            total: 32.0,
            health: "ok",
            bitlocker: "none",
            trim: None,
            is_system: false,
            disk_index: 3,
        }),
        device(DeviceSpec {
            id: "ssd-G",
            name: "Portable SSD (G:)",
            letter: Some("G:"),
            kind: "portable-ssd",
            fs: "NTFS",
            used: 256.1,
            total: 500.0,
            health: "ok",
            bitlocker: "none",
            trim: Some("on"),
            is_system: false,
            disk_index: 4,
        }),
    ]
}

pub fn device_by_id(id: &str) -> Option<Value> {
    all_devices().into_iter().find(|d| d["id"] == json!(id))
}

pub fn device_by_name(name: &str) -> Option<Value> {
    all_devices().into_iter().find(|d| d["name"] == json!(name))
}

pub fn device_inventory(sd_connected: bool, lost_partition_discovered: bool) -> Value {
    let all = all_devices();
    let mut local: Vec<Value> = all.iter().filter(|d| is_local(d)).cloned().collect();
    if lost_partition_discovered {
        local.push(device(DeviceSpec {
            id: "disk1-lost0",
            name: "Lost Partition (912.4 GB)",
            letter: None,
            kind: "hdd",
            fs: "NTFS",
            used: 0.0,
            total: 912.4,
            health: "lost",
            bitlocker: "none",
            trim: None,
            is_system: false,
            disk_index: 1,
        }));
    }
    let mut external: Vec<Value> = all.iter().filter(|d| !is_local(d)).cloned().collect();
    if sd_connected {
        external.push(device(DeviceSpec {
            id: SDR_DEVICE_ID,
            name: "SD Card (H:)",
            letter: Some("H:"),
            kind: "sdcard",
            fs: "RAW",
            used: 59.4,
            total: 64.0,
            health: "raw",
            bitlocker: "none",
            trim: None,
            is_system: false,
            disk_index: 5,
        }));
    }
    json!({ "localDisks": local, "externalDevices": external, "diskImages": [] })
}

fn is_local(d: &Value) -> bool {
    matches!(d["kind"].as_str(), Some("ssd") | Some("hdd"))
}

/// Per-target simulated find scale (files, bytes) — the headline
/// numbers; "usb-F" matches the reference capture exactly.
pub fn totals(device_id: &str) -> (u64, u64) {
    match device_id {
        "usb-F" => (124_532, gb(18.6)),
        "disk0-part2" => (1_139_275, gb(147.04)),
        "disk1-part1" => (214_830, gb(96.2)),
        "disk1-part2" => (41_268, gb(12.4)),
        "ssd-G" => (189_004, gb(141.7)),
        _ => (18_420, gb(6.8)),
    }
}

/// Sectors at 512B for the target's capacity.
pub fn sectors_total(device_id: &str) -> u64 {
    let bytes = device_by_id(device_id)
        .and_then(|d| d["total"].as_u64())
        .unwrap_or_else(|| gb(64.0));
    (bytes / 512).max(1)
}

/* ── SMART reports (CrystalDiskInfo-lineage) ─────────────────────────── */

fn attr(
    id: u64,
    name: &str,
    current: u64,
    worst: u64,
    threshold: u64,
    raw: &str,
    status: &str,
) -> Value {
    json!({ "id": id, "name": name, "current": current, "worst": worst, "threshold": threshold, "rawValue": raw, "status": status })
}

pub fn smart_report(device_id: &str) -> Value {
    let (model, firmware, serial, health, temp, hours, cycles, writes, iface, attrs): (
        &str,
        &str,
        &str,
        &str,
        u64,
        u64,
        u64,
        f64,
        &str,
        Vec<Value>,
    ) = match device_id {
        "disk0-part2" => (
            "Samsung SSD 990 PRO 2TB",
            "3B2QJXA7",
            "S6Z1NJ0T812345K",
            "good",
            38,
            4_812,
            1_204,
            96.4,
            "PCIe 4.0 x4 NVMe",
            vec![
                attr(5, "Reallocated Sectors Count", 100, 100, 10, "0", "ok"),
                attr(9, "Power-On Hours", 96, 96, 0, "4,812", "ok"),
                attr(12, "Power Cycle Count", 99, 99, 0, "1,204", "ok"),
                attr(23, "SSD Life Left (Wear)", 97, 97, 10, "97 %", "ok"),
                attr(241, "Total LBAs Written", 100, 100, 0, "96.4 TB", "ok"),
                attr(242, "Total LBAs Read", 100, 100, 0, "212.7 TB", "ok"),
            ],
        ),
        "usb-F" => (
            "SanDisk Ultra Fit 3.2",
            "1.03",
            "060A1F2B3C4D5E6F",
            "good",
            41,
            1_095,
            620,
            11.8,
            "USB 3.2 Gen1",
            vec![
                attr(5, "Reallocated Sectors Count", 100, 100, 10, "0", "ok"),
                attr(9, "Power-On Hours", 98, 98, 0, "1,095", "ok"),
                attr(12, "Power Cycle Count", 99, 99, 0, "620", "ok"),
                attr(194, "Temperature", 78, 71, 45, "41 °C", "ok"),
            ],
        ),
        "ssd-G" => (
            "SanDisk Extreme V2 2TB",
            "1.00.04",
            "AA2303F987654W",
            "caution",
            46,
            3_211,
            415,
            842.9,
            "USB 3.2 Gen2 NVMe",
            vec![
                attr(5, "Reallocated Sectors Count", 100, 100, 10, "0", "ok"),
                attr(9, "Power-On Hours", 91, 91, 0, "3,211", "ok"),
                attr(12, "Power Cycle Count", 99, 99, 0, "415", "ok"),
                attr(194, "Temperature", 64, 49, 45, "46 °C", "warn"),
                attr(23, "SSD Life Left (Wear)", 37, 34, 10, "37 %", "warn"),
                attr(241, "Total LBAs Written", 100, 100, 0, "842.9 TB", "ok"),
            ],
        ),
        _ => (
            "WDC WD40EZFX-68BKR10",
            "81.00A81",
            "WD-WX32A7R61294KL",
            "good",
            33,
            18_446,
            987,
            148.2,
            "SATA III",
            vec![
                attr(5, "Reallocated Sectors Count", 100, 100, 36, "0", "ok"),
                attr(9, "Power-On Hours", 89, 89, 0, "18,446", "ok"),
                attr(10, "Spin Retry Count", 100, 100, 97, "0", "ok"),
                attr(12, "Power Cycle Count", 100, 100, 0, "987", "ok"),
                attr(194, "Temperature", 72, 60, 45, "33 °C", "ok"),
                attr(196, "Reallocation Event Count", 100, 100, 0, "0", "ok"),
                attr(197, "Current Pending Sector Count", 100, 100, 0, "0", "ok"),
                attr(198, "Uncorrectable Sector Count", 100, 100, 0, "0", "ok"),
            ],
        ),
    };
    let device_name = device_by_id(device_id)
        .and_then(|d| d["name"].as_str().map(str::to_string))
        .unwrap_or_else(|| "Disk".into());
    json!({
        "deviceId": device_id, "deviceName": device_name,
        "model": model, "firmware": firmware, "serial": serial,
        "health": health, "temperature": temp,
        "powerOnHours": hours, "powerCycles": cycles, "totalWrites": writes,
        "interface": iface, "attributes": attrs,
    })
}

/* ── Cloud drives ────────────────────────────────────────────────────── */

pub fn cloud_drives() -> Value {
    json!([
        { "id": "cloud-icloud", "provider": "icloud", "account": "alex.morgan@icloud.com", "label": "iCloud Drive", "used": gb(82.4), "total": gb(200.0) },
        { "id": "cloud-gdrive", "provider": "google", "account": "alex.morgan@gmail.com", "label": "Google Drive", "used": gb(41.2), "total": gb(100.0) },
        { "id": "cloud-dropbox", "provider": "dropbox", "account": "alex@morganstudio.co", "label": "Dropbox", "used": gb(6.8), "total": gb(2048.0) },
        { "id": "cloud-onedrive", "provider": "onedrive", "account": "alex.morgan@outlook.com", "label": "OneDrive", "used": gb(118.9), "total": gb(1024.0) },
    ])
}

pub fn cloud_drive(id: &str) -> Value {
    cloud_drives()
        .as_array()
        .and_then(|a| a.iter().find(|d| d["id"] == json!(id)).cloned())
        .unwrap_or_else(|| json!({ "id": "cloud-icloud", "label": "iCloud Drive" }))
}

/* ── Recovery destinations ───────────────────────────────────────────── */

pub fn recover_destinations(source_id: Option<&str>) -> Value {
    let mut list: Vec<Value> = all_devices()
        .into_iter()
        .filter(|d| d["id"].as_str() != source_id && d["health"].as_str() != Some("bitlocker"))
        .map(|d| {
            let used = d["used"].as_u64().unwrap_or(0);
            let total = d["total"].as_u64().unwrap_or(1);
            json!({
                "id": d["id"], "name": d["name"], "kind": "local",
                "freeBytes": total.saturating_sub(used), "totalBytes": total,
                "isSource": d["id"].as_str() == source_id,
            })
        })
        .collect();
    for c in cloud_drives().as_array().into_iter().flatten() {
        let used = c["used"].as_u64().unwrap_or(0);
        let total = c["total"].as_u64().unwrap_or(1);
        list.push(json!({
            "id": c["id"], "name": c["label"], "kind": "cloud",
            "freeBytes": total.saturating_sub(used), "totalBytes": total,
        }));
    }
    json!(list)
}

/* ── Recent sessions (the reference page) ────────────────────────────── */

pub fn initial_sessions() -> Vec<Value> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = |n: u64| now.saturating_sub(n * 86400 + 4 * 3600);
    vec![
        json!({
            "id": "rs-1", "targetName": "USB Drive (F:)", "status": "completed",
            "filesFound": 1_248, "bytesFound": gb(18.6), "startedAt": days(2),
            "sessionFile": "auto_USB Drive (F_)10_15_21_04_12.rsf",
        }),
        json!({
            "id": "rs-2", "targetName": "Data (D:)", "status": "paused",
            "filesFound": 12_430, "bytesFound": gb(214.8), "startedAt": days(5),
            "sessionFile": "auto_Data (D_)10_12_09_41_55.rsf",
        }),
        json!({
            "id": "rs-3", "targetName": "Projects (E:)", "status": "completed",
            "filesFound": 856, "bytesFound": gb(4.2), "startedAt": days(9),
            "sessionFile": "auto_Projects (E_)10_08_16_22_31.rsf",
        }),
    ]
}

/// The recovery progress's current-file pool.
pub fn current_file(i: u64) -> String {
    const POOL: [&str; 12] = [
        "Vacation Photo.jpg",
        "Project Proposal.docx",
        "holiday-4k.mp4",
        "Family.png",
        "Invoice 2026-03.pdf",
        "Podcast Episode 12.mp3",
        "backup-2026.zip",
        "IMG_4812.jpg",
        "Thesis Chapter 3.docx",
        "Drone Flight 04.mov",
        "Demo Track.wav",
        "Resume.pdf",
    ];
    POOL[(i * 7) as usize % POOL.len()].to_string()
}
