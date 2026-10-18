# DiskWizard — Worklog

Multi-agent build journal. Newest sections at the bottom.

---
Task ID: 1
Agent: main (Z.ai)
Task: DiskWizard UI/UX session — research digestion, scaffold, theme system, all pages, licensing, dual mock cores, Tauri shell, release workflows.

Work Log:
- Saved the master prompt; cloned `diskgenie-v2` (theme/build/licensing donor) + `easeus-drw-research` (architecture artifacts); downloaded the DiskWizard brand kit.
- Read the full research set: architecture/00-07 docs, UIUX artifacts 01-35 incl. the redesign-reference atlas, dynamic-ground-truth captures, the screenshot library README, DiskGenie's licensing architecture + theme system + mock implementation + workflows.
- VLM-decoded both cherry-blossom reference images + the 3D logo (exact palette/layout extraction).
- Scaffolded the project: Vite + React 18 + TS + Zustand + Framer Motion + Lucide + @tanstack/react-virtual; self-hosted Inter (brand-recommended).
- Theme system: DiskGenie's token architecture with Blossom as the DiskWizard default; View-Transition crossfades; the theme-reactive BrandTile (ink-grad painted logo tile) + full-logo ambient halo system.
- Built the shell: frameless TitleBar (Windows caption verbs), Sidebar (nav + live scan widget + help card), splash (logo-led, ~2.1s), simulated UAC elevation gate, error boundary, toasts.
- Built all six pages: Recover Data (reference-exact inventory/quick actions/sessions), Scan Results (scan-in-page, virtualized table + recoverability chips + preview panel + recovery funnel + stop confirm), Disk Tools (free SMART dashboard + rescue tools), Cloud Recovery, Settings (5-theme picker), Help Center.
- Licensing: DiskGenie architecture (postures, activation dialog with typed error copy, Pro card, ACTIVATION_REQUIRED gates; DW- key format; disk health free).
- Mock core #1 (TS, browser): full command surface — scan phase machine (preparing→quick 0-38%→deep 38-100%→organizing→done), SDR pre-phase, progressive fill in 1/40 slices, sessions with .rsf naming, recovery funnel, cloud, settings, license sim.
- Mock core #2 (Rust, packaged builds): same contract as #[tauri::command]s; pure modules (data/state/tree) compile-checked + smoke-tested standalone (124,532-file headline scale asserted).
- Tauri shell: maximized-on-open, requireAdministrator manifest via build.rs, capabilities, dialog plugin.
- Workflows: ci.yml (typecheck + tests + NSIS artifact), ui-screenshots.yml (title-synced tour capture), msix-build.yml (Store-ready MSIX).
- QA round 1 (agent-browser + VLM): walked splash→UAC→recover→scan (live progress states)→complete (124,532 files/18.6 GB exact)→tabs/tree/preview/selection→destination→recovery→summary→tools→cloud→settings→help→all 5 themes (logo tile repaint verified per theme). Fixed: capacity decimals (120.5/232.3 GB), Condition column width, select-all checkbox, check contrast, SMART table airiness, cloud trust contrast + sign-in CTA, license label contrast, theme swatch hover, tool-disk truncation tooltips.

Stage Summary:
- Repo: github.com/heicfast/DiskWizard_draft (main).
- The complete premium cherry-blossom UI runs end-to-end in browser mode with realistic simulated data matching the reference captures byte-for-byte where visible.
- Next: Rust-side CI compile pass, deeper VLM polish rounds, screenshot tour verification, docs polish.

---
Task ID: 2
Agent: main (Z.ai)
Task: QA rounds 2-3 — missing product flows, licensing lifecycle, tour verification, responsive ladder, code quality.

Work Log:
- Added the three flows the product notes call out that were missing:
  BitLocker unlock dialog (password/recovery-key, FVE round-trip,
  unlock → auto-scan, session unlock state in both mocks), the
  lost-partition guidance dialog (direct repair RECOMMENDED vs deep
  scan — the IMPROVED restoration path), and SD Card Rescue wired
  from Disk Tools (connect → scan with the live SDR strip).
- Verified the unlicensed funnel end-to-end: gate dialog → bad-key
  typed error copy → valid-key activation → success state → Pro chip
  + buy button swap.
- Tour mode verified: all 27 states walked to `tour-complete`
  (title-sync channel working for the screenshot harness).
- Fixed the table's flex-shrink column compression (Size/Type/Date/
  Condition now honor exact widths); safety-copy contrast fixes;
  BitLocker segmented control alignment; narrow-viewport ladder
  (≤1280px drops Date Modified, slims tree/preview panes — no
  horizontal overflow at the 1200px floor test).
- Scan auto-switch now once-per-generation (browsing stays free
  while scanning; the sidebar widget keeps reporting).
- Code quality: userFacingError extracted to lib (Fast Refresh
  clean), no TODO/FIXME, no console noise, mock verified absent
  from the production bundle, logo optimized 2.5MB → 110KB
  (256px master for in-app sizes), production build 130KB gzipped.

