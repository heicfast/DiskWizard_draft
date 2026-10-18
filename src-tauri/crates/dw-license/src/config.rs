//! §8 CONFIG MODULE — the ONE file that carries every server URL and
//! key-material value (MASTER_PROMPT §8; worklog/open-questions.md OQ-3).
//!
//! **Current posture (TEMPORARY, operator-mandated):** the DiskWizard
//! license server is not yet deployed. Until it is, DiskWizard points
//! at the **DiskGenie** deployment and uses the **same secrets/keys/
//! protocol values**, so the DiskGenie license server
//! (`diskgenie-license-server-v2`, deployed as the `diskbytes-license`
//! worker) validates DiskWizard activations. Every value below is
//! quoted verbatim from the donor client
//! (`diskgenie-v2/src-tauri/src/license.rs`) and cross-checked against
//! the server source (`src/guard.ts`, `src/keys.ts`, `src/tokens.ts`).
//!
//! **The future swap (one file, this file):** when the DiskWizard
//! worker deploys, replace these values with the DiskWizard deployment
//! sheet (endpoint + fresh keypair + fresh client secret) and flip the
//! product-identity values (`KEY_PREFIX` back to `DW`, `TOKEN_ISSUER`,
//! `TOKEN_AUDIENCE`, `HEADER_APP`, `LICENSE_USER_AGENT`). No other
//! file in the crate hardcodes any of them.
//!
//! Honest threat model (donor §2 L2, unchanged): the HMAC secret ships
//! in a public-source binary — abuse friction, NOT the security
//! boundary; the boundary is the Ed25519 public key below.

/// Default production license-server base (donor `license.rs:34`).
pub const LICENSE_API_BASE: &str = "https://diskbytes-license.heictojpg-pics.workers.dev/";

/// Local/test override env var (client-side convenience; not protocol).
pub const LICENSE_API_ENV: &str = "DISKWIZARD_LICENSE_API";

/// The purchase page (DiskWizard's own product surface — NOT part of
/// server validation; the donor's equivalent is
/// `https://diskgenie.app/pricing`).
pub const LICENSE_PURCHASE_URL: &str = "https://diskwizard.app/pricing";

/// Ed25519 public key (64 hex) — the ONLY key this binary holds (donor
/// `license.rs:40-41`). The matching private seed is the Worker secret
/// `LICENSE_SIGNING_PRIVATE_KEY`; kid 1 = this key.
pub const LICENSE_PUBLIC_KEY_HEX: &str =
    "4e4f51ef1593c184a1d1863d0cd53f5c3718c1674a1dfb3b85ee552d775fbb89";

/// HMAC-SHA256 request secret (64 hex), shared with the Worker secret
/// `CLIENT_REQUEST_SECRET` (donor `license.rs:47`).
pub const CLIENT_SECRET_HEX: &str =
    "333d71177f5b8558e7ea8f45140742dfa71621e1000b899b682e340b80c61993";

/// Pinned request User-Agent (donor `license.rs:126`). The DEPLOYED
/// worker validates the pre-rename value; flip this together with the
/// endpoint at the DiskWizard cutover.
pub const LICENSE_USER_AGENT: &str = "DiskBytes-License-Client/1";

/// Auth-header prefix (donor `license.rs:695-704`; the server guard
/// reads `x-db-*` only — `src/guard.ts`). Swap target: `x-dw`.
pub const HEADER_PREFIX: &str = "x-db";

/// Auth-header app identity (donor sends `x-db-app: "diskgenie"`).
pub const HEADER_APP: &str = "diskgenie";

/// License-key prefix + shape: `DB` + 20 Crockford base32 chars
/// (donor `licenseKey.ts:13-33`; server canonical regex
/// `src/keys.ts:15`). The server's key registry only knows DB-keys —
/// a DW-prefixed key would 404 KEY_NOT_FOUND. Swap target: `DW`.
pub const KEY_PREFIX: &str = "DB";

/// Entitlement-token issuer (donor `license.rs:387`; server
/// `tokens.ts:68`). Swap target: `dw-license`.
pub const TOKEN_ISSUER: &str = "db-license";

/// Entitlement-token audience (donor `license.rs:348-351`; server
/// `tokens.ts:87` mints `aud: "diskgenie"`). Swap target: `diskwizard`.
pub const TOKEN_AUDIENCE: &str = "diskgenie";
