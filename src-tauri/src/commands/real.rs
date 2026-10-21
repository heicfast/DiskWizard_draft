//! The REAL command implementations (default builds) — the frozen
//! IPC surface backed by dw-core / dw-win / dw-license.
//!
//! Contract: `docs/ARCHITECTURE.md` §2 — every command name, arg and
//! event shape the mock established is preserved exactly; the engine
//! behind them is real.

pub mod devices;
pub mod license;
pub mod recover;
pub mod repair;
pub mod scan;
pub mod session;
pub mod settings;

use parking_lot::Mutex;
use serde_json::Value;
use std::sync::Arc;

/// The app-managed shared state (scan session + license manager).
pub struct AppState {
    pub scan: scan::ScanSession,
    pub license: Arc<license::LicenseManager>,
    pub settings: Mutex<Value>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            scan: scan::ScanSession::default(),
            license: Arc::new(license::LicenseManager::new()),
            settings: Mutex::new(serde_json::json!({
                "resultsMode": "essential",
                "confirmStop": true,
                "enableRepairDefault": true,
                "scanOnSdInsert": false,
                "launchAtLogin": false,
            })),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// The managed-state constructor.
pub fn shared_state() -> Arc<AppState> {
    Arc::new(AppState::new())
}
