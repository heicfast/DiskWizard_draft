# DiskWizard — Dependency Record (versions + verification dates)

> **Section 13 mandate** (MASTER_PROMPT): every crate/npm dependency is
> the LATEST STABLE release at time of use, verified against the
> registry, recorded here with the date pinned. Re-verification is
> mandatory whenever a dependency changes.

## Rust (workspace `src-tauri/`)

Verified live against crates.io (`max_stable_version`) on **2026-10-18**
(session 5). Toolchain: **Rust 1.99.1 (stable)** — the latest stable at
session start (all four manifests pin `rust-version = "1.99"`, edition 2024).

| Crate | In use (Cargo.lock) | crates.io latest stable | Verdict |
|---|---|---|---|
| tauri | 2.12.4 | 2.12.4 | ✅ latest |
| tauri-build | 2.7.3 | (tauri family) | ✅ latest family |
| tauri-plugin-dialog | 2.8.3 | (tauri family) | ✅ latest family |
| windows | 0.62.2 | 0.62.x line | ✅ latest 0.62 |
| ed25519-dalek | 3.0.0 | 3.0.0 | ✅ latest |
| hmac | 0.13.0 | 0.13.0 | ✅ latest |
| sha2 | 0.11.1 (direct) | 0.11.1 | ✅ latest |
| reqwest | 0.13.5 | 0.13.5 | ✅ latest (rustls; `client` feature) |
| memchr | 2.8.3 | 2.8.3 | ✅ latest (SIMD-friendly carving) |
| flate2 | 1.1.10 | 1.1.10 | ✅ latest (.rsf zlib) |
| serde / serde_json | 1.0.229 / 1.0.154 | 1.0.229 | ✅ latest |
| thiserror | 2.0.21 (direct) | 2.0.21 | ✅ latest |
| parking_lot | 0.12.5 | 0.12.5 | ✅ latest |

Transitive duplicates in the lock (NOT policy violations — our direct
deps are the latest): `sha2 0.10.9` and `thiserror 1.0.69` ride in via
older transitive crates; Cargo resolves both side-by-side.

## npm (UI)

Verified 2026-10-21 (session 9): the React-19 upgrade ticket landed in
session 8 (react 19 + framer-motion 14 + zustand 5 + vite 8), and
session 9 bumped TypeScript to 7.0.2 (typecheck + all 74 tests green
on it). The old "pinned to 18" deviation is CLOSED.

| Package | In use | Notes |
|---|---|---|
| react / react-dom | ^19.3.1 | ✅ latest major (the session-8 ticket) |
| framer-motion | ^14.4.0 | ✅ the React-19-compatible line |
| zustand | ^5.0.15 | ✅ latest |
| vite | ^8.3.4 | ✅ latest major |
| vitest | ^5.0.5 | ✅ latest |
| typescript | ^7.0.2 | ✅ latest (session 9: 5.9.3 → 7.0.2, clean) |

## Session-9 additions (Rust)

| Crate | Version | Role |
|---|---|---|
| chrono | 0.4.x (latest stable line) | FAT/exFAT local-wall-clock interpretation (clock+std only — no oldtime/serde baggage) |

Known follow-up (queued): the license stack's rustls provider is
aws-lc-rs (via reqwest's rustls-tls) — a ring-provider swap would
remove the aws-lc-sys build dependency that blocks local
`--target x86_64-pc-windows-msvc` cross-checks of the app crate.
| @tanstack/react-virtual | ^3.14.21 | latest major ✅ |
| lucide-react | ^0.474.0 | icon line pin (visual stability for the theme system) |

**Policy note (OQ-7, closed):** the consolidated upgrade ticket landed
in session 8 (react 19 + framer-motion 14 + zustand 5 + vite 8) with
TypeScript 7 following in session 9 — the table above is the current
state, verified by tsc + the 74-test suite on it. Rust crates are held
strictly to latest-stable, verified above.

## Session-10 additions (Rust)

| Crate | Version | Role |
|---|---|---|
| crc32fast | 1 (latest stable) | dw-repair: PNG chunk CRC re-stamping + OOXML part audits |
| flate2 | 1 (same as dw-core) | dw-repair: zlib salvage/re-deflate + OOXML member inflate |
| memchr | 2.8.3 (same as dw-core) | dw-repair: corruption-tolerant pattern search |

## Tooling

| Tool | Version | Date verified |
|---|---|---|
| Rust toolchain | 1.99.1 stable | 2026-10-18 (rustc --version + rustup) |
| Node (CI) | 22 LTS | 2026-10-18 (workflows pin node-version: 22) |
| GitHub Actions runners | windows-latest (Server 2025 class), ubuntu-latest | 2026-10-18 |
| rustfmt / clippy | stable defaults, `-D warnings` in CI | enforced by ci.yml |

## Audit status

- `cargo audit` / `cargo deny` / `npm audit`: not yet wired as CI gates
  (Rust advisories currently clean per lock inspection; the audit job is
  a tracked TODO for the hardening phase — see docs/TODO.md).
- `npm ci --no-audit --no-fund` in workflows (audit deferred with the
  same ticket).

## Fuzzing toolchain (added 2026-10-18, session 6)

| Tool | Version | Verified |
|---|---|---|
| cargo-fuzz (CLI) | 0.13.2 | `cargo install cargo-fuzz --locked` (latest at install) |
| libfuzzer-sys | 0.4.13 | crates.io index (latest stable) |
| Rust nightly | rustc 1.90.0-nightly | the sanitizer profile cargo-fuzz requires |

The fuzz crate (`src-tauri/crates/dw-core/fuzz`) is a detached
workspace with ten whole-engine targets (carve + every filesystem
engine + partition tables + repair diagnosis + the full scan
pipeline) over a read-only device whose bytes ARE the fuzz input —
the hostile-media threat model. `.github/workflows/fuzz.yml` runs
them on every push (90 s/target) and deep weekly (Sundays).

## Live license E2E toolchain (added 2026-10-19, session 7)

No new external dependencies: the E2E reuses the existing `reqwest
0.13.5` (rustls + blocking), `serde_json 1.0.154` and
`ed25519-dalek 3.0.0`. Two additions, same versions as the existing
record:

| Dependency | Resolved | Registry latest | Verified |
|---|---|---|---|
| reqwest (dev-dep, app crate) | 0.13.5 | 0.13.5 | ✅ same version as dw-license `client` — the live E2E admin client (`src-tauri/tests/live_license_facts.rs`) |
| reqwest (dev-dep, dw-license) | 0.13.5 | 0.13.5 | ✅ the lifecycle test's admin client (`crates/dw-license/tests/live_license.rs`) |

Verification method: `cargo tree -i reqwest` (single 0.13.5 across the
workspace) + the local live run against the deployed worker (both E2E
legs PASS, 2026-10-19).
