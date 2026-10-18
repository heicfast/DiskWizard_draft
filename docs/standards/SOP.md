# DiskWizard — Engineering SOP (binding for all core code)

> **Phase 2 deliverable** (MASTER_PROMPT §10 PHASE 2.3). Absorbs
> CORE-PLAN §9 (session-3 rules) and extends it to the full
> production bar. These rules are enforced by review checklists
> (§8) and CI gates.

## 1. Documentation-first

- No implementation decision without a research-artifact citation in
  the module/commit doc trail (`docs/maps/doc-tree.md` is the ledger).
- Ambiguities: `worklog/open-questions.md` first, best-match-research
  interpretation second, never silent invention.
- Every fix ships the test that would have caught it (master §7).

## 2. Error handling

- Library layers (dw-core/dw-license): `Result<T, TypedError>` with
  user-facing copy mapped at the command layer only.
- Command layer: stable machine-readable prefixes
  (`ACTIVATION_REQUIRED`, `DESTINATION_ON_SOURCE`, `NO_SCAN_RESULTS`,
  `STALE_GENERATION`, …) — the UI branches on them; never change a
  prefix without a wire-compat note.
- Recovery products maximize salvage: a bad record/sector never aborts
  a walk — skip, count, surface (the vendor's own fixup-skip contract).

## 3. unsafe policy

- `unsafe_code = "warn"` at the workspace level; every `unsafe` block
  carries a SAFETY comment naming the invariant (ownership, lifetime,
  Win32 contract). New unsafe requires a senior-review pass (§8).

## 4. Memory policy

- Carving validators: bounded streaming only — fixed scratch, LENGTH
  caps, never candidate-sized allocations (master §5.1-7).
- `AlignedBuf` for any NO_BUFFERING path; ordinary `Vec` only for
  staged/copied data.
- Per-candidate memory caps are part of the fuzz contract (zero
  unbounded allocations under fuzzing).

## 5. Concurrency

- No shared file position (positioned reads only); workers communicate
  through the scan-state slots + events; every worker thread is named.
- Control tokens: fresh `ScanControl` per run (the F6 lesson — never
  inherit a cancelled token).
- Pause/resume/stop must remain responsive within one chunk (≤ 8 MiB).

## 6. Testing policy

- Unit tests per parser/algorithm with byte-level structure fixtures.
- Integration: VHD end-to-end (windows-integration.yml) is the release
  truth; the real-core tour (real-core-tour.yml) is the UI-funnel truth.
- Fuzz (M3): every parser, zero panics, zero unbounded allocations.
- Both build variants tested on every change that touches the command
  surface: `--features sim` (mock intact) AND default (refusal arms).
- Regressions: every fixed defect grows a named test (F1-F12 pattern).

## 7. Naming, style, hygiene

- rustfmt (stable defaults) + clippy `-D warnings` — zero tolerance,
  run before EVERY push (`cargo fmt --all && cargo clippy --all-targets`).
- Frontend: tsc strict + eslint; no console noise; Fast Refresh clean.
- No TODO/FIXME/stub markers in shipped paths (grep gate); mock/sim
  code is allowed ONLY behind the `sim` / `ci-tour` features.
- Commit messages: `<area>: <what>` + WHY + governing doc path.

## 8. Review checklist (per phase boundary — multi-POV, master §11)

Recorded under `docs/reviews/<phase>-<pov>.md`. A change is done when:

- [ ] Senior dev: correctness, API design, memory safety, concurrency,
      zero clippy warnings, no new unsafe without SAFETY notes.
- [ ] Senior PM: feature set matches §6 exactly; journey intact.
- [ ] Senior security: hostile media bounded; path traversal guarded
      (FsDest.resolve); no writes to source (§5.1-8 enforced server-side);
      secrets hygiene (config module only); dependency audit clean.
- [ ] Senior QA: edge cases (RAW drive, lost partitions, BitLocker,
      1M+ results, pause/resume, offline mid-scan); determinism;
      regression coverage for every fixed defect.
- [ ] Paying user: progress/previews/error states honest; no dark
      patterns (retention gates etc. are donor behavior we deliberately
      do NOT copy).
- [ ] Instruction-following: re-read MASTER_PROMPT.md; deviations listed
      with justification (open-questions register).

## 9. Logging / telemetry

- Engine diagnostics via the tracing-style channels already in place;
  no PII beyond device identity strings required by the license
  protocol; no Firebase/Aliyun/analytics (excluded surfaces).
- CI logs are artifacts (app-stdout/stderr) for every tour run.

## 10. CI gates (push blocking)

1. `cargo fmt --all --check` + `cargo clippy -D warnings` (engines
   native + windows-msvc cross).
2. `cargo test` (engines, native) + web tsc/vitest.
3. windows-check (app crate, msvc, default + sim).
4. Windows Integration (VHD truth) — byte-compare mandatory.
5. Real-Core Tour (QA-1) — frame assertions + byte-truth mandatory.
6. NSIS + MSIX build green (no sim/ci-tour in release artifacts).
