//! Entitlement token — decode + verify (the anti-spoofing core).
//!
//! Token wire format: `b64url(payload_json) || "." || b64url(ed25519_sig)`.
//! Payload claims (all fields the client independently verifies):
//! `iss/ver/jti/iat/exp/key/tier/name/email/hw/plat/lexp/kid/aud`.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;

use crate::config::{TOKEN_AUDIENCE, TOKEN_ISSUER};
use crate::{CLOCK_SKEW_S, b64url_decode, from_hex};

/// The decoded token claims.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenClaims {
    /// Fixed issuer ([`crate::config::TOKEN_ISSUER`]).
    pub iss: String,
    /// Format version (1).
    pub ver: u32,
    /// Unique token id (hex) — signed wire field; no client branch.
    #[allow(dead_code)]
    pub jti: String,
    /// Issued-at (unix seconds).
    pub iat: i64,
    /// Token expiry (unix seconds) — the offline grace window.
    pub exp: i64,
    /// sha256(normalized key) hex — binds the token to OUR stored key.
    pub key: String,
    /// `yearly` | `lifetime`.
    pub tier: String,
    /// Customer display name.
    #[serde(default)]
    pub name: String,
    /// Customer email.
    #[serde(default)]
    pub email: String,
    /// The hardware fingerprint the server bound this token to.
    pub hw: String,
    /// `windows`.
    pub plat: String,
    /// License expiry (yearly) — absent for lifetime.
    #[serde(default)]
    pub lexp: Option<i64>,
    /// Key id (present tokens must be kid 1 = the embedded key).
    #[serde(default)]
    pub kid: Option<u32>,
    /// Audience ([`crate::config::TOKEN_AUDIENCE`] when present).
    #[serde(default)]
    pub aud: Option<String>,
}

/// Typed license errors with the user-facing copy (the server's
/// error-code contract mapped client-side).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LicenseError {
    /// 404 — key not in our records.
    InvalidKey,
    /// 403 — revoked / refunded / pending.
    Inactive,
    /// 403 — yearly license past its expiry.
    Expired,
    /// 409 — the platform slot is held by another device.
    DeviceSlotTaken,
    /// 403 — this device is not the registered one.
    DeviceMismatch,
    /// 429 — rate limit (D1 fixed window).
    RateLimited,
    /// Transport failure → offline grace path.
    Network,
    /// 5xx / parse failure.
    ServerError,
    /// A server response FAILED local signature/binding verification —
    /// the anti-spoofing tripwire (hard failure, never ok).
    Spoofed,
}

impl std::fmt::Display for LicenseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::InvalidKey => "Invalid license key. Please check it and try again.",
            Self::Inactive => "This license key is no longer active. Contact support.",
            Self::Expired => "Your yearly license has expired. Renew to keep DiskWizard Pro.",
            Self::DeviceSlotTaken => {
                "This key is already activated on another device. Contact support to move your license."
            }
            Self::DeviceMismatch => "This device isn't registered with this license key.",
            Self::RateLimited => "Too many attempts. Wait a minute and try again.",
            Self::Network => "Network unavailable. DiskWizard keeps working offline.",
            Self::ServerError => "The license server had a problem. Try again in a moment.",
            Self::Spoofed => {
                "The license server's response could not be verified. Reconnect and try again."
            }
        };
        f.write_str(s)
    }
}

impl LicenseError {
    /// Map a (status, error-code) pair per the server contract.
    #[must_use]
    pub fn from_response(status: u16, code: &str) -> Self {
        match (status, code) {
            (404, _) | (_, "KEY_NOT_FOUND") => Self::InvalidKey,
            (403, "KEY_REVOKED" | "KEY_REFUNDED" | "KEY_PENDING") => Self::Inactive,
            (403, "LICENSE_EXPIRED") => Self::Expired,
            (403, "DEVICE_MISMATCH") => Self::DeviceMismatch,
            (429, _) | (_, "RATE_LIMITED") => Self::RateLimited,
            (409, _) | (_, "DEVICE_SLOT_TAKEN") => Self::DeviceSlotTaken,
            _ => Self::ServerError,
        }
    }

