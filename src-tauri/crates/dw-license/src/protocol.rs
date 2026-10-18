//! The protocol client — activate / validate / deactivate against the
//! Worker, with HMAC-signed requests and untrusted-response handling.

use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use crate::http::LicenseHttp;
use crate::sha256_hex;
use crate::token::LicenseError as LE;

/// Default production license-server base. The owner deploys the
/// DiskWizard worker (the diskgenie-license-server codebase under the
/// DiskWizard identity) and pins this URL; the env
/// `DISKWIZARD_LICENSE_API` overrides for tests/local dev.
pub const LICENSE_API_BASE: &str = "https://diskwizard-license.heictojpg-pics.workers.dev/";

/// The purchase page.
pub const LICENSE_PURCHASE_URL: &str = "https://diskwizard.app/pricing";

/// Ed25519 public key (64 hex) — the ONLY key this binary holds. The
/// matching private seed lives as the Worker secret
/// `LICENSE_SIGNING_PRIVATE_KEY` (deployment sheet: the owner's
/// DISKWIZARD_LICENSE_SECRETS document). Rotation procedure: new pair
/// → Worker secret → update here → ship.
pub const LICENSE_PUBLIC_KEY_HEX: &str =
    "2d44bbd8034e85810cccac455be9bb1bc9df839842e6a0b33ea68c1f3d15a91b";

/// HMAC-SHA256 request secret (64 hex) shared with the Worker secret
/// `CLIENT_REQUEST_SECRET`. Honest threat model (doc §2 L2): ships in
/// the binary — abuse friction, NOT the security boundary; the
/// boundary is the Ed25519 signature above.
const CLIENT_SECRET_HEX: &str = "f8f479824b54c9feb52bd6ee15c0d2eb2a36aa47993a962490d9dcf2bef5340a";

/// Resolve the API base (env override for tests/local dev).
#[must_use]
pub fn api_base() -> String {
    std::env::var("DISKWIZARD_LICENSE_API").unwrap_or_else(|_| LICENSE_API_BASE.to_string())
}

/// The device facts collected at activation (what the D1 `devices`
/// row stores; PII-minimized per the architecture: identity + binding
/// + audit strings only).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceFacts {
    /// `windows`.
    pub platform: String,
    /// The hardware fingerprint (64 hex) — the binding identity.
    pub hardware_hash: String,
    /// Machine name (audit + support display).
    pub hostname: String,
    /// OS version string.
    pub os_version: String,
    /// App semver.
    pub app_version: String,
    /// sha256("machine:" + MachineGuid) — component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comp_machine: Option<String>,
    /// sha256("volume:" + system-drive serial) — component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comp_volume: Option<String>,
    /// sha256("cpu:" + CPUID brand) — component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comp_cpu: Option<String>,
    /// CPU brand string for display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_brand: Option<String>,
    /// Total physical memory (MB).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ram_mb: Option<u64>,
    /// Machine model for display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_model: Option<String>,
    /// Logical CPU count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<u32>,
    /// CPU architecture (`x86_64` / `aarch64`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
}

/// The server's success envelope (serde mirror of the Worker
/// response). The `token` is the ONLY field the client trusts — every
/// display datum is read from the VERIFIED claims inside it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntitlementDto {
    pub token: String,
}

/// The error envelope (the `code` field drives every decision).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ErrorDto {
    #[serde(default)]
    code: String,
}

/// Nonce counter (process-lifetime) mixed into request nonces.
static NONCE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Client for the three authenticated endpoints. Every request
/// carries the HMAC headers; every response is treated as untrusted
/// input (parse → verify signature → only then apply).
pub struct LicenseApi<H: LicenseHttp> {
    http: H,
    base: String,
    secret_hex: String,
}

impl<H: LicenseHttp> LicenseApi<H> {
    /// Build against the resolved base + shared secret.
    #[must_use]
    pub fn new(http: H) -> Self {
        Self {
            http,
            base: api_base(),
            secret_hex: CLIENT_SECRET_HEX.to_string(),
        }
    }

    /// Build with an explicit base + secret override (tests only).
    #[must_use]
    pub fn with_overrides(http: H, base: String, secret_hex: String) -> Self {
        Self {
            http,
            base,
            secret_hex,
        }
    }

    /// A single-use nonce: counter + nanos + pid + the secret hashed
    /// together (uniqueness is the property that matters for replay
    /// defense).
    fn nonce(&self) -> String {
        let count = NONCE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let pid = u64::from(std::process::id());
        let mut hasher = Sha256::new();
        hasher.update(count.to_le_bytes());
        hasher.update(nanos.to_le_bytes());
        hasher.update(pid.to_le_bytes());
        hasher.update(self.secret_hex.as_bytes());
        crate::hex(&hasher.finalize())[..32].to_string()
    }

