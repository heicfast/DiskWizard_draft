# DiskWizard — UI/UX Gap Register (research docs ⇄ shipped UI)

> **Phase 1 deliverable** (MASTER_PROMPT §10 PHASE 1.2/1.3). Sources:
> FULL-read UIUX research (10-preview-subsystem + architecture 05/07),
> the 5-c/5-e digests (`docs/maps/digests/uiux-longtail.md`,
> `uiux-savescan-filerepair-config.md`), the QA register, and code reads
> of the React/TS surface. Each gap is actionable: one document per gap
> lives in (or is referenced from) `todo-ui-ux/` for the parallel UI
> agents; core-wiring gaps are called out for the core session.

## A. Status of the reference journey (three steps — §6.2)

| Journey element (research) | UI state | Gap |
|---|---|---|
| Step 1 landing: device cards, TRIM/BitLocker/lost states, quick actions, lost partitions surfaced | Implemented (RecoverPage) | none blocking; lost-partition rows feed the guidance dialog |
| Step 2 scan-in-results-page: two-phase progress, live counters, pause/stop, browse-while-scan | Implemented + real progress wired (F9) | none |
| Step 3 results: 3-branch tree semantics (Windows (C:) / File Path Lost / Reconstructed) | Partial — tree + lost branches exist | The vendor's exact third branch ("Reconstructed", grows live during carving) should mirror the merged carve stream naming — low-priority copy/branch-name check for UI agents |
| Preview before recovery (per-type, caps, damage grading) | Implemented with honest caps | **QA-2/OQ-5** (100 MB research contract vs current 16 MiB/2 MiB) — owner call; video window copy already honest |
| Recovery funnel: destination rows, space gate, enable-repair toggle ON, different-drive warning | Implemented; backend source-disk exclusion landed this session | UI should visually disable (not just flag) the `isSource` row — backend now rejects it regardless (`DESTINATION_ON_SOURCE`) |

## B. Open UI gaps (ordered)

1. **UI-GAP-1 (QA-8, §5.1 item 8) — live-scan consistency disclosure.
   CLOSED (session 5):** the core now computes `source_volume_live`
   (mounted-volume targets) and carries it on `scan-done`; the results
   header renders the live-scan note (`.dw-live-scan-note`); both mocks
   emit the flag for tour coverage. VSS snapshots remain an M3 option —
   the disclosure arm of §5.1-8 is satisfied.
2. **UI-GAP-2 — destination dialog source-row hard state.** The dialog
   shows an `isSource` flag; make the row disabled + "Cannot save to
   original partition" copy (research's hard-block wording). Backend
   already enforces (this session).
3. **UI-GAP-3 — scan-complete celebration parity.** The vendor's
   completion is a standalone 540×380 window with two stat panels +
   "Go to Recover" (research `30-interaction-driver-ground-truth`). Our
   completion overlay exists inside the page — acceptable (in-page
   equivalent), but the eight stat tiles (incl. "Video Repaired /
   Video to be repaired" placeholders for Phase 2) need the Phase-2
   contract reserved now so the repair counters slot in without
   re-layout. → `todo-ui-ux/PHASE2-REPAIR-COUNTERS.md` (to create with
   the Phase-2 spec).
4. **UI-GAP-4 — filter-bar parity items.** Research filter row: Show /
   File Type / Date Modified / File Size dropdowns + toggles. Current
   implementation covers all four + junk/empty toggles; remaining:
   the "Custom suffix entry" (filter dropdown's custom-extension input)
   — small.
5. **UI-GAP-5 — preview damage grading copy.** Research PreviewFailed
   copy grades structure ("But this file is likely recoverable based on
   its structure" + Recovery Chances gauge). Our preview failure state
   exists; add the structure-based grading line when the header is
   damaged but the body parses (needs a small preview probe flag from
   the core).
6. **UI-GAP-6 — icon-view mode + context menu** (research 14/33):
   list⇄icon toggle + 6-item context menu (Recover / Preview / Open
   location / Select unrecovered / Smart select / Clear). Already in
   `docs/TODO.md` UI backlog — keep.
7. **AI surfaces** — all research AI elements remain excluded by
   mandate (complete skip list in the 5-c digest §cross-cutting). No
   action; listed to prevent "parity pressure" regressions.

## C. Core-wiring gaps surfaced by UI work this session

- `snapshot.deviceId` now flows to the destination dialog (was a
  hardcoded `null`) — UI agents should verify the source row flags
  render correctly in the sim (mock results also carry `deviceId`).
- Tour frames `recovery-*` are new named captures — the screenshot
  atlas/README tour list should gain them when the real-core tour goes
  green (the sim tour's step list is unchanged).

## D. Hand-off protocol

Each UI gap above becomes (or updates) one document in `todo-ui-ux/`
with: the research citation, the acceptance frames (which tour step
proves it), and the copy strings. The existing
`todo-ui-ux/CORE-INTERFACE-GAPS.md` (session-4) remains the wiring
contract; this register is the research-parity layer on top.