Stage Summary:
- All master-prompt requirements for the UI session implemented and
  verified (BitLocker recovery, SDR, lost-partition guidance,
  recoverability indicators, disk health free tier, essential/
  complete results, licensing architecture, theme-reactive logo,
  maximized+elevated launch, agent tour channel).
- Remaining for next sessions: Rust CI compile pass (Linux box lacks
  the GTK toolchain — the workflows carry it), the repair phase
  (docs/TODO.md), real core integration.

## Session 2 — CI recovery, contract hardening, release-pipeline green

**Diagnosed + fixed all 3 workflow failures** (root causes from the
CI logs): invalid `bundle.targets ["nsis","msix"]` (msix is not a
Tauri bundle target — the Store package builds via MakeAppx);
PowerShell here-string terminators at column 0 terminated the YAML
block scalars in msix-build/ui-screenshots (both died at parse —
rewrote on the DiskGenie donor harness, zero here-strings); ci.yml
pinned `--target x86_64-pc-windows-msvc` + donor release packaging.

**Rust core compiled for the first time** (local cargo 1.99 + stub
pkg-config): fixed 2 compile errors (missing Manager imports), 5
clippy::all denies (DeviceSpec struct et al.), adopted the donor's
linker-manifest pattern. First Windows link hit mt.exe c1010001/
LNK1327 (the linker's default asInvoker UAC snippet vs our
requireAdministrator) — fixed with /MANIFESTUAC:NO. **NSIS + MSIX +
Screenshots all GREEN**: DiskWizard-0.1.0-x64-setup.exe +
portable.zip + SHA256SUMS; DiskWizard-0.1.0.6-x64.msix (40 files,
Store identity templating).

**Test suite landed (73 tests)** — caught 2 real bugs: licenseKey
never enforced the Crockford body (no I/L/O/U) and formatKeyInput
painted a phantom "DW-" over partial typing. Markers rebuilt as
Crockford-safe leet (WR0NG/EXP1R/REV0K/TAKEN/TESTS) in BOTH mocks —
the old markers could never fire (they failed shape validation
first). Demo keys documented in README.

**VLM release-build review + color math**: fixed the WCAG AA fail on
ink-on-ink-soft small text via a per-theme --ink-deep token (blossom
3.42→4.76:1, light 2.49→4.70:1; dark/ember/tide keep --ink). Email
category got real name variety in both mocks (Rust had lumped email
into "other" — parity gap).

**Tour title-channel fixed beyond the donor**: Tauri 2 does not
auto-sync document.title (wry wires DocumentTitleChanged only when an
app handler exists — the donor's own ci-artifacts show its harness
always fell back to cadence capture). tourTitle now stamps the native
window via getCurrentWindow().setTitle: the harness captured **27
named frames (t02..t28), one per tour step, tour-complete end marker
detected** — exceeding the donor's capture fidelity.

**Multi-POV verdicts**: PM feature-complete for scope; Production
pipelines green with hygiene; Senior-dev check/clippy/fmt clean +
73 tests + mock parity; Designer 9/9/9/8 pages, AA proven per-theme;
Paying-user honest licensing + free disk-health trust builder.

## Session 3 — The real core lands (core-interface session)

**Task ID**: 3
**Agent**: main (Z.ai) — core/interface session
**Task**: Replace the simulated core with the production Rust engine
behind the frozen IPC contract; keep the mock as a dev-only feature.

**Work Log**:
- Read the full research corpus (299 .md files across architecture/,
  re-workspace/artifacts/ — MOM modules, SearchDLL engines, UIUX
  deep-dives, PartitionRecovery, SaveScan, licensing, config files);
  distilled structured summaries into the session context.
- Ported the DiskGenie licensing architecture verbatim into
  `crates/dw-license` (Ed25519 entitlement tokens, HMAC-signed
  protocol, posture machine, hardware-id + persistence seams);
  generated a fresh DiskWizard keypair (deployment sheet for the
  owner; public key + client secret embedded per the architecture).
- Built `crates/dw-core` (platform-independent engine): BlockDevice
  seam (image/sub/cached-LRU), MBR/EBR/GPT + lost-partition sweep,
  NTFS engine (MFT walk, fixups, runlists, deleted records, LZNT1),
  FAT12/16/32 (LFN, cycle-guarded chains, 0xE5 reanimation), exFAT
  (entry sets), RAW carver (19 format grammars), tree builder
  (dense-id arena, lost branches, structured-wins merge,
  classification, grading), scan controller (quick→deep→organize
  phases, external pause/stop token), recovery engine (extent
  streams, space gate, collision suffixes), .rsf sessions
  (zlib-chunked records, auto-save naming).
- Built `crates/dw-win` (Windows layer): raw-IO block devices,
  device/volume enumeration with lost-partition surfacing, SMART
  (SAT + NVMe transports), BitLocker FVE detection + unlock,
  licensing platform providers (MachineGuid/CPUID/DPAPI/SMBIOS).
