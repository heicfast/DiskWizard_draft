//! License commands — the DiskGenie-architecture client wiring:
//! status/activate/deactivate/validate + the 24 h scheduler + the
//! L6 command gate.

use std::sync::Arc;

use parking_lot::RwLock;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};

use dw_license::http::ReqwestLicense;
use dw_license::posture::{LicensePosture, LicenseState};
use dw_license::protocol::{DeviceFacts, LicenseApi};
use dw_license::store::StateStore;
use dw_license::token::LicenseError;
use dw_license::{
    LICENSE_PURCHASE_URL, VALIDATION_INTERVAL_S, key_shape_valid, normalize_key, sha256_hex,
};

use super::AppState;

/// The license manager (the DiskGenie `commands/license.rs` pattern).
pub struct LicenseManager {
    state: RwLock<LicenseState>,
    #[allow(dead_code)]
    store: Arc<dyn StateStore>,
}

impl LicenseManager {
    pub fn new() -> Self {
        // DPAPI-wrapped persistence on Windows; plain JSON elsewhere
        // (dev builds only — production is Windows).
        #[cfg(windows)]
        let store: Arc<dyn StateStore> = Arc::new(dw_win::license_plat::DpapiStore::new(
            dw_license::store::license_path(),
        ));
        #[cfg(not(windows))]
        let store: Arc<dyn StateStore> = Arc::new(dw_license::store::PlainJsonStore::new(
            dw_license::store::license_path(),
        ));
        let state = store.load().unwrap_or_default();
        Self {
            state: RwLock::new(state),
            store,
        }
    }

    fn now(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default()
    }

    /// The frozen `LicenseStatus` payload.
    pub fn status_json(&self) -> Value {
        let st = self.state.read();
        let p = dw_license::posture(&st, self.now());
        let (posture_str, is_pro, days) = match p {
            LicensePosture::Unlicensed => ("unlicensed", false, 0),
            LicensePosture::Pro => ("pro", true, 0),
            LicensePosture::Grace { days_left } => ("grace", true, days_left),
            LicensePosture::Degraded => ("degraded", false, 0),
        };
        json!({
            "posture": posture_str,
            "isPro": is_pro,
            "tier": st.tier,
            "customerName": st.customer_name,
            "customerEmail": st.customer_email,
            "licenseExpiresAt": st.license_expires_at,
            "graceDaysLeft": days,
            "purchaseUrl": LICENSE_PURCHASE_URL,
            "simulated": st.simulated,
        })
    }

    /// Collect this machine's device facts (best-effort strings;
    /// binding inputs required).
    fn device_facts() -> Result<DeviceFacts, String> {
        #[cfg(windows)]
        {
            use dw_license::hwid::HwIdProvider;
            let p = dw_win::license_plat::WinHwId;
            Ok(DeviceFacts {
                platform: "windows".into(),
                hardware_hash: dw_license::hwid::hardware_id_from(&p)?,
                hostname: dw_win::license_plat::hostname(),
                os_version: dw_win::license_plat::os_version(),
                app_version: env!("CARGO_PKG_VERSION").into(),
                comp_machine: dw_license::hwid::component_hash("machine:", p.machine_guid().ok()),
                comp_volume: dw_license::hwid::component_hash(
                    "volume:",
                    p.system_drive_serial().ok().map(|s| s.to_string()),
                ),
                comp_cpu: dw_license::hwid::component_hash("cpu:", p.cpuid_brand().ok()),
                cpu_brand: p.cpuid_brand().ok(),
                ram_mb: dw_win::license_plat::ram_mb(),
                machine_model: dw_win::license_plat::machine_model(),
                cpu_cores: dw_win::license_plat::cpu_cores(),
                arch: Some(std::env::consts::ARCH.into()),
            })
        }
        #[cfg(not(windows))]
        {
            Err("DEVICE_FACTS_UNAVAILABLE. Licensing requires Windows.".into())
        }
    }

    /// Apply a server response token: verify locally, persist.
    fn apply_token(&self, key: &str, token: &str) -> Result<(), LicenseError> {
        let facts = Self::device_facts().map_err(|_| LicenseError::Network)?;
        let expected_hw = facts.hardware_hash;
        let claims = dw_license::token::verify_token(
            token,
            dw_license::protocol::LICENSE_PUBLIC_KEY_HEX,
            &sha256_hex(&normalize_key(key)),
            &expected_hw,
            "windows",
            self.now(),
        )?;
        let mut st = self.state.write();
        st.license_key = normalize_key(key);
        st.hardware_id = claims.hw;
        st.platform = "windows".into();
        st.tier = claims.tier;
        st.customer_name = claims.name;
        st.customer_email = claims.email;
        st.license_expires_at = claims.lexp.unwrap_or_default();
        st.token = token.to_string();
        st.token_exp = claims.exp;
        st.last_validated_at = self.now();
        st.last_known_good = self.now();
        st.simulated = false;
        Ok(())
    }

    /// The activate round-trip.
    pub fn activate(&self, key: &str) -> Result<Value, String> {
        let normalized = normalize_key(key);
        if !key_shape_valid(&normalized) {
            return Err("BAD_REQUEST. The license key format is invalid.".into());
        }
        let facts = Self::device_facts()?;
        let api = LicenseApi::new(ReqwestLicense::new().map_err(String::from)?);
        match api.activate(&normalized, &facts) {
            Ok(dto) => {
                self.apply_token(&normalized, &dto.token)
                    .map_err(|e| user_facing(&e))?;
                self.persist();
                Ok(self.status_json())
            }
            Err(e) => Err(user_facing(&e)),
        }
    }

