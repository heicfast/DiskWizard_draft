//! Session commands — the .rsf list/resume.

use serde_json::{Value, json};
use tauri::State;

use super::AppState;

#[tauri::command]
pub fn list_sessions() -> Value {
    let dir = dw_core::session::sessions_dir();
    let list = dw_core::session::list_sessions(&dir).unwrap_or_default();
    Value::Array(
        list.iter()
            .map(|s| {
                json!({
                    "id": s.file,
                    "targetName": s.target_name,
                    "status": "completed",
                    "filesFound": s.files_found,
                    "bytesFound": s.bytes_found,
                    "startedAt": s.saved_at,
                    "sessionFile": s.file,
                })
            })
            .collect(),
    )
}

#[tauri::command]
pub fn resume_session(s: State<'_, AppState>, session_id: String) -> Result<Value, String> {
    super::license::require_licensed(&s)?;
    let path = std::path::Path::new(&session_id);
    let loaded = dw_core::session::load(path).map_err(|e| format!("SESSION_LOAD_FAILED. {e}"))?;
    // Adopt the loaded tree as the current results (generation bump).
    let generation = {
        let st = &s.scan;
        let next = st.generation() + 1;
        st.set_generation(next);
        st.set_target(loaded.device.name.as_str(), loaded.device.name.as_str());
        st.adopt_loaded(std::sync::Arc::new(loaded.nodes.clone()));
        *st.report_slot().lock() = Some(dw_core::scan::ScanReport {
            files_found: loaded.header.files_found,
            bytes_found: loaded.header.bytes_found,
            duration_sec: loaded.header.duration_sec,
            stopped: loaded.header.stopped,
            fs: dw_core::types::FsKind::Unknown,
        });
        *st.phase_slot().lock() = dw_core::scan::Phase::Done;
        next
    };
    Ok(json!({ "generation": generation, "deviceName": loaded.device.name }))
}

#[tauri::command]
pub fn get_dev_hooks() -> Value {
    let tour = std::env::var("DISKWIZARD_TOUR")
        .map(|v| v == "1")
        .unwrap_or(false);
    json!({
        // Which command surface answered: the TourDriver branches on this
        // (the real core picks a live device row; the sim core uses its
        // fixed synthetic device). The mock's get_dev_hooks reports "sim".
        "core": "real",
        "tour": tour,
        "tourMode": std::env::var("DISKWIZARD_TOUR_MODE").ok(),
        "tourLicenseKey": std::env::var("DISKWIZARD_TOUR_LICENSE_KEY").ok(),
        "scan": std::env::var("DISKWIZARD_SCAN").ok(),
        // QA-1 (real-core tour): substring filter that selects the device
        // row to scan (e.g. the CI job's VHD label "DWTEST"). Empty → the
        // first eligible row.
        "deviceFilter": std::env::var("DISKWIZARD_TOUR_DEVICE").ok(),
    })
}
