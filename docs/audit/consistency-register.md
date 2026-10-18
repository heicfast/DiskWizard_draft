# DiskWizard — Consistency Register (docs ⇄ code)

> **Phase 1 deliverable** (MASTER_PROMPT §10 PHASE 1.2). Method: every
> claim below was verified against the current code (file:line) and the
> research corpus (FULL-read documents per
> [docs/maps/doc-tree.md](../maps/doc-tree.md)). The QA register
> (`docs/QA-REGISTER.md`) carries the runtime/evidence flags; this
> register maps **documentation mandates → implementation state**.

## 1. MASTER_PROMPT §7 defect register — item-by-item verdicts

| # | Defect (master prompt) | Verdict | Evidence (verified this session) |
|---|---|---|---|
| 1 | `RawHandleDevice` seek+read under OVERLAPPED | **FIXED** (prior session, v2 rewrite) | `dw-win/src/blockdev.rs`: positioned `ReadFile` (offset in `OVERLAPPED`), pooled manual-reset events (ResetEvent fix F4), `IOCTL_DISK_GET_LENGTH_INFO`; regression-tested per QA-REGISTER §1 F4 |
| 2 | `FILE_FLAG_NO_BUFFERING` with plain `Vec<u8>` | **FIXED** | `AlignedBuf` (interior-offset alignment) + aligned staging for unaligned buffers; cache chunks allocate aligned |
| 3 | `BlockDevice: Sync` racy shared position | **FIXED** | No shared file pointer remains (positioned reads); sparse-salvage per sector; stress-tested via VHD workflow |
| 4 | `diskN-partM` rows resolve to whole-disk | **FIXED** (session 4) | `scan_units` slices partitions (SubDevice + table-index match + MBR slot fallback); regression tests for volume-target dispatch (F7) |
| 5 | Whole-disk targets don't dispatch across partitions | **FIXED** | Multi-partition dispatch with progress weighting + per-partition branches + one whole-device carve sweep |
| 6 | Progress polling reports zeros | **FIXED** (F9) | `get_scan_progress` serves the worker's live engine snapshot (percent/files/bytes/sectors + rate ETA) |
| 7 | Carving reads 1 sector / 32 KiB cache | **FIXED** (carver v2) | 8 MiB aligned windows, 64 KiB overlap, memchr SIMD-friendly matching, sparse window reads |
| 8 | Validators allocate 256 MiB…2 GiB | **FIXED** (carver v2) | Bounded streaming Prober walks, fixed ~1 MiB scratch; LENGTH caps, never allocations |
| 9 | ext, HFS+, ReFS detected but unimplemented | **FIXED** (session-5 parallel agent) | `fs/ext` (1262 lines, FUN_180004710 gates byte-exact), `fs/hfsplus` (1008 lines, "H+"/"HX" gate), `fs/refs` (425 lines, DBR gate + SUPB/CHKP/MSB census). My independent cross-check vs the research constants: ext gates (0xEF53, first_data_block cross-checks, 1024<<log, 64-bit block count, desc_size 0xFE under 64BIT) = kernel-correct and research-faithful; ReFS MSB tree records remain honest-boundary (carving covers content; M3 plan in docs/TODO.md) — the product marketing surface must gate the "all file systems" claim until the ReFS tree walk lands (see QA-3 note below) |
| 10 | File repair TODO in shipped paths | **COMPLIANT-BY-DESIGN** | Phase 2 per master prompt §2 out-of-scope; full spec captured at `TODO/phase-2-advanced-repair.md`; interface contracts exist (repair toggle through the funnel; `enable_file_repair` param) |
| 11 | README vs code drift (simulated vs real) | **FIXED** | README v0.2 describes the real core default + sim dev-only; this session added the same discipline for licensing docs (§3 below) |
| 12 | Worklog overclaims | **HELD** | worklog.md sessions are evidence-cited; session-5 entries carry verified-gates lines; this register + QA register keep the honesty loop |
| 13 | Dead/duplicate code | **ONGOING** | No TODO/FIXME/stub markers in shipped Rust (grep clean); mock/sim remains by design (§9) |

## 2. Research-contract gaps still open (P0/P1 — the session-5 queue)

