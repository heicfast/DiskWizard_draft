# DiskWizard QA Register — Independent Verification & Validation

> **Source**: the QA/verification session (2026-10-18) over the
> core-interface work. Method: full research-corpus digestion
> (SearchDLL + MOM + PartitionRecovery + SaveScan + UIUX + Licensing +
> DiskHealth + config artifacts digests), line-level code review of
> `dw-core` / `dw-win` / the app command layer / the frontend wiring,
> crates.io version verification, windows-rs 0.62 symbol-by-symbol
> cross-check against the crate source, and live Windows-runner
> evidence (VHD integration diagnostics).
>
> Every flag carries evidence (file:line, log lines, or artifact
> citation). Items are P0 (blocks release truth) / P1 (contract or
> research parity) / P2 (polish/next-phase).

## 1. Verified FIXED this session (do not re-fix — regression tests exist)

| # | Issue | Evidence → fix |
|---|---|---|
| F1 | windows-rs 0.62 API paths written against an imagined layout (`Storage::Ioctl`, `Threading::GetTokenInformation`, `SystemServices::GlobalMemoryStatusEx`, `Memory::LocalFree`) — every Windows build failed | CI logs run 39110180694/39110180756; fixed in 15a725a + 7eb0324; symbol-verified against windows-0.62.2 source (`System::Ioctl`, `Security::GetTokenInformation`, `Foundation::LocalFree`, `SystemInformation::GlobalMemoryStatusEx/MEMORYSTATUSEX/GetSystemFirmwareTable`, `Storage::IscsiDisc::ATA_PASS_THROUGH_EX`) |
| F2 | NVMe health-log offsets hallucinated (power cycles@48, hours@64, percentage-used@336 — the real page has them at 112/128/5) | Old `smart.rs:294-319` vs `NVME_HEALTH_INFO_LOG` in windows-0.62 `Storage::Nvme`; fixed via the struct read (`9cb4194` line-level review) |
| F3 | ATA SMART task-file register layout shifted by one (0x4F/0xC2 at wrong indices), `ATA_PASS_THROUGH_EX.Length` unset | Fixed in the same rewrite; layout now `[0xD0,0,0,0x4F,0xC2,0xA0\|0x40,0xB0,0]`, `Length=40` |
| F4 | Stale OVERLAPPED event reuse — pooled manual-reset events never reset; queued reads could observe a stale signaled state (partial reads/`ERROR_IO_INCOMPLETE`) | `blockdev.rs` checkout had no `ResetEvent`; fixed in 7eb0324 (MSDN OVERLAPPED contract: the event must be non-signaled when the read begins) |
| F5 | `AppState` managed as `Arc<AppState>` while commands request `State<AppState>` — Tauri's state map is exact-type keyed; EVERY command would panic at first invoke | `real.rs::shared_state` before the fix; fixed by the core agent ("AppState wiring", eb785af) |
| F6 | `start_recovery` worker reused the SCAN's control token — a previously stopped scan (cancelled token) would abort every subsequent recovery instantly | `recover.rs:175` before fix; fresh `ScanControl::new()` in 7eb0324 |
| F7 | `scan_units` parsed a partition table out of the VOLUME's NTFS boot sector — bootstrap code read as phantom MBR entries; the filesystem never scanned (23 s carve-only, 0 files) | VHD run 39117038656 log (`present.txt found by the scan` panic); probe-before-layout fix in c5c217a + regression tests |
| F8 | NTFS `$MFT` addressing: BPB 0x30 is an LCN but the engine multiplied by `bytes_per_sector` — 8× off on 4 KiB-cluster volumes; spc=1 fixtures masked it | VHD diag: `boot ok (spc=8, mft_lba=21754)` but `mft0 head = [00×64]`; fixed in 2f4ae63 (cluster_size scaling + spc=8 regression test) |
| F9 | `get_scan_progress` served zeros (percent/files/sectors) while events progressed | Owner review item; live snapshot mirror landed in the ADR-1 rev2 + 7eb0324 polish |
| F10 | `rust-version = "1.80"` in all four manifests (owner directive: latest stable only) | Now 1.99 + edition 2024 (15a725a) |
| F11 | VHD workflow PowerShell here-strings (indented `"@` never terminates) + no-op `"flush" \| Out-Null` + wrong libtest filter order | Fixed in 7eb0324 (array-join scripts, `Write-VolumeCache`, `-- --ignored … filters`) |
| F12 | `read_bytes` unresolved in `recover.rs` (missing `BlockRead` import); `f64::from(u64)`/`usize::from(u32)` type errors; `device_id: &str` escaping into the `'static` worker | CI check job run 39117038665; fixed in 7eb0324 |
| F13 | `probe_capacity(physical)` read a hallucinated `DISK_GEOMETRY_EX` layout — MediaType (12) parsed as sector size, DiskSize at +40 instead of +24; every whole-physical-drive open carried garbage geometry (volume path was correct, masking it) | The new IOCP runner test panicked `sector size 12`; fixed in 73e3d86 with the typed `read_unaligned` struct read; the iocp integration test asserts sector size + physical-path reads as the regression |
| F14 | ext rev-1 superblock field block parsed +4 bytes off (missing `s_def_resuid/resgid` 0x50/0x52): rev_level read at 0x50, inode_size at 0x5C, compat at 0x60, incompat at 0x64, volume_name at 0x7C — real mkfs images (inode size 256) had every inode read land mid-slot → zero entries; hand-built fixtures encoded the same offsets (shared-hallucination trap) | The ext2 real-mkfs CI gate failed its first completed run; byte-verified against e2fsprogs 1.47.4 output (mkfs.ext2 + debugfs + dumpe2fs ground truth); fixed in d71570d (engine + `probe_filesystem` ext3/ext4 flag discrimination + fixture true offsets + s_journal_inum 0xE0); regressions: embedded-real-superblock byte-truth test + probe discrimination test |
| F15 | Fuzz harness panicked on usize add-overflow (`fuzz/src/lib.rs:33`) when fuzzed structures drove an engine to request near-2^64 LBAs — the harness must reject, never abort | CI fuzz run 39137841955 (ntfs target, `libFuzzer: deadly signal`); fixed with checked u64 math + typed `ReadFailed` in 74d599a |
| F16 | `iocp_batch` integration test raced `vhd_end_to_end` in parallel test threads — the mounted NTFS's own metadata writes ($MFT/$LogFile) legitimately changed window 0 between the sequential reference and the batch read (diverged with ZERO bad sectors; passed untouched on the next run) | Runner evidence: panicked 14:18:11 while the VHD test finished 14:18:38; fixed with the `VHD_LOCK` serialization + first-divergence-offset assert diagnostics in 74d599a |
| F17 | Real-Core Tour silent skips: the driver skipped recovery on empty selection/destination, burned the 180 s wait, stamped `recovery-complete` anyway; the app's `beginRecovery` ALSO swallowed `start_recovery` errors (a real user would stare at "Preparing…" forever — same defect class) | Tour run 39135278286: all 33 frames captured incl. t30-recovery-complete, then `RECOVERY_OUTPUT_MISSING`; fixed in 74d599a — t99-tour-fail-<reason> loud frames + stage precondition asserts + workflow TOUR_FAIL detection + honest error summary (`Recovery Could Not Start` variant over the backend's typed error) |

