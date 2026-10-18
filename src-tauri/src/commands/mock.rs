//! Mock-core (this session): the full command surface against a
//! realistic simulation mirroring the reverse-engineered architecture
//! (research artifacts: engine phase machine, results tree branches,
//! recoverability grading, session persistence, the DiskGenie
//! licensing postures).
//!
//! Realism contract (identical to the TS browser mock, so both mocks
//! behave the same):
//!  - Devices: the cherry-blossom reference inventory (C:/D:/E: + F:/G:
//!    + BitLocker H:), byte-exact capacities.
//!  - Scan: preparing → quick (0..38%) → deep (38..100%) → organizing
//!    → done, with live sector counters, a searching label, progressive
//!    result filling and auto-saved sessions (.rsf naming).
//!  - SD Card Rescue pre-phase for RAW cards.
//!  - Licensing: unlicensed by default; ACTIVATION_REQUIRED gates the
//!    privileged commands; disk health stays free; the tour's
//!    `license_sim_set` flips pro ↔ unlicensed.
pub mod data;
pub mod state;
pub mod tree;

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};

use state::{LicensePosture, MockState};

pub type SharedState = Arc<Mutex<MockState>>;

/// The app-managed state instance.
pub fn shared_state() -> SharedState {
    Arc::new(Mutex::new(MockState::new()))
}

/// Async sleep that never blocks an IPC or the UI thread (the mock's
/// simulated round-trips: device refresh, activation, OAuth…).
async fn delay(ms: u64) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(Duration::from_millis(ms));
    })
    .await;
}

/* ── Licensing gate (the L6 command-layer boundary) ─────────────────── */

fn license_gate(s: &State<'_, SharedState>) -> Result<(), String> {
    let posture = s.lock().license_posture;
    if matches!(posture, LicensePosture::Pro | LicensePosture::Grace) {
        Ok(())
    } else {
        Err("ACTIVATION_REQUIRED. Activate DiskWizard Pro to use this.".into())
    }
}

/* ── Devices & health (SMART = free tier) ────────────────────────────── */

/// Apply the session's unlock state over a stored device row.
fn apply_unlock(st: &state::MockState, mut d: Value) -> Value {
    if let Some(id) = d["id"].as_str() {
        if st.bitlocker_unlocked.contains(id) {
            d["health"] = json!("ok");
            d["bitLocker"] = json!("unlocked");
        }
    }
    d
}

#[tauri::command]
pub fn list_devices(s: State<'_, SharedState>) -> Value {
    let st = s.lock();
    let inv = data::device_inventory(st.sd_connected, st.lost_partition_discovered);
    let map = |key: &str| -> Vec<Value> {
        inv[key]
            .as_array()
            .map(|a| a.iter().map(|d| apply_unlock(&st, d.clone())).collect())
            .unwrap_or_default()
    };
    json!({
        "localDisks": map("localDisks"),
        "externalDevices": map("externalDevices"),
        "diskImages": inv["diskImages"].clone(),
    })
}

#[tauri::command]
pub async fn refresh_devices() -> Result<Value, String> {
    delay(1_400).await;
    Ok(json!({ "ok": true }))
}

#[tauri::command]
pub async fn connect_demo_device(s: State<'_, SharedState>) -> Result<Value, String> {
    delay(900).await;
    s.lock().sd_connected = true;
    Ok(json!({ "ok": true, "deviceId": data::SDR_DEVICE_ID }))
}

#[tauri::command]
pub fn smart_report(device_id: String) -> Result<Value, String> {
    Ok(data::smart_report(&device_id))
}

#[tauri::command]
pub async fn sdr_diagnose(s: State<'_, SharedState>) -> Result<Value, String> {
    license_gate(&s)?;
    delay(2_800).await;
    Ok(json!({
        "restoredAccess": true,
        "message": "Access restored. Continuing with file recovery.",
        "fileSystemRecovered": "exFAT",
    }))
}

