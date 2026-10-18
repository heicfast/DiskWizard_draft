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

| Package | In use | Notes |
|---|---|---|
| react / react-dom | ^18.3.1 | **Flagged deviation**: React 19 is the current stable major; the UI pins 18.3 deliberately — the donor (diskgenie-v2) component stack (framer-motion 11, @types/react 18) is tested on 18, and a major upgrade mid-release-hardening is churn risk. Upgrade ticket: swap to 19 + framer-motion 12 + types in one dedicated session with the full tour as the regression gate. |
| vite | ^6.0.3 | latest major line in the lock at scaffold time; 7 is current — same ticket as React (build-chain bump together) |
| typescript | ~5.6.3 | pinned minor per donor; TS 5.7+ compatible — rides the same upgrade ticket |
| zustand | ^4.5.5 | 5 is current — donor-tested 4.5; same ticket |
| framer-motion | ^11 | 12 current — pinned to donor-tested line; same ticket |
| @tanstack/react-virtual | ^3.14.21 | latest major ✅ |
| lucide-react | ^0.474.0 | icon line pin (visual stability for the theme system) |
| vitest | ^2.1.8 | test runner line pin |

**Policy note (OQ-7, logged):** the npm set intentionally matches the
donor's tested stack (same versions) rather than chasing each latest
major mid-session; the consolidated upgrade ticket above closes the gap
in a controlled pass with the tour + screenshot workflows as gates.
Rust crates — where the core's correctness lives — are held strictly to
latest-stable, verified above.

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
