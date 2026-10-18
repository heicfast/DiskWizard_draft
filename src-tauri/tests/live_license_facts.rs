//! LIVE license E2E — the REAL-device facts leg (runs ONLY when the
//! `live-license-e2e` cargo feature is enabled AND on Windows — never
//! in the normal `cargo test` pass, never in NSIS/MSIX release builds):
//!
//! ```text
//! cargo test -p diskwizard --features live-license-e2e --test live_license_facts -- --nocapture
//! DISKWIZARD_LIVE_LICENSE_E2E=1
//! DISKWIZARD_LIVE_ADMIN_KEY=<admin bearer>
//! ```
//!
//! Same-device determinism + real-claim round-trip (the DiskGenie
//! donor's v3 ask: "test on same device, check hashes matches or
//! not"):
//!
//! 1. Two INDEPENDENT collections on THIS machine must agree exactly —
//!    the fingerprint is deterministic, not a function of timing.
//! 2. Every fact is present and sanitized on a real Windows runner
//!    (control chars must never reach the server).
//! 3. Activating with the REAL claim stores EXACTLY those values
//!    server-side (admin lookup asserts field-for-field equality —
//!    the "hashes match" proof, LIVE).
//! 4. Revalidating with the SAME claim is a pure refresh.
//! 5. Cleanup: deactivate + revoke.
//!
//! Nothing here touches a real customer key: every run mints its own.
//! Ported 1:1 from the DiskGenie donor
//! (`diskgenie-v2/src-tauri/tests/live_license.rs`, test 2).

#![cfg(all(feature = "live-license-e2e", target_os = "windows"))]

use dw_license::http::ReqwestLicense;
use dw_license::protocol::{DeviceFacts, EntitlementDto, LicenseApi};
use dw_license::token::verify_token;
use dw_license::{LICENSE_PUBLIC_KEY_HEX, sha256_hex};

const ADMIN_KEY: Option<&str> = option_env!("DISKWIZARD_LIVE_ADMIN_KEY");
const LIVE: bool = option_env!("DISKWIZARD_LIVE_LICENSE_E2E").is_some();

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

fn live_base() -> String {
    std::env::var("DISKWIZARD_LICENSE_API")
        .unwrap_or_else(|_| dw_license::config::LICENSE_API_BASE.to_string())
}

/// Minimal admin HTTP (plain bearer JSON — no HMAC on admin routes).
struct AdminHttp {
    client: reqwest::blocking::Client,
    base: String,
}

impl AdminHttp {
    fn new() -> Result<Self, String> {
        Ok(Self {
            client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| format!("admin client: {e}"))?,
            base: live_base().trim_end_matches('/').to_string(),
        })
    }

    fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let resp = self
            .client
            .get(format!("{}{path}", self.base))
            .header(
                "authorization",
                format!("Bearer {}", ADMIN_KEY.unwrap_or_default()),
            )
            .send()
            .map_err(|e| format!("admin get: {e}"))?;
        let status = resp.status().as_u16();
        let text = resp.text().map_err(|e| format!("admin read: {e}"))?;
        if !(200..300).contains(&status) {
            return Err(format!("admin {path} -> {status}: {text}"));
        }
        serde_json::from_str(&text).map_err(|e| format!("admin parse: {e}"))
    }

    fn post<T: serde::de::DeserializeOwned>(&self, path: &str, body: &str) -> Result<T, String> {
        let resp = self
            .client
            .post(format!("{}{path}", self.base))
            .header(
                "authorization",
                format!("Bearer {}", ADMIN_KEY.unwrap_or_default()),
            )
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .map_err(|e| format!("admin post: {e}"))?;
        let status = resp.status().as_u16();
        let text = resp.text().map_err(|e| format!("admin read: {e}"))?;
        if !(200..300).contains(&status) {
            return Err(format!("admin {path} -> {status}: {text}"));
        }
        serde_json::from_str(&text).map_err(|e| format!("admin parse: {e}"))
    }
}