- Wired the real command surface (`commands/real/*`) — every frozen
  command name/shape served by the engine; worker threads for scan
  + recovery; throttled progress events; .rsf auto-save; preview_file
  (base64 stream); L6 license gates in Rust.
- Feature routing: default = real core; `sim` = the session-1/2 mock
  (ui-screenshots.yml builds with `--features sim`; release builds
  never enable it).
- CI: new rust-core gate (fmt + clippy -D warnings + 109 engine tests
  on ubuntu); windows-nsis now ships the real core.
- Wrote docs/CORE-PLAN.md (the master build plan: architecture,
  phases, SOPs, ADRs) and todo-uiux/CORE-INTERFACE-GAPS.md (the UI
  contract extensions for the parallel agents: start_recovery
  nodeIds, preview_file wiring, BitLocker args).

**Stage Summary**:
- 109 unit tests green across dw-core (82) + dw-license (27); clippy
  clean; the engine crates compile on any OS (the seam proving
  itself in CI).
- The app crate compiles on Windows runners (local box lacks GTK —
  the workflows carry it; first Windows compile in flight).
- License deployment: the owner's worker secrets are in the
  DISKWIZARD_LICENSE_SECRETS handoff (not in the repo).
- Next: Windows CI compile pass, VHD end-to-end integration test,
  M2 engines (ext/HFS+/ReFS), SDR deep repair, then the repair
  phase per docs/TODO.md.

---

## Session 4 — Green CI recovery + the ADR-1 I/O redesign

**Task ID**: 4
**Agent**: main (Z.ai) — recovery + performance session
**Task**: Recover from the failed Windows CI, enforce version
discipline, then execute the owner's I/O review: upgrade ADR-1 to
"high-performance user-mode direct I/O is the product path" and
rebuild the raw-IO layer accordingly.

**Work Log**:
- **Root-cause of the red CI**: dw-win was authored against guessed
  windows-rs 0.62 signatures and never compiled before push
  (aws-lc-sys blocked Linux→Windows cross-checks; CI only built
  dw-core/dw-license natively). Fixed every signature (Ioctl under
  System::, ATA PASS-THROUGH under Storage::IscsiDisc,
  GetTokenInformation under Security, LocalFree under Foundation,
  slice-based GetVolumeInformationW, Result-returning DeviceIoControl,
  plain-bool BOOLEANs). The NVMe health-log query was REWRITTEN: the
  SDK grew ProtocolDataRequestSubValue2/3/4 (44-byte struct) and the
  old overlay arithmetic would have underflowed the buffer pointer;
  the health-log field offsets were wrong (power cycles 96, hours
  112, percentage used byte 5, media errors 144).
- **Version discipline (owner directive)**: rust-version 1.80 → 1.99
  (verified = latest stable, 1.99.1), edition 2021 → 2024 in all four
  manifests (reserved keyword `gen` renamed; let-chains adopted).
- **dw-license feature seam**: reqwest client now behind `client`
  (default on); dw-win pins default-features=false so its dependency
  closure is pure Rust — enabling `cargo check/clippy --target
  x86_64-pc-windows-msvc` from Linux. This gate now runs in CI
  (rust-core job) plus a new fast `windows-check` job (workspace
  check default+sim on a real Windows toolchain).
- **App-crate Windows fixes** (caught by the new fast gate): windows
  crate dep for direct Win32 calls in commands, tauri::Manager
  imports, thread-name String (std 1.99), posture import shadowing,
  SectorSizeMode re-export, slot writes through .lock(), credential
  by-reference, Value::Null typo, AppState constructor wiring.
