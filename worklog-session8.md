# Session 8 (2026-10-21) — the owner's symptom triage: scan broken end-to-end

Owner report (verbatim symptoms):
1. Scan "very very slow", gets stuck; estimated size climbs into TB on a 320 GB disk.
2. "Files found" counter ticks (GB→TB) but the results table below stays EMPTY.
3. Speed far below competitor tools.
4. SMART shows for the first disk only; switching disks hides the dashboard and
   shows rescue tools instead.
5. Cloud Recovery must be REMOVED completely.

## Root-cause analysis (code-level, all verified against main)

### A. The carver's weak-validation fallbacks inflate bytes into TB and stall the sweep
`dw-core/src/raw/mod.rs`:
- JPEG: 3-byte magic (FFD8FF, ~1 hit / 16 MiB of binary data). `size_jpeg`
  walks up to 64 MiB hunting EOI; garbage hit → no EOI → `hit(span, 55)` =
  carves the FULL 64 MiB span. Thousands of false hits × 64 MiB = PB of
  logical bytes → live `bytes_found` shows TB, the walk itself (64 KiB chunk
  reads via the 32 KiB chunk cache) takes minutes per hit → "stuck".
- Same class: PNG fallback 256 MiB, GIF fallback 64 MiB, PDF backward-scans
  a 2 GiB cap, OGG `count_last` scans 256 MiB, MP3 no-ID3 arm carves 16 MiB
  per literal "ID3" ASCII hit, WAV/OLE2 fallback carve = `limit` (up to
  2 GiB), PST carves 512 KiB per `!BDN` hit unconditionally.
- `Prober::read()` allocates a fresh Vec per call (even 4-byte u32 reads) →
  malloc storm inside box walks.
- No `ctrl.checkpoint()` inside validators → pause/stop unresponsive.
- Carve progress emits per window only → UI looks frozen during long walks.

### B. Results never appear during the scan (by design)
`commands/real/scan.rs`: `state.scan.tree` is populated ONLY when the worker
finishes. `get_scan_results` returns Null while scanning. The UI's 1.4 s
progressive-fill poll gets Null every time. Combined with (A) (scan never
finishes), the table stays empty forever while the header counts tick.

### C. NTFS folder tree collapses into "File Path Lost"
`tree.rs resolve_parent`: `@mft:` parent refs are NEVER resolved ("resolved
later (pass 2)" — no pass 2 exists). Every NTFS file lands under the lost
branch; the Folders tree is one flat blob.

### D. SMART switching: silent failure + UI-thread stalls
- `smart_report`/`list_devices` are SYNC Tauri commands → run on the MAIN
  thread (Tauri 2 semantics); `list_devices` runs a full enumeration incl.
  lost-partition sweeps, `smart_report` runs 10 s-timeout IOCTLs — the
  ToolsPage prefetch loop (one call per disk) freezes the window.
- `nvme_query_health_log` uses `StorageAdapterProtocolSpecificProperty`;
  the documented pattern for the NVMe health log on a PhysicalDrive handle
  is `StorageDeviceProtocolSpecificProperty` → transport fails on many
  disks → `smart[id]` absent → the whole dashboard section silently hides
  (only rescue tools remain) — exactly the owner's symptom.
- The UI shows NO "SMART unavailable" state — silent disappearance.

### E. Estimated line extrapolates the broken counter
ResultsPage: `formatBytes((bytesFound / percent) * 100)` — linear
extrapolation of the TB-inflated counter → "estimated size in TB".

## Fix plan (this session)
1. Cloud Recovery: total removal (backend commands, mock, UI page, sidebar,
   destination cloud tab, tour step, types).
2. Carver: per-signature probe caps + weak-validation fallbacks carve a
   SMALL bounded span (or reject), Prober scratch buffer (no per-read
   allocs), checkpoints inside walks, content-key from window bytes,
   honest validator math + regression tests (incl. hostile-garbage fixture
   proving bounded time/bytes).
3. Tree: FoundEntry.record_id (MFT record number) + TreeBuilder pass-2
   `@mft:` resolution (pending buffer flushed at finish; unresolved →
   lost branch). NTFS engine stamps record ids.
4. Progressive results: ProgressObserver::on_tree_update (throttled),
   worker publishes Arc<Vec<TreeNode>> snapshots into state;
   `get_scan_results(generation, since)` delta protocol; frontend
   accumulates nodes.
5. Live stats: clamp running bytes to device capacity (same QA-17 rule,
   applied continuously, not only at the end); kill the extrapolated
   "estimated" line in favor of the honest found-bytes counter.
6. SMART: StorageDeviceProtocolSpecificProperty first (adapter fallback),
   async commands via spawn_blocking, per-disk error surface in the UI
   ("SMART unavailable: <reason>" card instead of silent hide).
7. dw-cli: headless junction (devices/smart/scan/recover/selftest) with
   verbose logging + JSON output — the Windows-runner test surface.
8. CI: core-e2e workflow — VHD, 100+ multi-format files, delete incl.
   recycle bin, CLI scan, recover, byte-compare, logs as artifacts.
9. Dependency bumps (npm + cargo), fmt/clippy/test gates, docs sync.