#[tauri::command]
pub async fn sdr_repair(
    s: State<'_, SharedState>,
    _device_id: String,
    _confirm: bool,
) -> Result<Value, String> {
    // The dev-sim shape: same verdict + progress contract as the
    // real repair (dev harness parity for the UI agents).
    license_gate(&s)?;
    delay(3_200).await;
    Ok(json!({
        "ok": true,
        "restoredAccess": true,
        "sectorsWritten": 1,
        "description": "Restored the boot sector from the backup copy.",
        "fs": "exFAT",
    }))
}

/* ── Scan lifecycle (licensed) ───────────────────────────────────────── */

#[tauri::command]
pub async fn unlock_bitlocker(
    s: State<'_, SharedState>,
    device_id: String,
) -> Result<Value, String> {
    // The FVE-service unlock round-trip (password / recovery key
    // verified through the pipe; the volume mounts decrypted).
    delay(1_700).await;
    let device = data::device_by_id(&device_id).ok_or(format!("UNKNOWN_DEVICE. {device_id}"))?;
    if device["health"].as_str() != Some("bitlocker") {
        return Err("NOT_LOCKED. This drive is not BitLocker-locked.".into());
    }
    s.lock().bitlocker_unlocked.insert(device_id);
    Ok(json!({ "ok": true, "unlocked": true }))
}

#[tauri::command]
pub fn start_scan(s: State<'_, SharedState>, device_id: String) -> Result<Value, String> {
    license_gate(&s)?;
    let device = {
        let st = s.lock();
        apply_unlock(
            &st,
            data::device_by_id(&device_id).ok_or(format!("UNKNOWN_DEVICE. {device_id}"))?,
        )
    };
    if device["health"].as_str() == Some("bitlocker") {
        return Err("DEVICE_LOCKED. Unlock BitLocker before scanning this drive.".into());
    }
    let generation = {
        let mut st = s.lock();
        st.start(&device);
        st.generation
    };
    Ok(json!({ "generation": generation }))
}

#[tauri::command]
pub fn start_folder_scan(s: State<'_, SharedState>, label: String) -> Result<Value, String> {
    license_gate(&s)?;
    let device = json!({ "id": format!("folder-{label}"), "name": label });
    let generation = {
        let mut st = s.lock();
        st.start(&device);
        st.generation
    };
    Ok(json!({ "generation": generation }))
}

#[tauri::command]
pub fn start_disk_image_scan(
    s: State<'_, SharedState>,
    image_path: String,
) -> Result<Value, String> {
    license_gate(&s)?;
    let device = json!({ "id": format!("image-{image_path}"), "name": image_path });
    let generation = {
        let mut st = s.lock();
        st.start(&device);
        st.generation
    };
    Ok(json!({ "generation": generation }))
}

#[tauri::command]
pub fn start_cloud_scan(s: State<'_, SharedState>, drive_id: String) -> Result<Value, String> {
    license_gate(&s)?;
    let drive = data::cloud_drive(&drive_id);
    let device = json!({ "id": drive["id"], "name": drive["label"] });
    let generation = {
        let mut st = s.lock();
        st.start(&device);
        st.generation
    };
    Ok(json!({ "generation": generation }))
}

#[tauri::command]
pub fn pause_scan(s: State<'_, SharedState>) -> Value {
    let mut st = s.lock();
    if !st.paused
        && !matches!(
            st.phase,
            state::ScanPhase::Idle | state::ScanPhase::Done | state::ScanPhase::Stopped
        )
    {
        st.paused = true;
        st.saved_phase = st.phase;
        st.phase = state::ScanPhase::Paused;
    }
    json!({ "ok": true })
}

#[tauri::command]
pub fn resume_scan(s: State<'_, SharedState>) -> Value {
    let mut st = s.lock();
    if st.paused {
        st.paused = false;
        st.phase = st.saved_phase;
        st.phase_started = std::time::Instant::now();
    }
    json!({ "ok": true })
}

