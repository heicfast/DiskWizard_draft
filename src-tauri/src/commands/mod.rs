//! The command layer — feature-routed between the REAL core
//! (default; `docs/CORE-PLAN.md` §6) and the development simulation
//! (`sim` feature: the session-1/2 mock, identical behavior, NEVER in
//! release builds — the ui-screenshots workflow opts in explicitly).

#[cfg(feature = "sim")]
pub mod mock;

#[cfg(feature = "sim")]
pub use mock::*;

#[cfg(not(feature = "sim"))]
mod real;

#[cfg(not(feature = "sim"))]
pub use real::{devices::*, license::*, recover::*, scan::*, session::*, settings::*};

/// The managed-state constructor (mirrors the mock's `shared_state`).
#[cfg(not(feature = "sim"))]
pub fn real_state() -> real::AppState {
    real::AppState::new()
}

/// Bootstrap: start whatever background machinery the active build
/// needs (the scan ticker for sim; nothing for the real core — its
/// threads spawn per scan).
pub fn bootstrap(app: &tauri::AppHandle) {
    #[cfg(feature = "sim")]
    mock::spawn_scan_ticker(app.clone());
    #[cfg(not(feature = "sim"))]
    {
        // Real-core licensing: launch the 24 h revalidation scheduler.
        real::license::spawn_scheduler(app.clone());
    }
}
