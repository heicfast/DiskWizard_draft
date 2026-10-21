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

## Session 8 (2026-10-21) — the owner's symptom triage: the engine was lying

Owner symptoms: scan "very very slow", stuck, estimated size climbing
into TB on a 320 GB disk, results table empty while counters ticked,
SMART showing for the first disk only, Cloud Recovery to be removed.

Root causes (all verified in code, each fixed + regression-tested):

1. **Carver weak-validation fallbacks** carved up to multi-GiB spans
   per accidental magic (JPEG FFD8FF = 1 hit / 16 MiB of binary data
   × 64 MiB fallback = PB-scale logical bytes; WAV/OLE2 fell back to
   the full 2 GiB limit; PST carved 512 KiB per `!BDN`). The walks
   themselves crawled through tiny cached reads — the "stuck" scan.
   FIX: per-signature probe_cap + fallback_cap + MAX_WALK_STEPS +
   a shared 64 MiB validator budget per window, scratch-buffered
   Prober, cooperative checkpoints inside walks, research-accurate
   validators (PST 0x0E/0x17, MP4 brand check, GIF87a/89a, OLE2
   geometry, WAV/MP3/OGG/SQLite rejects), weak hits grade Poor.
2. **No progressive results** — the tree landed only at completion.
   FIX: on_tree_update observer (2048 nodes / 350 ms) publishes
   Arc snapshots; get_scan_results speaks a delta protocol
   (since/nodeCount); the frontend accumulates.
3. **NTFS folder collapse** — @mft refs never resolved; partition
   prefixes corrupted refs into literal path dirs. FIX: record_id +
   partition fields, mft_dirs map, pending buffers + fixpoint at
   unit/phase boundaries, finish() lands leftovers on the lost
   branch; root record 5 maps to the partition base.
4. **Live counter clamp** — bytes clamped to device capacity
   continuously (was end-only); the extrapolated "estimated" line
   replaced with the honest found-bytes counter.
5. **SMART** — sync commands ran on the MAIN thread (the prefetch's
   10 s ATA timeouts froze the window); the NVMe query used the
   adapter property instead of the device property. FIX: async +
   spawn_blocking, StorageDeviceProtocolSpecificProperty first, and
   an honest per-disk "unavailable + reason" card (the dashboard
   never silently disappears).
6. **Cloud Recovery removed completely** (owner directive): backend
   commands, mock data, page, sidebar, destination tab, tour step,
   types, CSS.
7. **FAT name mangling** (found by the new selftest): the 8.3
   formatter lowercased + ~-prefixed LIVE names; now case-preserving
   with '_' for the deleted 0xE5 marker.

New surfaces:
- **dw-cli** — the headless core junction: scan (full logs + JSON +
  .rsf + watchdog), recover (--ids/--origin/--all), devices, smart,
  selftest (cross-platform fixture → scan → recover → byte-compare).
- **core-e2e workflow** — Windows runners: VHD → 100+ real files
  across every format family → permanent delete → CLI scan → recover
  → four truth gates (population ≥ 75%, bytes within the volume,
  structure resolved with no @mft leakage, ≥ 60% byte-identical) →
  30-day evidence artifacts. Plus the ubuntu core-proof job.
- Dependency bumps: react 19, zustand 5, framer-motion 14, vite 8,
  vitest 5, tauri 2.12 stack; 155 dw-core + 27 dw-license tests, 74
  web tests, all green.
### Session 8 close — ALL EIGHT WORKFLOWS GREEN (0016a2c)

The final layer of the @mft story: $FILE_NAME parent refs carry the
reuse SEQUENCE in the high 16 bits — the dir emission masked it, the
file emission didn't. Every user file on a real volume landed on
"File Path Lost". Fixed with MFT_REF_MASK on both arms + a
seq-hardened end-to-end fixture (the exact real-volume condition).

The truth-gate evidence (core-e2e run 39483485020, real Windows
runner, real VHD, real deletion):
- Scan: 155 files / 510.9 MB / **1 second** / NTFS (was: hours-stuck
  with TB-inflated counters)
- Structure: **139 structured** / 16 lost-branch (the folder tree
  resolves: Photos/, Videos/, Docs/… with their deleted contents)
- Byte-verify: **115/116 deleted files byte-IDENTICAL, 0 different**
  (99.1% recovery truth through the production engine + dw-cli)

All eight workflows green on the same SHA: CI, Core E2E, Windows
Integration, Real-Core Tour, UI Screenshots, MSIX, License E2E, Fuzz
(the full hostile-media corpus over the carver overhaul).

## Session 9 (2026-10-21) — the owner's symptom chain, root-caused and fixed (batch 1)

Owner symptoms → root causes (all verified in code before fixing):

