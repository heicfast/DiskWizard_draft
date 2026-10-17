//! The IPC command surface (mock-core session).
//!
//! Registers every command the frontend knows via the standard
//! `generate_handler!` table; the implementations live in [`mock`]
//! against realistic synthetic data that mirrors the reverse-engineered
//! architecture. When the production engine lands, this module
//! re-points the same commands at the real core — the frontend
//! contract (`src/lib/ipc.ts` + `src/types.ts`) is frozen.
pub mod mock;

use tauri::AppHandle;

/// Manage the mock-core state on the app + spawn the background
/// ticker (scan-progress / scan-done / recover-* events).
pub fn bootstrap(app: &AppHandle) {
    app.manage(mock::shared_state());
    mock::spawn_scan_ticker(app.clone());
}