#[tauri::command]
pub fn stop_scan(s: State<'_, SharedState>) -> Value {
    s.lock().phase = state::ScanPhase::Stopped;
    json!({ "ok": true })
}

#[tauri::command]
pub fn get_scan_progress(s: State<'_, SharedState>) -> Value {
    let st = s.lock();
    if matches!(st.phase, state::ScanPhase::Idle) {
        Value::Null
    } else {
        st.progress_payload()
    }
}

#[tauri::command]
pub fn get_scan_results(s: State<'_, SharedState>) -> Value {
    s.lock().results.clone().unwrap_or(Value::Null)
}

/// Preview a file's leading bytes (simulated: deterministic pattern
/// bytes synthesized from the node id/name — same payload shape as
/// the real core; images decode client-side from the base64).
#[tauri::command]
pub fn preview_file(
    s: State<'_, SharedState>,
    generation: u64,
    node_id: u64,
    max_bytes: Option<u64>,
) -> Result<Value, String> {
    let cap = max_bytes.unwrap_or(2 * 1024 * 1024).min(16 * 1024 * 1024);
    let st = s.lock();
    if st.generation != generation {
        return Err("STALE_GENERATION. The results changed; re-open the preview.".into());
    }
    let nodes = st
        .results
        .as_ref()
        .and_then(|r| r["nodes"].as_array().cloned())
        .ok_or("NO_SCAN_RESULTS. Nothing to preview.")?;
    let node = nodes
        .iter()
        .find(|n| n["id"].as_u64() == Some(node_id))
        .ok_or("UNKNOWN_NODE. That file is no longer in the results.")?;
    if node["isDir"].as_bool().unwrap_or(false) {
        return Err("IS_DIRECTORY. Directories have no preview bytes.".into());
    }
    let name = node["name"].as_str().unwrap_or("file").to_string();
    let size = node["size"].as_u64().unwrap_or(0);
    let take = usize::try_from(size.min(cap)).unwrap_or(0);
    // Deterministic synthetic content: a seeded byte pattern that
    // previews identically on every tour run.
    let mut buf = Vec::with_capacity(take);
    let seed = u32::try_from(node_id).unwrap_or(1).max(1);
    let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    for i in 0..take {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        buf.push((x as u8).wrapping_add((i % 251) as u8));
    }
    let mime = if name.to_lowercase().ends_with(".png") {
        "image/png"
    } else if name.to_lowercase().ends_with(".jpg") || name.to_lowercase().ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    };
    Ok(json!({
        "mime": mime,
        "size": size,
        "data": b64_sim(&buf),
    }))
}

/// Standard base64 (the preview payload codec — no external dep).
fn b64_sim(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 0x3F] as char);
        out.push(T[(n >> 12) as usize & 0x3F] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 0x3F] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 0x3F] as char
        } else {
            '='
        });
    }
    out
}

/* ── Sessions ────────────────────────────────────────────────────────── */

#[tauri::command]
pub fn list_sessions(s: State<'_, SharedState>) -> Value {
    s.lock().sessions_json()
}

#[tauri::command]
pub fn resume_session(s: State<'_, SharedState>, session_id: String) -> Result<Value, String> {
    license_gate(&s)?;
    let (device_name, files) = {
        let st = s.lock();
        let session = st
            .sessions
            .iter()
            .find(|x| x["id"] == json!(session_id))
            .cloned()
            .ok_or(format!("UNKNOWN_SESSION. {session_id}"))?;
        (
            session["targetName"]
                .as_str()
                .unwrap_or("USB Drive (F:)")
                .to_string(),
            session["filesFound"].as_u64().unwrap_or(0),
        )
    };
    let device = data::device_by_name(&device_name)
        .unwrap_or_else(|| json!({ "id": "usb-F", "name": device_name }));
    let generation = {
        let mut st = s.lock();
        st.start(&device);
        // Resume: jump straight into the deep pass at ~46%.
        st.phase = state::ScanPhase::Deep;
        st.saved_phase = state::ScanPhase::Deep;
        st.percent = 46.0;
        st.elapsed_base = 240.0;
        st.files_found = (files as f64 * 0.82) as u64;
        st.results = Some(tree::generate(&st.device_id, st.generation, 0.46));
        st.generation
    };
    Ok(json!({ "generation": generation, "deviceName": device_name }))
}