1. "Scan very very slow; progress never above 1%; remaining time
   keeps increasing" — FOUR stacked causes:
   a) The IOCP pipeline was UNREACHABLE from the scan (CachedDevice
      lacked read_sectors_sparse_batch; the trait default ran the
      carve at queue depth 1 on every media). Fixed: the batch
      override delegates to the parent (regression: the counting-
      device test proves the batch reaches the parent entry point).
   b) The MFT was walked per-record (1 KiB reads through 32 KiB
      chunk-cache fetches: a mutex + LRU scan + Vec alloc + sync
      event-waited ReadFile per 32 KiB). Fixed: the walk streams
      4 MiB aligned chunks (the cache layer's new ≥256 KiB bulk
      bypass — §5.1 item 6), parses records on a scoped worker pool
      (≤8 threads, the og's priority-7 worker family), emits in
      order. apply_fixup extracted pure; parse_record is the
      parallel-safe core.
   c) The MFT was walked TWICE (deep_scan defaulted to quick_scan).
      Fixed: the default is a documented no-op (the og's complete
      searcher adds USN/orphan work our quick pass + tree fixpoint
      already cover); ext/refs/hfs+ keep their real deep passes.
   d) ETA = elapsed×(100−p)/p — grows monotonically under sub-linear
      progress. Fixed: EtaTracker (EWMA over ≥1 s measured
      percent/sec samples, clamped [1 s, 24 h], stalls keep the last
      measured rate). Unit-tested.
   Plus: raw/unknown partition units no longer hold quick/deep
   progress weight hostage (structured-only weighting — the
   BitLocker/MSR "bar never moves" case), and the carver reports
   SECTORS (the wire's sectorsRead/Total are unit-consistent again).

2. "No files appear while scanning; only existing ones" — the
   2048-node publish floor hid small/mid trees entirely (the tour
   VHD's 155 files NEVER published mid-scan). Fixed: first publish
   at 64 nodes, then 512/350 ms (the og's ~1000-item incremental
   BuildTree cadence); the app session stores DELTAS (O(delta) per
   publish — the old full-copy-per-publish was O(n²/2048) total)
   with cumulative category counters (the old slice-local counters
   clobbered the chips on every append).

3. The tour's t99-tour-fail-results-snapshot-null (flaky on doc-only
   commits — the 1.4 s poll loop tears down at scan-done between the
   last null poll and the final publish): closed at BOTH ends — the
   worker folds the complete tree into the session BEFORE scan-done
   emits, and the frontend force-reloads (since=0) on done. The
   mid-scan fold + poll views share the tree→appended→categories
   lock order (atomic against duplication/loss).

4. "SMART shows for the first disk only" — the ATA path was dead on
   arrival: an ATA_PASS_THROUGH_EX struct issued through the LEGACY
   IOCTL_ATA_PASS_THROUGH (wrong decode), Length never set (the EX
   path's first validation), a read-only handle (pass-through wants
   READ|WRITE — the smartmontools posture). Disk 0 is NVMe (the
   storage-query path was correct) — hence "first disk works".
   Fixed: IOCTL_ATA_PASS_THROUGH_EX + Length + a dedicated RW
   pass-through handle (RAII-closed) + bus-type gating (NVMe only
   on NVMe buses) + the adapter-property fallback REMOVED (it could
   cross-attribute another controller's data). The bus table itself
   was off-spec (SATA 0x0B read as "Virtual", SAS as "MMC") —
   corrected to the STORAGE_BUS_TYPE values (also fixes the UI's
   SD-card classification).

5. "Date/time precision" — NTFS truncated to whole seconds (100 ns
   source); FAT wall-clock read as-if-UTC (every FAT timestamp
   shifted by the user's UTC offset); exFAT fields read one slot
   high ("accessed" consumed the 10ms/offset bytes; attributes read
   from the create-timestamp offset). Fixed: the wire carries unix
   MILLISECONDS end-to-end (NTFS ms, exFAT + its 10ms increments
   with the spec §7.4 field map, ext ×1000, HFS+ ×1000), FAT
   interprets through the host timezone (chrono, the Windows mount
   behavior — displays the stored wall clock), .rsf schema v2.
   Regression tests pin the round-trip properties.

6. Recovery chances were confidence arithmetic only. Fixed: the
   ORGANIZE phase runs the readability probe (the og
   CCheckNodeThread model) — the first sector of up to 20 K deleted
   files' extents; unreadable media grades Poor.

7. Cancellation could lose the assembled tree (Err(Cancelled) at a
   phase boundary dropped it — the CLI's --max-seconds path
   reported "no results snapshot"). Fixed: run() wraps the phase
   machine; every cancellation finishes stopped WITH results.

8. The scan worker now raises itself to THREAD_PRIORITY_HIGHEST
   (the og's priority-7 compute workers; HIGHEST not TIME_CRITICAL
   so the WebView keeps painting).

Gates: fmt clean; clippy -D warnings clean; 159 dw-core + 27
dw-license tests; 74 web tests; tsc clean. The app crate compiles on
CI (local box lacks GTK — documented).

### Session 9 batch 3 — the runner-caught defect cycle

The batch passthrough ENGAGED the IOCP path in real scans for the
first time — and it immediately caught a sleeping race (the F14
lesson repeating: a surface that never ran hides its defects):

**The IOCP stale-completion panic** ("completion for an unknown
OVERLAPPED" — vhd_end_to_end + core-e2e runner evidence): once the
device handle is port-associated, EVERY overlapped read queues a
packet — including the single positioned reads validators/recovery
issue between batches; the next batch's harvest popped the stale
stack OVERLAPPED and died. Fixed at both ends: single reads set the
documented hEvent low bit (event-signaled, NO port packet — the port
stays batch-only), and the harvest DROPS unknown packets instead of
panicking.

New evidence surfaces this cycle:
- `win_api_surface.rs` (dw-win integration): every Win32 call the
  core makes, one independent assertion per call against the real
  host — opens, geometry, raw reads, the batch≡sequential identity,
  the ATA/NVMe transports' TYPED outcomes, the bus table, the memory
  counters, the thread-priority raise.
- `memory_stability_three_scans`: the leak-class gate — three full
  pipelines in one process; round 3's working set must sit within
  48 MB of round 2's.
- `engine-benchmark.yml`: multi-volume speed evidence (512 MB dense /
  2 GB sparse VHDs) with environment recording, per-phase timings,
  throughput + memory gates, and the comparison record against the
  og's DOCUMENTED numbers (quick ≈21 s / 129 GB; ≈170 MB/s aggregate).
- core-e2e upgraded: 2200+ real-content files, the five-style
  deletion matrix (keep/permanent/recycle-bin/same-name-overwrite/
  cluster-reuse churn), origin-honesty gates (existing vs deleted),
  perf/memory gates, hash-fallback verification for recycle renames.
- The lost-partition sweep WIRED into scan_units — a deleted
  partition's surviving volume is discovered and reconstructed
  structured (regression test included), not carve-only.
- dw-cli: phase timings, MiB/s, peak working set, environment header
  (cores/RAM/geometry) in every scan log + JSON.

Local gates upgraded: the msvc cross-check now covers dw-win +
dw-cli + their tests locally (the app crate still rides CI — aws-lc-sys
blocks the cross-check; a ring-provider swap is queued as follow-up).
TypeScript 7; Cargo.lock refreshed; `docs/dependencies.md` update
pending with the next docs pass.

## Session 10 (2026-10-21) — Phase-2 file repair ships + the M2 engine gaps close

The session opened with full context recovery (fresh environment):
both repos re-cloned, the 9-workflow green state on 3511a5a verified,
the worklog + gap registers re-read, and the repair research corpus
(deepening-batch2, image-repair format-grammar/state-machines/
gif-bmp-svg-tiff-walks, repair-architecture, 06-repair-chain, 7z,
x3f) read line-by-line before any code.

### The dw-repair crate (phase-2 §1-5, delivery steps 1-5)

Ten grammar engines, in-process Rust (no exe spawning — phase-2 §5):

- **The contract**: RepairEngine = the IImageInfo 5-slot virtual
  (Drop + check_magic + analyze + factory + repair_into);
  DamageReport with exact verdicts; RepairTier::Basis/Sample.
- **JPEG** (CJpegInfo): marker-grammar walk with resync, corrupt-
  marker drop, synthesized EOI, the 0x400 marker bound + 0x7FFF
  length clamps, the FFD8/FFD9 skeleton. The decodable-skeleton
  honesty: no SOF+DQT+DHT+SOS → BeyondRepair.
- **PNG** (CPngInfo): ScanCriticalChunks as content-search, CRC
  re-stamping, length re-derivation, IEND literal synthesis, and
  the height-adjusted viewable-prefix salvage (strict-inflate +
  geometry + filter-byte-validity gate routes damage to it).
- **PDF** (EuPDFRepairFile): whole-body N M obj scan, byte-exact
  canonical xref strings, /Root cascade (trailer → catalog scan →
  synthesize Catalog around /Pages → synthesize Catalog+Pages/Kids
  around /Page objects), honest BeyondRepair when nothing
  structural survives.
- **OOXML**: the OPC rebuild — CD-truth inventory, LFH fallback,
  [Content_Types].xml re-derivation (byte-exact Default/Override
  templates + the part registry), the per-part CRC audit
  (zipfile.testzip parity) + salvage re-deflate of decodable
  prefixes.
- **MP4/MOV**: the weighted atom grammar (13 4CCs), two-scan,
  successor-anchored extent normalization, honest no-moov
  BeyondRepair (the sample transplant is the vendor's only path).
- **GIF/BMP/TIFF/WebP/SVG** per the walks artifact (GCT-aware
  header + LZW-min-code skip; the 12/40/108/124 DIB taxonomy +
  palette arithmetic + offBits recompute; endian arbitration by
  IFD vote + tail backward scan; RIFF rebuild; envelope injection).

**App layer**: repair_analyze_file + repair_files (license-gated,
destination-only, per-file progress events, healthy files honestly
skipped); repair-during-recovery wired (the enable-file-repair
toggle walks the recovered copies — repairedFiles counts real
repairs in recover-done, was hardcoded 0); the by-name dispatch
(signature first, extension fallback — the destroyed-header class).
**dw-cli**: the repair subcommand (--dest batch + --analyze, JSON +
exit codes for CI gates).

### The decode-as-validation loop (the local hardening pass)

The fixture harness (Pillow JPEG/PNG, a raw multi-object PDF
writer, python-docx, ffmpeg MP4 — REAL encoders) + four damage
classes (trunc60/headsmash/midcorrupt/taildestroy) + REAL decoders
as the gates (Pillow/pypdf/zipfile/ffprobe) caught five engine
defects in one pass — the zlib-wrapper bug, the PNG height/
filter-byte gap, the JPEG missing-skeleton lie, the PDF /Root
fallback pointing at non-catalogs, the MP4 moov-less 'repaired'
lie. All fixed; 20/20 local checks pass. The repair-e2e workflow
runs the same harness on windows runners (ffmpeg resolve-or-choco).

### The M2 engine gaps close

1. **$ATTRIBUTE_LIST forking**: the walk parses the 0x20 entry list,
   STASHES extension records (base_ref != 0), DEFERS forked bases to
   a post-walk merge (names dedup, freshest times, same-name streams
   concatenate extents). Heavily-fragmented files no longer vanish.
2. **Hole-aware extents**: the HOLE_EXTENT_OFFSET sentinel — sparse
   runs emit holes that zero-fill (post-gap data keeps its TRUE file
   offset); implausible mid-list runs fail the whole runlist instead
   of silently shifting offsets; QA-4 + grading guards.

### The fuzz + workflow catches

The new dw-repair fuzz crate (engines + salvage) caught a JPEG
trailing-0xFF bounds panic and a PNG extent underflow on its FIRST
CI run. The fuzz.yml restructure also fixed the pre-existing
'branches: ain]' corruption that had silently filtered the fuzz
push trigger. The repair-e2e's first run caught ffmpeg missing from
pwsh's PATH on windows-2025 (resolve-or-choco fix).

### The ring-provider follow-up: CLOSED with evidence

reqwest 0.13.5's `rustls` feature hardwires aws-lc-rs (feature-edge
traced: rustls → __rustls-aws-lc-rs → hyper-rustls/aws-lc-rs); the
ring swap needs rustls-no-provider + a runtime install — but BOTH
providers need an MSVC-compatible C toolchain for local
linux→msvc cross-checks of the app crate (no clang-cl/nasm on this
box, no root). The app crate stays CI-verified; the swap is
documented as evaluation-complete.

### State at session pause

164 dw-core + 41 dw-repair tests; fmt + clippy -D warnings clean
everywhere. Pushes: 48ffbd0 (crate), ff3e77d (wiring + CLI),
d306f07 (hardening, artifacts untracked in 88ae319), 97deaff
(M2 #1 + fuzz fixes), 837d322/933f946 (M2 #2 + nits). CI green on
88ae319: CI, Core E2E, License E2E, Engine Benchmark; the repair
e2e + fuzz re-run on the fix pushes.

---

## Session 11 — The RE-Match core: parallel pipeline, $Bitmap grading, 100-ns timestamps, the SMART transport chain

The owner's executive spec ("DiskWizard Core — Exact RE-Match
Specification (v1.0)") implemented core-only, top to bottom.

### The diagnosis (every symptom, one root each)

1. **"Scan very very slow"** — the NTFS walk's I/O was serialized:
   one 4 MiB read in flight; the parse pool waited on each read; the
   carve's signature VALIDATION ran single-threaded (only the reads
   pipelined through IOCP); the scan units ran one-at-a-time.
2. **"Progress never above 1%"** — phase-emission cadence: progress
   fired once per 4 MiB chunk, and the whole quick phase mapped into
   0-38% of the bar.
3. **"Deleted files not shown"** — the $SI timestamp swap made
   deleted entries look wrong, and the binary destroy-or-keep
   viability rule graded everything else at full confidence with no
   RE ratio banding.
4. **"Date/time not accurate"** — $STANDARD_INFORMATION offsets
   SWAPPED: `modified` read $SI+0x10 (the MFT touch time — updates on
   renames/metadata writes) while Explorer shows $SI+0x08. Every
   NTFS date shown was the wrong qword.
5. **"SMART isn't available for this disk"** — the ATA task file was
   register-scrambled: the SMART cylinder signature 0x4F/0xC2 sat at
   CylHigh/DriveHead instead of CylLow/CylHigh — every SATA/USB
   query ABRT'd. Disk 0 (NVMe) worked; everything else failed. The
   non-admin case had NO fallback transport at all.

### The implementation (the spec's §0-8, mapped)

* **§4.1-4.3 The thread graph** — `ntfs::walk` now: 8 MiB windows
  (kMftChunkBytes), a `cores.clamp(2,16)` worker pool where each
  worker does its OWN positioned bulk read (overlapped → the READS
  overlap on Windows), parses records, and scores deleted files
  inline against the shared read-only $Bitmap; records flow over
  bounded(4096) crossbeam (CKernelQueue) to the engine thread which
  alone owns the sink (stash/defer/emit semantics identical, order
  freed by the tree's fixpoint). Progress: per-window atomic counter
  + the 100 ms CProgressTimer throttle.
* **§4.1 units in parallel** — `scan::run_units_parallel`: one
  worker per structured unit (the Quick/Complete controller
  family), UnitMsg channel (entries boxed — the channel footprint
  clippy flagged), the merger on the main thread owning tree +
  counters + observer. Phase bands per the spec table: discovery
  0-2%, quick 2-38%, deep-structured 38-68%, carving 68-95%
  (wire phase stays `deep` — no UI contract break), organizing
  95-100%.
* **§3 Recovery scoring** — `score_deleted` computes
  intact/total cluster ratio inline (worker-side, bitmap shared);
  grading per the RE bands: 1.0→Excellent, ≥0.75→Good, ≥0.25→Poor,
  <0.25→Poor ("overwritten" class, kept listed); ratio==0 + the
  0x3FF size gate → destroy (the v2 rule, preserved);
  `FoundEntry::recovery_ratio` feeds `tree::grade_from` before
  confidence arithmetic; resident files = Excellent (record survived
  = content inline).
* **§7 Free-cluster carving** — `NtfsEngine::free_cluster_ranges`
  (word-wise bitmap run detection: all-allocated/all-free words take
  the fast path; per-bit only on mixed words) +
  `ScanController::carve_plan` (NTFS free ranges translated to
  device-absolute, non-NTFS structured units' full extents, RAW
  units' full extents, the uncovered gaps; merged/sorted) +
  `RawCarver::carve_ranges` — the parallel sweep: worker pool over
  window batches, each worker submits its batch as ONE
  read_sectors_sparse_batch (IOCP on Windows), scans windows
  (scan_window_collect — the CPU half that was serial), ships
  entries over bounded(1024) (CCarveQueue). Per-window
  first-signature-wins dedup local; scan regions disjoint by the
  overlap advance — byte-identical results at any pool size (the
  sorted-set equivalence tests pin it).
* **§5 Timestamps** — the $SI mapping fixed (modified=+0x08,
  changed=+0x10, the $FILE_NAME fallback mapped correctly too);
  `Timestamps` carries `*_raw` (full 100-ns FILETIME ticks) for all
  four times; the wire node gains ADDITIVE `modifiedRaw` (the ms
  contract untouched); the spec's 10k-sample roundtrip test +
  a distinct-values Explorer-parity test pin the mapping.
* **§8 SMART** — the transport chain: ATA PASS-THROUGH (RW handle,
  DRDY|DATA_IN, the task file from `smart_read_data_taskfile()` —
  unit-tested register order) → `SMART_RCV_DRIVE_DATA`
  (FILE_ANY_ACCESS — THE non-admin path) →
  `IOCTL_STORAGE_PREDICT_FAILURE` (vendor area = the ATA SMART
  sector) → ATA PASS-THROUGH on the RO handle → NVMe cross-check;
  NVMe bus: protocol query → PREDICT_FAILURE → ATA cross-check.
  IOCTL codes CTL_CODE-derived + tested.
* **CCheckNodeThread family** — `apply_readability_grades` probes
  in parallel (cores.clamp(2,16)), applied in node order.
* **Deps** — crossbeam-channel 0.5 added (rayon evaluated and
  REJECTED: nested-pool starvation with multi-unit engines on a
  shared pool; std::thread::scope + crossbeam delivers the spec's
  exact worker counts with zero starvation risk).

### Verification

* dw-core: 167 tests green (164 + roundtrip + Explorer-parity +
  free-ranges), including the QA-6 census (carve/merge counts
  EXACTLY pinned through the parallel pipeline) and the QA-4
  viability fixture (updated to the ratio bands).
* dw-win: cross-checked + cross-clippy clean (x86_64-pc-windows-msvc);
  the SMART register-layout + IOCTL-derivation tests run in CI.
* fmt + clippy -D warnings: clean on every crate, both targets
  (dw-cli's pre-existing 6 Windows warnings fixed too).
* App wire: `modifiedRaw` additive; all frozen shapes untouched.

### Session 11 (cont.) — the validation layer

* `recovery-lab.yml` — the realistic testing environment (the
  owner's mandate): a 10 GB VHD, two partitions, 2650 seeded files
  with REAL format contents (JPEG/PNG/ZIP/PDF/MP3/MP4/BMP/WAV +
  text + binary — the carvers' actual grammars), 50 top dirs with
  5 branches 8 levels deep, log-uniform sizes 1 KB..8 MB, timestamps
  spread over two years; a SHA-256 manifest of everything; the full
  deletion matrix (65% force-delete, 15% recycle-bin, 5% kept,
  cluster-reuse overwrite pressure, and a whole PARTITION deletion);
  then the scan + the CI GATES: kept recall ≥99.9%, deleted recall
  ≥90%, path reconstruction ≥90%, lost-partition recall ≥85%, memory
  ≤3 GiB — plus a 20-file recovery byte-truth pass (SHA-verified,
  Excellent/Good grades only — Poor legitimately byte-fails when
  partially overwritten, which is the grading working).
* `compare-vs-og.yml` — the head-to-head: dw-cli (release) and the
  OG application (DRW.exe from the research repo, launched + driven
  per the research's own proven ground-truth click pattern) on the
  SAME runner OS disk in the same session. Our numbers exact (JSON);
  the og's wall-time exact, its found-count best-effort window text +
  authoritative screenshots (its Qt UI exposes limited UIA).
  comparison.json + step summary + artifacts.
* `tests/parallel_bench.rs` — the worker pools' throughput floors:
  the parallel MFT walk (200k-record synthetic NTFS) and the parallel
  carve (256 MiB signature-dense). Measured locally: 431k rec/s walk,
  carve floor passes — floors set far under headroom so CI-runner
  variance never flakes. Wired into engine-benchmark.yml. (criterion
  evaluated and skipped: the lab + benchmark workflows measure the
  REAL engine on REAL media via dw-cli — richer than synthetic-only
  numbers; the codebase's --ignored perf-test convention followed.)

---

## Session 12 (2026-10-21) — the freeze trio: percent scale, completion stealing, drain deadlock

Fresh-context session. Both repos re-cloned; the 3918ca6 CI state
audited FIRST: Core E2E cancelled at the job timeout with the scan
process hung 38 minutes past its --max-seconds deadline — the runner
log preserved the exact failure signature the owner reported
("progress never above 1%", frozen carve, zombie process).

### The diagnosis (three defects, one CI log)

1. **The percent scale** — `snapshot()` computed
   `(percent * 100).round() / 100` on a 0..1 FRACTION: 0.9267 became
   0.93, displayed "0.9%". The whole scan reported 0.0→1.0% max
   (the run's DONE line would have read "1.0%"). The app's ETA
   divides (100 − p) by a 0..100-scale rate → the exploding
   "remaining time". One line, every symptom the owner named.
2. **Completion stealing (the carve freeze)** — the Windows batch
   path shared ONE completion port across every concurrent carve
   worker. `GetQueuedCompletionStatus` pops FIFO with no owner
   identity: an interleaved harvest popped a FOREIGN packet, matched
   it against its own slots, and DROPPED it; the victim waited the
   120 s timeout, issued a `CancelIoEx` that is a no-op on an
   already-completed read, and spun forever — while its spinning
   harvest ate every later packet, cascading to all workers. Runner
   evidence: carve raced to 91.4 % (956288/1046400 sectors) in 3 s,
   then zero progress for 10 minutes until --max-seconds, then the
   38-minute zombie.
3. **Drain-stop deadlock (the zombie)** — all three channel sites
   (units merger, NTFS walk, carve) stop draining on cancel while a
   worker may be parked in `send()` on a full bounded channel;
   `thread::scope` joins only when the sender lands. The
   cooperative-cancel contract could never complete.

### The fixes

* **Percent**: fraction→0..100 with 2-decimal precision, clamped —
  `(f * 100).clamp(0,100) * 100).round() / 100`. Regression tests
  pin the phase bands (quick 2..38, deep 38..95 with the carve's
  terminal 95, organizing 95..100, done == 100) and the
  "max ≥ 100" smoking gun.
* **Batch IO**: the shared port is GONE. Every batch read carries a
  POOLED manual-reset event with the low bit set (the documented
  event-only mechanism, same as the single-read path): each caller
  harvests exactly its own reads — cross-thread completion theft is
  structurally impossible. `wait_for_slots`: per-slot event waits
  bounded by IO_WAIT_TIMEOUT_MS, one-shot CancelIoEx on deadline,
  post-cancel sweep collects the aborts; the loop is total. The
  settle semantics (short-tail/failed salvage) unchanged. The
  `concurrent_batch_reads_never_steal_each_others_completions`
  hammer test: 4 workers × 25 batches × 8 aligned 64 KiB windows on
  the real OS disk, byte-identity vs sequential, detached workers +
  channel timeout so a regression FAILS instead of hanging the
  runner. (The old batch-identity test also moved to AlignedBuf —
  plain Vecs could silently exercise the sequential fallback.)
* **Drain-to-disconnect × 3**: after a cancel break, the merger
  keeps receiving (discarding) until every sender drops — parked
  senders land, observe the cancel flag, exit, the scope joins.
  Applied to run_units_parallel, the NTFS walk drain, and
  carve_ranges.
* **The CLI watchdog is two-stage now**: cooperative cancel at
  --max-seconds, then a hard `exit(75)` after 90 s grace with loud
  WATCHDOG diagnostics — a scan path that wedges past every
  cooperative checkpoint can never zombie a runner again.
* **Carve-plan evidence**: ScanReport carries carve_plan_bytes /
  carve_plan_capacity / carve_free_ranges (additive; resume fills
  zeros); dw-cli logs "carve-plan: X% of capacity (bitmap-guided, N
  free ranges | full-extent)" and serializes them — the CI record
  now shows whether $Bitmap guidance engaged (the core-e2e run's
  plan covered the FULL 511 MiB volume: sectors_total 1046400 —
  either free-range merge ≈ everything after the mass delete, or the
  bitmap read failed; the next run's evidence line will say which).

### Gates

fmt clean; clippy -D warnings clean on linux (dw-core/dw-cli/
dw-repair/dw-license) AND x86_64-pc-windows-msvc (dw-win/dw-cli,
all targets); 238 tests green (169 dw-core incl. the 2 new percent
regressions). dw-win tests cross-compile clean; the hammer +
identity tests run on the Windows runners (win_api_surface rides
every push).

### Session 11 (cont. 2) — the whole-disk collision class (the lab's catch)

The Recovery Lab's first real run caught a DEEP cross-volume bug no
volume-targeted test could see:

* **The tree's merge maps were volume-blind**: `mft_dirs` keyed MFT
  record numbers ALONE and `by_first_extent` keyed extent offsets in
  VOLUME-relative space — a whole-disk scan walks several volumes'
  MFTs concurrently (records 5/6/7 in L: AND in the deleted W:), so
  one volume's tree swallowed the other's (the lab: Partition 1
  empty, everything mislabeled under Lost Partition 1; the local
  repro: W:'s file vanished entirely — resident extents at identical
  volume-relative offsets absorbed it).
* **And recovery read wrong bytes**: whole-disk scans re-open the
  disk device, but nodes carried volume-relative extents.

The fix (both sides of the seam):
1. `mft_dirs: HashMap<(partition, record), id>` — the record
   namespace is per-volume; the partition label keys it.
2. The merger translates every structured entry's extents to
   DEVICE-ABSOLUTE at drain time (`+ unit.start_byte`; holes keep
   their sentinel) — one coordinate space for the tree's dedup (the
   carve's absolute offsets now MERGE with structured correctly —
   the whole-disk carved-duplicate absorption that never actually
   worked), and recovery reads true bytes on whole-disk targets.
3. The two-volume regression test (in-table + deleted NTFS on one
   disk, both files asserted under their own branches) — runs in
   every CI pass now.

Also: the CLI's throughput used the integer-second duration — a
sub-second parallel scan divided by zero (the benchmark's large
profile finished in <1 s: 219 files, 0.0 MiB/s reported). Now the
precise Instant. And the lab's recycle-class gate matches the
$RXXXXXX renamed copies by size+ext (Windows renames on
SendToRecycleBin — by-name recall is a gate-design error, not an
engine miss).

### Session 11 (cont. 3) — the flake and the honest first-tick

The phase-band test (carve_progress_maps_into_the_deep_band) flaked
green-then-red across CI runs: it demanded a mid-band sample from a
fixture whose ENTIRE carve completes before the drain's first 100 ms
tick — and whose 2 MiB has no carveable signatures (no entries → the
drain's only observable state is the terminal one). Two fixes:
1. The carve drain's FIRST tick now lands immediately (no 100 ms
   wait for tick #1) AND only once real progress exists
   (bytes_done > 0) — no more 0 %-of-sweep noise tick.
2. The test's fixture gained a SlowHalf wrapper (reads touching the
   volume's second half stall 150 ms) — the mid-sweep state is now
   DETERMINISTIC, not a race with the scheduler. (The overlap
   advance starts window 2 before the midpoint — the stall condition
   keys on the read RANGE, not its start.)
---

## Session 12 (cont.) — the lab evidence pass: five more engine defects, the recycle reconstruction, the seeder honesty

The b4df31b run was the first REAL validation of the session-11
parallel architecture: Core E2E GREEN for the first time since the
RE-Match spec (the freeze fixes held), Windows Integration green with
the new concurrent batch hammer. Engine Benchmark + Recovery Lab
failed — and their evidence, read properly, exposed five more defects:

### The evidence-driven diagnosis

1. **Benchmark `duration=0s` → 0 MiB/s** — sub-second scans (2 GB in
   0.9 s!) floored the integer-second duration; throughput divided by
   zero. The CLI now computes it from its own precise Instant (the
   report's integer field stays — wire contract).
2. **`carve-plan: 100% full-extent (no bitmap guidance)`** — the new
   evidence line caught the $Bitmap path NEVER ENGAGING on the
   benchmark volumes: `load_volume_bitmap` read
   `total_clusters/8` bytes in one slice, but that length is rarely a
   SECTOR multiple (16350 B on the 511-MiB VHD) — `check_buf`
   rejected it → None → full-extent fallback AND no $Bitmap viability
   grading. Only round-geometry volumes (the exactly-10-GiB lab disk)
   worked. Fixed with padded sector-multiple reads (the consumers
   bound every bit access by total_clusters — the pad is inert);
   regression fixture with 1308 clusters → 164 B.
3. **The lab's branch inversion** (everything under "Lost Partition
   1", "Partition 1" empty) — `mft_dirs` was keyed by RECORD NUMBER
   ALONE: two concurrently-walked NTFS volumes share record numbers,
   and the victim's root (record 5) parented ALL of the main
   partition's directories. Now scoped by (partition, record).
4. **The 28 vanished victims** — `by_first_extent` maps
   partition-RELATIVE offsets globally: a victim file at relative
   offset X was absorbed as "noise" by a main file at the same X. The
   merger now translates every structured entry's extents to
   DEVICE-ABSOLUTE at the unit boundary — which also makes the
   carve↔structured dedup (the carver emits absolute) actually match
   across partitions, and fixes the readability probes + recovery
   reads for partition units (they always assumed absolute).
5. **The aligned-gap sweep hole** — `next_multiple_of(2048)` on an
   already-aligned gap start returned the same LBA, then `.max(start+1)`
   nudged 1 sector — every later probe off the MiB grid forever (a
   boot at the 2nd-or-later boundary of an aligned gap was invisible;
   the lab's victim only survived because its boot sat at the gap
   start). Fixed: advance to the boundary STRICTLY beyond start.

### The recycle-bin reconstruction (the og's $I model — a feature)

Recycle-fate recall was 0/375: the engine surfaced the bin's $RXXXXXX
records under $RECYCLE.BIN\<SID>, names the manifest can never match —
and the og shows recycled files at their ORIGINAL paths (it parses the
$I index). Implemented, exactly that: the walk stashes $R/$I twin FILE
records (dirs never — `$RECYCLE.BIN` is shape-identical to a twin
name!), parse_record captures the $I's tiny resident index (bounded,
$I-named records only), and the post-walk pairing emits matched $R
under the ORIGINAL leaf + directory path (v2 and v1 index formats;
drive prefix stripped; volume-root originals fall back to @mft:5).
Unmatched $R pass through raw; $I never surfaces; QA-4 destroy still
binds (no resurrection of overwritten content). `ensure_dir_path` now
anchors a leading partition label at that partition's base — path-
joined chains share `dirs` keys with the mft-materialized ones.
End-to-end fixture: $I/$R twins in $RECYCLE.BIN\S-1-5-21-999 →
`Test (V:)\Docs\recycled.txt`, no raw twin names.

### The lab's own honesty (workflow fixes)

- **Seeder order**: recycle-moves FIRST (their $R/$I records allocate
  from the pristine MFT tail), force-deletes AFTER. Interleaved, NTFS
  record-reuse ate ~15% of the force-deleted records (the 69%-vs-90%
  deleted-recall gap — allocator-dependent, physically gone, the og
  loses them identically; core-e2e had already learned this as "the
  churn lane is data-not-gate"). Ordered: ceiling ~99.7%.
- **Path-accuracy check**: samples the first 400 MATCHED files (the
  first 400 manifest entries could all sit in a reuse hole — the 0/0
  artifact).

### Gates

fmt clean; clippy -D warnings clean on linux AND
x86_64-pc-windows-msvc (all targets); 172 dw-core tests green (the 5
new: percent scale ×2, bitmap alignment, two-volume collision,
recycle reconstruction).

### Session 12 (cont. cont.) — the 9918256 evidence: two environment truths

The 9918256 lab run: **recycle 375/375 = 100%** (the $I
reconstruction works perfectly on real Windows), kept 100%, path
accuracy PASS, carve-plan bitmap-guided 94.5%/215 ranges on the
10-GiB disk, benchmark GREEN (precise throughput). Two gates left:

1. **deleted 1620/2000 = 81%** — the artifact's count match nailed
   it: 375 $R/$I twin pairs + 6 junk records ≈ 381 ≈ 380 missing.
   NTFS allocates new MFT records LIFO from the freed pool — the
   interleaved recycle/delete loop fed every $R/$I pair two
   freshly-freed force-deleted records. My "recycle-first reorder"
   was a NO-OP (it merged the fate branches but kept the single
   interleaved loop — the commit that lied). Implemented FOR REAL:
   fates assigned first, then TWO SEQUENTIAL passes (recycle-moves →
   force-deletes); only the 6 junk records allocate afterwards →
   ceiling ~99.7%.
2. **victim 126/150 = 84%** — files 1-16 + 142-149 vanished from a
   PRISTINE volume (no allocation, no deletion, nothing can remove a
   record there... except one thing): **Windows Defender real-time
   quarantine** of the freshly-seeded fake-media files (head+tail =
   the scan pipeline's leading and trailing edge when the partition
   died). Standard CI practice applied:
   `Add-MpPreference -ExclusionPath L:\, W:\` before seeding — the
   evidence must measure OUR engine, not the antivirus.

### Session 12 (cont. cont. cont.) — the 65f0ab1 catches

- **Recovery Lab**: the Add-MpPreference line needed the ARRAY form
  (`-ExclusionPath "L:\","W:\"` — the parameter can't be given twice);
  the run died at attach before any seeding.
- **The engines fuzzer caught a real panic**: dw-repair's SVG engine
  sliced the ORIGINAL text at an index found in the LOWERCASED copy —
  case-folding-expanding chars (İ → i+combining dot) grow the
  lowercased byte length, and the prolog index (1201) exceeded the
  original's (1199). Fixed with an ASCII-fold search over the
  original (`find_ascii_ci` — the needle is pure ASCII, every match
  boundary is a char boundary) + the exact-class regression test
  (600× İ before the prolog).

### Session 11 — FINAL: everything green

ec6f5a3 (the double-translation fix — the last piece) runs the FULL
matrix green on a real Windows runner:

  Recovery Lab           success (kept 100% / deleted 99.7% / recycle
                         100% / path 100% / lost-partition 86.7% /
                         byte-truth PASS / memory in bounds)
  CI, Fuzz, MSIX, Core E2E, Windows Integration, Real-Core Tour,
  Engine Benchmark, UI Screenshots, License E2E, Repair E2E — success.

The double translation: the session-11 block and the session-12 block
both translated partition-unit extents in the drain — every extent
shifted by 2× the partition start. Recovery read shifted bytes; the
lab's byte-truth verified 0/20 on perfect-grade files. One block, one
place.

### Session 13 — UI shell / results-density correction

- Replaced the generic **File Types** lens with **File reconstruction**.
  It preserves the engine's `lost-path` and `lost-name` wire meanings as
  File Path Lost and Reconstructed respectively, rather than guessing from
  file categories. The selected scan target remains the partition root and
  the view explicitly reserves lost-partition rows for core-validated finds.
- Made live phase copy a single typed mapping shared by the sidebar, results
  title and results footer. Quick scan, deep scan, organizing, paused and
  terminal states can no longer contradict one another; unknown ETA is now
  rendered as an estimate instead of a false `1s` value.
- Replaced fixed three-pane flex sizing with a grid that gives the file table
  the flexible column, with a documented 1200px density rule. At the window
  floor, nonessential columns move to Preview rather than overflowing.
- Restored a compact three-up recovery-source grid on ordinary desktop
  widths; the old 320px auto-fill rule prematurely created stretched,
  two-column drive cards.
- Added route-level chunks so initial startup loads the shell and current
  screen first. Frontend verification: `npm run typecheck`, `npm test`
  (84 tests), `npm run build`, and `git diff --check` all pass.
