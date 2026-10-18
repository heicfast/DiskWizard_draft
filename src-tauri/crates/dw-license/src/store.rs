//! License-state persistence — the store seam.
//!
//! At rest the state is DPAPI-wrapped (`CryptProtectData`) by the app
//! layer's Windows provider; this seam keeps the read/write/locate
//! contract platform-independent. `license.bin` under the app-data
//! dir; a corrupt/missing file degrades to default (unlicensed).

use std::path::PathBuf;

use crate::posture::LicenseState;

/// Where the (encrypted) license blob lives.
#[must_use]
pub fn license_path() -> PathBuf {
    app_data_dir().join("license.bin")
}

/// The app data directory (`%APPDATA%\DiskWizard` on Windows; the
/// XDG fallback keeps Linux CI of the app crate honest).
#[must_use]
pub fn app_data_dir() -> PathBuf {
    if let Ok(base) = std::env::var("APPDATA") {
        return PathBuf::from(base).join("DiskWizard");
    }
    if let Ok(base) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(base).join("DiskWizard");
    }
    PathBuf::from(".diskwizard")
}

/// The encryption seam the app layer injects (DPAPI on Windows; the
/// plain pass-through serves tests and non-Windows builds of the
/// app crate where the file is dev-only).
pub trait StateStore: Send + Sync {
    /// Serialize + protect + write the state.
    /// # Errors
    /// String on any I/O or crypto failure.
    fn save(&self, state: &LicenseState) -> Result<(), String>;

    /// Read + unprotect + parse the state (default when absent).
    /// # Errors
    /// String when present-but-unreadable (corruption surfaces —
    /// silently defaulting would hide tamper evidence).
    fn load(&self) -> Result<LicenseState, String>;

    /// Remove the stored state (deactivation path).
    /// # Errors
    /// String on failure (the caller proceeds to default anyway).
    fn clear(&self) -> Result<(), String>;
}

/// Plain-JSON store (tests; the dev fallback). Never used in
/// production Windows builds — the app injects the DPAPI provider.
pub struct PlainJsonStore {
    path: PathBuf,
}

impl PlainJsonStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl StateStore for PlainJsonStore {
    fn save(&self, state: &LicenseState) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("mkdir: {e}"))?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| format!("serialize: {e}"))?;
        std::fs::write(&self.path, json).map_err(|e| format!("write: {e}"))
    }

    fn load(&self) -> Result<LicenseState, String> {
        match std::fs::read_to_string(&self.path) {
            Ok(json) => {
                serde_json::from_str(&json).map_err(|e| format!("corrupt license state: {e}"))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(LicenseState::default()),
            Err(e) => Err(format!("read: {e}")),
        }
    }

    fn clear(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("remove: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_store_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = PlainJsonStore::new(dir.path().join("license.json"));
        let st = LicenseState {
            license_key: "DWTESTSDEM00PR000C0DE0".into(),
            tier: "lifetime".into(),
            token_exp: 2_000_000_000,
            ..LicenseState::default()
        };
        store.save(&st).unwrap();
        let back = store.load().unwrap();
        assert_eq!(back, st);
    }

    #[test]
    fn missing_file_is_default_not_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = PlainJsonStore::new(dir.path().join("absent.json"));
        assert_eq!(store.load().unwrap(), LicenseState::default());
    }

    #[test]
    fn corrupt_file_surfaces() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("license.json");
        std::fs::write(&path, "{ not json").unwrap();
        let store = PlainJsonStore::new(path);
        assert!(store.load().is_err());
    }

    #[test]
    fn clear_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let store = PlainJsonStore::new(dir.path().join("license.json"));
        store.clear().unwrap();
        store.clear().unwrap();
    }
}