/* ── Recovery funnel (licensed) ──────────────────────────────────────── */

#[tauri::command]
pub fn list_recover_destinations(source_device_id: Option<String>) -> Value {
    data::recover_destinations(source_device_id.as_deref())
}

#[tauri::command]
pub fn start_recovery(
    s: State<'_, SharedState>,
    file_count: Option<u64>,
    bytes: Option<u64>,
    destination_name: Option<String>,
    enable_repair: Option<bool>,
) -> Result<Value, String> {
    license_gate(&s)?;
    let mut st = s.lock();
    st.recover = Some(state::RecoverRun {
        total_files: file_count.unwrap_or(1).max(1),
        done_files: 0,
        failed_files: 0,
        total_bytes: bytes.unwrap_or(1024).max(1024),
        done_bytes: 0,
        current_file: "Preparing…".into(),
        destination: destination_name.unwrap_or_else(|| "Windows (C:)".into()),
        enable_repair: enable_repair.unwrap_or(true),
        repaired_files: 0,
        finished: false,
    });
    Ok(json!({ "ok": true }))
}

/* ── Cloud ───────────────────────────────────────────────────────────── */

#[tauri::command]
pub fn list_cloud_drives() -> Value {
    data::cloud_drives()
}

#[tauri::command]
pub async fn cloud_sign_in() -> Result<Value, String> {
    delay(1_800).await;
    Ok(json!({ "ok": true, "account": "alex.morgan@outlook.com" }))
}

/* ── Licensing ───────────────────────────────────────────────────────── */

#[tauri::command]
pub fn license_status(s: State<'_, SharedState>) -> Value {
    s.lock().license_json()
}

#[tauri::command]
pub async fn activate_license(s: State<'_, SharedState>, key: String) -> Result<Value, String> {
    let normalized: String = key
        .to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    // Shape: DW prefix + 20 Crockford base32 chars (no I/L/O/U) — the
    // same contract as the frontend normalize (lib/licenseKey.ts) and
    // the future server's normalized-hash form.
    const CROCKFORD: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let body: String = normalized[2..].to_string();
    if normalized.len() != 22
        || !normalized.starts_with("DW")
        || !body.chars().all(|c| CROCKFORD.contains(c))
    {
        return Err("BAD_REQUEST. The license key format is invalid.".into());
    }
    delay(1_600).await;
    // Marker words are Crockford-safe 5-char leet so they ride inside
    // a shape-valid key (WR0NG / EXP1R / REV0K / TAKEN / TESTS).
    if normalized.contains("WR0NG") {
        return Err("KEY_NOT_FOUND. Invalid license key.".into());
    }
    if normalized.contains("EXP1R") {
        return Err(
            "LICENSE_EXPIRED. Your yearly license has expired. Renew to continue recovering."
                .into(),
        );
    }
    if normalized.contains("REV0K") {
        return Err("KEY_REVOKED. This license has been revoked. Contact support.".into());
    }
    if normalized.contains("TAKEN") {
        return Err("DEVICE_SLOT_TAKEN. This license is already active on another device. Contact support to move it.".into());
    }
    let lifetime = normalized.contains("TEST");
    let mut st = s.lock();
    st.license_posture = LicensePosture::Pro;
    st.license = Some(state::pro_status(lifetime, false));
    Ok(st.license_json())
}