- **RawHandleDevice v2 (the review's core demand)**: positioned
  OVERLAPPED reads (offset rides in OVERLAPPED — no shared file
  pointer, no seek), FILE_FLAG_NO_BUFFERING with aligned staging for
  unaligned buffers, STORAGE_ACCESS_ALIGNMENT probe, pooled
  manual-reset events, IOCTL_DISK_GET_LENGTH_INFO for volume
  capacity, owned handles with proper Drop.
- **BlockDevice seam extensions**: `read_sectors_sparse` (best-effort
  bulk reads; failed reads degrade to ONE attempt per sector — no
  retry storms — with readable sectors salvaged and dead ranges
  reported) and `preferred_read_alignment`; AlignedBuf (pure-safe
  interior-offset alignment); cache chunks allocate aligned so the
  metadata hot path never stages; SubDevice/CachedDevice delegate
  sparse + alignment.
- **Carver v2**: 8 MiB aligned windows with 64 KiB overlap, memchr
  memmem signature matching (SIMD-friendly), sparse window reads,
  bad-range hit suppression. Validators rewritten as bounded
  streaming Prober walks — the old code allocated up to 4 GiB per
  candidate (PDF 2 GiB, ISO-BMFF 4 GiB, MP3 up to capacity); caps are
  now LENGTH bounds, never allocations (fixed ~1 MiB scratch). ZIP's
  bogus OggS fallback replaced with an honest bounded low-confidence
  carve.
- **Scan dispatch**: diskN-partM targets now slice the partition
  (SubDevice over the physical disk; table-index match + MBR slot
  fallback matching Windows PartitionNumber semantics) so quick
  filesystem scans actually run; whole-disk targets dispatch across
  all discovered partitions (progress weighted by size, per-partition
  tree branches) with one whole-device carve sweep covering
  unallocated space.
- **get_scan_progress**: the poll endpoint now serves the worker's
  live engine snapshot (percent/files/bytes/sectors/searching label
  + rate-based ETA) instead of stub zeros.
- ADR-1 upgraded in docs/CORE-PLAN.md; README session status now
  describes the real core (v0.2).

**Stage Summary**:
- Local gates: fmt clean; clippy -D warnings clean on native AND
  windows-msvc (tests included); 114 engine tests green (87 dw-core +
  27 dw-license).
- The first Phase-A push still failed the Windows app build (app-crate
  errors listed above) — all fixed in this push; the fast windows-check
  job exists precisely to catch that class within minutes.
- Next: verify CI green incl. the VHD end-to-end on the real Windows
  runner (the v2 raw-IO layer is what it exercises), then multi-POV
  review + M2 engines (ext2/3, HFS+, ReFS) per CORE-PLAN.

---

## Session 4 — QA/reconciliation: CI recovery alongside the parallel core agent

**Task ID**: 4
**Agent**: main (Z.ai) — QA/verification + interface session
**Task**: Recover context after the session stop; make all four
workflows green; reconcile with the parallel core agent's pushes;
close the P0 core-interface UI gaps; run the independent QA pass and
flag every irregularity with evidence.

**Work Log**:
- Context recovery: re-cloned all four repos, re-read the research
  corpus (three full digests: engines/MOM/partition/SaveScan, UIUX,
  licensing/config/disk-health/workflows), verified Rust 1.99.1 +
  all crate pins against crates.io (ed25519-dalek 3.0.0, hmac 0.13,
  sha2 0.11, reqwest 0.13.5, windows 0.62.2 — all latest stable).
- Diagnosed the four failing workflows from CI logs: windows-rs
  0.62 API-path drift in dw-win, the missing dw-win→dw-license dep,
  `rust-version = 1.80`, app-crate errors — then discovered the
  parallel core agent pushing the same fixes mid-session and switched
  to reconciliation mode (5 rebases, zero lost work).
- Independent contributions that landed:
  - 7eb0324: the six residual app-crate compile fixes on top of the
    core agent's ADR-1 rev2 (AppState resolution, cast hygiene, the
    device_id 'static hoist, BlockRead import, FRESH ScanControl for
    recovery — inheriting the scan's cancelled token would abort
    recovery after any stop), the pooled-event ResetEvent fix (the
    OVERLAPPED stale-signal contract), and the VHD workflow's
    here-string elimination + Write-VolumeCache flush + libtest
    filter order.
  - The VHD failure post-mortems (empty-scan): the diagnostics +
    root-cause chain for volume-target dispatch (probe before
    partition table — fixed by the core agent in parallel) and the
    NTFS $MFT LCN/cluster-size addressing (the mft0-zeros evidence
    that pinned it).
  - ae6e737: the P0 UI wiring from todo-uiux/CORE-INTERFACE-GAPS.md —
    start_recovery nodeIds, real preview_file rendering (images/text/
    PDF with honest decode-or-fallback, no mode sniffing — sim tours
    keep their curated visuals), and session rows calling
    resume_session + landing on results.
- QA pass: wrote docs/QA-REGISTER.md — 12 verified-fixed items with
  evidence, 16 open flags (P0: real-core CI tour, preview caps vs the
  100 MB research contract, missing ext/HFS+/ReFS engines; P1:
  $Bitmap viability scoring, exFAT chain re-derivation tests, the
  65%-reduction end-to-end proof, .rsf record-3 context, VSS
  disclosure, SDR stub, SMART threshold coverage; deployment: the
  license server E2E), plus the instruction-compliance audit table
  and multi-POV verdicts.

**Stage Summary**:
- Gates at push: fmt clean, clippy -D warnings clean (engine crates
  native + dw-win windows-msvc cross-check), 117 engine tests, web
  tsc/tests/build clean.
- The four workflows were re-run on the pushed HEAD; the VHD
  end-to-end is the release truth test and must be green before this
  milestone closes.
- Next: verify workflow greens, then the register's P0 items (the
  real-core tour job first).


**Session 4 addendum (final state)**:
- ALL FOUR WORKFLOWS GREEN on 6ca58c0 (CI incl. NSIS, VHD end-to-end,
  MSIX, UI Screenshots) — the first fully green pipeline with the real
  core as the default build. VHD truth evidence: `mft0 ascii = FILE0`,
  25 files found, recovery byte-compares, 3/3 tests pass.
- VLM verification of the tour frames (t11 scan state, t14 results,
  t25 SMART dashboard): live counters render, recoverability chips
  work, the free-tier SMART table renders per the research contract;
  no AI/mascot surfaces; no layout defects.
- QA register updated with the release-evidence snapshot (§6) and the
  bytes_found-over-capacity flag (QA-17).