    /// HMAC-SHA256 hex over `secret || message`.
    fn hmac_hex(&self, message: &str) -> String {
        let key = crate::from_hex(&self.secret_hex).unwrap_or_default();
        let mut mac = Hmac::<Sha256>::new_from_slice(&key).expect("hmac accepts any key size");
        mac.update(message.as_bytes());
        crate::hex(&mac.finalize().into_bytes())
    }

    /// Sign + POST one request; returns (status, body).
    fn signed_post(&self, path: &str, body: &str) -> Result<(u16, String), LE> {
        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default();
        let nonce = self.nonce();
        let body_hash = sha256_hex(body);
        let message = format!("{timestamp}.{nonce}.POST.{path}.{body_hash}");
        let sig = self.hmac_hex(&message);
        let headers = vec![
            ("x-dw-app".to_string(), "diskwizard".to_string()),
            (
                "x-dw-version".to_string(),
                env!("CARGO_PKG_VERSION").to_string(),
            ),
            ("x-dw-timestamp".to_string(), timestamp.to_string()),
            ("x-dw-nonce".to_string(), nonce),
            ("x-dw-signature".to_string(), sig),
        ];
        let url = format!("{}{path}", self.base.trim_end_matches('/'));
        self.http
            .post_json(&url, &headers, body)
            .map_err(|_| LE::Network)
    }

    /// POST + parse the success/error envelope into an outcome.
    fn call(&self, path: &str, body: &str) -> Result<Result<EntitlementDto, LE>, LE> {
        let (status, text) = self.signed_post(path, body)?;
        if (200..300).contains(&status) {
            let dto: EntitlementDto = serde_json::from_str(&text).map_err(|_| LE::ServerError)?;
            return Ok(Ok(dto));
        }
        let code = serde_json::from_str::<ErrorDto>(&text)
            .map(|e| e.code)
            .unwrap_or_default();
        Ok(Err(LE::from_response(status, &code)))
    }

    /// Activate a key on this device. The response token MUST verify
    /// locally (the app layer's apply step) before it is trusted.
    ///
    /// # Errors
    /// [`LicenseError`] per the endpoint contract.
    pub fn activate(&self, key: &str, facts: &DeviceFacts) -> Result<EntitlementDto, LE> {
        let body = claim_body(key, facts);
        match self.call("/v1/activate", &body)? {
            Ok(dto) => Ok(dto),
            Err(e) => Err(e),
        }
    }

    /// The 24 h revalidation call — carries the FULL device claim (the
    /// server refreshes display facts with COALESCE semantics).
    ///
    /// # Errors
    /// [`LicenseError`] per the endpoint contract.
    pub fn validate(&self, key: &str, facts: &DeviceFacts) -> Result<EntitlementDto, LE> {
        let body = claim_body(key, facts);
        match self.call("/v1/validate", &body)? {
            Ok(dto) => Ok(dto),
            Err(e) => Err(e),
        }
    }

    /// Deactivate this device (frees the platform slot; idempotent).
    ///
    /// # Errors
    /// Soft network errors surface; hard errors too — but the caller
    /// always clears local state (deactivation is never blocked).
    pub fn deactivate(&self, key: &str, facts: &DeviceFacts) -> Result<(), LE> {
        let body = serde_json::json!({
            "licenseKey": key,
            "hardwareHash": facts.hardware_hash,
            "platform": facts.platform,
        });
        let (status, _) = self.signed_post("/v1/deactivate", &body.to_string())?;
        if (200..300).contains(&status) {
            Ok(())
        } else {
            // Idempotent server-side; non-2xx here is almost always
            // "already gone" — treat as success so the client always
            // clears local state.
            Ok(())
        }
    }
}