#[tauri::command]
pub async fn deactivate_license(s: State<'_, SharedState>) -> Result<Value, String> {
    delay(700).await;
    let mut st = s.lock();
    st.license_posture = LicensePosture::Unlicensed;
    let purchase = st
        .license
        .as_ref()
        .and_then(|l| l["purchaseUrl"].as_str().map(str::to_string))
        .unwrap_or_else(|| "https://diskwizard.app/pricing".into());
    st.license = Some(json!({
        "posture": "unlicensed", "isPro": false, "tier": "",
        "customerName": "", "customerEmail": "",
        "licenseExpiresAt": 0, "graceDaysLeft": 0,
        "purchaseUrl": purchase, "simulated": false,
    }));
    Ok(st.license_json())
}

#[tauri::command]
pub async fn validate_now(s: State<'_, SharedState>) -> Result<Value, String> {
    delay(1_100).await;
    Ok(s.lock().license_json())
}

/// CI tour hook (the ci-license-sim analog): flips pro ↔ unlicensed.
/// Registered unconditionally in the mock (the mock itself never ships
/// in a production core build; the command surface documents itself).
#[tauri::command]
pub fn license_sim_set(s: State<'_, SharedState>, pro: Option<bool>) -> Value {
    let on = pro.unwrap_or(false);
    let mut st = s.lock();
    if on {
        st.license_posture = LicensePosture::Pro;
        st.license = Some(state::pro_status(true, true));
    } else {
        st.license_posture = LicensePosture::Unlicensed;
        st.license = None;
    }
    st.license_json()
}

/* ── Settings ────────────────────────────────────────────────────────── */

#[tauri::command]
pub fn get_settings(s: State<'_, SharedState>) -> Value {
    s.lock().settings.clone()
}

/// The frontend sends a FLAT partial patch (`set_settings({ resultsMode:
/// "complete" })`) — the command accepts the known keys (camelCase JS
/// args map onto the snake_case parameters) and merges them over the
/// stored settings, returning the full object (the store adopts it).
#[allow(clippy::fn_params_excessive_bools)]
#[tauri::command]
pub fn set_settings(
    s: State<'_, SharedState>,
    results_mode: Option<String>,
    confirm_stop: Option<bool>,
    enable_repair_default: Option<bool>,
    scan_on_sd_insert: Option<bool>,
    launch_at_login: Option<bool>,
) -> Value {
    let mut st = s.lock();
    let target = st.settings.as_object_mut().expect("settings object");
    if let Some(v) = results_mode {
        target.insert("resultsMode".into(), json!(v));
    }
    if let Some(v) = confirm_stop {
        target.insert("confirmStop".into(), json!(v));
    }
    if let Some(v) = enable_repair_default {
        target.insert("enableRepairDefault".into(), json!(v));
    }
    if let Some(v) = scan_on_sd_insert {
        target.insert("scanOnSdInsert".into(), json!(v));
    }
    if let Some(v) = launch_at_login {
        target.insert("launchAtLogin".into(), json!(v));
    }
    st.settings.clone()
}

/* ── Dev hooks (tour / CI) ───────────────────────────────────────────── */

#[tauri::command]
pub fn get_dev_hooks() -> Value {
    let tour = std::env::var("DISKWIZARD_TOUR")
        .map(|v| v == "1")
        .unwrap_or(false);
    json!({
        "tour": tour,
        "tourMode": std::env::var("DISKWIZARD_TOUR_MODE").ok(),
        "tourLicenseKey": std::env::var("DISKWIZARD_TOUR_LICENSE_KEY").ok(),
        "scan": std::env::var("DISKWIZARD_SCAN").ok(),
    })
}

/* ── The background ticker ───────────────────────────────────────────── */

/// Advances the phase machine ~5×/s and emits `scan-progress` /
/// `scan-done` / `recover-progress` / `recover-done`. Needs a clone of
/// the managed state Arc.
pub fn spawn_scan_ticker(app: AppHandle) {
    let state: SharedState = app
        .try_state::<SharedState>()
        .map(|s| s.inner().clone())
        .expect("mock state managed before ticker spawn");
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(220));
            let event = {
                let mut st = state.lock();
                st.tick()
            };
            if let Some((name, payload)) = event {
                let _ = app.emit(name, payload);
            }
        }
    });
}
