/**
 * Scan Results — the screen users live on ("Recovery results"):
 * the COMPACT header (breadcrumb → title+tools row → slim scan strip
 * → tabs+mode row — four rows, never seven), the folders /
 * reconstruction tree, the virtualized file table with recoverability
 * indicators, the preview panel and the recovery CTA — with the scan
 * running INSIDE this page (the CDetailPage architecture: progressive
 * fill while scanning, pause/stop from here, then browse + recover).
 */
import { useEffect, useMemo, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { FilterIcon, SearchIcon, XIcon, ChevronRightIcon } from "lucide-react";
import { formatBytes, formatCountFull, formatDuration, formatRemaining, formatScanSpeed, formatSectors } from "../../lib/format";
import { scanActive, useScanStore } from "../../state/scan";
import { filterIsActive, useResultsStore, isVisible } from "../../state/results";
import { useViewStore } from "../../state/view";
import { CATEGORY_LABELS, CATEGORY_ORDER, type FileCategory, type FileNode } from "../../types";
import { CategoryIcon } from "./categoryIcon";
import { TreePanel } from "./TreePanel";
import { FileTable } from "./FileTable";
import { PreviewPanel } from "./PreviewPanel";
import { DestinationDialog } from "./DestinationDialog";
import { StopConfirmDialog } from "./StopConfirmDialog";
import { RecoverOverlay } from "./RecoverOverlay";

export function ResultsPage() {
  const progress = useScanStore((s) => s.progress);
  const done = useScanStore((s) => s.done);
  const stop = useScanStore((s) => s.stop);
  const generation = useScanStore((s) => s.generation);
  const snapshot = useResultsStore((s) => s.snapshot);
  const loading = useResultsStore((s) => s.loading);
  const load = useResultsStore((s) => s.load);
  const activeCategory = useResultsStore((s) => s.activeCategory);
  const setCategory = useResultsStore((s) => s.setCategory);
  const search = useResultsStore((s) => s.search);
  const setSearch = useResultsStore((s) => s.setSearch);
  const filterOpen = useResultsStore((s) => s.filterOpen);
  const setFilterOpen = useResultsStore((s) => s.setFilterOpen);
  const filter = useResultsStore((s) => s.filter);
  const resultsMode = useResultsStore((s) => s.resultsMode);
  const setResultsMode = useResultsStore((s) => s.setResultsMode);
  const activePartition = useResultsStore((s) => s.activePartition);
  const setPartition = useResultsStore((s) => s.setPartition);
  const selectedIds = useResultsStore((s) => s.selectedIds);
  const openDestination = useResultsStore((s) => s.openDestination);
  const setPage = useViewStore((s) => s.setPage);
  const [stopConfirm, setStopConfirm] = useState(false);

  const live = scanActive(progress);
  const scanning = live && progress;

  // Load / refresh the snapshot whenever the generation moves (also
  // refreshes as the scan fills progressively — the events bump the
  // generation adoption through get_scan_results).
  useEffect(() => {
    if (generation > 0) void load(generation);
  }, [generation, load]);

  // Progressive fill: poll-adopt fresh results while scanning.
  useEffect(() => {
    if (!live) return undefined;
    const t = window.setInterval(() => {
      if (generation > 0) void load(generation);
    }, 1_400);
    return () => window.clearInterval(t);
  }, [live, generation, load]);

  /* Derived: visible rows under the active tab + partition scope +
   * search + filter (every dimension composes — the filterbar family
   * contract from the research). */
  const visibleFiles = useMemo(() => {
    if (!snapshot) return [] as FileNode[];
    const q = search.trim().toLowerCase();
    return snapshot.nodes.filter(
      (n) =>
        !n.isDir &&
        (activeCategory === "all" || n.category === activeCategory) &&
        (activePartition === null || n.partition === activePartition) &&
        isVisible(n, resultsMode, filter) &&
        (q === "" || n.name.toLowerCase().includes(q)),
    );
  }, [snapshot, activeCategory, activePartition, search, filter, resultsMode]);

  const selectedBytes = useMemo(() => {
    if (!snapshot) return 0;
    let b = 0;
    for (const id of selectedIds) {
      const n = snapshot.nodes[id];
      if (n) b += n.size;
    }
    return b;
  }, [snapshot, selectedIds]);

  /* Category chips with live counts. */
  const chips = useMemo(() => {
    if (!snapshot) return [];
    const map = new Map<FileCategory, number>();
    for (const c of snapshot.categories) map.set(c.category, c.count);
    const total = snapshot.categories.reduce((a, c) => a + c.count, 0);
    return [
      { id: "all" as const, label: "All Files", count: total },
      ...CATEGORY_ORDER.filter((c) => (map.get(c) ?? 0) > 0).map((c) => ({
        id: c,
        label: CATEGORY_LABELS[c],
        count: map.get(c) ?? 0,
      })),
    ];
  }, [snapshot]);

  const targetName = snapshot?.targetName ?? progress?.targetName ?? "Scan";
  const doneInfo = done && !live ? done : null;
  const activePartitionInfo = useMemo(
    () => snapshot?.partitions?.find((p) => p.id === activePartition) ?? null,
    [snapshot, activePartition],
  );

  /* Empty states. */
  if (!snapshot && !scanning && !doneInfo) {
    return (
      <div className="dw-page dw-results-empty">
        <div className="dw-empty-state dw-settle-in">
          <div className="dw-empty-art">
            <CategoryIcon category="other" size={30} />
          </div>
          <h2 className="dw-display">No scan results yet</h2>
          <p>
            Pick a recovery source and start a scan — found files will appear here in
            real time, and you can pause, preview and recover as they stream in.
          </p>
          <button type="button" className="dw-ink-button" onClick={() => setPage("recover")}>
            Go to Recover Data
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="dw-page dw-page-results">
      {/* Row 1 — breadcrumb */}
      <header className="dw-results-head">
        <div className="dw-breadcrumb" aria-label="Breadcrumb">
          <button type="button" className="dw-crumb" onClick={() => setPage("recover")}>Scan Results</button>
          <ChevronRightIcon size={13} className="dw-crumb-sep" />
          <span className="dw-crumb is-here">{targetName}</span>
          <ChevronRightIcon size={13} className="dw-crumb-sep" />
          <span className="dw-crumb is-here">
            {activePartitionInfo ? activePartitionInfo.label : activeCategory === "all" ? "All Files" : CATEGORY_LABELS[activeCategory]}
          </span>
        </div>

        {/* Row 2 — title + subtitle ⟷ search + filter + stop */}
        <div className="dw-results-titlebar">
          <div>
            <h1 className="dw-page-title dw-display">
              {scanning ? (progress?.phase === "organizing" ? "Organizing found files" : "Scanning") : "Recovery results"}
            </h1>
            <p className="dw-page-sub dw-num">
              {scanning && progress ? (
                <>
                  Deep scanning {Math.round(progress.percent)}% · {formatCountFull(progress.stats.filesFound)} files found ({formatBytes(progress.stats.bytesFound)})
                </>
              ) : doneInfo ? (
                <>
                  Scan completed in {formatDuration(doneInfo.durationSec)} · {formatCountFull(doneInfo.stats.filesFound)} files ({formatBytes(doneInfo.stats.bytesFound)})
                  {doneInfo.sourceVolumeLive ? (
                    <span className="dw-live-scan-note" title="Live scan — the source volume was mounted while scanning">
                      {" "}· Live scan: files changed during the scan may not be captured
                    </span>
                  ) : null}
                </>
              ) : (
                <>Browsing saved results</>
              )}
            </p>
          </div>
          <div className="dw-results-head-actions">
            <div className="dw-search-box">
              <SearchIcon size={14} className="dw-search-icon" />
              <input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search files, folders or keywords…"
                aria-label="Search found files"
              />
              {search && (
                <button type="button" className="dw-search-clear" onClick={() => setSearch("")} aria-label="Clear search">
                  <XIcon size={12} />
                </button>
              )}
            </div>
            <button
              type="button"
              className={`dw-outline-button ${filterOpen ? "is-on" : ""}`}
              onClick={() => setFilterOpen(!filterOpen)}
              aria-expanded={filterOpen}
            >
              <FilterIcon size={13} />
              Filter
              {filterIsActive(filter) && <span className="dw-filter-dot" aria-label="Filter active" />}
            </button>
            {scanning && (
              <button type="button" className="dw-outline-button dw-danger-outline" onClick={() => setStopConfirm(true)}>
                Stop Scan
              </button>
            )}
          </div>
        </div>

        {/* Row 3 — the searching strip (live ticker, slim) */}
        <AnimatePresence>
          {scanning && progress && (
            <motion.div
              className="dw-scan-strip"
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              exit={{ opacity: 0, height: 0 }}
              transition={{ duration: 0.28, ease: [0.2, 0.8, 0.3, 1] }}
            >
              <div className="dw-scan-strip-bar" style={{ width: `${progress.percent}%` }} />
              <div className="dw-scan-strip-row">
                <span className="dw-scan-strip-label">
                  <span className="dw-scan-strip-ticker" aria-hidden="true" />
                  {progress.searchingLabel}
                </span>
                <span className="dw-scan-strip-meta dw-num">
                  {progress.stats.sectorsTotal > 0 && (
                    <>
                      <span className="dw-strip-stat" title="Sectors read">
                        Sector {formatSectors(progress.stats.sectorsRead)} / {formatSectors(progress.stats.sectorsTotal)}
                      </span>
                      <span className="dw-strip-sep" aria-hidden="true">·</span>
                    </>
                  )}
                  <span className="dw-strip-stat" title="Read throughput">
                    {formatScanSpeed(progress.stats.sectorsRead, progress.elapsedSec)}
                  </span>
                  <span className="dw-strip-sep" aria-hidden="true">·</span>
                  <span className="dw-strip-stat" title="Data found so far">
                    {formatBytes(progress.stats.bytesFound)} found
                  </span>
                  <span className="dw-strip-sep" aria-hidden="true">·</span>
                  <span className="dw-strip-stat" title="Estimated time to finish">
                    {formatRemaining(progress.remainingSec)} remaining
                  </span>
                </span>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        {/* Row 4 — category tabs ⟷ essential/complete + partition scope */}
        <div className="dw-results-toolbar">
          <div className="dw-cat-tabs" role="tablist" aria-label="File categories">
            {chips.map((chip) => (
              <button
                key={chip.id}
                type="button"
                role="tab"
                aria-selected={activeCategory === chip.id}
                className={`dw-cat-tab ${activeCategory === chip.id ? "is-active" : ""}`}
                onClick={() => setCategory(chip.id)}
              >
                <CategoryIcon category={chip.id === "all" ? "other" : chip.id} size={14} />
                {chip.label}
              </button>
            ))}
          </div>
          <div className="dw-results-tools">
            {activePartitionInfo && (
              <button
                type="button"
                className="dw-chip dw-chip-soft dw-partition-scope"
                onClick={() => setPartition(null)}
                title="Clear the partition scope"
              >
                {activePartitionInfo.label}
                <XIcon size={11} />
              </button>
            )}
            <div className="dw-mode-switch" role="group" aria-label="Results detail">
              {(["essential", "complete"] as const).map((mode) => (
                <button
                  key={mode}
                  type="button"
                  className={resultsMode === mode ? "is-on" : ""}
                  onClick={() => setResultsMode(mode)}
                  title={mode === "essential" ? "Filtered view — duplicates and nonessential results hidden" : "Every found file, unfiltered"}
                >
                  {mode === "essential" ? "Essential results" : "Complete results"}
                </button>
              ))}
            </div>
          </div>
        </div>

        {/* The advanced filter bar */}
        <AnimatePresence>
          {filterOpen && <FilterBar key="filterbar" />}
        </AnimatePresence>
      </header>

      {/* 3-pane body */}
      <div className="dw-results-body">
        <TreePanel />
        <FileTable files={visibleFiles} loading={loading || !!scanning} />
        <PreviewPanel />
      </div>

      {/* Bottom status bar */}
      <footer className="dw-results-foot">
        <div className="dw-results-foot-status dw-num">
          {scanning && progress ? (
            <>Deep scanning — {formatCountFull(progress.stats.filesFound)} files found ({formatBytes(progress.stats.bytesFound)})</>
          ) : doneInfo ? (
            <>Scan Completed · Files Found: {formatCountFull(doneInfo.stats.filesFound)} ({formatBytes(doneInfo.stats.bytesFound)})</>
          ) : (
            <>{formatCountFull(visibleFiles.length)} files in view</>
          )}
        </div>
        <div className="dw-results-foot-cta">
          {selectedIds.size > 0 && (
            <span className="dw-num dw-selection-note">
              {formatCountFull(selectedIds.size)} file{selectedIds.size === 1 ? "" : "s"} selected ({formatBytes(selectedBytes)})
            </span>
          )}
          <button
            type="button"
            className="dw-ink-button dw-recover-cta"
            disabled={selectedIds.size === 0}
            onClick={() => void openDestination()}
          >
            {selectedIds.size > 0 ? "Recover Selected Files" : "Select files to recover"}
            <ChevronRightIcon size={15} />
          </button>
        </div>
      </footer>

      <DestinationDialog />
      <RecoverOverlay />
      {stopConfirm && (
        <StopConfirmDialog
          filesFound={progress?.stats.filesFound ?? 0}
          elapsed={progress?.elapsedSec ?? 0}
          onConfirm={() => {
            setStopConfirm(false);
            void stop();
          }}
          onCancel={() => setStopConfirm(false)}
        />
      )}
    </div>
  );
}

/* ── The advanced filter bar ──────────────────────────────────────────── */

const SIZE_OPTIONS: { label: string; min: number | null; max: number | null }[] = [
  { label: "Any size", min: null, max: null },
  { label: "< 512 KB", min: null, max: 512 * 1024 },
  { label: "512 KB – 1 MB", min: 512 * 1024, max: 1024 * 1024 },
  { label: "1 MB – 512 MB", min: 1024 * 1024, max: 512 * 1024 * 1024 },
  { label: "> 512 MB", min: 512 * 1024 * 1024, max: null },
];

function FilterBar() {
  const filter = useResultsStore((s) => s.filter);
  const patch = useResultsStore((s) => s.patchFilter);
  const reset = useResultsStore((s) => s.resetFilter);
  const sizeIdx = SIZE_OPTIONS.findIndex(
    (o) => o.min === filter.sizeMin && o.max === filter.sizeMax,
  );

  return (
    <motion.div
      className="dw-filter-bar"
      initial={{ opacity: 0, height: 0 }}
      animate={{ opacity: 1, height: "auto" }}
      exit={{ opacity: 0, height: 0 }}
      transition={{ duration: 0.26, ease: [0.2, 0.8, 0.3, 1] }}
    >
      <select
        className="dw-select"
        value={sizeIdx >= 0 ? sizeIdx : 0}
        onChange={(e) => {
          const o = SIZE_OPTIONS[Number(e.target.value)];
          patch({ sizeMin: o.min, sizeMax: o.max });
        }}
        aria-label="File size"
      >
        {SIZE_OPTIONS.map((o, i) => (
          <option key={o.label} value={i}>{o.label}</option>
        ))}
      </select>
      <select
        className="dw-select"
        value={filter.existingOnly ? "existing" : "all"}
        onChange={(e) => patch({ existingOnly: e.target.value === "existing" })}
        aria-label="File state"
      >
        <option value="all">All states</option>
        <option value="existing">Existing files only</option>
      </select>
      <label className="dw-check">
        <input type="checkbox" checked={filter.hideEmpty} onChange={(e) => patch({ hideEmpty: e.target.checked })} />
        Hide 0 KB files
      </label>
      <label className="dw-check">
        <input type="checkbox" checked={filter.hideJunk} onChange={(e) => patch({ hideJunk: e.target.checked })} />
        Hide system &amp; temp files
      </label>
      <button type="button" className="dw-link" onClick={reset}>
        Reset Filter
      </button>
    </motion.div>
  );
}
