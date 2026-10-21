//! DiskWizard `Tauri` app entry.
//!
//! The REAL core build (default): every IPC command routes to the
//! production engine (dw-core scan/recover/session + dw-win devices/
//! SMART/BitLocker + the DiskGenie-architecture dw-license client).
//! `sim` builds swap the identical surface to the development mock
//! (tour/CI-screenshot determinism) — the release pipelines never
//! enable it (docs/CORE-PLAN.md §6).
//!
//! Window: opens MAXIMIZED at a 1440×880 logical default with an
//! 1180×760 floor; frameless with the Overlay titlebar style. The
//! process launches elevated — build.rs embeds requireAdministrator
//! (raw-disk access).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// The command layer is the crate's IPC surface; `pub` so the live
// license E2E test (tests/live_license_facts.rs) shares the exact
// production facts-collection path (the donor's pattern).
pub mod commands;

use tauri::Manager;

/// The logical default (the config creates the window maximized with
/// these as the restore size).
const DEFAULT_WINDOW_W: f64 = 1440.0;
const DEFAULT_WINDOW_H: f64 = 880.0;

/// Design floor (matches minWidth/minHeight in tauri.conf.json).
const MIN_WINDOW_W: f64 = 1180.0;
const MIN_WINDOW_H: f64 = 760.0;

/// Parse the `DISKWIZARD_WINDOW` dev hook (`1280x760`, any-case `x`).
fn parse_window_spec(spec: &str) -> Option<(f64, f64)> {
    let lowered = spec.trim().to_lowercase();
    let (w, h) = lowered.split_once('x')?;
    let (w, h) = (w.trim().parse::<f64>().ok()?, h.trim().parse::<f64>().ok()?);
    (w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0).then_some((w, h))
}

/// Guarantee the window never opens below the design floor (screens
/// smaller than the floor get an over-screen window, not a squeezed
/// layout), then center and show it. Runs in `setup` while the window
/// is still hidden so nothing flashes. The owner's default: MAXIMIZED.
fn fit_and_show(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let mut size = (DEFAULT_WINDOW_W, DEFAULT_WINDOW_H);
    if let Ok(spec) = std::env::var("DISKWIZARD_WINDOW") {
        if let Some(parsed) = parse_window_spec(&spec) {
            size = parsed;
        }
    }
    let w = size.0.max(MIN_WINDOW_W);
    let h = size.1.max(MIN_WINDOW_H);
    let _ = window.set_size(tauri::LogicalSize::new(w, h));
    let _ = window.center();
    // Owner requirement: open in full screen (maximum) by default —
    // unless the CI tour pins an exact window for responsive steps.
    if std::env::var("DISKWIZARD_WINDOW").is_err() {
        let _ = window.maximize();
    }
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage({
            #[cfg(not(feature = "sim"))]
            {
                commands::real_state()
            }
            #[cfg(feature = "sim")]
            {
                commands::shared_state()
            }
        })
        .invoke_handler(tauri::generate_handler![
            // Devices & health
            commands::list_devices,
            commands::refresh_devices,
            commands::connect_demo_device,
            commands::smart_report,
            commands::sdr_diagnose,
            commands::sdr_repair,
            commands::unlock_bitlocker,
            // File repair (phase-2)
            commands::repair_analyze_file,
            commands::repair_files,
            // Scan lifecycle
            commands::start_scan,
            commands::start_folder_scan,
            commands::start_disk_image_scan,
            commands::pause_scan,
            commands::resume_scan,
            commands::stop_scan,
            commands::get_scan_progress,
            commands::get_scan_results,
            // Sessions
            commands::list_sessions,
            commands::resume_session,
            // Recovery funnel
            commands::list_recover_destinations,
            commands::start_recovery,
            commands::preview_file,
            // Licensing
            commands::license_status,
            commands::activate_license,
            commands::deactivate_license,
            commands::validate_now,
            commands::license_sim_set,
            // Settings
            commands::get_settings,
            commands::set_settings,
            // Dev hooks
            commands::get_dev_hooks,
        ])
        .setup(|app| {
            commands::bootstrap(app.handle());
            fit_and_show(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running DiskWizard");
}