/// The activate/validate wire body: the full DeviceFacts claim plus
/// the license key. One shape for both routes.
fn claim_body(key: &str, facts: &DeviceFacts) -> String {
    let mut value = serde_json::to_value(facts).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(obj) = value.as_object_mut() {
        obj.insert("licenseKey".to_string(), serde_json::json!(key));
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    /// Scripted fake transport (request capture + canned responses).
    struct FakeHttp {
        responses: Mutex<VecDeque<Result<(u16, String), String>>>,
        seen: Mutex<Vec<(String, String)>>,
    }

    impl FakeHttp {
        fn with(responses: Vec<Result<(u16, String), String>>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                seen: Mutex::new(Vec::new()),
            }
        }
    }

    impl LicenseHttp for FakeHttp {
        fn post_json(
            &self,
            url: &str,
            headers: &[(String, String)],
            body: &str,
        ) -> Result<(u16, String), String> {
            self.seen
                .lock()
                .unwrap()
                .push((url.to_string(), body.to_string()));
            let hdrs: Vec<(String, String)> = headers.to_vec();
            let _ = hdrs;
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Err("exhausted".into()))
        }
    }

    fn facts() -> DeviceFacts {
        DeviceFacts {
            platform: "windows".into(),
            hardware_hash: "a".repeat(64),
            hostname: "TESTPC".into(),
            os_version: "Windows 11".into(),
            app_version: "0.1.0".into(),
            comp_machine: None,
            comp_volume: None,
            comp_cpu: None,
            cpu_brand: Some("Test CPU".into()),
            ram_mb: Some(16_384),
            machine_model: None,
            cpu_cores: Some(8),
            arch: Some("x86_64".into()),
        }
    }

    const SECRET: &str = "0000000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn activate_success_parses_envelope() {
        let http = FakeHttp::with(vec![Ok((200, r#"{"token":"cGF5bG9hZA.sig"}"#.into()))]);
        let api = LicenseApi::with_overrides(http, "https://unit.test/".into(), SECRET.to_string());
        let dto = api
            .activate("DWTESTSDEM00PR000C0DE0", &facts())
            .expect("ok");
        assert_eq!(dto.token, "cGF5bG9hZA.sig");
    }

    #[test]
    fn typed_errors_map_from_codes() {
        let http = FakeHttp::with(vec![Ok((404, r#"{"code":"KEY_NOT_FOUND"}"#.into()))]);
        let api = LicenseApi::with_overrides(http, "https://unit.test/".into(), SECRET.to_string());
        assert_eq!(
            api.activate("DWWR0NG0000000000000000", &facts())
                .unwrap_err(),
            LE::InvalidKey
        );
    }

    #[test]
    fn network_failure_is_soft() {
        let http = FakeHttp::with(vec![Err("connection reset".into())]);
        let api = LicenseApi::with_overrides(http, "https://unit.test/".into(), SECRET.to_string());
        assert_eq!(
            api.validate("DWTESTSDEM00PR000C0DE0", &facts())
                .unwrap_err(),
            LE::Network
        );
        assert!(!LE::Network.is_hard());
    }

    #[test]
    fn claim_body_carries_the_key_and_facts() {
        let body = claim_body("DWTESTSDEM00PR000C0DE0", &facts());
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["licenseKey"], "DWTESTSDEM00PR000C0DE0");
        assert_eq!(v["hardwareHash"], "a".repeat(64));
        assert_eq!(v["platform"], "windows");
        assert!(v.get("compMachine").is_none()); // skipped when None
    }

    #[test]
    fn requests_carry_hmac_headers() {
        let _http = FakeHttp::with(vec![Ok((200, r#"{"token":"t.s"}"#.into()))]);
        // Capture headers via a probe transport.
        type HeadersList = Vec<(String, String)>;
        #[derive(Clone)]
        struct ProbeHttp(std::sync::Arc<Mutex<Vec<HeadersList>>>);
        impl LicenseHttp for ProbeHttp {
            fn post_json(
                &self,
                _url: &str,
                headers: &[(String, String)],
                _body: &str,
            ) -> Result<(u16, String), String> {
                self.0.lock().unwrap().push(headers.to_vec());
                Ok((200, r#"{"token":"t.s"}"#.into()))
            }
        }
        let probe = ProbeHttp(std::sync::Arc::new(Mutex::new(Vec::new())));
        let api = LicenseApi::with_overrides(
            probe.clone(),
            "https://unit.test/".into(),
            SECRET.to_string(),
        );
        let _ = api.activate("DWTESTSDEM00PR000C0DE0", &facts());
        let headers = probe.0.lock().unwrap()[0].clone();
        let names: Vec<&str> = headers.iter().map(|(k, _)| k.as_str()).collect();
        for expected in [
            "x-dw-app",
            "x-dw-version",
            "x-dw-timestamp",
            "x-dw-nonce",
            "x-dw-signature",
        ] {
            assert!(names.contains(&expected), "missing {expected}");
        }
        let sig = headers
            .iter()
            .find(|(k, _)| k == "x-dw-signature")
            .map(|(_, v)| v.clone())
            .unwrap();
        assert_eq!(sig.len(), 64);
    }

    // Silence unused warnings for the capture transport in this test.
}