    /// Hard failures deactivate local state; soft ones keep grace.
    #[must_use]
    pub fn is_hard(&self) -> bool {
        !matches!(self, Self::Network | Self::ServerError | Self::RateLimited)
    }
}

/// Verify a compact token end-to-end: Ed25519 signature against
/// `public_key_hex`, issuer/version/platform, key-hash binding,
/// hardware binding, token window, license expiry. ANY failure →
/// [`LicenseError::Spoofed`] (never a partial accept).
///
/// # Errors
/// [`LicenseError::Spoofed`] when any check fails.
pub fn verify_token(
    token: &str,
    public_key_hex: &str,
    expected_key_hash: &str,
    expected_hw: &str,
    platform: &str,
    now: i64,
) -> Result<TokenClaims, LicenseError> {
    let (payload_b64, sig_b64) = token.split_once('.').ok_or(LicenseError::Spoofed)?;
    let payload = b64url_decode(payload_b64).ok_or(LicenseError::Spoofed)?;
    let sig_bytes = b64url_decode(sig_b64).ok_or(LicenseError::Spoofed)?;
    let arr: [u8; 64] = sig_bytes.try_into().map_err(|_| LicenseError::Spoofed)?;
    let sig = Signature::from_bytes(&arr);

    let key_bytes = from_hex(public_key_hex).ok_or(LicenseError::Spoofed)?;
    let key_arr: [u8; 32] = key_bytes.try_into().map_err(|_| LicenseError::Spoofed)?;
    let verifying = VerifyingKey::from_bytes(&key_arr).map_err(|_| LicenseError::Spoofed)?;

    verifying
        .verify(&payload, &sig)
        .map_err(|_| LicenseError::Spoofed)?;

    let claims: TokenClaims =
        serde_json::from_slice(&payload).map_err(|_| LicenseError::Spoofed)?;
    if claims.iss != TOKEN_ISSUER
        || claims.ver != 1
        || claims.plat != platform
        || claims.key != expected_key_hash
        || claims.hw != expected_hw
        || claims.exp <= now
        || claims.iat > now + CLOCK_SKEW_S
        || matches!(claims.lexp, Some(lexp) if lexp <= now)
        || matches!(claims.kid, Some(k) if k != 1)
        || matches!(claims.aud.as_deref(), Some(a) if a != TOKEN_AUDIENCE)
    {
        return Err(LicenseError::Spoofed);
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{b64url_encode, sha256_hex};
    use ed25519_dalek::{Signer, SigningKey};

    fn fixture_keypair() -> SigningKey {
        // Deterministic test key (seed 0..31).
        let mut seed = [0u8; 32];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = u8::try_from(i).expect("32-byte seed");
        }
        SigningKey::from_bytes(&seed)
    }

    fn mint_token(
        signing: &SigningKey,
        key_hash: &str,
        hw: &str,
        exp: i64,
        iat: i64,
        lexp: Option<i64>,
    ) -> String {
        let payload = serde_json::json!({
            "iss": TOKEN_ISSUER,
            "ver": 1,
            "jti": "0123456789abcdef",
            "iat": iat,
            "exp": exp,
            "key": key_hash,
            "tier": "lifetime",
            "name": "Alex Morgan",
            "email": "alex@example.com",
            "hw": hw,
            "plat": "windows",
            "lexp": lexp,
        });
        let payload_bytes = payload.to_string().into_bytes();
        let sig = signing.sign(&payload_bytes);
        format!(
            "{}.{}",
            b64url_encode(&payload_bytes),
            b64url_encode(&sig.to_bytes())
        )
    }

    #[test]
    fn valid_token_verifies_with_all_bindings() {
        let kp = fixture_keypair();
        let key_hash = sha256_hex("DWTESTSDEM00PR000C0DE0");
        let hw = "a".repeat(64);
        let token = mint_token(&kp, &key_hash, &hw, 1_800_000_000, 1_700_000_000, None);
        let claims = verify_token(
            &token,
            &crate::hex(kp.verifying_key().as_bytes()),
            &key_hash,
            &hw,
            "windows",
            1_750_000_000,
        )
        .expect("verifies");
        assert_eq!(claims.tier, "lifetime");
        assert_eq!(claims.name, "Alex Morgan");
    }

    #[test]
    fn wrong_hardware_is_spoofed() {
        let kp = fixture_keypair();
        let key_hash = sha256_hex("DWTESTSDEM00PR000C0DE0");
        let token = mint_token(
            &kp,
            &key_hash,
            &"a".repeat(64),
            1_800_000_000,
            1_700_000_000,
            None,
        );
        assert_eq!(
            verify_token(
                &token,
                &crate::hex(kp.verifying_key().as_bytes()),
                &key_hash,
                &"b".repeat(64),
                "windows",
                1_750_000_000
            )
            .unwrap_err(),
            LicenseError::Spoofed
        );
    }

    #[test]
    fn expired_token_window_rejects() {
        let kp = fixture_keypair();
        let key_hash = sha256_hex("DWTESTSDEM00PR000C0DE0");
        let hw = "a".repeat(64);
        let token = mint_token(&kp, &key_hash, &hw, 1_700_000_100, 1_700_000_000, None);
        assert_eq!(
            verify_token(
                &token,
                &crate::hex(kp.verifying_key().as_bytes()),
                &key_hash,
                &hw,
                "windows",
                1_750_000_000
            )
            .unwrap_err(),
            LicenseError::Spoofed
        );
    }

    #[test]
    fn tampered_payload_rejects() {
        let kp = fixture_keypair();
        let key_hash = sha256_hex("DWTESTSDEM00PR000C0DE0");
        let hw = "a".repeat(64);
        let token = mint_token(&kp, &key_hash, &hw, 1_800_000_000, 1_700_000_000, None);
        let (p, s) = token.split_once('.').unwrap();
        let mut payload = b64url_decode(p).unwrap();
        payload[3] ^= 0x01; // flip a byte
        let tampered = format!("{}.{}", b64url_encode(&payload), s);
        assert_eq!(
            verify_token(
                &tampered,
                &crate::hex(kp.verifying_key().as_bytes()),
                &key_hash,
                &hw,
                "windows",
                1_750_000_000
            )
            .unwrap_err(),
            LicenseError::Spoofed
        );
    }

    #[test]
    fn yearly_license_expiry_enforced() {
        let kp = fixture_keypair();
        let key_hash = sha256_hex("DWTESTSDEM00PR000C0DE0");
        let hw = "a".repeat(64);
        // lexp in the past while the token window is still open.
        let token = mint_token(
            &kp,
            &key_hash,
            &hw,
            1_800_000_000,
            1_700_000_000,
            Some(1_740_000_000),
        );
        assert_eq!(
            verify_token(
                &token,
                &crate::hex(kp.verifying_key().as_bytes()),
                &key_hash,
                &hw,
                "windows",
                1_750_000_000
            )
            .unwrap_err(),
            LicenseError::Spoofed
        );
    }

    #[test]
    fn error_mapping_contract() {
        assert_eq!(
            LicenseError::from_response(404, "KEY_NOT_FOUND"),
            LicenseError::InvalidKey
        );
        assert_eq!(
            LicenseError::from_response(409, "DEVICE_SLOT_TAKEN"),
            LicenseError::DeviceSlotTaken
        );
        assert_eq!(
            LicenseError::from_response(429, ""),
            LicenseError::RateLimited
        );
        assert_eq!(
            LicenseError::from_response(500, ""),
            LicenseError::ServerError
        );
        assert!(!LicenseError::Network.is_hard());
        assert!(LicenseError::InvalidKey.is_hard());
    }
}
