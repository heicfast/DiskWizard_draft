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

- **QA-1. The ui-screenshots tour still validates only the SIM core.**
  CORE-PLAN §7.5 requires a REAL-core tour step (Windows runner, VHD
  target, walk recover→scan→results with the production build, VLM
  decode the frames). Evidence: `ui-screenshots.yml:51` builds
  exclusively with `--features sim`. The real funnel (device rows from
  `list_devices` → `start_scan` on the runner's VHD → live progress →
  results → `preview_file` → `start_recovery` to a folder →
  byte-compare) is currently exercised only by the dw-win integration
  test, not through the UI. **Owner**: add a `real-core-tour` job
  (build WITHOUT sim + a seeded VHD; assert named frames via the
  existing title channel).

- **QA-2. Preview payload size cap vs the research contract.** The
  real `preview_file` caps at 16 MiB input / 2 MiB payload
  (`recover.rs` + the UI's `PREVIEW_MAX_BYTES = 2_000_000`). The
  research's preview subsystem caps at **100 MB** with a 15-second
  video-preview window (`artifacts/UIUX/10-preview-subsystem.md`).
  The 2 MiB cap is fine for images/text but truncates medium PDFs and
  denies any video/audio preview. **Owner**: decide the v1 contract
  (the UI's honest "plays after recovery" copy currently covers it)
  and raise the cap when the video preview surface lands.

- **QA-3. ext2/3, HFS+, ReFS: detected but no engine.** Product spec
  ("Restore data from all file systems… ext2/ext3/HFS+/ReFS") vs
  `fs::create_engine` → `CoreError::Unsupported` (M2 scope per
  CORE-PLAN §8). Release-blocking for the "all file systems" claim —
  the marketing surface must either gate these or M2 must land.

### P1 — research-parity gaps

- **QA-4. Deleted-viability bitmap scoring (NTFS) uses cluster-start
  sectors only.** The research (`SearchDLL/ntfs` digest) tests EVERY
  data-run cluster against $Bitmap (`sector = cluster × spc`,
  `bitmap.test(sector)`), destroying candidates only when
  `intact == total`. Our `dw-core` viability path grades via extent
  readability + the deep pass, not the $Bitmap cross-check. Impact:
  overwritten-but-listed files may surface as recoverable. **Owner**:
  add the $Bitmap build + per-cluster test to the NTFS deep pass
  (research: `CNTFSBuildVolumeBitmap`).

- **QA-5. exFAT deleted-detection relies on the InUse-bit triple.**
  The research's primary deleted-exFAT path is the **backward triple
  walk** (stream dentry − 0x40 strides into three list builders) plus
  FAT-chain re-derivation; carving keys `0xE2FF..0xE7FF` are the
  fallback. Our engine implements the InUse triple (good) but the
  chain re-derivation hint path ("when FAT damaged") should be
  verified against a damaged-FAT image test (none exists in the
  suite).

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

- **QA-8. Live-volume consistency (VSS) unimplemented and
  undisclosed.** The owner's ADR-1 review point 8: "prefer VSS
  snapshots or clearly disclose live-scan consistency limits" for
  mounted/system volumes. No VSS code exists; the UI copy does not
  disclose the live-scan caveat. Minimum viable: a notice in the
  results header when the source volume is mounted+writable.

- **QA-9. SD Card Rescue is a stub message.** `sdr_diagnose` returns
  "diagnostics complete… deep repair pass arrives with the next
  engine update" — the research's SDR contract is access-restore
  THEN recovery (`CSDCardDeviceListPage` flow). The UI's SDR strip +
  tools entry exist; the core's RawFixer-lite (DBR/backup-DBR restore
  per the `PartitionRecovery/rawfixer` artifact) is M2. Users on
  today's build get recovery, not repair.

- **QA-10. SMART threshold table is partial.** `threshold_and_status`
  covers 0x05/0xC5/0xC6/0xBB/0xC7/0x0A/0x0B/0xC2/0xBE/0xE7. The
  research's DiskHealth (CrystalDiskInfo lineage) applies
  vendor-specific rules incl. raw-value heuristics (e.g., 0x05 raw
  > 0 → caution, reallocated counts by vendor, temperature via
  0xC2/0xBE + sensor arrays). Flag: expand the table + unit tests per
  drive family once real-drive telemetry exists from CI.

- **QA-17. Scan `bytes_found` can exceed volume capacity.** The
  passing VHD run reports `bytes_found: 546,825,304` on a
  267 MB volume — logical sizes of sparse metafiles ($LogFile, $MFT
  growth) and possibly overlapping carve extents sum past physical
  capacity. The recovery itself byte-compares correctly (the test
  proves it), but the stat feeds the UI's "X found" summary. Fix:
  report `min(logical, extents-sum)` or clamp per-node sizes to the
  device capacity at tree-build time, and dedupe carve overlaps at
  merge. Evidence: VHD run on fc4e23a (`diag: report` line).

### P2 — hardening / next phase (per plan, not defects)

- **QA-11.** Parallel read pipeline (IOCP worker pool) — ADR-1 rev2
  targets adaptive queueing (HDD 1–4 MiB QD1–2 / NVMe 4–16 MiB
  QD4–16); current reads are synchronous positioned OVERLAPPED
  (correct, single-depth). M3 item.
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
  CPUID). `LICENSE_PUBLIC_KEY_HEX` embedded; `LICENSE_API_BASE` →
  `https://diskwizard-license.heictojpg-pics.workers.dev/`.
- **QA-16 (deployment)**: the live E2E (activate/validate/deactivate
  against the real Worker with a real DW- key) has not been run —
  the server must be deployed with the matching Ed25519 seed + the
  client secret from the `DISKWIZARD_LICENSE_SECRETS` handoff. Until
  then the CI license tests use fake transports only.
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
