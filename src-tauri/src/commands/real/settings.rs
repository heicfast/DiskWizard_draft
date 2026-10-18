//! Settings commands — persisted JSON at AppData.

use serde_json::Value;
use tauri::State;

use super::AppState;

fn settings_path() -> std::path::PathBuf {
    dw_license::store::app_data_dir().join("settings.json")
}

fn load_settings() -> Value {
    std::fs::read(settings_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| default_settings())
}

fn default_settings() -> Value {
    serde_json::json!({
        "resultsMode": "essential",
        "confirmStop": true,
        "enableRepairDefault": true,
        "scanOnSdInsert": false,
        "launchAtLogin": false,
    })
}

#[tauri::command]
pub fn get_settings(s: State<'_, AppState>) -> Value {
    // Adopt disk state once, then serve the managed copy.
    let mut managed = s.settings.lock();
    if *managed == default_settings() {
        *managed = load_settings();
    }
    managed.clone()
}

#[allow(clippy::fn_params_excessive_bools)]
#[tauri::command]
pub fn set_settings(
    s: State<'_, AppState>,
    results_mode: Option<String>,
    confirm_stop: Option<bool>,
    enable_repair_default: Option<bool>,
    scan_on_sd_insert: Option<bool>,
    launch_at_login: Option<bool>,
) -> Value {
    let mut st = s.settings.lock();
    if let Some(v) = results_mode {
        st["resultsMode"] = serde_json::json!(v);
    }
    if let Some(v) = confirm_stop {
        st["confirmStop"] = serde_json::json!(v);
    }
    if let Some(v) = enable_repair_default {
        st["enableRepairDefault"] = serde_json::json!(v);
    }
    if let Some(v) = scan_on_sd_insert {
        st["scanOnSdInsert"] = serde_json::json!(v);
    }
    if let Some(v) = launch_at_login {
        st["launchAtLogin"] = serde_json::json!(v);
    }
    let _ = std::fs::write(settings_path(), st.to_string());
    st.clone()
}