**Verified correct by direct evidence** (no action needed):
`bitlocker.rs` exit codes compare on the `cast_unsigned()` view with
hex HRESULT constants (the i32-wrap bug is fixed); no
TODO/FIXME/stub markers in shipped Rust; the UI is AI/robot-free (the
only "assistant" string is the deliberate AI-free note in
`HelpPage.tsx:3`); `smart_report` carries no license gate (disk health
is free per product requirements); crate versions are the actual
latest stable on crates.io (ed25519-dalek 3.0.0, hmac 0.13.0, sha2
0.11.1, reqwest 0.13.5, windows 0.62.2 — verified live); README v0.2
status matches the shipped state (real core default, sim never in
release).

## 2. Open flags for the agents

### P0 — release-truth gaps

- **QA-1. The ui-screenshots tour still validates only the SIM core.
  RESOLVED IN CODE (session 5c); completing run pending.**
  `real-core-tour.yml` now builds WITHOUT sim (only the narrow
  `ci-tour` license-seed feature — DiskGenie's refusal-arm pattern),
  seeds a VHD (kept 1 MiB + deleted 64 KiB deterministic patterns),
  walks the REAL funnel through the UI (dual-mode TourDriver: live
  device row → gate → seeded Pro → real scan → results → REAL recovery
  funnel with new recovery-* frames), asserts key frame names, then
  byte-compares BOTH seeds against the recovered output. First runs
  were cancelled mid-flight by the concurrent agent-push churn (the
  concurrency group cancels in-progress runs — 6 cancellations); the
  run on the quiet repo completes it. `workflow_dispatch` re-runs on
  demand. The flag closes when a completing run posts green.

- **QA-2. Preview payload size cap vs the research contract. OPEN
  (owner call — OQ-5).** The real `preview_file` caps at 16 MiB input
  / 2 MiB payload; the research's preview subsystem caps at **100 MB**
  with a 15-second video-preview window
  (`artifacts/UIUX/10-preview-subsystem.md`). The UI's honest "plays
  after recovery" copy covers video. Raise the caps when the
  video-preview surface lands.

- **QA-3. ext2/3, HFS+, ReFS engines. LANDED (session-5 engine
  batch); fixture coverage in progress.** `fs::create_engine` serves
  ext/HFS+/HFSX/ReFS (see the consistency register §1 item 9 for the
  research cross-check). The ext2 real-mkfs fixture gate runs in CI
  (mkfs.ext2 + debugfs + byte-truth recovery); HFS+ (hfsprogs) and the
  ReFS runner leg (`Format-Volume -FileSystem ReFS` on Server images)
  are the remaining fixture legs before the "all file systems" claim
  is fully evidence-backed.

### P1 — research-parity gaps

- **QA-4. Deleted-viability bitmap scoring (NTFS). LANDED (session
  5, commit 5911705): the $Bitmap per-cluster scoring + the research
  destroy rule** (`intact == total → destroy`, files ≥ 0x3FF only) per
  `ntfs-engine-algorithms-v2--deleted-viability.md`. CI-green gate at
  push. Regrade if the fixture corpus grows damaged-bitmap cases.

- **QA-5. exFAT deleted-detection. LANDED (session 5, commit b8eb224):
  the chain-resolution ladder** (NoFatChain / FAT walk / damaged-FAT
  re-derivation) per the research's backward-triple walk + carving
  fallback. CI-green gate at push.

- **QA-6. Carve-result merge (`structured wins`) lacks a test
  fixture proving the ~65% essential-mode reduction.** The product
  notes claim "filtering duplicate and nonessential results reduced
  files to review by about 65% in testing." Evidence needed: an
  integration fixture where the same file exists as a structured NTFS
  node AND a carved signature hit, asserting the merge collapses
  them. (The unit suite tests the components; the end-to-end claim is
  unproven.)

- **QA-7. `.rsf` record model is a 3-record simplification.** ADR-4
  permits "same architecture, not byte-identical," but the research's
  record 3 (per-partition context: raw 0x6F5 partition struct +
  0x1518 volume-device blob incl. the boot-sector snapshot) is
  absent, so a resumed session cannot re-derive its partition
  context (matters for whole-disk scans with partition prefixes).
  Flag for M2 when multi-partition sessions persist.

- **QA-8. Live-volume consistency disclosure. FIXED (session 5c,
  commit 653a1ce — the "or clearly disclose" arm of §5.1 item 8):**
  the core computes `source_volume_live` (mounted-volume targets)
  and carries it on `scan-done`; the results header renders the
  live-scan note ("Live scan: files changed during the scan may not
  be captured"); both mocks emit the flag for tour coverage. VSS
  snapshots remain an optional M3 escalation — the disclosure arm
  satisfies the directive.

- **QA-9. SD Card Rescue. LANDED (session 5, commit d644c85): the
  production access-restore flow** — `sdr_diagnose` real diagnosis +
  confirm-gated `sdr_repair` with verify-or-rollback, the RawFixer
  model in dw-core (boot diagnosis, FS-specific backups, exFAT
  checksum gate, FixFromDBR/FixByScanSector plans with CopySectors
  undo snapshots) and the write-enabled handle in dw-win. Per the
  research's SDR contract (access-restore THEN recovery). The
  damaged-DBR CI fixture (corrupt sector 0 → restore → verify)
  remains the evidence leg.

- **QA-10. SMART threshold table is partial.** `threshold_and_status`
  covers 0x05/0xC5/0xC6/0xBB/0xC7/0x0A/0x0B/0xC2/0xBE/0xE7. The
  research's DiskHealth (CrystalDiskInfo lineage) applies
  vendor-specific rules incl. raw-value heuristics (e.g., 0x05 raw
  > 0 → caution, reallocated counts by vendor, temperature via
  0xC2/0xBE + sensor arrays). Flag: expand the table + unit tests per
  drive family once real-drive telemetry exists from CI.

- **QA-17. Scan `bytes_found` can exceed volume capacity. FIXED
  (session 5c, commit 653a1ce):** the aggregate stat clamps to the
  scanned device's capacity on BOTH report sites (Done + Stopped) —
  `clamp_report_bytes`, regression test
  `qa17_report_bytes_never_exceed_capacity`. Per-file sizes stay
  honest; the carve-overlap dedup landed earlier with the dedup
  batch (content-key absorb). Evidence: the original VHD run on
  fc4e23a (`bytes_found: 546,825,304` on a 267 MB volume).

### P2 — hardening / next phase (per plan, not defects)

- **QA-11. Parallel read pipeline (IOCP). LANDED (session 5, commit
  0f2d98e): the §5.1 item-3+5 completion pipeline** — IOCP batch
  reads + adaptive media profiles; the benchmark harness +
  `docs/benchmarks/io-backend.md` record landed with it (a003342,
  §5.1 item 10). **Session 6 update: real-runner numbers recorded**
  (d71570d run 39137841845 — 8-profile sweep, steady ~370–430 MiB/s
  through the runner's cache-backed VHD stack, flat 9.7 MiB working
  set, CPU ≤ 0.0001 s/MiB; batch≡sequential byte-identity asserted
  on-device). Soak/stress regrade at M3 close.
- **QA-12.** 24 h soak + crash-resilience scan journal (M3 per
  CORE-PLAN).
- **QA-13.** BitLocker raw XTS-AES stream recovery for
  damaged-metadata volumes (M3; ADR-5).
- **QA-14.** Cloud recovery OAuth2 (M3; `list_cloud_drives`/`cloud_sign_in`
  return the typed NOT_AVAILABLE markers correctly today).
- **QA-15.** File repair phase (photo/doc/video) — the owner's
  explicit next priority after core production-readiness; tracked in
  `docs/TODO.md`.

## 3. Licensing — verified + one deployment flag

- Architecture verified against the DiskGenie donor + license-server
  repo: Ed25519 entitlement tokens (client-side verify vs embedded
  public key), HMAC-SHA256 request signing, postures
  (unlicensed/pro/grace 14 d/degraded), 24 h revalidation scheduler,
  DPAPI at rest, hardware fingerprint (MachineGuid ‖ volume serial ‖
  CPUID).
- **§8 fix (session 5, OQ-3)**: the client was re-pointed at the
  **DiskGenie deployment** exactly per MASTER_PROMPT §8 — endpoint
  `https://diskbytes-license.heictojpg-pics.workers.dev/`, the donor's
  Ed25519 public key + HMAC client secret, `x-db-*` auth headers, UA
  `DiskBytes-License-Client/1`, token `iss="db-license"` /
  `aud="diskgenie"`, and the `DB` key prefix (the server registry knows
  DB keys only). ALL values live in the ONE config module
  `dw-license/src/config.rs` — the future DiskWizard-server swap is a
  one-file change (the fresh DiskWizard keypair from the
  DISKWIZARD_LICENSE_SECRETS handoff is the swap target, not the
  current key).
- **QA-16 (deployment)**: the live E2E (activate/validate/deactivate
  against the real DiskGenie Worker with a real DB- key) has not been
  run — until the owner issues a DB-key for a test activation, the CI
  license tests use fake transports only.
- Free-tier boundary verified: only `smart_report`,
  `list_devices`/`refresh_devices`, settings and `get_dev_hooks` are
  ungated; every scan/recovery/session command passes
  `require_licensed` (the L6 Rust-side gate the owner specified).

## 4. Instruction-compliance audit (owner's directive)

| Directive | Status | Evidence |
|---|---|---|
| Latest stable Rust (1.99.1) | ✅ fixed | All manifests `rust-version = "1.99"`, edition 2024; local toolchain 1.99.1 |
| Latest stable crates, no fallback | ✅ | Live crates.io check (§1 F-list) |
| No TODO/mock/stub in shipped code | ✅ | Grep clean across `crates/` + `commands/real/` |
| Mock stays dev-only, never in release | ✅ | `sim` feature never in NSIS/MSIX flows; ui-screenshots opts in explicitly |
| No AI/robot/mascot/pop-ups | ✅ | UI grep clean; HelpPage documents the deliberate exclusion |
| Disk health free, everything else licensed | ✅ | Gate audit §3 |
| Worklog maintained + pushed | ✅ | `worklog.md` sessions 1–3 + core-agent entries |
| Multi-POV checks during the session | ✅ | §5 below |
| Periodic pushes, conflict-safe | ✅ | 5 reconciliation rebases with the parallel core agent this session, zero lost work |
| Real-Windows validation with logs | ✅ | VHD integration diagnostics drove F7/F8 root-cause fixes |

## 5. Multi-POV verdicts (this session's spot audit)

- **Senior dev (25y)**: the IO layer is now sound at the seam level
  (positioned OVERLAPPED, aligned staging, event hygiene, sparse
  fallback). The two engine bugs that shipped (F7/F8) were
  fixture-blindness — the spc=8 + volume-target regression tests now
  pin them. Watch: the `read_sectors_sparse` per-sector salvage path
  allocates one `read_at_offset` per sector on failure — acceptable
  for isolated damage, revisit for a 5400-rpm USB drive full of bad
  sectors (cap the salvage window).
- **Product manager**: the P0 user journey (select → scan → preview →
  recover) is end-to-end real on Windows now, with the VHD truth test
  as the gate. The "all file systems" claim is the open positioning
  risk (QA-3) — gate the wording or land M2 before store listing.
- **Security researcher**: recovery paths sanitize destinations
  (traversal guard in `FsDest::resolve`); carved names flow through
  the same join; the license client verifies tokens client-side
  (spoofed-server resistance). No secrets in logs observed. VSS
  disclosure (QA-8) is the honesty gap.
- **Production manager**: all four workflows must be green on the
  pushed HEAD before calling the wiring milestone done; the
  real-core tour (QA-1) is the missing release-evidence artifact.
- **Paying-user simulation**: unlicensed → every recovery action
  presents the activation dialog with typed server errors; SMART
  works free; the destination picker blocks the source volume; the
  stop-confirm copy matches the researched retention guardrails.

## 6. Release-evidence snapshot (the green pipeline)

On commit 6ca58c0 all four workflows are green for the first time with
the real core wired:

| Workflow | Result | Key evidence |
|---|---|---|
| CI (web + rust-core + windows-app-check + NSIS) | ✅ | NSIS real-core installer builds; 117 engine tests; dw-win msvc cross-check |
| Windows Integration (VHD end-to-end) | ✅ | `mft0 ascii = FILE0` (cluster-scaled $MFT found), 25 files, recovery byte-compares, 3/3 tests |
| MSIX Build | ✅ | Store-ready package |
| UI Screenshots (sim tour) | ✅ | 28 named frames t00..t28 + tour-complete marker |

VLM verification of the captured frames (tour artifact
`diskwizard-ui-screenshots`): the live scan state renders real counters
("66,615 files found (9.2 GB)" at 51%); the Disk Tools SMART dashboard
shows the full attribute table, temperature, "Good" verdict and drive
identity (the free-tier feature working as researched); the results page
shows the Excellent/Good/Poor recoverability chips, category tabs and
the preview panel. No AI/robot/mascot surfaces anywhere. No layout
defects or placeholder text detected.
