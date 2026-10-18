# Phase 4+ Reviews — Session 5 (the engine-completion batch)

Multi-POV gate per `MASTER_PROMPT.md` §11, covering: the ext2/3/4,
HFS+/HFSX, and ReFS engines; the dedup/nonessential filter; the
carver multi-hit fix; the SDR/repair engine; the BitLocker BEK
credential.

---

## Senior Developer — correctness, memory, concurrency, structure

**Verified green**: 125 dw-core tests, clippy `-D warnings` clean,
rustfmt applied; CI green through d644c85→90c27e4 (the one
Windows-compile break was caught by the VHD job and fixed same-day:
raw access constants + OVERLAPPED positioned writes + sector-size
awareness in the repair handle).

**Findings (this batch)**:

1. **CRITICAL — fixed**: the carver's post-emit `break` limited
   carving to ONE file per signature per 8 MiB window. Found by the
   new content-key test; fixed with an emitted-position set
   (regression test: three JPEGs in one window → three carves).
2. **ext dirent slack scan** — both kernel deletion shapes covered
   (merged rec_len; zeroed inode). The name-only shape (shape B)
   emits metadata-only entries; content surfaces via the orphan
   sweep separately (documented M3 item 4 — association heuristic).
3. **HFS+ B-tree bounds** — leaf-chain walks are hop-bounded
   (MAX_LEAVES 4M), record counts capped at 1024/node, fork reads
   capped by MAX_TREE_BYTES (64 MiB). Fork zero-fill on unreadable
   runs (salvage model) — B-trees partially parse rather than fail.
4. **ReFS census cap** — 256 MiB census bound on damaged volumes;
   no fabricated record parsing (the honest-boundary call — see
   Security POV).
5. **Repair transactionality** — verify-or-rollback on failure;
   every overwritten sector snapshotted. The write path is the
   ONLY write-enabled handle in the codebase (scan stays
   read-only per §5.1.8).
6. **Deferred (documented in docs/TODO.md)**: ext4 journal
   reanimation, HFS+ journal replay, ReFS MSB tree enumeration,
   NTFS $ATTRIBUTE_LIST forking, hole-aware extents, resident-
   content recovery — each with its evidence trail.

**Verdict**: PASS with the deferred register acknowledged.

## Senior Product Manager — Section 6 coverage

- §6.1 "Restore data from all supported file systems" — the
  Section 6.3 matrix is now fully covered at engine level:
  FAT12/16/32 + exFAT + NTFS/NTFS5 (deep, prior sessions) and
  ext2/3/4 + HFS+/HFSX (full structured walks) + ReFS (detection +
  page census + carving; MSB trees documented as M3 with the
  evidence gap stated openly).
- §6.1 dedup/nonessential filtering (~65%): the complete contract
  now runs core-side (region-identical absorb + content-key
  Version marking) + client-side (essential mode hides
  lost-name/version origins; junk prefixes) + the user toggle
  ("complete results any time") — the Settings + Results UI
  surfaces shipped in the UI batch.
- §6.5 SDR: real diagnosis + confirm-gated repair with
  verify-or-rollback; the "repair not applicable → file recovery"
  fallback is the honest CarveOnly verdict. BEK-file unlock
  completes the BitLocker credential surface (password, recovery
  key, key file).
- Phase 2 repair: `TODO/phase-2-advanced-repair.md` captures the
  full spec (photo/document/video/archive grammars, all
  evidence-cited) per §2's mandate.

**Gap**: ReFS structured results (users see carved results, not
named trees, until M3) — disclosed, not hidden.

**Verdict**: PASS with the ReFS disclosure.

## Senior Security Researcher — hostile input, hygiene

- All three new engines parse hostile media with: bounds caps
  everywhere (dirent counts, B-tree hops, extent depths, census
  ranges), cycle guards (visited sets in extent/indirect walks),
  geometry gates (vendor validation sets — the ext first_data_block
  cross-checks, HFS+ fork clamps, ReFS cluster-size whitelist).
  No panics on corrupt fixtures (chain-break + truncation tests).
- Repair path: explicit confirm parameter; write handle scoped to
  the repair commands only; undo log restores on verify failure;
  no path traversal (LBAs are numeric, plan comes from the engine
  not the client).
- PowerShell surfaces (unlock/BEK): single-quoted escaping
  ('' doubling) on all interpolated strings; -NoProfile
  -NonInteractive fixed args.
- No secrets in code; the licensing keys stay in the config module
  (unchanged this batch).

**Verdict**: PASS.

## Senior QA — coverage, determinism, regressions

- 20 new tests this batch (ext 11, HFS+ 5, ReFS 9→net, dedup 3,
  repair 6, minus consolidation). Every fixed defect has its
  regression test — including the carver multi-hit bug (the exact
  class of bug the "write the test that would have caught it" rule
  demands).
- Fixture strategy: in-code image builders for ext2/ext4 (kernel-
  exact deletion shapes), HFS+ (TN1150 layouts), ReFS (DBR + page
  census), repair (FAT32 backup/scan/undo). Deterministic, no
  network, no OS dependencies.
- Windows runner: the VHD end-to-end continues to verify the NTFS
  path on real hardware; ext/HFS+/ReFS engines are image-fixture
  tested (cross-platform by design — the RamDevice seam).
  **Queued**: extend the VHD workflow with an ext4 image case
  (mkfs in WSL-less mode is non-trivial on the runner; the in-code
  fixtures carry the truth burden meanwhile).

**Verdict**: PASS with the VHD-ext4 case queued.

## Real Paying User

- "My Linux drive shows its real folders and file names now, not
  just carved files" — the ext engine's deleted-name recovery
  (both kernel shapes) and orphan sweep mean deleted files surface
  with names + content.
- "My Mac drive's files recover with both forks" — HFS+ data +
  resource forks both surface.
- "The SD card that prompts to format": diagnosis tells the truth
  (repairable with the exact consequence stated — "your files stay
  untouched"), repair asks for confirmation, and rolls back if the
  result doesn't verify. When it can't repair, it says so and
  offers the deep scan.
- "Fewer files to review": the essential view hides duplicate
  copies and synthesized-name noise; one toggle brings everything.

**Verdict**: PASS.

## Instruction-Following Check

- §6.3 filesystem matrix: engines delivered (ReFS at the
  documented evidence depth — ambiguity logged, not invented).
- §6.1 dedup contract: complete. §6.5 SDR: real. §6.4 repair
  spec: `TODO/phase-2-advanced-repair.md` written.
- §5.1.8 never-write-source: upheld — the repair path is the
  explicit, user-confirmed exception the SDR feature requires
  (owner decision recorded; snapshot + rollback mitigate).
- Zero-hallucination rule: the ReFS scope decision is the material
  test — evidence-level documented, nothing fabricated.
- Worklog + TODO registers updated; commits reference the
  governing artifacts.

**Verdict**: PASS.
