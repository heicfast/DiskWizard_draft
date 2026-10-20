# UI/UX Gap Register — Core Interface Session (for the parallel UI/UX agents)

> **Session-8 status**: items 1-3 are CLOSED (verified in code); item 4
> (BitLocker args) and 5 (scan-phase adoption) remain as-is; item 6
> verified. The register stays for the remaining P1/P2 polish.

> **Source**: the core-interface session's audit of the shipped UI
> against (a) the frozen IPC contract in `docs/ARCHITECTURE.md`, (b)
> the research artifacts (UIUX deep-dives, especially
> `re-workspace/artifacts/UIUX/09-results-phase-deep-dive.md`,
> `10-preview-subsystem.md`, `31-filterbar-family-decode.md`), and
> (c) the REAL core's capabilities that just landed.
>
> The real core is now the default build. The items below are the UI
> changes needed to consume it fully. Priority order: P0 = blocks
> real-core usage, P1 = contract gaps the research requires, P2 =
> polish aligned with the research.

## P0 — Contract extensions the real core adds (frontend must adopt)

### 1. `start_recovery` needs to send the selected node IDs — CLOSED (results.ts sends nodeIds + generation)
- **Today**: `beginRecovery(nodeIds, bytes)` sends only
  `fileCount: nodeIds.length` + `bytes` — the core cannot know WHICH
  files to recover.
- **Change** (`src/state/results.ts` → `beginRecovery`): pass
  `nodeIds` and `generation` through:
  ```ts
  await invoke("start_recovery", {
    generation: get().snapshot?.generation ?? 0,
    nodeIds,                      // ← the actual selection
    bytes,
    destinationName: dest?.name ?? "Destination",
    enableRepair: get().enableRepair,
  });
  ```
- The core command signature now accepts
  `generation: Option<u64>, node_ids: Option<Vec<u64>>` (plus the
  legacy fields — backward compatible).

### 2. Real previews via `preview_file` — CLOSED (PreviewPanel invokes preview_file with generation+nodeId)
- **Today**: `PreviewPanel` renders bundled sample photos +
  procedural sakura art (mock realism).
- **Change** (`src/pages/results/PreviewPanel.tsx`): when
  `snapshot.generation > 0` and the node is a file ≤ 16 MB, call
  ```ts
  const r = await invoke<{mime: string; size: number; data: string}>(
    "preview_file", { generation, nodeId, maxBytes: 2_000_000 });
  ```
  and render `data:image/jpeg;base64,${r.data}` (or text for
  `text/*`, a download-notice for video/audio per the 15s-cap
  research contract). Fall back to the current art when the call
  fails (deleted/truncated files) — the honest damage state
  (`PreviewFailedWnd` analog).
- Optional: keep the bundled samples ONLY in browser-dev mode
  (mock backend), never when a real core generation exists.

### 3. Resume-session flow shows the adopted tree — CLOSED (scan store resumeSession → get_scan_results → results page)
- `resume_session` now loads the real `.rsf` and returns
  `{generation, deviceName}` with the tree already adopted — the UI
  just navigates to results (no scan-in-progress state). Verify the
  recover page's session list wires `id` → the session FILE path
  (the core's `sessionFile`/`id` is the path).

## P1 — Research-contract gaps in the current UI

### 4. BitLocker unlock args
- The core's `unlock_bitlocker` now takes `password` /
  `recoveryKey` (optional strings). The current
  `BitLockerDialog.tsx` should pass whichever field the user filled:
  ```ts
  invoke("unlock_bitlocker", {
    deviceId, password: pw || null, recoveryKey: key || null })
  ```
  and surface typed failures (`NOT_LOCKED`, `UNLOCK_FAILED` with the
  PowerShell error copy) via the existing userFacingError mapper.

### 5. Progress events now carry `scan-phase`
- The real core emits an additional `scan-phase` event
  (`{generation, phase}`) at each transition. The scan store may
  adopt it for instant phase flips (optional — `scan-progress`
  already carries `phase`).

### 6. Smart report device ids — CLOSED (row ids pass through; per-disk errors surface in the unavailable card)
- `smart_report(deviceId)` accepts the inventory row's `id`
  (`disk0-part2` etc.). The tools page's per-disk rows already carry
  ids — verify the mapping passes the ROW id (not a synthetic one).

## P2 — Polish items surfaced by the research (lower priority)

### 7. Essential/Complete toggle semantics
- The core grades carved duplicates down (structured-wins merge);
  essential mode hiding `lost-name`/`version` origins stays
  client-side as today. No change needed — confirmed aligned.

### 8. Recovery summary failure list
- The core's `recover-done` payload now includes accurate
  `failedFiles` and the recovery log lands at
  `%APPDATA%\DiskWizard\logs`. A future summary-row could show the
  failed count with the existing copy ("for details, check the
  recovery log").

### 9. Session list naming
- Real sessions carry `auto_<Volume>_….rsf` file names — the list
  already displays `sessionFile`; consider trimming the `auto_`
  prefix for display.

## Verification checklist for the UI agents
- [ ] Browser-dev mock still passes `npm test` (the mock's
      `start_recovery` keeps accepting the legacy args).
- [ ] With the real build (Windows), select → recover → verify files
      land under `<dest>:\DiskWizard Recovered <ts>\`.
- [ ] Preview a JPEG from a real scan (double-click) — real bytes
      render.
- [ ] BitLocker-locked volume → password unlock → card flips to
      unlocked + scan proceeds.