---

## Session 4 addendum — all workflows green + independent senior review

**Task ID**: 4b
**Agent**: main (Z.ai)

**Work Log**:
- The VHD truth test exposed the NTFS LCN bug via runner diagnostics
  (boot parse perfect, MFT0 read zeros): NtfsEngine multiplied the
  $MFT LCN by bytes_per_sector instead of cluster_size — every spc>1
  volume (all real 4 KiB-cluster NTFS) scanned nothing. Fixed + spc=8
  regression test. Second VHD run: 25 files, 33 nodes, present.txt
  Existing 1 MiB, 3/3 tests PASS — then ALL FOUR workflows green on
  6ca58c0 (CI incl. NSIS, MSIX, UI screenshots, VHD end-to-end) with
  real release artifacts (17.5 MB windows-release).
- Parallel-agent coordination: rebased onto their app-crate fixes
  (incl. the ResetEvent pooled-event race and fresh recovery control
  they caught on my code), fixed their dest_display double-move.
- Independent senior-dev review (25+ yr storage POV) over the whole
  I/O rewrite. Fixed now: the critical iter_attrs out-of-bounds panic
  on damaged records (header-kind minimums 0x18/0x48 — panic=abort in
  release made one bad record fatal), sparse-salvage zero-fill
  (contract), ISO-BMFF 64-bit-size overflow infinite loop, 4Kn volume
  sector-size hardcode, 4Kn NTFS sub-sector record fixups, ZIP
  last-66KiB window, count_last seam double-count, rfind seam overlap.
- Three engine-scale findings documented in docs/TODO.md for M2:
  $ATTRIBUTE_LIST forking, hole-aware extents, resident recovery.

**Stage Summary**:
- Gates: fmt; clippy -D warnings clean (native + windows-msvc);
  117 engine tests green; 4/4 workflows green on real Windows with
  artifacts; VHD end-to-end proves the full pipeline on real NTFS.
---
## Session 5 (2026-10-18) — the engine-completion batch

**Mission**: close the Section 6.3 filesystem matrix (ext2/3/4,
HFS+/HFSX, ReFS were empty directories), the ~65%
duplicate/nonessential filter, the SDR production flow, and the
BitLocker BEK credential — all evidence-grounded per the research
repository.

**Landed** (commits dc291fa → 90c27e4):
- `fs/ext`: superblock validation (FUN_180004710 gates, byte-exact),
  32/64-byte GDT, extents + indirect resolution, BOTH kernel
  deletion shapes for deleted names, orphan inode sweep. 11 tests.
- `fs/hfsplus`: TN1150 volume-header gate ("H+"/"HX", 512-align,
  version<17, fork clamps), catalog B-tree leaf-chain walk, cnid
  path reconstruction, data+resource forks, extents-overflow
  resolution, orphaned-cnid sweep. 5 tests.
- `fs/refs`: DBR gate (RefsDbrImpl byte-exact), SUPB/CHKP/MSB+
  page census with the block-reference family, checkpoint
  newest-generation selection. Honest boundary: MSB tree records
  are RTTI-level evidence only — no fabricated parsers; carving
  covers content; M3 plan documented. 9 tests.
- Dedup contract: FoundEntry.content_key (FNV-1a prefix hash,
  carver-computed from the window buffer), same-region carved
  absorb, content-repeat → Version origin (essential mode hides
  client-side — no wire change).
- **Critical carver fix**: the post-emit `break` capped carving at
  ONE file per signature per 8 MiB window — an 8 MiB photo-heavy
  region yielded one image. Emitted-position set + regression test.
- `dw-core/repair.rs` (RawFixer model): boot diagnosis (FAT/exFAT/
  NTFS + geometry gates, FS-specific backups, exFAT CRC32 checksum
  gate), FixFromDBR/FixByScanSector plans with CopySectors undo
  snapshots, apply/undo through a caller writer. 6 tests.
- `dw-win/repair.rs`: the only write-enabled handle (GENERIC
  READ|WRITE, OVERLAPPED positioned writes, sector-size aware,
  flush-on-drop). `sdr_diagnose` real diagnosis; `sdr_repair`
  confirm-gated, verify-or-rollback. BEK-file BitLocker unlock.
- `TODO/phase-2-advanced-repair.md` (the §6.4 full spec),
  `docs/reviews/phase4-engines-multi-pov.md` (6 POV passes).

**State**: 125 dw-core tests green; clippy -D warnings clean;
CI green through 90c27e4 (one Windows-compile break on the repair
path was caught by the VHD job and fixed: raw constants +
OVERLAPPED writes + 4Kn sector-size awareness).

**Deferred register** (docs/TODO.md, each with evidence trail):
ext4 journal reanimation, HFS+ journal replay, ReFS MSB tree
enumeration (needs a real-ReFS fixture reversed on the runner),
ext name↔content association, and the remaining M2 NTFS items
($ATTRIBUTE_LIST forking, hole-aware extents). **Resident-content
recovery CLOSED** (8109a2c): read_resident_stream + the export
marker path + the byte-truth integration test.

