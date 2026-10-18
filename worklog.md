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
