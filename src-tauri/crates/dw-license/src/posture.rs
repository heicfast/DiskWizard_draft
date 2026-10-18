//! Persisted state + the posture state machine (doc §6, STRICT
//! semantics: unlicensed/degraded are LOCKED — disk health only).

use serde::{Deserialize, Serialize};

use crate::VALIDATION_INTERVAL_S;

/// Persisted license state (DPAPI-wrapped at rest by the app layer).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LicenseState {
    /// Normalized key (22 chars, uppercase).
    #[serde(default)]
    pub license_key: String,
    /// 64-hex hardware fingerprint captured at activation.
    #[serde(default)]
    pub hardware_id: String,
    /// `windows`.
    #[serde(default)]
    pub platform: String,
    /// `yearly` | `lifetime` ("" before activation).
    #[serde(default)]
    pub tier: String,
    /// Customer display name (verified claims).
    #[serde(default)]
    pub customer_name: String,
    /// Customer email (verified claims).
    #[serde(default)]
    pub customer_email: String,
    /// License expiry (unix; 0 = lifetime / unknown).
    #[serde(default)]
    pub license_expires_at: i64,
    /// The latest verified token (its `exp` drives the grace window).
    #[serde(default)]
    pub token: String,
    /// Token expiry — the offline grace window's hard stop.
    #[serde(default)]
    pub token_exp: i64,
    /// When this device activated (display).
    #[serde(default)]
    pub activated_at: i64,
    /// Last successful server validation.
    #[serde(default)]
    pub last_validated_at: i64,
    /// Monotonic "last known good time" (clock-rollback guard).
    #[serde(default)]
    pub last_known_good: i64,
    /// CI simulation marker (`sim` feature only).
    #[serde(default)]
    pub simulated: bool,
}

/// The runtime posture derived from state + now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicensePosture {
    /// No license on file → locked (disk health only).
    Unlicensed,
    /// Verified, fresh (checked within the interval).
    Pro,
    /// Verified, offline, token still inside its grace window.
    Grace { days_left: i64 },
    /// Token window exhausted (or license expired) → locked.
    Degraded,
}

impl LicensePosture {
    /// The wire string (`LicenseStatus.posture` in the UI contract).
    #[must_use]
    pub fn wire(self) -> &'static str {
        match self {
            Self::Unlicensed => "unlicensed",
            Self::Pro => "pro",
            Self::Grace { .. } => "grace",
            Self::Degraded => "degraded",
        }
    }

    /// Full-feature access?
    #[must_use]
    pub fn is_pro(self) -> bool {
        matches!(self, Self::Pro | Self::Grace { .. })
    }
}

/// Derive the posture (PURE — injected clock for tests).
#[must_use]
pub fn posture(state: &LicenseState, now: i64) -> LicensePosture {
    if state.license_key.is_empty() {
        return LicensePosture::Unlicensed;
    }
    if state.token_exp <= now {
        return LicensePosture::Degraded;
    }
    // A yearly license past its own expiry is degraded even while the
    // token is technically valid (honest UX; the server also refuses).
    if state.license_expires_at > 0 && state.license_expires_at <= now {
        return LicensePosture::Degraded;
    }
    let reference = state.last_validated_at.max(state.last_known_good);
    let age = now - reference;
    if age < VALIDATION_INTERVAL_S {
        return LicensePosture::Pro;
    }
    let seconds_left = state.token_exp - now;
    LicensePosture::Grace {
        days_left: (seconds_left + 86_399) / 86_400,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pro_state() -> LicenseState {
        LicenseState {
            license_key: "DWTESTSDEM00PR000C0DE0".into(),
            hardware_id: "a".repeat(64),
            platform: "windows".into(),
            tier: "lifetime".into(),
            customer_name: "Alex Morgan".into(),
            customer_email: "alex@example.com".into(),
            license_expires_at: 0,
            token: "tok".into(),
            token_exp: 2_000_000_000,
            activated_at: 1_000_000_000,
            last_validated_at: 1_000_000_000,
            last_known_good: 1_000_000_000,
            simulated: false,
        }
    }

    const NOW: i64 = 1_000_050_000; // ~14 h after activation

    #[test]
    fn fresh_activation_is_pro() {
        assert_eq!(posture(&pro_state(), NOW), LicensePosture::Pro);
        assert!(posture(&pro_state(), NOW).is_pro());
    }

    #[test]
    fn offline_past_interval_is_grace_with_days() {
        // 3 days past the 24h interval, token valid 14 more days.
        let st = pro_state();
        let now = NOW + 3 * 86_400;
        assert_eq!(
            posture(&st, now),
            LicensePosture::Grace {
                days_left: (2_000_000_000 - now + 86_399) / 86_400
            }
        );
        assert!(posture(&st, now).is_pro());
    }

    #[test]
    fn token_window_exhausted_degrades() {
        let st = pro_state();
        assert_eq!(posture(&st, 2_000_000_001), LicensePosture::Degraded);
        assert!(!posture(&st, 2_000_000_001).is_pro());
    }

    #[test]
    fn yearly_expiry_degrades_even_with_valid_token() {
        let mut st = pro_state();
        st.license_expires_at = NOW - 1;
        assert_eq!(posture(&st, NOW), LicensePosture::Degraded);
    }

    #[test]
    fn no_key_is_unlicensed() {
        assert_eq!(
            posture(&LicenseState::default(), NOW),
            LicensePosture::Unlicensed
        );
    }

    #[test]
    fn wire_strings() {
        assert_eq!(LicensePosture::Pro.wire(), "pro");
        assert_eq!(LicensePosture::Grace { days_left: 3 }.wire(), "grace");
    }

    #[test]
    fn state_round_trips_through_json() {
        let st = pro_state();
        let json = serde_json::to_string(&st).unwrap();
        let back: LicenseState = serde_json::from_str(&json).unwrap();
        assert_eq!(st, back);
        assert_eq!(back.customer_name, "Alex Morgan");
    }
}