    /// The deactivate path (server + local clear).
    pub fn deactivate(&self) -> Result<Value, String> {
        let st = self.state.read().clone();
        if !st.license_key.is_empty() {
            if let Ok(facts) = Self::device_facts() {
                let api = LicenseApi::new(ReqwestLicense::new().map_err(String::from)?);
                let _ = api.deactivate(&st.license_key, &facts);
            }
        }
        *self.state.write() = LicenseState::default();
        self.persist();
        Ok(self.status_json())
    }

    /// The validate-now path (the 24 h check on demand).
    pub fn validate(&self) -> Result<Value, String> {
        let st = self.state.read().clone();
        if st.license_key.is_empty() {
            return Ok(self.status_json());
        }
        let Ok(facts) = Self::device_facts() else {
            return Ok(self.status_json());
        };
        let api = LicenseApi::new(ReqwestLicense::new().map_err(String::from)?);
        match api.validate(&st.license_key, &facts) {
            Ok(dto) => {
                self.apply_token(&st.license_key, &dto.token)
                    .map_err(|e| user_facing(&e))?;
                self.persist();
                Ok(self.status_json())
            }
            Err(e) if e.is_hard() => {
                // Hard failures deactivate locally (doc §6).
                *self.state.write() = LicenseState::default();
                self.persist();
                Err(user_facing(&e))
            }
            Err(_) => Ok(self.status_json()), // soft: grace continues
        }
    }

    fn persist(&self) {
        let _ = self.store.save(&self.state.read());
    }

    /// The L6 gate: privileged commands refuse when unlicensed.
    pub fn require_licensed(&self) -> Result<(), String> {
        let p = dw_license::posture(&self.state.read(), self.now());
        if p.is_pro() {
            Ok(())
        } else {
            Err("ACTIVATION_REQUIRED. Activate DiskWizard Pro to use this.".into())
        }
    }
}

impl Default for LicenseManager {
    fn default() -> Self {
        Self::new()
    }
}

fn user_facing(e: &LicenseError) -> String {
    format!("{}. {e}", marker_for(e))
}

fn marker_for(e: &LicenseError) -> &'static str {
    match e {
        LicenseError::InvalidKey => "KEY_NOT_FOUND",
        LicenseError::Inactive => "KEY_REVOKED",
        LicenseError::Expired => "LICENSE_EXPIRED",
        LicenseError::DeviceSlotTaken => "DEVICE_SLOT_TAKEN",
        LicenseError::DeviceMismatch => "DEVICE_MISMATCH",
        LicenseError::RateLimited => "RATE_LIMITED",
        LicenseError::Network => "NETWORK",
        LicenseError::ServerError => "SERVER_ERROR",
        LicenseError::Spoofed => "SPOOFED",
    }
}

/// The L6 gate for command handlers (accepts `&AppState`; `State`
/// call-sites coerce through `Deref`).
pub fn require_licensed(s: &AppState) -> Result<(), String> {
    s.license.require_licensed()
}

/// The 24 h revalidation scheduler (+ launch check).
pub fn spawn_scheduler(app: AppHandle) {
    std::thread::Builder::new()
        .name("dw-license-scheduler".to_string())
        .spawn(move || {
            // Launch validation after a short settle.
            std::thread::sleep(std::time::Duration::from_secs(20));
            let state: State<'_, AppState> = app.state();
            if let Ok(new_status) = state.license.validate() {
                let _ = app.emit("license-changed", new_status);
            }
            loop {
                std::thread::sleep(std::time::Duration::from_secs(
                    VALIDATION_INTERVAL_S.try_into().unwrap_or(86_400),
                ));
                let state: State<'_, AppState> = app.state();
                if let Ok(new_status) = state.license.validate() {
                    let _ = app.emit("license-changed", new_status);
                }
            }
        })
        .ok();
}

#[tauri::command]
pub fn license_status(s: State<'_, AppState>) -> Value {
    s.license.status_json()
}

#[tauri::command]
pub async fn activate_license(s: State<'_, AppState>, key: String) -> Result<Value, String> {
    // Long round-trip on the async runtime (never block IPC).
    let mgr = Arc::clone(&s.license);
    tauri::async_runtime::spawn_blocking(move || mgr.activate(&key))
        .await
        .map_err(|e| format!("INTERNAL. {e}"))?
}

#[tauri::command]
pub async fn deactivate_license(s: State<'_, AppState>) -> Result<Value, String> {
    let mgr = Arc::clone(&s.license);
    tauri::async_runtime::spawn_blocking(move || mgr.deactivate())
        .await
        .map_err(|e| format!("INTERNAL. {e}"))?
}

#[tauri::command]
pub async fn validate_now(s: State<'_, AppState>) -> Result<Value, String> {
    let mgr = Arc::clone(&s.license);
    tauri::async_runtime::spawn_blocking(move || mgr.validate())
        .await
        .map_err(|e| format!("INTERNAL. {e}"))?
}

/// The CI/tour simulation hook — sim builds only (mirrors the mock's
/// contract; production builds refuse).
#[tauri::command]
pub fn license_sim_set(_s: State<'_, AppState>, _pro: Option<bool>) -> Result<Value, String> {
    Err("NOT_AVAILABLE. License simulation is a dev-build feature.".into())
}