| Item | Research mandate | Code state | Owner |
|---|---|---|---|
| **QA-1 real-core tour** | Release-truth: install/launch release build, tour the REAL core (scan → results → recovery), capture, verify | **LANDED this session**: `.github/workflows/real-core-tour.yml` (build WITHOUT sim + ci-tour license seed; VHD seeded with kept + deleted files; title-synced frames; frame assertions; byte-compare of both seeds; artifacts). First run in flight at push time — green required before the flag closes | this session |
| **QA-3 engines** | ext2/3, HFS+, ReFS full read/scan/recover | Engines landed (§1 item 9) BUT fixture coverage is unit-level; the VHD E2E covers NTFS only. ext/HFS+ need mkfs-generated fixture images in CI (Linux job) and ReFS needs a `Format-Volume -FileSystem ReFS` leg on the Windows runner (Server images support it) — tracked as the immediate next CI task | next |
| **QA-2 preview cap** | Research contract: 100 MB input cap, 15 s video window (`UIUX/10-preview-subsystem.md`) | `preview_file` caps at 16 MiB input / 2 MiB payload; honest "plays after recovery" copy covers video. Decision log: OQ-5 (owner call; not silently resolved) | owner |
| **QA-4 $Bitmap viability** | Per-cluster $Bitmap test, `intact == total → destroy` (`SearchDLL/ntfs-engine-algorithms-v2`) | dw-core grades via extent readability + deep pass; the $Bitmap cross-check not implemented | next session |
| **QA-6 65% reduction proof** | Essential mode ≈ 65% reduction (product changelog) | Mechanism landed (content-key Version marking + same-region absorb + client-side essential mode); end-to-end fixture proof missing | next session |
| **QA-8 VSS disclosure** | §5.1 item 8: VSS or disclosed live-scan limits | No VSS; no UI disclosure of live-scan consistency limits | this session (queued) |
| **QA-17 bytes_found > capacity** | Stat honesty | Clamp at tree-build + dedupe carve overlaps at merge | this session (queued) |
| **QA-11 IOCP queue depth** | §5.1 item 3/5: completion-based worker pool, adaptive QD | Current: synchronous positioned OVERLAPPED (correct, QD-1). The parallel-pipeline escalation is M3 per CORE-PLAN; ADR-1 rev2 language covers the decision | next session |

## 3. Documentation drift fixed this session

| Doc | Was | Now |
|---|---|---|
| `docs/ARCHITECTURE.md` key-format section | `DW` prefix described as the frozen contract | `DB` prefix + §8 posture + swap procedure (config module path) |
| `docs/QA-REGISTER.md` §3 | described the not-yet-deployed diskwizard-license endpoint as the wiring | §8 port recorded verbatim (endpoint, keys, headers, UA, iss/aud, prefix) + one-file swap note; QA-16 reframed (live E2E against the DiskGenie worker with a real DB-key) |
| `README.md` demo keys | `DW-…` | `DB-…` (matching `key_shape_valid`) |
| `worklog/open-questions.md` OQ-3 | adopted, fix pending | RESOLVED with evidence pointers |
| `docs/maps/*` (Phase 0) | did not exist | doc-tree + coverage ledger + context-synthesis + five digests (299/299 corpus) |

## 4. Hallucination watch (claims made by prior agents, re-verified)

- "All four workflows green on 6edfddb" — **TRUE** (GitHub API, verified at session start).
- "125 dw-core tests green" (parallel agent's close-out) — **TRUE** locally: 126 dw-core (one more landed with 8e8a6fd-era work) + 27 dw-license.
- Engine constants ("FUN_180004710 gates byte-exact") — **VERIFIED** against my FULL read of
  `SearchDLL/ext4-engine--deep-dive.md` + `ext4-hfs-engine-algorithms-v2.md`; one nuance
  recorded: the engine uses the kernel-correct `s_desc_size@0xFE` under the 64BIT incompat
  flag, while the vendor decompile quotes buffer-relative offsets (+0x196) — the research
  artifact's constants and the implementation agree semantically (both select 32/64-byte
  GDT entries by the 64-bit feature).
- The "positioned OVERLAPPED" claims in worklog session 4 — **VERIFIED** in
  `dw-win/src/blockdev.rs` (read this session; no seek+read path remains).

## 5. Register discipline

Every future session appends verdicts here when it touches a §7 item or a
research-contract gap; a fix without a register line is incomplete per the
master prompt's evidence-driven rule.
