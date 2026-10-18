# Phase 7 hardening cycle — multi-POV review (session 6, commits d71570d..b84a868)

> Scope reviewed: the runner-caught defect batch — ext rev-1
> superblock offsets (F14), fuzz-harness overflow (F15), IOCP test
> race (F16), real-core-tour silent skips + the swallowed
> `start_recovery` error (F17), the §5.1-10 benchmark record, and the
> QA-register/worklog/doc syncs. Method: each POV re-read the diffs,
> the runner logs cited as evidence, and the governing MASTER_PROMPT
> sections. Findings below are what each reviewer would file.

## Senior Developer

* **F14 (ext offsets) is the cycle's most important fix and the
  hardest class**: the parser, the probe, AND the hand-built fixture
  shared the same wrong layout, so every test was self-consistent
  and wrong. The fix's regression strategy is correct — embedding
  the REAL 352-byte mkfs.ext2 superblock as byte-level ground truth
  (`superblock_offsets_match_real_mkfs`) means any future offset
  drift fails against reality, not against a mirror. Verified: the
  embedded bytes match the e2fsprogs 1.47.4 dump field-for-field
  (rev 1 @0x4C, first_ino 11 @0x54, inode_size 0x0100 @0x58, compat
  0x38 @0x5C, incompat 0x02 @0x60, ro 0x03 @0x64).
* The probe fix (`fs/mod.rs` compat 0x5C / incompat 0x60) keeps the
  discrimination semantics (extents→ext4, journal→ext3) — ext3 was
  silently mislabeled ext2 before; now pinned by
  `ext_probe_discriminates_true_layout`.
* F16's fix (static `VHD_LOCK`) is the right scope: the race was
  test-vs-test, not engine-vs-engine; poisoning is deliberately
  ignored so one panicked test cannot cascade. The enriched assert
  (first divergence offset + both values) converts any future
  failure from "diverges" to diagnosable evidence.
* The `beginRecovery` error path (F17) follows the store's existing
  state shapes — no new state machine, the summary panel gets an
  `error` field with a rendering variant. Minimal, reviewable.
* Nits: none blocking. The `--warn` CSS variable fallback in
  `RecoverOverlay`'s badge style degrades gracefully if the theme
  lacks the token.

## Senior Product Manager

* The user-visible win is honesty: a recovery that cannot start now
  says WHY ("Recovery Could Not Start" + the backend's typed error,
  e.g. DESTINATION_ON_SOURCE) instead of a green "Successful" check
  or an eternal spinner. This directly serves §6.2's honest-progress
  mandate and the paying-user trust bar.
* The ext fix is a marketing-truth item: "Restore data from all
  supported file systems" (§6.1) is only true if real-world ext2/3/4
  volumes actually scan — real mkfs output now does (CI gate green),
  not just synthetic structures.
* The tour's t99 fail frames make the release-evidence artifact
  self-diagnosing: a red tour now names its broken stage in the CI
  title instead of a generic RECOVERY_OUTPUT_MISSING minutes later.
* No scope creep: everything in this cycle is a fix or evidence
  closure; no new features were snuck in.

## Senior Security Researcher

* F15 is a hardening fix with a security edge: the fuzz harness
  aborted (not just failed) on hostile input — a fuzzer-killing
  harness blinds the entire §12.3 program. The fix rejects absurd
  LBAs with a typed error (checked u64 math, `saturating_mul` in the
  error path), matching the real device boundary's behavior.
  Engines remain the next line: any engine-INTERNAL unchecked u64
  multiply that overflows under fuzzing will now surface as an
  engine-source panic the fuzzer can attribute correctly.
* F14 removes a parser operating on wrong offsets — wrong-offset
  parsers are the classic OOB-read/absurd-value generator. The
  engine's validation gates (magic, geometry cross-checks, GDT
  bounds) were already correct; now the field decode beneath them is
  too.
* No new unsafe, no new dependencies, no secret material in any
  diff this cycle. The benchmark doc contains only runner hardware
  numbers.

## Senior QA

* Every fix carries its catching regression — the cycle's discipline
  test: F13 → the iocp test asserts sector size through the physical
  path; F14 → real-superblock byte-truth + probe discrimination +
  the CI real-mkfs gate itself; F15 → the fuzz suite re-runs the
  exact corpus shape that aborted; F16 → serialized tests +
  divergence-offset diagnostics; F17 → the t99 frame protocol +
  workflow TOUR_FAIL detection.
* F16's flakiness analysis is timing-evidenced (14:18:11 panic vs
  14:18:38 sibling completion) and the same test passed untouched on
  the next run — the race explanation fits both observations, which
  is what separates a diagnosis from a guess.
* Remaining QA debt, explicitly not hidden: the Real-Core Tour has
  never been green — the t99 instrumentation in 74d599a will name
  the broken stage on the next run; the fix for the named stage is
  the next action, not assumed done. Fuzz targets run 90 s/push +
  weekly deep — corpus growth (and crash-artifact triage) is
  ongoing.

## Real Paying User

* "I picked a destination and clicked recover, and it said
  Successful — but there was no folder" is the worst possible paid
  experience, and it is now structurally impossible: the command's
  failure renders as an explicit error dialog naming the reason, and
  the release tour would catch any regression of that contract
  before shipping.
* ext2/3/4 drives (common in external disks from Linux tools, dual
  boot, NAS rescues) now actually scan — that is a capability I
  paid for, verified against real filesystem-formatter output.

## Instruction-Following Check (MASTER_PROMPT re-read)

* §12 SOP items 1 (byte-level structure tests from real reference
  data — the embedded superblock), 3 (fuzzing, zero panics — F15),
  7 (runner integration with logs as artifacts), 8 (install test /
  real-core tour with code-level verification, not VLM alone) — all
  advanced this cycle.
* §5.1 item 10: benchmark numbers recorded with methodology and
  honest interpretation (no overclaiming — the cache-backed-VHD
  caveat is explicit).
* §14 commit protocol: three coherent pushes (d71570d, 74d599a,
  ee53823/b84a868), each with area-prefixed subjects and evidence
  in the body.
* §16 context recovery: executed at session start (this session
  began as a crash continuation).
* Deviations: none unaddressed. The tour's green run remains open —
  tracked as the active P0 until the next runner cycle reports.
