# Multi-POV review records — index (§11 quality gates)

> MASTER_PROMPT §11 requires a multi-POV review at every phase
> boundary, recorded in `docs/reviews/<phase>-<pov>.md`. The build ran
> across six agent sessions with parallel agents sharing the repo;
> this index maps every phase boundary to its review evidence —
> dedicated records where they exist, and the register sections that
> carry the equivalent verdicts where the review was folded into a
> broader audit document (noted honestly, not backfilled).

| Phase | Boundary scope | Review evidence |
|---|---|---|
| 0 — Context ingestion | corpus coverage, synthesis | `docs/maps/doc-tree.md` (299/299 ledger) + `docs/maps/context-synthesis.md`; QA register §4 instruction-compliance audit covers the ingestion mandates |
| 1 — Audit & gap analysis | consistency + UI-gap registers | `docs/audit/consistency-register.md`, `docs/audit/ui-gap-register.md`; P0 triage recorded in `docs/QA-REGISTER.md` §2 |
| 2 — Architecture & plan | target tree, build plan, SOP | `docs/architecture/target-codebase-tree.md`, `phase-build-plan.md`, `docs/standards/SOP.md`; cross-links to governing artifacts inside each |
| 3 — Core I/O backend | driverless ADR, §5.1 ten requirements | `docs/architecture/` ADR-1 rev2 (upgraded decision record); the §5.1 audit + benchmarks in `docs/benchmarks/io-backend.md`; F13/F16 regression evidence in `docs/QA-REGISTER.md` §1; this session's phase-7 record re-reviewed the I/O evidence closure |
| 4 — Recovery engines | NTFS/FAT/exFAT/ReFS/ext/HFS+, carving | `docs/reviews/phase4-engines-multi-pov.md` (dedicated record) |
| 5 — UI↔core wiring | IPC surface, tour, screenshots | QA register §5 multi-POV verdicts (wiring spot audit) + §6 release-evidence snapshot; UI Screenshots workflow frames + VLM verification notes in §6 |
| 6 — Licensing | DiskGenie port, §8 swap-readiness | QA register §3 (licensing verification: protocol, crypto, postures, endpoint, header set) |
| 7 — Hardening & testing | fuzzing, races, tour truth, benches | `docs/reviews/phase7-hardening-multi-pov.md` (dedicated record — the F13–F17 cycle) |
| 8 — Release pipeline | NSIS/MSIX parity, install tests | QA register §6 (green-pipeline snapshot per workflow with evidence columns); MSIX + NSIS build runs cited by SHA |
| 9 — Documentation & handoff | README/worklog/dependencies accuracy | This index + `worklog.md` session entries + `docs/handoff/session-report.md`; the instruction-following check in each dedicated review record |

**Standing rule going forward** (from the phase-7 record's lesson):
every new QA surface that catches a defect gets its catching
regression in the same commit, and every fix cycle gets its
dedicated multi-POV record in this directory.
