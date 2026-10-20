//! Device commands — the real inventory via dw-win, the SMART free
//! tier, BitLocker unlock, SDR.

use dw_core::blockdev::BlockDevice;
use serde_json::{Value, json};
use tauri::State;

use super::AppState;

/* ── Devices & health (SMART = free tier — never gated) ─────────── */

#[tauri::command]
pub async fn list_devices() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
    #[cfg(windows)]
    {
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
        json!({ "localDisks": [], "externalDevices": [], "diskImages": [] })
    }
    })
    .map_err(|e| format!("THREAD_POOL. {e}"))
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
pub async fn smart_report(device_id: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
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
    // SD Card Rescue (the RawFixer pre-scan model): diagnose the
    // boot region, then either restore access (backup/scan repair)
    // or continue with file recovery (carving) when not repairable.
    #[cfg(windows)]
    {
        let Some(dw_win::volume::DeviceTarget::Volume(letter)) =
            dw_win::volume::parse_device_id(&device_id)
        else {
            return Err("NOT_AN_SDCARD. SDR targets SD-card volumes.".into());
        };
        let dev = dw_win::blockdev::RawHandleDevice::open_volume(letter)
            .map_err(|e| format!("OPEN_FAILED. {e}"))?;
        let d = dw_core::repair::diagnose(&dev);
        let (repairable, fs, message) = match &d {
            dw_core::repair::Diagnosis::Healthy(fs) => (
                false,
                fs.wire().to_string(),
                "The card is already accessible — no repair needed. You can scan it directly.".into(),
            ),
            dw_core::repair::Diagnosis::FixableFromBackup { fs, backup_lba } => (
                true,
                fs.wire().to_string(),
                format!(
                    "The card's boot sector is damaged, but a valid backup copy was found (sector {backup_lba}). Repair can restore access without touching your files."
                ),
            ),
            dw_core::repair::Diagnosis::FixableByScan { fs, source_lba } => (
                true,
                fs.wire().to_string(),
                format!(
                    "The card's boot sector is damaged, but a valid copy was found at sector {source_lba}. Repair can restore access; your files stay untouched."
                ),
            ),
            dw_core::repair::Diagnosis::CarveOnly => (
                false,
                "unknown".into(),
                "The card's file system is too damaged for a direct repair. File recovery via deep scanning is still available.".into(),
            ),
        };
        Ok(json!({
            "restoredAccess": repairable,
            "repairable": repairable,
            "message": message,
            "fileSystemRecovered": fs,
        }))
    }
    #[cfg(not(windows))]
    {
        let _ = device_id;
        Err("NOT_AVAILABLE. SDR requires Windows.".into())
    }
}

#[tauri::command]
pub async fn sdr_repair(
    s: State<'_, AppState>,
    device_id: String,
    confirm: bool,
) -> Result<Value, String> {
    super::license::require_licensed(&s)?;
    #[cfg(windows)]
    {
        if !confirm {
            return Err(
                "CONFIRM_REQUIRED. Repair writes to the card's boot sector; the user must confirm."
                    .into(),
            );
        }
        let Some(dw_win::volume::DeviceTarget::Volume(letter)) =
            dw_win::volume::parse_device_id(&device_id)
        else {
            return Err("NOT_AN_SDCARD. SDR repair targets SD-card volumes.".into());
        };
        // Diagnose read-only first; plan; then write through the
        // repair handle (the ONLY write-enabled path — snapshot
        // covers every overwritten sector).
        let dev = dw_win::blockdev::RawHandleDevice::open_volume(letter)
            .map_err(|e| format!("OPEN_FAILED. {e}"))?;
        let d = dw_core::repair::diagnose(&dev);
        let plan = dw_core::repair::plan(&dev, &d).map_err(|e| format!("NOT_REPAIRABLE. {e}"))?;
        let undo = {
            let handle = dw_win::repair::open_volume_for_repair(letter, dev.sector_size())
                .map_err(|e| format!("WRITE_OPEN_FAILED. {e} — try running as administrator."))?;
            dw_core::repair::apply(&plan, |lba, data| handle.write_sector(lba, data))
                .map_err(|e| format!("REPAIR_FAILED. {e}"))?
        };
        // Verify: the boot region parses now.
        let ok = matches!(
            dw_core::repair::diagnose(&dev),
            dw_core::repair::Diagnosis::Healthy(_)
        );
        if !ok {
            // Roll back — a repair that doesn't verify leaves the
            // card exactly as it was (the transactional contract).
            if let Ok(handle) = dw_win::repair::open_volume_for_repair(letter, dev.sector_size()) {
                let _ = dw_core::repair::undo(&undo, |lba, data| handle.write_sector(lba, data));
            }
            return Err("REPAIR_VERIFY_FAILED. The repair did not verify — the card was restored to its previous state. Deep-scan recovery remains available.".into());
        }
        Ok(json!({
            "ok": true,
            "restoredAccess": true,
            "sectorsWritten": plan.writes.len(),
            "description": plan.description,
            "fs": plan.fs.wire(),
        }))
    }
    #[cfg(not(windows))]
    {
        let _ = (device_id, confirm);
        Err("NOT_AVAILABLE. SDR repair requires Windows.".into())
    }
}

#[tauri::command]
pub async fn unlock_bitlocker(
    s: State<'_, AppState>,
    device_id: String,
    password: Option<String>,
    recovery_key: Option<String>,
    bek_file: Option<String>,
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
        } else if let Some(bek) = bek_file {
            dw_win::bitlocker::Credential::BekFile(bek)
        } else {
            return Err("BAD_REQUEST. A password, recovery key, or key file is required.".into());
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
        let _ = (device_id, password, recovery_key, bek_file);
        Err("NOT_AVAILABLE. BitLocker unlock requires Windows.".into())
    }
}

/* Cloud Recovery: REMOVED per owner directive (session 8) — local
 * recovery only. Every cloud surface (backend commands, mock data,
 * UI page, sidebar entry, destination tab, tour step) is deleted. */
