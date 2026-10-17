//! DiskWizard `Tauri` app entry.
//!
//! This session ships the UI with a SIMULATED core (the owner's
//! "second mock"): every IPC command is implemented against realistic
//! synthetic data that mirrors the reverse-engineered architecture —
//! device inventory, the quick→deep→organize scan phase machine with
//! progressive result filling, recovery sessions, the recovery funnel,
//! cloud drives, disk health (free tier) and the licensing gates.
//! When the production Rust core lands it replaces [`commands::mock`]
//! without touching the command surface or the frontend contract.
//!
//! Window: opens MAXIMIZED (owner requirement) at a 1440×880 logical
//! default with an 1180×760 floor; frameless with the Overlay titlebar
//! style (the React TitleBar paints the chrome). The process launches
//! elevated — build.rs embeds requireAdministrator (raw-disk access).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

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
        .invoke_handler(tauri::generate_handler![
            // Devices & health
            commands::mock::list_devices,
            commands::mock::refresh_devices,
            commands::mock::connect_demo_device,
            commands::mock::smart_report,
            commands::mock::sdr_diagnose,
            // Scan lifecycle
            commands::mock::start_scan,
            commands::mock::start_folder_scan,
            commands::mock::start_disk_image_scan,
            commands::mock::start_cloud_scan,
            commands::mock::pause_scan,
            commands::mock::resume_scan,
            commands::mock::stop_scan,
            commands::mock::get_scan_progress,
            commands::mock::get_scan_results,
            // Sessions
            commands::mock::list_sessions,
            commands::mock::resume_session,
            // Recovery funnel
            commands::mock::list_recover_destinations,
            commands::mock::start_recovery,
            // Cloud
            commands::mock::list_cloud_drives,
            commands::mock::cloud_sign_in,
            // Licensing
            commands::mock::license_status,
            commands::mock::activate_license,
            commands::mock::deactivate_license,
            commands::mock::validate_now,
            commands::mock::license_sim_set,
            // Settings
            commands::mock::get_settings,
            commands::mock::set_settings,
            // Dev hooks
            commands::mock::get_dev_hooks,
        ])
        .setup(|app| {
            commands::bootstrap(app.handle());
            fit_and_show(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running DiskWizard");
}
