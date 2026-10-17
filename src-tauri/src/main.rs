//! DiskWizard binary entry: starts the Tauri app via
//! [`diskwizard_lib::run`].

// Prevents an additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    diskwizard_lib::run();
}
