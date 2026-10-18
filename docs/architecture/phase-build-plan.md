# DiskWizard — Phase Build Plan (M0 → full production engine)

> **Phase 2 deliverable** (MASTER_PROMPT §10 PHASE 2.2). Milestones,
> deliverables, dependencies, tests, exit criteria — from zero to the
> full production-grade engine. Supersedes CORE-PLAN §8 (the session-3
> milestone list) as the live plan; each completed milestone carries its
> evidence pointer. The master prompt's Phase numbering (0-9) maps to
> the M-scale below.

## M0 — Context & plan (master Phase 0/2) ✅

- Deliverables: doc-tree + coverage ledger (299/299), context-synthesis,
  donor digests, target-codebase-tree, this plan, SOP, audit registers.
- Exit: ✅ all artifacts pushed; corpus synthesis from memory verified.

## M1 — Core I/O + engines v1 (master Phase 3/4) ✅

- BlockDevice seam + AlignedBuf + positioned-OVERLAPPED RawHandleDevice
  (§5.1 items 1,2,4,9), partition slicing + whole-disk dispatch (§5.2),
  NTFS/FAT/exFAT engines, carver v2 (bounded streaming), recovery,
  sessions, SMART, BitLocker unlock, progress truth.
- Tests: 126 dw-core unit tests; VHD end-to-end (windows-integration.yml)
  with byte-compare; spc=8 + volume-target regressions (F7/F8).
- Exit: ✅ four workflows green on the real core (6ca58c0 → current).

## M2 — Engine completion + release-truth (master Phase 5/6 + QA P0s)

| Item | Status | Exit criteria |
|---|---|---|
| ext2/3 + HFS+/HFSX engines | ✅ landed (session-5 parallel agent) | fixture images in CI (mkfs.ext2/3 + mkfs.hfsplus on ubuntu job) scanning + recovering ground-truth files — **OPEN** |
| ReFS engine (DBR + page census) | ✅ landed, honest boundary | a real ReFS VHD leg (Server runner `Format-Volume -FileSystem ReFS`) proving DBR/page detection on a live volume — **OPEN** |
| QA-1 real-core tour | ✅ landed this session (workflow + ci-tour seed + dual-mode driver + byte-truth) | first GREEN run on `main` (in flight at writing) — **PENDING CI** |
| Licensing §8 re-point | ✅ landed this session (config module) | live E2E against the DiskGenie worker with an owner-issued DB-key (QA-16) — **PENDING OWNER** |
| Dedup/essential mode | ✅ mechanism landed | end-to-end ~65% reduction fixture (QA-6) — **OPEN** |
| SDR access-restore | ✅ RawFixer model + write path | damaged-DBR fixture test in CI (corrupt sector 0, restore from backup, verify) — **OPEN** |
| NTFS M2 findings | resident recovery ✅; $ATTRIBUTE_LIST forking, hole-aware extents | fixtures with fragmented MFT + sparse files — **OPEN** |

## M3 — Performance & hardening (master Phase 7 + §5.1-3/5)

- IOCP completion-based worker pool (QA-11): queue depth 4-16 by device
  profile, benchmark harness (`docs/benchmarks/io-backend.md` numbers),
  race stress, 24 h soak + crash-resilient scan journal (QA-12).
- Fuzzing (cargo-fuzz) on every parser; property tests for carving
  overlap/dedup; RSS/leak monitors.
- $Bitmap viability scoring (QA-4) per the v2 artifact; exFAT damaged-FAT
  chain re-derivation tests (QA-5); SMART threshold breadth (QA-10).
- Exit: benchmarks recorded + regression gates in CI; zero panics under
  fuzz corpus; all P1 QA flags closed or re-graded.

## M4 — Repair suite (master Phase 2-out-of-scope → TODO/phase-2 spec)

- Photo repair (JPEG marker walk, PNG BasisRepair 20/30/50/80, GIF
  re-chain, TIFF vote), document repair (PDF xref synthesis, OOXML
  shell, HWPX), video repair (atom grammar + sample-moov + regroup).
- Interface contracts + repair-during-recovery counters wire first;
  engines behind the same engine-seam discipline. Exit: repair fixtures
  (corrupted samples per format) + before/after preview verification.

## M5 — Release parity & store readiness (master Phase 8/9)

- NSIS + MSIX install tests on runners (install → tour → artifacts),
  VLM frame verification + code-read verification (both, per §12.8).
- The "all file systems" marketing claim gated until the ReFS tree walk
  lands (or the claim is scoped honestly in the store listing).
- Live license E2E; dependency audit gates (`cargo audit`/`deny`,
  `npm audit`) wired; handoff docs current.

## Dependency order (critical path)

M2-fixture-CI → M3-IOCP (needs the CI fixtures for honest benchmarks) →
M4 (independent; UI contracts land anytime) → M5 (last, everything
green). The parallel-agent pattern continues: engine work and CI/tooling
work proceed on separate files wherever possible; the worklog remains
the coordination point (5 rebases this session, zero lost work).
