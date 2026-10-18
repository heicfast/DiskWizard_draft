//! Device commands — the real inventory via dw-win, the SMART free
//! tier, BitLocker unlock, SDR.

use serde_json::{Value, json};
use tauri::State;

use super::AppState;

/* ── Devices & health (SMART = free tier — never gated) ─────────── */

#[tauri::command]
pub fn list_devices(s: State<'_, AppState>) -> Value {
    #[cfg(windows)]
    {
        let _ = &s;
        let inv = dw_win::devices::enumerate_inventory();
        let to_json = |d: &dw_win::devices::DeviceRow| {
            json!({
                "id": d.id, "name": d.name, "letter": d.letter,
                "kind": d.kind, "fileSystem": d.file_system,
                "used": d.used, "total": d.total,
                "health": d.health, "bitLocker": d.bit_locker,
                "trim": d.trim, "isSystem": d.is_system,
                "diskIndex": d.disk_index,
            })
        };
        json!({
            "localDisks": inv.local_disks.iter().map(to_json).collect::<Vec<_>>(),
            "externalDevices": inv.external_devices.iter().map(to_json).collect::<Vec<_>>(),
            "diskImages": [],
        })
    }
    #[cfg(not(windows))]
    {
        let _ = &s;
        json!({ "localDisks": [], "externalDevices": [], "diskImages": [] })
    }
}

#[tauri::command]
pub async fn refresh_devices() -> Result<Value, String> {
    // Enumeration happens per list_devices call (device-change events
    // arrive via the frontend's devices-changed listener); a fresh
    // round-trip IS the refresh.
    Ok(json!({ "ok": true }))
}

#[tauri::command]
pub async fn connect_demo_device() -> Result<Value, String> {
    // Dev/demo-only in the mock contract; the real core sees real SD
    // cards when they appear.
    Err("NOT_AVAILABLE. Demo devices are a simulation-only feature.".into())
}

#[tauri::command]
pub fn smart_report(device_id: String) -> Result<Value, String> {
    #[cfg(windows)]
    {
        // Device ids: diskN-partM / diskN-lostL / vol-X / usb-X.
        let disk = dw_win::volume::parse_device_id(&device_id)
            .map(|t| match t {
                dw_win::volume::DeviceTarget::WholeDisk(n)
                | dw_win::volume::DeviceTarget::Partition { disk: n, .. }
                | dw_win::volume::DeviceTarget::LostPartition { disk: n, .. } => Some(n),
                dw_win::volume::DeviceTarget::Volume(letter) => {
                    dw_win::blockdev::RawHandleDevice::open_volume(letter)
                        .ok()
                        .and_then(|d| d.device_number().ok().map(|(n, _)| n))
                }
            })
            .flatten();
        let Some(index) = disk else {
            return Err(format!("UNKNOWN_DEVICE. {device_id}"));
        };
        let report = dw_win::smart::report_for_disk(index)
            .map_err(|e| format!("SMART_UNAVAILABLE. Disk health data could not be read: {e}"))?;
        Ok(json!({
            "deviceId": device_id,
            "deviceName": report.device_name,
            "model": report.model,
            "firmware": report.firmware,
            "serial": report.serial,
            "health": report.health,
            "temperature": report.temperature,
            "powerOnHours": report.power_on_hours,
            "powerCycles": report.power_cycles,
            "totalWrites": report.total_writes,
            "interface": report.interface,
            "attributes": report.attributes.iter().map(|a| json!({
                "id": a.id, "name": a.name, "current": a.current,
                "worst": a.worst, "threshold": a.threshold,
                "rawValue": a.raw_value, "status": a.status,
            })).collect::<Vec<_>>(),
        }))
    }
    #[cfg(not(windows))]
    {
        let _ = device_id;
        Err("SMART_UNAVAILABLE. Disk health requires Windows.".into())
    }
}

#[tauri::command]
pub async fn sdr_diagnose(s: State<'_, AppState>, device_id: String) -> Result<Value, String> {
    super::license::require_licensed(&s)?;
    // SD Card Rescue: quick sanity verdict (the deep RawFixer-lite
    // repair pass lands in M2 per docs/CORE-PLAN.md).
    #[cfg(windows)]
    {
        let target_ok = matches!(
            dw_win::volume::parse_device_id(&device_id),
            Some(dw_win::volume::DeviceTarget::Volume(_))
        );
        if !target_ok {
            return Err("NOT_AN_SDCARD. SDR targets SD-card volumes.".into());
        }
        Ok(json!({
            "restoredAccess": false,
            "message": "Diagnostics complete. The deep repair pass arrives with the next engine update; raw-volume file recovery is available now.",
            "fileSystemRecovered": "unknown",
        }))
    }
    #[cfg(not(windows))]
    {
        let _ = device_id;
        Err("NOT_AVAILABLE. SDR requires Windows.".into())
    }
}

#[tauri::command]
pub async fn unlock_bitlocker(
    s: State<'_, AppState>,
    device_id: String,
    password: Option<String>,
    recovery_key: Option<String>,
) -> Result<Value, String> {
    let _ = &s;
    #[cfg(windows)]
    {
        let Some(dw_win::volume::DeviceTarget::Volume(letter)) =
            dw_win::volume::parse_device_id(&device_id)
        else {
            return Err("NOT_LOCKED. BitLocker unlock targets drive-letter volumes.".into());
        };
        if !dw_win::bitlocker::volume_is_bitlocker_locked(letter) {
            return Err("NOT_LOCKED. This drive is not BitLocker-locked.".into());
        }
        let credential = if let Some(key) = recovery_key {
            dw_win::bitlocker::Credential::RecoveryKey(key)
        } else if let Some(pw) = password {
            dw_win::bitlocker::Credential::Password(pw)
        } else {
            return Err("BAD_REQUEST. A password or recovery key is required.".into());
        };
        dw_win::bitlocker::unlock_volume(letter, &credential)?;
        if dw_win::bitlocker::volume_unlocked(letter) {
            Ok(json!({ "ok": true, "unlocked": true }))
        } else {
            Err("UNLOCK_FAILED. The drive did not mount after unlocking.".into())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (device_id, password, recovery_key);
        Err("NOT_AVAILABLE. BitLocker unlock requires Windows.".into())
    }
}

/* ── Cloud (M3 per the plan) ─────────────────────────────────────── */

#[tauri::command]
pub fn list_cloud_drives() -> Value {
    json!([
        { "id": "icloud", "provider": "icloud", "account": "", "label": "iCloud Drive", "used": 0, "total": 0 },
        { "id": "google", "provider": "google", "account": "", "label": "Google Drive", "used": 0, "total": 0 },
        { "id": "dropbox", "provider": "dropbox", "account": "", "label": "Dropbox", "used": 0, "total": 0 },
        { "id": "onedrive", "provider": "onedrive", "account": "", "label": "OneDrive", "used": 0, "total": 0 },
    ])
}

#[tauri::command]
pub async fn cloud_sign_in(_drive_id: String) -> Result<Value, String> {
    Err("NOT_AVAILABLE. Cloud recovery connects in the M3 engine update.".into())
}

#[tauri::command]
pub async fn start_cloud_scan(s: State<'_, AppState>, drive_id: String) -> Result<Value, String> {
    super::license::require_licensed(&s)?;
    let _ = drive_id;
    Err("NOT_AVAILABLE. Cloud recovery connects in the M3 engine update.".into())
}
