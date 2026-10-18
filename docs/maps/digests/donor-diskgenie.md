# Digest — DiskGenie donor study (Task 5-d)

Research-only digest of the two DiskGenie donor repos, produced for the
DiskWizard licensing/workflow/theme port (master prompt §8: port 1:1;
until a DiskWizard server is deployed, use the DiskGenie endpoint +
the SAME secrets/keys/values, isolated in ONE config module).
Every constant below is quoted verbatim from the file:line shown.

Donor repos:
- `repos/diskgenie-v2` (client app: Rust + Tauri 2 + React/TS)
- `repos/diskgenie-license-server-v2` (Cloudflare Worker + D1)

---

## 1. Licensing client (diskgenie-v2)

Files (all read completely):
- `src-tauri/src/license.rs` (1593 lines incl. tests) — protocol + crypto + postures + DPAPI
- `src-tauri/src/commands/license.rs` (722 lines) — Tauri commands, scheduler, hard gate, CI sim
- `src-tauri/tests/live_license.rs` — LIVE E2E vs the deployed worker (feature `live-license-e2e`)
- `src/lib/licenseKey.ts` — key shape helpers (frontend)
- `src/state/license.ts`, `src/components/LicenseDialog.tsx`, `src/components/ActivationGate.tsx` (UI layer)
- `docs/LICENSING-ARCHITECTURE.md` — the governing doc (layer table §2, crypto §3, protocol §4, D1 §5, client §6)

### 1.1 Constants (EXACT — port verbatim)

All in `src-tauri/src/license.rs` unless noted:

| Value | Quote | Location |
|---|---|---|
| API base URL | `"https://diskbytes-license.heictojpg-pics.workers.dev/"` | license.rs:34 (`const LICENSE_API_BASE`) |
| Env override | `DISKGENIE_LICENSE_API` | license.rs:66 |
| Ed25519 public key (64 hex) | `"4e4f51ef1593c184a1d1863d0cd53f5c3718c1674a1dfb3b85ee552d775fbb89"` | license.rs:40-41 (`pub const LICENSE_PUBLIC_KEY_HEX`) |
| HMAC client secret (64 hex) | `"333d71177f5b8558e7ea8f45140742dfa71621e1000b899b682e340b80c61993"` | license.rs:47 (`const CLIENT_SECRET_HEX`) — "shared with the Worker secret `CLIENT_REQUEST_SECRET`" (license.rs:43-44); ships in a public-source binary: abuse friction, NOT the boundary (license.rs:44-46) |
| Purchase URL | `"https://diskgenie.app/pricing"` | license.rs:51 |
| Revalidation interval | `VALIDATION_INTERVAL_S: i64 = 24 * 60 * 60` | license.rs:54 |
| Offline grace | `GRACE_DAYS: i64 = 14` ("keep in sync with the Worker var `TOKEN_TTL_DAYS`") | license.rs:58 |
| Clock skew | `CLOCK_SKEW_S: i64 = 300` | license.rs:61 |
| User-Agent | `"DiskBytes-License-Client/1"` | license.rs:126 — the DEPLOYED worker validates the pre-rename value; the renamed server source accepts BOTH `"DiskGenie-License-Client/1"` and `"DiskBytes-License-Client/1"` (server `src/guard.ts:25-28`); the client UA flips in the SAME change as `LICENSE_API_BASE` (license.rs:118-125) |
| Test fixture seed (both repos) | `"9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60"` (RFC 8032 test-vector #1) | license.rs:928 `TEST_SEED_HEX`; server `test/constants.ts:11-12 TEST_SIGNING_SEED` — identical; client test asserts derived pubkey `"d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"` (license.rs:1181) |

Key format (client side):
- `src/lib/licenseKey.ts:13` — Crockford alphabet `"0123456789ABCDEFGHJKMNPQRSTVWXYZ"` (no I/L/O/U)
- `licenseKey.ts:16` — `KEY_LENGTH = 22` (`"DB"` + 20 chars)
- `licenseKey.ts:24-33` `isValidKeyShape`: length 22 AND startsWith `"DB"` AND all body chars ∈ alphabet (structural check, not a regex)
- `licenseKey.ts:43-47` `formatKey`: display `"DB-"` + 4×5-char groups → `DB-XXXXX-XXXXX-XXXXX-XXXXX`
- Rust twin `commands/license.rs:106-120 normalize_key`: uppercase → strip non-alphanumeric → same 22/`"DB"`/Crockford check (alphabet literal at commands/license.rs:118)
- Server-side canonical regex: `src/keys.ts:15` — `const KEY_RE = /^DB[0123456789ABCDEFGHJKMNPQRSTVWXYZ]{20}$/;`
- Server display generation `keys.ts:23`: `` `DB-${body.slice(0,5)}-${body.slice(5,10)}-${body.slice(10,15)}-${body.slice(15,20)}` ``

### 1.2 Wire protocol (exact)

Endpoints (all POST JSON; base + path, trailing `/` trimmed — license.rs:705):
- `POST /v1/activate` (license.rs:737)
- `POST /v1/validate` (license.rs:753) — 24 h revalidation
- `POST /v1/deactivate` (license.rs:769) — body is ONLY `{licenseKey, hardwareHash, platform}` (license.rs:764-768); idempotent: client treats any non-2xx as success (license.rs:770-777)
- (server also has `POST /v1/verify` debug + `GET /v1/health` public — client never calls them)

Request auth (HMAC-SHA256; license.rs:686-709 + server `src/guard.ts:47-75`):
- Headers: `x-db-app: "diskgenie"`, `x-db-version: <CARGO_PKG_VERSION>`, `x-db-timestamp: <unix ms>`, `x-db-nonce: <32 hex>`, `x-db-signature: <64 hex>` (license.rs:695-704)
- Signed message: `"{timestamp}.{nonce}.POST.{path}.{body_hash}"` where `body_hash = sha256_hex(body)` (license.rs:693) — server recomputes over raw body, method uppercase, pathname (guard.ts:67-73)
- Nonce = sha256(counter ‖ nanos ‖ pid ‖ secret)[..32] (license.rs:671-683); single-use, server records in `nonce_seen` (db.ts:544-551)
- Server window: ±300 s (`CLOCK_WINDOW_S = 300`, guard.ts:16); header shape checks guard.ts:59 (`ts` `^\d{1,15}$`, nonce `^[0-9a-f]{32}$`, sig `^[0-9a-f]{64}$`)
- Body size cap: 16 384 bytes (`MAX_BODY_BYTES`, server routes/shared.ts:39)
- UA pinned (guard.ts:25-28, 53-54); no CORS ever emitted (index.ts:51-52)

Request bodies: activate + validate send ONE shape — full `DeviceFacts`
(camelCase serde, absent optional fields skipped) + `"licenseKey"` added
(license.rs:785-791 `claim_body`). v2 mandate: validate carries the SAME
full claim as activate (the v1 omission caused the server-side
fact-wipe bug — license.rs:743-756, server routes/validate.ts:8-17).

Response envelope: success = `EntitlementDto { token }` (serde mirror,
license.rs:286-293) — "the ONLY field the client trusts; every display
datum is read from the VERIFIED claims inside it" (license.rs:289-292).
Server actually returns `{ok:true, token, license:{tier,name,email,expiresAt,last4}, device:{platform,activatedAt,lastSeenAt}}` (server types.ts:112-130) — client ignores the rest.
Error envelope: `{code, ...}` → `ErrorDto.code` drives mapping (license.rs:297-302, 192-202).

Error contract (status, code) → typed error (license.rs:192-202):
- 404 / `KEY_NOT_FOUND` → InvalidKey
- 403 `KEY_REVOKED`|`KEY_REFUNDED`|`KEY_PENDING` → Inactive
- 403 `LICENSE_EXPIRED` → Expired
- 403 `DEVICE_MISMATCH` → DeviceMismatch
- 409 / `DEVICE_SLOT_TAKEN` → DeviceSlotTaken
- 429 / `RATE_LIMITED` → RateLimited (SOFT — never retried, hourly window can't expire in a retry loop, commands/license.rs:202-207)
- 401 `REPLAYED` / `BAD_SIGNATURE` / `BAD_TIMESTAMP`, 403 `BAD_UA`, 400 `BAD_REQUEST` → ServerError bucket (fall-through)
- hard = everything except Network/ServerError/RateLimited (license.rs:207-209)

Transport: `ReqwestLicense` = reqwest blocking client, 30 s timeout, rustls (license.rs:88-105; Cargo.toml:42-46), `CONTENT_TYPE: application/json` + pinned UA (license.rs:117, 126). All commands run blocking work via `spawn_blocking` (freeze fix, commands/license.rs:159-166).

### 1.3 Entitlement token (crypto)

Format: `b64url(payloadJson).b64url(sig64)` (license.rs:354-404; server crypto.ts:91-97). RFC 4648 base64url, no padding, canonical-only decode (license.rs:871-904).

Claims (`TokenClaims`, license.rs:310-352; server types.ts:92-109):
`iss`="db-license" (fixed, license.rs:387), `ver`=1, `jti` (hex id; client doesn't branch), `iat`, `exp` (token expiry = offline grace hard stop), `key` = sha256(normalized key) hex, `tier` "yearly"|"lifetime", `name`, `email`, `hw` (64-hex fingerprint), `plat` "windows"|"macos", `lexp` (license expiry, absent for lifetime), `kid` (Option; kid 1 = the embedded production key, license.rs:344-347, 398), `aud` (Option; "diskgenie", license.rs:348-351, 399).

`verify_token` checks (ANY failure → `LicenseError::Spoofed`, never partial accept — license.rs:354-404):
Ed25519 signature against `LICENSE_PUBLIC_KEY_HEX` (ed25519-dalek `VerifyingKey`), then: `iss=="db-license"`, `ver==1`, `plat==platform`, `key==expected_key_hash` (sha256 of the stored normalized key), `hw==expected_hw`, `exp > now`, `iat <= now+CLOCK_SKEW_S`, `lexp` (if present) `> now`, `kid` (if present) `== 1`, `aud` (if present) `== "diskgenie"` (fleet compat window: absent claims stay valid).

Server-side signing: `@noble/ed25519` (`ed.signAsync`) over the exact payload bytes, seed = Worker secret `LICENSE_SIGNING_PRIVATE_KEY` (64-hex seed; crypto.ts:91-97, 81-85). Server mint adds `kid: 1` + `aud: "diskgenie"` to every NEW token (tokens.ts:67-87); `iss: "db-license"` (tokens.ts:68). Key rotation design: kid registry (tokens.ts:26-28) — kid 1 IS `LICENSE_SIGNING_PRIVATE_KEY`.

### 1.4 Posture machine + grace/degraded rules

`LicensePosture` (license.rs:457-470): `Unlicensed | Pro | Grace { days_left: i64 } | Degraded`.

Derivation (PURE fn, license.rs:474-495):
1. `license_key` empty → `Unlicensed`
2. `token_exp <= now` → `Degraded`
3. yearly license past its own `license_expires_at` → `Degraded` (even with a valid token)
4. `now - max(last_validated_at, last_known_good) < VALIDATION_INTERVAL_S` (24 h) → `Pro`
5. else → `Grace { days_left = ceil((token_exp - now)/86400) }`

STRICT semantics (owner decision, session 10): no free tier —
unlicensed/degraded keep the Monitor tab ONLY; `require_licensed`
refuses at the Rust boundary (commands/license.rs:506-525; markers
`GATE_ACTIVATION_REQUIRED = "ACTIVATION_REQUIRED"` commands/license.rs:25,
`GATE_STALE = "LICENSE_STALE"` :28; Degraded copy says "couldn't verify
your license for over {GRACE_DAYS} days"). Grace KEEPS pro features
(`is_pro = pro || grace`, commands/license.rs:85). The WebView greps the
marker prefixes to open the activation flow (ipc.ts:35-44 intercept).

Revalidation cycle (commands/license.rs:417-479 `validate_cycle`):
- verified fresh token → refresh tier/name/email/lexp/token/token_exp/last_validated_at, `last_known_good = max(prev, now)`
- transport/parse failure OR failed local verification (spoofed server) → grace continues, timestamps FROZEN (a mimic cannot extend anything; token `exp` is the hard stop; honest during key rotation)
- explicit hard errors (revoked/expired/device-mismatch) → `dpapi::clear()` + local state reset
- unreadable fingerprint → keep grace path (commands/license.rs:424-432)
- scheduler thread `"db-license"`: immediate launch validation + loop every `VALIDATION_INTERVAL_S` (commands/license.rs:483-500)
- activation retries transport twice with jittered backoff (250 ms × attempt + 0-149 ms jitter), never retries RATE_LIMITED (commands/license.rs:208-222)

Persistence: `license.bin` in app_data_dir, DPAPI `CryptProtectData` on Windows / Keychain on macOS (`dpapi` mod, license.rs:797-830; platform seam `platform::os::dpapi_protect`). Cleared on deactivate; `last_known_good` = monotonic clock-rollback guard (license.rs:447-449).

### 1.5 Hardware fingerprint composition

`hardware_id()` (license.rs:507-517) — FROZEN algorithm (changing it orphans every v1 activation):
`SHA-256(MachineGuid ‖ system-drive-volume-serial-as-le-bytes ‖ CPUID-brand-string)` — inputs via platform seam (macOS: IOPlatformUUID + root fsid + CPU brand). All three required, else error (no partial identity).

Per-component hashes `sha256(label ‖ value)` (license.rs:522-528), labels: `"machine:"`, `"volume:"`, `"cpu:"`, `"board:"`, `"firmware:"` (license.rs:606-619) — swap forensics, not binding.

`collect_device_facts()` (license.rs:560-595) → `DeviceFacts` (camelCase wire): `platform, hardwareHash, hostname, osVersion, appVersion, compMachine, compVolume, compCpu, cpuBrand, ramMb, machineModel, baseboardSerial, firmwareUuid, biosVersion, cpuCores, arch, compBoard, compFirmware` (struct at license.rs:231-283).

Key-hash binding: `key_hash_of(normalized) = sha256_hex(normalized)` (commands/license.rs:131-134) — matches server `keyHashOf` (tokens.ts:117-119); D1 stores ONLY this hash (schema 0001:10-13).

### 1.6 Tauri command surface

`license_status`, `activate_license`, `deactivate_license`, `validate_now`, `license_sim_set` (commands/license.rs:137, 153, 309, 358, 556). Status view is camelCase `LicenseStatusView` {posture, isPro, tier, customerName, customerEmail, licenseExpiresAt, graceDaysLeft, purchaseUrl, simulated} (commands/license.rs:57-74). Events: `license-changed` emitted on every state change (commands/license.rs:172, 324, 375...).

### 1.7 LIVE E2E (CI, feature `live-license-e2e`)

`tests/live_license.rs` — runs ONLY with `--features live-license-e2e` + env `DISKGENIE_LIVE_LICENSE_E2E=1` + `DISKGENIE_LIVE_ADMIN_KEY` (compile-time `option_env!`, live_license.rs:29-37). Flow: admin-generate a throwaway key (yearly, days=1 → self-expiring cleanup) → activate with full claim → verify token against the compiled-in PRODUCTION public key → revalidate with CHANGED facts → admin-lookup asserts the device row kept them (the v1 wipe regression, LIVE) → slot rule → deactivate → revoke. Second test: run the REAL platform collector on the CI machine twice — fingerprint must be deterministic, v3 facts present + sanitized, activation stores EXACTLY those values. Env `DISKGENIE_LICENSE_API` overrides the base (live_license.rs:60-62).

---

## 2. License server (diskgenie-license-server-v2)

Read completely (all of src/, migrations/, scripts/, wrangler.jsonc, README, tests/configs).

### 2.1 Architecture

Cloudflare Worker (wrangler v4, `src/index.ts` router) + D1 (binding `DB`, db name `diskgenie-license`). Name: **`diskbytes-license` is the DEPLOYED worker name/URL** — wrangler.jsonc:11 sets `"name": "diskgenie-license"` with a NAME-CUTOVER note (wrangler.jsonc:23-37): the deployed worker is still `diskbytes-license` answering at `diskbytes-license.heictojpg-pics.workers.dev` (where the app points); deploying the renamed config moves the URL and kills the old hostname. Zero-break cutover: attach `license.diskgenie.app` custom domain → flip app `LICENSE_API_BASE` → deploy rename. Keep `"workers_dev": true` while on the subdomain (error 1042 gotcha, wrangler.jsonc:15-22, README §2).

Self-healing schema: `ensureSchema()` runs on EVERY request (per-isolate latch) applying migration-0003's additive columns verbatim if missing (src/schema.ts:50-66).

Compatibility date 2025-09-01 (wrangler.jsonc:13). Observability enabled (wrangler.jsonc:44-47).

### 2.2 Routes

App routes (HMAC + UA pinned, index.ts:61-117):
- `POST /v1/activate` (routes/activate.ts) — bind slot + mint token
- `POST /v1/validate` (routes/validate.ts) — 24 h recheck + fresh token (full claim, COALESCE writes)
- `POST /v1/deactivate` (routes/deactivate.ts) — idempotent slot release (unknown key/device still `{ok:true}`)
- `POST /v1/verify` (index.ts:76-117) — debug: HMAC-auth verify a token, returns payload; enforces kid/aud when present (403 `BAD_AUDIENCE`, 401 `BAD_SIGNATURE`)
- `GET /v1/health` (public) → `{"ok":true,"service":"diskgenie-license","version":3,"time":...}` (index.ts:61-62)

Admin routes (bearer `Authorization: Bearer ADMIN_API_KEY`, routes/admin.ts; rate-limited BEFORE the bearer check — admin.ts:54-106):
`POST /v1/admin/keys` (generate, atomic batch + Idempotency-Key replay), `POST /v1/admin/lookup` (raw-key resolve), `GET /v1/admin/keys[?email=]`, `GET /v1/admin/keys/:id`, `GET /v1/admin/devices`, `POST /v1/admin/keys/:id/{revoke|refund|renew|transfer}`, `POST /v1/admin/devices/:id/{revoke|revive}`, `GET /v1/admin/stats`. Generate body: `{count?, tier, customerName, customerEmail, note?, days?, source?, idempotencyKey?}`; raw keys appear EXACTLY ONCE in the first response (admin.ts:152-226).

Activate flow (activate.ts:49-162): guard → normalize key (len 22) → platform ∈ {windows,macos} → `hardwareHash` `^[0-9a-f]{64}$` → sanitizeClaim (shared.ts:108-159; control chars CLEANED not dropped, ≤120 chars printable ASCII, hex64 strict, UUID36, arch whitelist x86_64/aarch64/x86/arm, ram 0..1048576, cores 0..1024) → consumeNonce (AFTER structural validation so garbage doesn't burn nonces) → rate limit → license lookup by hash → status checks (revoked/refunded 403, pending 403, expired 403) → slot bind via partial unique index (new/refresh/revive; `SlotTakenError` → 409 DEVICE_SLOT_TAKEN) → mintToken → audit → 200 entitlement.

Validate flow (validate.ts:42-114): same guard/claim; refuses on license status/expiry; device must be the bound one (else 403 `DEVICE_MISMATCH`, incl. revoked device row); refreshes facts COALESCE-only (THE WIPE FIX) + change-detection audit; mints fresh token.

Rate limits (shared.ts:161-205): `ACTIVATE_RATE` {key 10/h, ip 30/h}, `VALIDATE_RATE` {key 60/h, env `RATE_VALIDATE_KEY_PER_HR`}, `DEACTIVATE_RATE` {key 10/h}; fixed-window D1 atomic upsert (`rate_buckets`, db.ts:576-589). Admin: 120/h per route-shape + 240/h per IP (admin.ts:68-70; vars `RATE_ADMIN_PER_HR`, `RATE_ADMIN_IP_PER_HR`).

Wrangler vars (wrangler.jsonc:48-63): `TOKEN_TTL_DAYS: "14"`, `RATE_ACTIVATE_KEY_PER_HR: "10"`, `RATE_ADMIN_PER_HR: "120"`, `RATE_ADMIN_IP_PER_HR: "240"`.

Secrets (README §5, wrangler.jsonc:5-8; never committed):
- `LICENSE_SIGNING_PRIVATE_KEY` — 64-hex Ed25519 seed (generate via `node scripts/generate-keypair.mjs`)
- `ADMIN_API_KEY` — random 32+ chars (generate-secret.mjs)
- `CLIENT_REQUEST_SECRET` — 64-hex HMAC secret — "MUST match the app's `CLIENT_SECRET_HEX`" (README.md:139)
The PRODUCTION seed/private key value is NOT FOUND in either repo (by design — Worker secret only). Test fixtures: `test/constants.ts` `TEST_SIGNING_SEED = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60"`, `TEST_ADMIN_KEY = "test-admin-key-0123456789abcdef"`, `TEST_CLIENT_SECRET = "a3f7c2d1e4b5a6978f0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a"` (constants.ts:11-15) — injected via vitest miniflare bindings (vitest.config.ts:20-25).

### 2.3 D1 schema (KV/D1 storage)

Tables (migrations 0001-0003; `schema.sql` mirror):
- `licenses`: id, `key_hash` UNIQUE (sha256 hex of normalized key), `key_last4`, tier CHECK('yearly','lifetime'), status CHECK('active','revoked','refunded','pending') DEFAULT 'active', customer_name, customer_email, note, source (v2: 'admin'|'webhook'|'seed'|'idem:<sha>'), issued_at, expires_at (NULL=lifetime), created_at, updated_at
- `devices`: license_id FK, platform CHECK('windows','macos'), `hardware_hash` (64 hex), hostname, os_version, app_version, comp_machine/comp_volume/comp_cpu (v2), cpu_brand, ram_mb, machine_model (v2), baseboard_serial, firmware_uuid, bios_version, cpu_cores, arch, comp_board, comp_firmware, facts_v3 (v3), activated_at, last_seen_at, revoked (0/1)
  - **`idx_devices_live_slot`** partial unique index `ON devices(license_id, platform) WHERE revoked = 0` (migration 0002:45-47) — the race-proof 1 Windows + 1 macOS slot rule
- `audit_events`: license_id, key_last4, event (activate|validate|deactivate|denied|admin|replay|device_update), platform, hw_prefix (first 12 hex only), reason, ip_hash (salted: `sha256(sha256(seed+":ip-salt:v2") + ":" + ip)`, crypto.ts:128-152), detail (JSON old→new diff), created_at
- `nonce_seen`: nonce PK, seen_at (replay defense; swept >600 s)
- `rate_buckets`: bucket_key PK, window_start, count (fixed window)

Device slots: 1 Windows + 1 macOS per key (owner decision). Same hardware always re-activates freely (refresh path); support-reset row re-registers via atomic `reviveDevice` (NOT EXISTS guard, db.ts:447-464).

Error codes (README §7): `KEY_NOT_FOUND, KEY_REVOKED, KEY_REFUNDED, KEY_PENDING, LICENSE_EXPIRED, DEVICE_MISMATCH, DEVICE_SLOT_TAKEN, REPLAYED, BAD_SIGNATURE, RATE_LIMITED (429), BAD_UA, BAD_AUDIENCE (403), SERVER_ERROR, BAD_REQUEST, NOT_FOUND, GEN_FAILED`.

### 2.4 Token issuance details

`mintToken` (tokens.ts:54-88): payload `{iss:"db-license", ver:1, kid:1, aud:"diskgenie", jti:randomHex(16), iat:now, exp:now+tokenTtlSeconds, key:keyHash, tier, name, email, hw, plat, lexp}`; `tokenTtlSeconds` = `TOKEN_TTL_DAYS` (default 14, clamp 1..90) × 86400 (tokens.ts:47-51). `TOKEN_AUDIENCE = "diskgenie"` (tokens.ts:13), `CURRENT_KID = 1` (tokens.ts:19).

Testing: 72 vitest tests on REAL workerd + D1 via `@cloudflare/vitest-pool-workers` (README §10, vitest.config.ts).

---

## 3. Workflows (diskgenie-v2/.github/workflows/ — all 6 read)

| File | name | Trigger | Jobs (runner) | Artifacts (exact names) |
|---|---|---|---|---|
| `ci.yml` | CI | push branches [main], pull_request, workflow_dispatch; concurrency `ci-<ref>` | `gates` (windows-latest; fmt, clippy --workspace -D warnings, cargo test --workspace, 1M-node bench `cargo test --release -p diskgenie-core -- --ignored bench_1m`, npm ci + typecheck + vitest, version sync via `node scripts/check-version-sync.mjs`, R7.1 no-direct-delete grep, R2.2 no-TODO grep, frontend build), `core-mac` (matrix [macos-latest, macos-14]; core+app clippy/tests, platform tests, bench gate), `benchmarks` (windows-latest; criterion `cargo bench -p diskgenie-core --bench suite`, dupes engine bench env `DB_BENCH_GB=2`), `license-e2e` (windows-latest; guarded on secret `LIVE_ADMIN_KEY` → `cargo test --manifest-path src-tauri/Cargo.toml -p diskgenie --features live-license-e2e --test live_license -- --nocapture` with env `DISKGENIE_LIVE_LICENSE_E2E=1`; + health check against `https://diskbytes-license.heictojpg-pics.workers.dev/v1/health` ci.yml:271), `bundle` (windows-latest, needs gates; `npx tauri build --target x86_64-pc-windows-msvc`, icon fanout `scripts/make_icon.ps1`, portable zip + SHA256SUMS into `dist-release`) | `criterion-windows` (path src-tauri/target/criterion); `diskgenie-windows-release` (nsis/*.exe + dist-release/*) |
| `ui-screenshots.yml` | UI Screenshots | workflow_dispatch, push [main] | `screenshots-windows` (windows-latest): build `npx tauri build --no-bundle --features ci-license-sim`; stage ~15 GB balanced synthetic tree under `C:\DiskGenieTest` (fsutil sparse files); `Set-DisplayResolution 1920x1080`; launch exe with env `DISKGENIE_SCAN=C:\DiskGenieTest`, `DISKGENIE_MODE=Folders`, `DISKGENIE_TOUR=1`, `DISKGENIE_POSTHOG_KEY=''`, `DISKGENIE_LICENSE_SIM=1`; TITLE-SYNCED capture (poll `MainWindowTitle` for `· tNN-name`, 1300 ms settle, one frame per step, dedup set, `· tour-done` ends; cadence fallback step-NN.png × 48) | `diskgenie-ui-screenshots` (${{ runner.temp }}/shots, retention 14 d, if-no-files-found: error); `diskgenie-app-logs` (app-stdout.log + app-stderr.log) |
| `macos-screenshots.yml` | macOS Screenshots | workflow_dispatch, push [main] | `ui-audit-macos` (macos-latest): build `npx tauri build --no-bundle --features ci-license-sim`; `scripts/ci/mac-stage-tree.sh`; brew displayplacer (1920×1080 bump); run via `scripts/ci/mac-capture.sh` with `SYNC_TITLE=1 FRAMES=44 BOOT_WAIT_S=5` and `LAUNCH_ENV="DISKGENIE_SCAN=$HOME/diskgenie-test DISKGENIE_TOUR=1 DISKGENIE_MODE=Folders DISKGENIE_LICENSE_SIM=1 DISKGENIE_WINDOW=1440x860"` | `diskgenie-macos-screenshots` (${{ runner.temp }}/shots, retention 14 d, error) |
| `macos-build.yml` | macOS Build | workflow_dispatch, push [main] | `macos` (macos-latest): `npx tauri build` (.app + .dmg); bundle smoke: launch the .app binary via `mac-capture.sh` (`FRAMES=6 CADENCE_MS=3000 FULL_CONTEXT=1`, `LAUNCH_ENV="DISKGENIE_SCAN=$HOME/diskgenie-test DISKGENIE_WINDOW=1440x860"`), quit via osascript | `diskgenie-macos-screenshots` (shots, warn); `diskgenie-macos-dmg` (bundle/dmg/*.dmg, retention 14 d, warn) |
| `macos-benchmark.yml` | macOS Benchmark | workflow_dispatch + schedule cron `"17 6 * * 1"` (weekly Mon 06:17 UTC) | `benchmark-macos` (macos-latest): ci-license-sim build; stage tree; machine profile md; `RUN_SECONDS=96` + `LAUNCH_ENV="... DISKGENIE_TOUR=1 DISKGENIE_TOUR_MODE=bench DISKGENIE_LICENSE_SIM=1"` via `scripts/ci/mac-benchmark.sh`; assembles BENCHMARK-FULL.md from `[bench]` stderr lines + memory-samples.csv | `diskgenie-macos-benchmark` (BENCHMARK-FULL.md, memory-samples.csv, app-stderr.log, app-stdout.log; retention 30 d, error) |
| `msix-build.yml` | MSIX Build (Microsoft Store) | workflow_dispatch (inputs `store_identity_name` default "DiskGenie", `store_publisher` default "CN=DiskGenie") + push [main] | `msix` (windows-latest): `npx tauri build --no-bundle`; version = Cargo.toml version + `.${{ github.run_number }}` (4-part MSIX); generate layout msix-layout/ (exe → `DiskGenie.exe`, Assets from `packaging/icon/icon-source.png` via System.Drawing, templated `packaging/msix/AppxManifest.xml` with `__IDENTITY_NAME__`/`__PUBLISHER__`/`__VERSION__`); pack with Windows SDK `MakeAppx.exe pack /o /v /d msix-layout /p dist-msix/DiskGenie-<ver>-x64.msix` + SHA256SUMS.txt; optional WACK (continue-on-error) — Store signs on submission, CI never signs | `diskgenie-msix-store` (dist-msix/*, error); `diskgenie-wack-report` (wack-report/, ignore) |

Build commands summary: Windows release `npx tauri build --target x86_64-pc-windows-msvc` (NSIS) / `npx tauri build --no-bundle` (+ `--features ci-license-sim` for screenshots); mac `npx tauri build`. Rust toolchain: dtolnay/rust-toolchain@stable; cache Swatinem/rust-cache@v2 with `workspaces: src-tauri`; Node 20 (ci) / 22 (other).

---

## 4. Mock/sim + agents SDK pattern

### 4.1 Frontend mock (browser dev/test)

- Gating: `src/main.tsx:10-13` — `if (!isTauri() && import.meta.env.DEV) { const { installMock } = await import("./mock/commands"); installMock(); }` — dynamic import behind `isTauri()` (checks `__TAURI_INTERNALS__`/`__TAURI__` in window, `src/lib/ipc.ts:16-18`) + Vite DEV; NEVER bundled into a Tauri production build.
- `src/lib/ipc.ts` — unified `invoke`/`listen` seam; `setMockBackend(handler)` routes commands to the mock registry; a local `mockListeners` bus for events; `emitMockEvent` for mock-driven events. License-gate interception on any command error (ipc.ts:35-44 → `state/license.ts interceptLicenseGate`).
- `src/mock/commands.ts` (61.9 KB) — implements the FULL Tauri command surface against a synthetic tree (`mock/tree.ts` MockTree + `mock/layouts.ts`), incl. a mock license flow with the same typed errors; supports Playwright/agent-browser screenshot passes in a plain browser.
- `src/mock/quickwins.test.ts` — mock unit tests.

### 4.2 CI license simulation (cargo feature)

- `src-tauri/Cargo.toml:115-121`: `[features] ci-license-sim = []` and `live-license-e2e = []` — "NEVER enabled in NSIS/MSIX/production builds".
- Seeding: `commands/license.rs:46-50` — under `#[cfg(feature = "ci-license-sim")]`, env `DISKGENIE_LICENSE_SIM=1` (checked in `license_manager()`) seeds `sim_state(now)`.
- `sim_state` (commands/license.rs:534-550): key `"DBSIM0CI0TOUR0SIM0KEY0"`, tier lifetime, name "Alex Morgan", email `alex@diskgenie.app`, `token_exp = now + 365d`, `activated_at = now - 32d`, `simulated: true`.
- Runtime flip: `license_sim_set(mode)` command (commands/license.rs:556-583) — ALWAYS registered (cfg inside `generate_handler!` is macro-territory), but the body is feature-gated: production builds compile a hard refusal ("license simulation is not available in this build"); sim builds accept `mode: "pro"` → sim_state, anything else → default (unlicensed).
- `validate_cycle` short-circuits when `state.simulated` (commands/license.rs:419); deactivate skips the network for simulated state (commands/license.rs:337).
- Workflows opt in at BUILD time: `npx tauri build --no-bundle --features ci-license-sim` + run-time env `DISKGENIE_LICENSE_SIM=1` (ui-screenshots.yml:53-56, 155; macos-screenshots.yml:52, 88; macos-benchmark.yml:45, 77).

### 4.3 Tour driver + screenshot harness (the "agents SDK" pattern)

- `src/shell/TourDriver.tsx` — React component (returns null) mounted in the shell. Queries `get_dev_hooks` (Rust command `commands/scan.rs:509-590`) for env-driven hooks: `DISKGENIE_SCAN` (auto-scan target), `DISKGENIE_MODE` (viz mode), `DISKGENIE_TOUR=1` (enable tour), `DISKGENIE_TOUR_MODE` (`"ui"` default | `"license"` | `"bench"`), `DISKGENIE_TOUR_LICENSE_KEY` (real minted key for the mac license E2E), `DISKGENIE_WINDOW` (`WxH`, parsed lib.rs:70-84).
- Sync channel: each step sets `document.title = "DiskGenie · tNN-name"` (slug, TourDriver.tsx:56-59); the native harness polls the WINDOW TITLE (Win32 `MainWindowTitle` / mac CGWindowList probe) and shoots exactly ONE named frame per step + 1300 ms settle; dedup via captured-marker set; `· tour-done` ends the loop. Also sets `window.__DB_TOUR_STATE` / `__DB_TOUR_DONE`.
- Programs: "ui" = every Explore mode, treemap color modes, all five themes, all tabs, duplicates-run (dwell ×10), license states (status card / dialog empty / key filled via `db-tour-license-key` event with key `DB-7XK2M-9QF3P-8NR4T-2VW6Y` / locked gate via `license_sim_set "unlicensed"` / restore pro), cleanup queue, responsive extremes (window-min 1280×760, fullscreen, restore 1440×860). "license" = real lifecycle (malformed → unknown → REAL activate → status → validate → unlocked). "bench" = scan B → re-scan A (flip-cache restore) → dupes.
- Capture scripts: `scripts/ci/mac-capture.sh` (env-driven: APP_BIN, OUT_DIR, FRAMES, CADENCE_MS, BOOT_WAIT_S, LAUNCH_ENV, DISPLAY_BUMP=1920x1080, SYNC_TITLE=1, SETTLE_MS=1300, FULL_CONTEXT, KEEP_RUNNING; window-exact `screencapture -l <id>`, compiled Swift CGWindowList probe, `[bench]` stdout lines) — shared by all mac workflows; Windows uses inline pwsh (System.Drawing CopyFromScreen + WScript.Shell AppActivate). `scripts/ci/mac-stage-tree.sh` stages the synthetic tree; `scripts/ci/mac-benchmark.sh` samples memory + writes BENCHMARK.md.
- Agents-SDK pattern (per master prompt §9): the mock backend + tour + title-sync capture is what lets agents (agent-browser/Playwright against the browser mock, or native harness against the release build) click through the UI and capture screenshots WITHOUT interactive automation; DiskGenie's `skills/` dir (coding-agent, skill-creator, …) and `AGENTS.md` (read PRINCIPLES.md → linked principles → codex/ worklog → quality gates before push) document the agent operating method.

---

## 5. Theme system

Two-layer attribute system on the document root (`src/theme/themes.ts:1-30`):
- `data-theme="<id>"` — the concrete palette (token block in `src/theme/tokens.css`), what the user picks
- `data-scheme="light|dark"` — the FAMILY (canvas saturation, vivid/pastel tone family, ~25 dark-family CSS rules); both attributes are ALWAYS written together (`applyThemeAttributes`, themes.ts:65-69)

Themes (`THEME_IDS`, themes.ts:33; `THEMES` meta :46-52): `light` (Light, light family — default), `dark` (Dark, dark), `ember` (Ember, dark — espresso + honey gold), `tide` (Tide, dark — marine + lagoon cyan), `blossom` (Blossom, light — blush cream + rose). THEME_LIST = picker order.

Selection/persistence: localStorage key `"diskgenie.theme"` (themes.ts:78); unknown values fall back to `light` (`validateThemeId`, :82-86); pre-mount inline script in index.html applies saved/default before React mounts (no flash); `useTheme()` (src/theme/useTheme.ts) exposes `{theme, isDark, setTheme, toggle}`; `setTheme` = View Transitions API crossfade (Chromium 111+/Safari 18+, reduced-motion fallback) + persist + native window theme sync via `getCurrentWindow().setTheme(nativeThemeFor(id))` (useTheme.ts:70-101); `toggle` flips the canonical light/dark pair only (custom theme → opposite family's canonical member, useTheme.ts:103-108); MutationObserver keeps React state synced when the tour driver flips attributes directly (useTheme.ts:63-67).

Tokens: `src/theme/tokens.css` (428 lines) — ONE source of truth; `:root` = light default; `[data-scheme="dark"]` family layer; `[data-theme="dark"|"ember"|"tide"|"blossom"]` palette blocks AFTER the scheme layer (source order wins at equal specificity). Surfaces (`--background/--panel/--card/--border/--divider/--chip/--track/--overlay`), text (`--text/--text-secondary/--text-tertiary/--on-pastel`), control affordances, brand coral (`--ink: #ff6b4a`, grad + glow), status (`--used/--free`), 9 theme-independent data pastels (`--cat-video` … `--cat-other`), 8 by-folder pastels. tests: `src/theme/themes.test.ts`.

DiskWizard note (master prompt §3): the DW theme is **cherry blossom** — borrow the two-layer token ARCHITECTURE, not the DiskGenie palettes.

---

## 6. Differences DiskWizard must respect (vs the donor)

1. **§8 mandate vs current dw-license crate (CRITICAL):** DiskWizard's existing `src-tauri/crates/dw-license` already deviates from the donor:
   - `protocol.rs:18` `LICENSE_API_BASE = "https://diskwizard-license.heictojpg-pics.workers.dev/"` (NOT deployed; donor's live base is `https://diskbytes-license.heictojpg-pics.workers.dev/`)
   - `protocol.rs:28-29` `LICENSE_PUBLIC_KEY_HEX = "2d44bbd8034e85810cccac455be9bb1bc9df839842e6a0b33ea68c1f3d15a91b"` (donor: `4e4f51ef1593…`)
   - `protocol.rs:35` `CLIENT_SECRET_HEX = "f8f479824b54c9feb52bd6ee15c0d2eb2a36aa47993a962490d9dcf2bef5340a"` (donor: `333d7117…` — must match the deployed Worker's `CLIENT_REQUEST_SECRET`)
   - headers `x-dw-*` + `x-dw-app: "diskwizard"` (donor/server: `x-db-*`, `x-db-app: "diskgenie"`) — the server guard reads ONLY `x-db-timestamp/x-db-nonce/x-db-signature`, so x-dw-* requests fail BAD_SIGNATURE
   - key prefix `DW` (donor/server: `DB` — keys minted by the DiskGenie server are DB-prefixed; a DW- key hashes to nothing → 404 KEY_NOT_FOUND)
   - `token.rs:151` requires `iss == "dw-license"` and `token.rs:160` `aud == "diskwizard"` (donor server mints `iss: "db-license"`, `aud: "diskgenie"`) — DW's verifier would reject EVERY DiskGenie-issued token
   - env override `DISKWIZARD_LICENSE_API` (donor: `DISKGENIE_LICENSE_API`)
   Per §8, until the DiskWizard server exists, ALL of these must equal the DiskGenie values (endpoint, public key, client secret, UA, header names, issuer/audience, key prefix), and all of it isolated in ONE config module (`licensing/config.rs`) for the later one-file swap. Key prefix/audience/UA belong in that same module so the later rebrand (DW-, dw-license, diskwizard) is part of the same swap.
2. **UA compat window:** the DEPLOYED worker still validates the pre-rename UA, so the client sends `"DiskBytes-License-Client/1"` (license.rs:118-126); the renamed server source accepts both values (guard.ts:25-28). DiskWizard pointed at the deployed worker must send the same OLD UA; when the owner cuts over, flip it in the same change as the base URL.
3. **Worker name cutover:** server repo's wrangler name (`diskgenie-license`) ≠ deployed name (`diskbytes-license`); do not redeploy the donor repo casually — it kills the old hostname every installed client points at (wrangler.jsonc:23-37).
4. **Product identity in protocol strings:** `"diskgenie"` appears as `x-db-app` value, `aud` claim, health `service`, response header `x-db-license-server: "diskgenie/3"` (shared.ts:18). For 1:1 porting while validated by the DiskGenie server, keep them identical; rebrand later with the endpoint swap.
5. **Posture/gating scope:** donor gates everything except the Monitor tab (commands/license.rs:5-8); DiskWizard's mandate is SMART/disk-health free, everything else gated — same mechanism, different free surface.
6. **Purchase URL / customer email domains** (`https://diskgenie.app/pricing`, `alex@diskgenie.app` sim) are product-specific — config-module residents.
7. **Theme:** borrow the token architecture + two-layer system + tour theme-step pattern, but palettes are cherry blossom per the master prompt (NOT the donor's five).
8. **Hardware fingerprint:** the composite is FROZEN on the donor server (license.rs:497-503) — reuse the exact recipe (SHA-256(MachineGuid ‖ volume serial LE ‖ CPUID)) since the deployed server's device rows and token `hw` claims already match it.

### Key-material cross-check between the two donor repos (asked by task)

- Production Ed25519 seed: **NOT FOUND** in either repo (Worker secret `LICENSE_SIGNING_PRIVATE_KEY` only; README §2 step 4-5, §5). The production public key `4e4f51ef…` exists ONLY in the client (license.rs:40-41); the server repo carries NO production public key — the pair cannot be cross-verified from source (by design).
- Production HMAC secret: **NOT FOUND** server-side (secret, README.md:139 says it MUST match the app's `CLIENT_SECRET_HEX` = `333d7117…`, license.rs:47).
- Test fixtures MATCH: `TEST_SEED_HEX` (license.rs:928) == `TEST_SIGNING_SEED` (test/constants.ts:11-12) == RFC 8032 test-vector #1 `9d61b19d…`; derived public key `d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a` pinned by the client test (license.rs:1179-1183). Minor doc bug: test/constants.ts:7-8 comments describe the derived key as "4cb5abf19e2cbb12e6c6a1b5b1a5d1c9c2b9a0f… style fixture" — stale/inaccurate prose (the code is correct); the actual derived key is d75a98….
- UA mismatch (compat window, intentional): client sends DiskBytes UA; renamed server accepts both.
- Env-var names differ from a naive port: `DISKGENIE_LICENSE_API`, `DISKGENIE_LICENSE_SIM`, `DISKGENIE_LIVE_ADMIN_KEY`, `DISKGENIE_LIVE_LICENSE_E2E` — keep the same names for 1:1 CI parity.