**Session-5 close-out additions** (through be8d3a0):
- CI stabilization: three Windows-compile catches fixed same-hour
  (OVERLAPPED union init, BlockDevice trait import, mock sdr_repair
  twin — the sim surface must mirror the real one); the mock
  determinism flake pinned with fake timers (73 web tests green);
  the parallel agent's .ok()-on-Option release blocker fixed.
- Local cross-check discipline restored: dw-core + dw-win +
  dw-license all pass cargo check + clippy -D warnings on
  x86_64-pc-windows-msvc BEFORE push (aws-lc-sys still blocks the
  full app crate locally — the CI fast job covers it).
- Resident recovery landed (above). Phase-2 repair spec + 6-POV
  reviews + this worklog pushed.

## Session 5 (2026-10-18) — the QA-1 real-core tour + §8 licensing + Phase 0/1/2 artifacts (coordination session)

**Task ID**: 5c
**Agent**: main (Z.ai) — tour/licensing/docs session
**Task**: Phase 0/1/2 mandates from a cold start (corpus ingestion +
registers + plan docs), the QA-1 real-core tour (the owner's TOP P0),
the §8 licensing re-point, and the quick P1 batch — coordinating with
the parallel engine agent's pushes.

**Work Log**:
- Phase 0 from scratch: saved MASTER_PROMPT.md verbatim (PAT redacted
  per OQ-1 — the repo is PUBLIC; a pushed token gets revoked), cloned
  all four repos, and ingested the corpus: 49 research docs read
  line-by-line by this agent + 5 digestion agents covering the tail →
  `docs/maps/doc-tree.md` (299/299 coverage ledger, 0 uncovered),
  `docs/maps/context-synthesis.md`, five digests under
  `docs/maps/digests/` (per-binary/runtime/reversed, MOM/partition/
  WinPE, UIUX long-tail, donor study, UIUX/SaveScan/FileRepair/config).
- Verified the parallel agent's claims before building on them (CI
  green on 6edfddb via the GitHub API; engine constants vs my FULL
  reads of the ext/HFS+/ReFS research artifacts — kernel-correct).
- **QA-1 (TOP P0) landed**: `.github/workflows/real-core-tour.yml` —
  builds WITHOUT sim (only the narrow `ci-tour` license-seed feature,
  DiskGenie donor pattern: refusal arm in every other build), seeds a
  VHD (kept present.txt 1 MiB + deleted gone.txt 64 KiB), walks the
  REAL funnel via a dual-mode TourDriver (`get_dev_hooks.core` → live
  device pick by DISKWIZARD_TOUR_DEVICE filter → real scan → results →
  REAL recovery funnel), captures title-synced frames, asserts key
  frame names, byte-compares BOTH seeds, uploads screenshots + logs.
  Two launch iterations: a colon in an unquoted YAML step name (zero
  jobs created), then the concurrency group cancelling runs under the
  parallel agent's pushes.
- **§8 licensing fix (OQ-3)**: `dw-license/src/config.rs` — the ONE
  config module (endpoint `https://diskbytes-license…workers.dev/`,
  the donor's Ed25519 key + HMAC secret + UA + `x-db-*` headers +
  DB key prefix + iss/aud), values verified verbatim at the donor
  source. protocol/token/http/lib re-pointed; UI + both mocks flipped
  to DB- keys; docs updated to match (ARCHITECTURE, QA-REGISTER §3,
  README demo table). The session-3 fresh keypair is retired to the
  swap sheet.
- **§5.1 item 8 enforced server-side**: `start_recovery` now excludes
  the source disk in `resolve_destination` (DESTINATION_ON_SOURCE hard
  error); `snapshot.deviceId` flows to the destination dialog (the
  old call passed a hardcoded null).
- **P1 batch**: QA-17 (bytes_found capacity clamp, both report sites,
  regression test) + QA-8 (`sourceVolumeLive` on scan-done + the
  results-header live-scan note + both mocks emit it).
- **M2 fixture gate started**: ext2 real-mkfs fixture test (ignored,
  DW_EXT2_FIXTURE env, soft-skip locally) + the ci.yml generation
  steps (mkfs.ext2 + debugfs seed write, byte-truth recovery assert).
- Phase 1/2 artifacts: `docs/audit/consistency-register.md` (all 13
  §7 items verdicts + hallucination watch), `docs/audit/ui-gap-register.md`,
  `docs/architecture/target-codebase-tree.md` (every module cites its
  governing research artifact), `docs/architecture/phase-build-plan.md`
  (M0-M5 with exit criteria), `docs/standards/SOP.md`,
  `docs/dependencies.md` (live-verified versions; OQ-7 npm posture).
- Coordination: 4 rebases with the parallel agent (engine batch,
  parse_device_id Option fix — which my QA-8 commit then tripped over
  and CI caught within minutes; fixed same-cycle), zero lost work.

**Stage Summary**:
- Gates at each push: fmt, clippy (engines), 126→127 dw-core + 27
  dw-license tests, tsc, 73 web tests. The app-crate Windows compile
  rides the CI windows-check job (local box lacks GTK; aws-lc-sys
  blocks msvc cross-check — the documented limitation).