#[derive(serde::Deserialize)]
struct GenKeyResponse {
    #[allow(dead_code)]
    ok: bool,
    keys: Vec<GenKey>,
}

#[derive(serde::Deserialize)]
struct GenKey {
    key: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LookupResponse {
    license: LookupLicense,
    devices: Vec<LookupDevice>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LookupLicense {
    id: i64,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LookupDevice {
    hardware_hash: String,
    hostname: Option<String>,
    os_version: Option<String>,
    app_version: Option<String>,
    ram_mb: Option<u64>,
    machine_model: Option<String>,
    cpu_brand: Option<String>,
    cpu_cores: Option<u32>,
    arch: Option<String>,
    comp_machine: Option<String>,
    comp_volume: Option<String>,
    comp_cpu: Option<String>,
    #[allow(dead_code)]
    revoked: bool,
}

fn api() -> Result<LicenseApi<ReqwestLicense>, String> {
    Ok(LicenseApi::new(ReqwestLicense::new()?))
}

/// Verify a returned token with the production public key + the REAL
/// machine fingerprint — the same call the command layer makes.
fn verify(dto: &EntitlementDto, key: &str, facts: &DeviceFacts) -> Result<(), String> {
    let claims = verify_token(
        &dto.token,
        LICENSE_PUBLIC_KEY_HEX,
        &sha256_hex(key),
        &facts.hardware_hash,
        &facts.platform,
        now_unix(),
    )
    .map_err(|e| format!("token verification failed: {e}"))?;
    assert_eq!(claims.plat, "windows");
    Ok(())
}

/// Same-device determinism + real-claim round-trip.
#[test]
#[allow(clippy::too_many_lines, clippy::assertions_on_constants)]
fn real_device_fingerprint_is_deterministic_and_round_trips() {
    assert!(
        LIVE && ADMIN_KEY.is_some(),
        "live-license-e2e feature enabled but DISKWIZARD_LIVE_LICENSE_E2E / DISKWIZARD_LIVE_ADMIN_KEY not set"
    );
    let admin = AdminHttp::new().expect("admin client");
    let health: serde_json::Value = admin.get("/v1/health").expect("worker healthy");
    println!("[e2e] worker v{} healthy", health["version"]);

    // 1. Determinism: two independent collections agree on EVERY byte.
    //    This is the app crate's REAL collection path — the same one the
    //    activation command uses (shared `collect_device_facts`).
    let facts_a = diskwizard_lib::commands::collect_device_facts()
        .expect("real device facts collectable on the CI machine");
    let facts_b = diskwizard_lib::commands::collect_device_facts()
        .expect("real device facts collectable (2nd collection)");
    assert_eq!(
        facts_a.hardware_hash, facts_b.hardware_hash,
        "same-device composite fingerprint must be deterministic"
    );
    println!(
        "[e2e] fingerprint stable across two collections: {}",
        &facts_a.hardware_hash[..12]
    );

    // 2. Fact sanity + sanitation on a real Windows runner.
    println!(
        "[e2e] real facts: hostname={:?} os={:?} cpu={:?} cores={:?} arch={:?} model={:?} ram={:?}",
        facts_a.hostname,
        facts_a.os_version,
        facts_a.cpu_brand,
        facts_a.cpu_cores,
        facts_a.arch,
        facts_a.machine_model,
        facts_a.ram_mb
    );
    assert!(
        facts_a.hostname.len() >= 2,
        "hostname collected on a real machine"
    );
    assert!(
        facts_a.os_version.contains("Windows"),
        "os_version reads the real OS"
    );
    assert!(facts_a.cpu_cores.unwrap_or(0) >= 2, "cpu_cores collected");
    assert_eq!(
        facts_a.arch.as_deref(),
        Some("x86_64"),
        "arch on the windows runner"
    );
    for fact in [
        facts_a.cpu_brand.as_deref(),
        Some(facts_a.hostname.as_str()),
    ]
    .into_iter()
    .flatten()
    {
        assert!(
            !fact.chars().any(|c| c.is_control()),
            "facts must be control-char clean before the wire: {fact:?}"
        );
    }
    if let Some(brand) = &facts_a.cpu_brand {
        assert!(!brand.contains("  "), "brand spaces collapsed: {brand:?}");
    }

    // 3. Round-trip: activate with the REAL claim → the stored row
    //    matches field-for-field (the "hashes match" proof).
    let minted: GenKeyResponse = admin
        .post(
            "/v1/admin/keys",
            &serde_json::json!({
                "tier": "yearly",
                "customerName": "E2E Real Facts Bot",
                "customerEmail": "e2e-real@diskwizard.test",
                "days": 1,
                "note": "ci live e2e v3",
                "source": "webhook",
            })
            .to_string(),
        )
        .expect("generate key");
    let key = &minted.keys[0].key;

    let dto = api()
        .expect("api")
        .activate(key, &facts_a)
        .expect("activate with REAL facts");
    verify(&dto, key, &facts_a).expect("verify REAL activation token");

    let look: LookupResponse = admin
        .post(
            "/v1/admin/lookup",
            &serde_json::json!({ "key": key }).to_string(),
        )
        .expect("lookup real facts");
    let dev = look
        .devices
        .iter()
        .find(|d| d.hardware_hash == facts_a.hardware_hash)
        .expect("device row for the real fingerprint");
    assert_eq!(dev.hostname.as_deref(), Some(facts_a.hostname.as_str()));
    assert_eq!(dev.os_version.as_deref(), Some(facts_a.os_version.as_str()));
    assert_eq!(
        dev.app_version.as_deref(),
        Some(facts_a.app_version.as_str())
    );
    assert_eq!(dev.cpu_cores, facts_a.cpu_cores);
    assert_eq!(dev.arch.as_deref(), facts_a.arch.as_deref());
    assert_eq!(dev.cpu_brand.as_deref(), facts_a.cpu_brand.as_deref());
    assert_eq!(dev.ram_mb, facts_a.ram_mb);
    assert_eq!(
        dev.machine_model.as_deref(),
        facts_a.machine_model.as_deref()
    );
    assert_eq!(dev.comp_machine.as_deref(), facts_a.comp_machine.as_deref());
    assert_eq!(dev.comp_volume.as_deref(), facts_a.comp_volume.as_deref());
    assert_eq!(dev.comp_cpu.as_deref(), facts_a.comp_cpu.as_deref());
    println!("[e2e] REAL claim round-trip: every stored field matches the collected facts");

    // 4. Revalidating with the SAME real claim is a pure refresh (no
    //    device_update diff — determinism extends to the server view).
    let dto2 = api()
        .expect("api")
        .validate(key, &facts_a)
        .expect("revalidate REAL facts");
    verify(&dto2, key, &facts_a).expect("verify revalidation token");
    let look2: LookupResponse = admin
        .post(
            "/v1/admin/lookup",
            &serde_json::json!({ "key": key }).to_string(),
        )
        .expect("lookup 2");
    let dev2 = look2
        .devices
        .iter()
        .find(|d| d.hardware_hash == facts_a.hardware_hash)
        .expect("device row after revalidation");
    assert_eq!(
        dev2.hostname.as_deref(),
        dev.hostname.as_deref(),
        "stable across revalidation"
    );

    // 5. Cleanup: free the slot + revoke the key.
    api()
        .expect("api")
        .deactivate(key, &facts_a)
        .expect("deactivate real-facts device");
    let _: serde_json::Value = admin
        .post(&format!("/v1/admin/keys/{}/revoke", look.license.id), "{}")
        .expect("cleanup revoke");
    println!("[e2e] real-facts device deactivated + license revoked — v3 E2E PASS");
}
