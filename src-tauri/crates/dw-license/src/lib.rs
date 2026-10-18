//! # dw-license — the DiskWizard licensing client
//!
//! Wire-faithful port of the DiskGenie licensing architecture
//! (`diskgenie-v2/docs/LICENSING-ARCHITECTURE.md`, owner directive:
//! "exactly same and as it is") adapted to the DiskWizard identity:
//!
//! * **Ed25519-signed entitlement tokens** (`payload.sig`,
//!   b64url) — verified client-side against the embedded public key;
//!   a spoofed license server cannot forge one (the private seed
//!   lives only as a Worker secret).
//! * **HMAC-SHA256 request auth** — `X-DW-*` headers over
//!   (timestamp, nonce, method, path, body-hash) with ±300 s window
//!   and server-side single-use nonces.
//! * **Device binding** — the hardware fingerprint inside the signed
//!   payload; state at rest is DPAPI-wrapped by the app layer.
//! * **Postures** — `unlicensed / pro / grace / degraded` with the
//!   24 h revalidation + 14-day offline grace semantics.
//!
//! Layering: this crate is platform-independent and fully unit-tested
//! (fake transport + injected clock + fixture keypairs). The Windows
//! fingerprint/DPAPI providers are injected through the
//! [`hwid::HwIdProvider`] and [`store::StateStore`] seams by the app
//! crate; nothing here touches Win32.

pub mod http;
pub mod hwid;
pub mod posture;
pub mod protocol;
pub mod store;
pub mod token;

pub use http::LicenseHttp;
#[cfg(feature = "client")]
pub use http::ReqwestLicense;
pub use posture::{LicensePosture, posture};
pub use protocol::{DeviceFacts, LICENSE_API_BASE, LICENSE_PURCHASE_URL, LicenseApi};
pub use store::{StateStore, license_path};
pub use token::{TokenClaims, verify_token};

/// Revalidation interval (24 h — doc §6).
pub const VALIDATION_INTERVAL_S: i64 = 24 * 60 * 60;

/// Offline grace window baked into every issued token (14 days; keep
/// in sync with the Worker var `TOKEN_TTL_DAYS`).
pub const GRACE_DAYS: i64 = 14;

/// Allowed clock skew for token issuance times.
pub const CLOCK_SKEW_S: i64 = 300;

/// Normalize a license key: uppercase, strip non-alphanumerics.
/// The DW format: `DW` + 20 Crockford base32 chars (no I/L/O/U) = 22.
#[must_use]
pub fn normalize_key(raw: &str) -> String {
    raw.to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

/// The Crockford base32 body alphabet (excludes I/L/O/U — no
/// confusables; the same contract as the frontend `licenseKey.ts` and
/// the mock marker keys).
pub const CROCKFORD: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Validate the normalized-key shape (22 chars: `DW` + 20 Crockford).
#[must_use]
pub fn key_shape_valid(normalized: &str) -> bool {
    normalized.len() == 22
        && normalized.starts_with("DW")
        && normalized[2..].chars().all(|c| CROCKFORD.contains(c))
}

/// sha256 hex of a string (key-hash binding + request body hashes).
#[must_use]
pub fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    hex(&h.finalize())
}

/// Lowercase hex.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Parse lowercase/uppercase hex into bytes.
#[must_use]
pub fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len().div_ceil(2))
        .map(|i| u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok())
        .collect()
}

/// Standard base64url (unpadded) decode for token parts.
#[must_use]
pub fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    const ALT: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for &c in s.as_bytes() {
        if c == b'=' {
            break;
        }
        let v = u32::try_from(ALT.iter().position(|&a| a == c)?).ok()?;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

/// Standard base64url (unpadded) encode.
#[must_use]
pub fn b64url_encode(bytes: &[u8]) -> String {
    const ALT: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for &b in bytes {
        acc = (acc << 8) | u32::from(b);
        bits += 8;
        while bits >= 6 {
            bits -= 6;
            out.push(ALT[((acc >> bits) & 0x3F) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALT[((acc << (6 - bits)) & 0x3F) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_normalization_and_shape() {
        assert_eq!(
            normalize_key("dw-test s dem00 pr000 c0de0"),
            "DWTESTSDEM00PR000C0DE0"
        );
        assert!(key_shape_valid("DWTESTSDEM00PR000C0DE0"));
        assert!(!key_shape_valid("DWTESTSDEM00PR000C0DE")); // 21
        assert!(!key_shape_valid("DWTESTSDEM00PR000C0DEI")); // I not Crockford
        assert!(!key_shape_valid("ABTESTSDEM00PR000C0DE0")); // wrong prefix
    }

    #[test]
    fn b64url_round_trip() {
        let data = b"DiskWizard entitlement payload \x00\x01\xFF";
        let enc = b64url_encode(data);
        assert_eq!(b64url_decode(&enc).unwrap(), data.to_vec());
    }

    #[test]
    fn hex_helpers_round_trip() {
        assert_eq!(from_hex(&sha256_hex("x")).unwrap(), {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(b"x");
            h.finalize().to_vec()
        });
        assert!(from_hex("zz").is_none());
    }
}