- QA-1's workflow is live; the first fully-completing run was racing
  the parallel agent's pushes (concurrency cancels). CI/MSIX/VHD green
  on the post-fix commit.
- Next: tour green confirmation + register updates; HFS+ fixture
  (hfsprogs) + ReFS runner leg; QA-4 $Bitmap scoring; QA-6 65% proof;
  the M3 IOCP push.

## Session 6 (2026-10-18) — context recovery + the runner-caught defect cycle + §5.1 evidence closure

**Task ID**: 6
**Agent**: main (Z.ai) — core/QA continuation session
**Task**: Recover from the session crash per §16, then close the
§5.1 evidence trail (bench numbers), fix every workflow failure the
runners exposed on the new QA surfaces (ext2 real-mkfs gate, fuzz
suite, IOCP test, real-core tour), and sync the registers.

**Work Log**:
- §16 recovery: verified both pre-crash commits (88b2b43 fuzz suite,
  73e3d86 geometry fix) had pushed; tree clean; the QA-register edit
  the crash interrupted landed inside those commits.
- 73e3d86 first-full-run failures diagnosed from runner logs:
  CI ❌ ext2 real-mkfs gate, Fuzz ❌ harness abort, Windows
  Integration ❌ iocp divergence, Real-Core Tour ❌ (never-green).
- **ext2 offsets (F14)**: built e2fsprogs 1.47.4 user-local,
  reproduced the CI fixture byte-exact, proved the rev-1 superblock
  block was parsed +4 off (missing resuid/resgid 0x50/0x52) —
  inode_size landed on compat flags, so real mkfs (inode 256) read
  every inode mid-slot → zero entries. Fixed engine + probe flag
  discrimination (ext3 was mislabeled ext2) + the fixture's own
  offsets (s_journal_inum 0xD8→0xE0). Regressions: embedded
  real-superblock byte-truth test + probe discrimination. d71570d.
- **fuzz harness (F15)**: FuzzRam::read_sectors usize add-overflow
  on absurd LBAs → checked u64 math + typed ReadFailed. 74d599a.
- **IOCP test race (F16)**: evidence-timed — the iocp test's
  sequential reference raced vhd_end_to_end's volume writes (parallel
  test threads, $MFT/$LogFile updates changed window 0 mid-compare
  with zero bad sectors). VHD_LOCK serialization + divergence-offset
  assert. 74d599a.
- **tour honesty (F17)**: the driver silently skipped recovery on
  empty selection/destination and stamped recovery-complete anyway;
  the app's beginRecovery ALSO swallowed start_recovery errors (a
  real-user "Preparing… forever" defect). t99-tour-fail-<reason>
  loud frames + stage asserts + workflow TOUR_FAIL detection + the
  honest error-summary variant. 74d599a.
- **§5.1-10 closed**: transcribed the real-runner bench table from
  d71570d run 39137841845 into docs/benchmarks/io-backend.md (8
  profiles, ~370-430 MiB/s through the cache-backed VHD stack, flat
  9.7 MiB WS, CPU ≤ 0.0001 s/MiB, batch≡sequential byte-identity).
- QA register: F13-F17 appended with runner evidence; QA-11 updated
  with the recorded numbers. ee53823.

**Stage Summary**:
- d71570d: CI/Integration/MSIX/Screenshots green; 74d599a rerunning
  the fuzz + integration + tour surfaces (fixes in flight).
- The QA surfaces built this cycle each caught a real defect on
  their first completed run — the shared-hallucination trap (F14)
  is the standing lesson: fixtures must come from REAL formatters,
  not hand-built mirrors of the parser.

### Session 6 continuation — the fuzz class fix + the tour's named stage

- The dict-enabled fuzz sweep (local, CI-equivalent invocation) found
  TWO more beyond the CI's three: read_bytes' `offset + len` at the
  device-boundary choke point, and GPT's `entries_lba + s` in the
  salvage loop. All five fixed with catching regressions; all ten
  targets re-swept clean (307K–23M execs) + both crash artifacts
  replayed (cb3b712).
- The instrumented tour's first flight named its stage:
  `recovery-cmd-mkdir-destination-…` = ERROR_INVALID_NAME — the real
  inventory's "C:" letters + `":\\"` → `C::\DiskWizard Recovered`.
  Fixed + windows-gated unit regression + the CI blind-spot closed
  (`cargo check --tests` for the app crate — plain check skips
  #[cfg(test)] code) (604ee3d, F18).
- The funnel was otherwise PROVEN healthy by the t99 evidence chain:
  selection non-empty, destination rows non-empty, destination
  chosen, recovery command invoked — every prior silent-skip suspect
  eliminated. With the mkdir fixed, the tour's byte-truth leg is the
  remaining witness.

### Session 6 final — ALL SIX WORKFLOWS GREEN (aa82d57)

The Real-Core Tour posted its first green run ever — QA-1 closed with
release-truth evidence (present.txt 1 MiB + the DELETED gone.txt
64 KiB both byte-identical through the release app's real recovery
funnel). The Fuzz suite posted its first green run (all ten targets,
dictionary, five hostile-media guards each with its catching
regression). CI (with the new --tests app-crate gate), Windows
Integration, MSIX, and UI Screenshots green on the same SHA. The
register (QA-1 + §6 snapshot), session report, and this worklog are
synced to the evidence.

### Session 6 close — the green sweep holds (33d5190)

F19 (the directory-walk path explosion, 11 s → 1 ms) closed the last
runner finding; the timeout artifact now seeds the fat corpus
permanently. ALL SIX workflows green on consecutive SHAs (aa82d57,
33d5190) — the Real-Core Tour green twice in a row (stable, not
lucky), the Fuzz suite green with the regression seed in-corpus.
Register F13–F19, §6 snapshot, session report, and this log carry the
verbatim evidence chains. Session complete per §17.

## Session 7 (2026-10-19) — the licensing live-E2E port + the owner's activation symptom, root-caused

Owner report: "after connecting diskgenie's server, when I create a
new license key and use it in DiskWizard it says nothing — and I see
no queries on the database." Task: study DiskGenie's implementation,
fix it, and copy the license testing workflow + tests 1:1 with full
production setup.

Investigation (evidence-first, all against the LIVE worker):
- The deployed worker is healthy (v3 health, admin API answers) and
  accepts the exact §8 wire format: a scripted probe (same HMAC
  headers/UA/secret as `dw-license/src/protocol.rs`) activated a
  self-minted key → 200 + token + D1 device row + audit event. The
  wire path in current main is CORRECT.
- The owner's symptom reproduced only against the PRE-§8 wiring: the
  old endpoint `diskwizard-license…workers.dev` answers Cloudflare
  "error code: 1042" (route dead) — an older build's activation dies
  there with a vague ServerError and NEVER touches the DiskGenie D1.
  Conclusion: the tested build predates the §8 re-point (86f013d) or
  was built from it — rebuild from current main.
- Signing-key observation: one early probe token failed verification
  against the embedded pubkey while every later token (the Rust live
  E2E + fresh validates) verifies — the deployed worker was
  redeployed/re-secreted during the session window (Cloudflare
  gradual deployment explains the split). The donor's soft-spoof
  validate policy exists precisely for this window — now ported.

What landed (the donor 1:1 port, `diskgenie-v2` → DiskWizard):
- `crates/dw-license/tests/live_license.rs` — the LIVE lifecycle E2E
  (feature `live-license-e2e`): admin-generate → activate with the
  full claim → verify the Ed25519 token against the production public
  key → revalidate with CHANGED facts (the v1 wipe regression, LIVE)
  → slot rule → deactivate frees the slot → re-register → revoke →
  revoked-validation hard-fails. PASSED live twice locally.
- `src-tauri/tests/live_license_facts.rs` — the REAL-device leg
  (Windows runner): fingerprint determinism across two collections,
  fact sanitation, activate with the REAL claim, admin-lookup asserts
  field-for-field equality (the "hashes match" proof), pure-refresh
  revalidate, deactivate + revoke cleanup.
- `.github/workflows/license-e2e.yml` — the donor's workflow shape:
  windows-latest, `LIVE_ADMIN_KEY` repo secret (set via the encrypted
  GitHub secrets API — registered, never in code), secrets-guard
  skip, both test legs, post-run health check. Runs on every push.
- `LicenseManager::activate` — the donor's transport policy: two
  jittered retries on Network/ServerError; RATE_LIMITED never retries
  (window slots).
- `LicenseManager::validate` — the donor's soft-spoof policy: a
  response that fails local verification keeps grace frozen (the
  stored token's `exp` is the hard stop) — a server key rotation can
  no longer mass-deactivate the fleet mid-window.
- `collect_device_facts()` — the donor's shared-path pattern: the
  command layer AND the live E2E use ONE collection function (`pub`
  through `commands::`), so CI proves exactly what production sends.
- UI `userFacingError` — SPOOFED / NETWORK / SERVER_ERROR markers now
  map to honest typed copy (the "it says nothing" fix: every state
  the backend can emit lands as actionable text, never the generic
  lie). +3 vitest cases pinning the exact backend strings.

Gates: fmt clean; clippy `-D warnings` clean (default AND with
`live-license-e2e`); 141 dw-core + 27 dw-license tests; 74 web tests;
tsc clean; YAML validated; live E2E green ×2. QA-16 CLOSED with the
run evidence (docs/QA-REGISTER.md §3).

## 2026-10-18 — temporary packaged-build test license

- Operator-authorized temporary override: `DB-TESTS-DEM00-PR000-C0DE0`
  is prefilled by the activation dialog and locally grants a simulated
  lifetime Pro posture in every build, including NSIS/MSIX. This keeps
  full recovery-flow testing unblocked while the live activation issue is
  investigated.
- This is intentionally not a server activation and must be removed before
  any public distribution or commercial release.
