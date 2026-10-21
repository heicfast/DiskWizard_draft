/**
 * Scan Results — the screen users live on ("Recovery results"), the
 * reference layout: breadcrumb (back + trail), title row with the
 * live stats + search + view toggle, category tab row with the Filter
 * button, the 3-pane body (tree | table | preview) and the scan
 * running INSIDE the page (the CDetailPage architecture).
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Filter,
  LayoutGrid,
  List,
  Search,
  X,
} from "lucide-react";
import { formatBytes, formatCountFull, formatDuration } from "../../lib/format";
import { scanActive, useScanStore } from "../../state/scan";
import {
  filterIsActive,
  useResultsStore,
  isVisible,
} from "../../state/results";
import { useViewStore } from "../../state/view";
import { CATEGORY_LABELS, CATEGORY_ORDER, type FileCategory, type FileNode } from "../../types";
import { nodeInPartition } from "./lib/reconstruction";
import { CategoryIcon } from "./components/categoryIcon";
import { TreePanel } from "./components/TreePanel";
import { FileTable } from "./components/FileTable";
import { PreviewPanel } from "./components/PreviewPanel";
import { DestinationDialog } from "./dialogs/DestinationDialog";
import { StopConfirmDialog } from "./dialogs/StopConfirmDialog";
import { RecoverOverlay } from "./overlays/RecoverOverlay";

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
  const activePartition = useResultsStore((s) => s.activePartition);
  const search = useResultsStore((s) => s.search);
  const setSearch = useResultsStore((s) => s.setSearch);
  const filterOpen = useResultsStore((s) => s.filterOpen);
  const setFilterOpen = useResultsStore((s) => s.setFilterOpen);
  const filter = useResultsStore((s) => s.filter);
  const resultsMode = useResultsStore((s) => s.resultsMode);
  const setResultsMode = useResultsStore((s) => s.setResultsMode);
  const setPage = useViewStore((s) => s.setPage);
  const [stopConfirm, setStopConfirm] = useState(false);

  const live = scanActive(progress);
  const scanning = live && progress;

  // Load / refresh the snapshot whenever the generation moves.
  useEffect(() => {
    if (generation > 0) void load(generation);
  }, [generation, load]);

  // Selection-follow preview (the reference flow): when exactly one
  // file is selected, the preview pane shows it immediately.
  const selectedIds = useResultsStore((s) => s.selectedIds);
  const openPreview = useResultsStore((s) => s.openPreview);
  const previewId = useResultsStore((s) => s.previewId);
  useEffect(() => {
    if (selectedIds.size === 1) {
      const only = selectedIds.values().next().value as number | undefined;
      if (only !== undefined && only !== previewId) openPreview(only);
    }
  }, [selectedIds, previewId, openPreview]);

  // Progressive fill: poll-adopt fresh results while scanning (the
  // events keep progress/stats live; this cadence is for the tree).
  useEffect(() => {
    if (!live) return undefined;
    const t = window.setInterval(() => {
      if (generation > 0) void load(generation);
    }, 1_400);
    return () => window.clearInterval(t);
  }, [live, generation, load]);

  /* Derived: visible rows under the active tab + partition scope +
   * search + filter. */
  const visibleFiles = useMemo(() => {
    if (!snapshot) return [] as FileNode[];
    const q = search.trim().toLowerCase();
    return snapshot.nodes.filter(
      (n) =>
        !n.isDir &&
        (activeCategory === "all" || n.category === activeCategory) &&
        (!activePartition || nodeInPartition(n, activePartition)) &&
        isVisible(n, resultsMode, filter) &&
        (q === "" || n.name.toLowerCase().includes(q)),
    );
  }, [snapshot, activeCategory, activePartition, search, filter, resultsMode]);

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
  /** The visible tab set (reference): All + the five majors; the rest
   * collapses into the Others dropdown (Email included). */
  const primaryChips = chips.filter(
    (c) => c.id === "all" || ["images", "videos", "documents", "audio", "archives"].includes(c.id),
  );
  const otherChips = chips.filter(
    (c) => c.id !== "all" && !["images", "videos", "documents", "audio", "archives"].includes(c.id),
  ) as { id: FileCategory; label: string; count: number }[];

  /* Empty states. */
  if (!snapshot && !scanning && !doneInfo) {
    return (
      <div className="dw-page dw-results-empty">
        <div className="dw-empty dw-settle-in">
          <div className="dw-empty__art">
            <CategoryIcon category="other" size={26} />
          </div>
          <h2 className="dw-empty__title dw-display">No scan results yet</h2>
          <p className="dw-empty__sub">
            Pick a recovery source and start a scan — found files will appear here in
            real time, and you can pause, preview and recover as they stream in.
          </p>
          <div className="dw-empty__cta">
            <button type="button" className="dw-btn dw-btn--primary" onClick={() => setPage("recover")}>
              Go to Recover Data
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="dw-results">
      {/* ── Breadcrumb row ─────────────────────────────────────── */}
      <div className="dw-crumb-row">
        <button
          type="button"
          className="dw-crumb-back"
          onClick={() => setPage("recover")}
          aria-label="Back to Recover Data"
        >
          <ChevronLeft size={15} />
        </button>
        <nav className="dw-crumbs" aria-label="Breadcrumb">
          <button
            type="button"
            className="dw-crumb is-link"
            onClick={() => setPage("recover")}
          >
            Scan Results
          </button>
          <ChevronRight size={12} className="dw-crumb-sep" aria-hidden="true" />
          <span className="dw-crumb is-active">{targetName}</span>
          <ChevronRight size={12} className="dw-crumb-sep" aria-hidden="true" />
          <span className="dw-crumb is-here">
            {activePartition ?? (activeCategory === "all" ? "All Files" : CATEGORY_LABELS[activeCategory])}
          </span>
        </nav>
      </div>

      {/* ── Title row ──────────────────────────────────────────────── */}
      <div className="dw-results-titlebar">
        <div style={{ minWidth: 0 }}>
          <h1 className="dw-results-title dw-display">Recovery results</h1>
          <p className="dw-results-stats dw-num">
            {scanning && progress ? (
              <>
                <span className="is-strong">{formatCountFull(progress.stats.filesFound)}</span> files
                found ({formatBytes(progress.stats.bytesFound)})
                <span className="is-dot">•</span>
                <span className="is-phase">
                  {progress.phase === "quick"
                    ? "Quick"
                    : progress.phase === "deep"
                      ? "Deep"
                      : "Organizing"} scanning{" "}
                  {Math.round(progress.percent)}%
                </span>
                <span className="is-dot">•</span>
                <span>{formatDuration(progress.elapsedSec)} elapsed</span>
              </>
            ) : doneInfo ? (
              <>
                <span className="is-strong">{formatCountFull(doneInfo.stats.filesFound)}</span> files
                found ({formatBytes(doneInfo.stats.bytesFound)})
                <span className="is-dot">•</span>
                <span>Scan completed in {formatDuration(doneInfo.durationSec)}</span>
                {doneInfo.sourceVolumeLive && (
                  <span
                    className="dw-live-scan-note"
                    title="Live scan — the source volume was mounted while scanning"
                  >
                    • Live scan: files changed during the scan may not be captured
                  </span>
                )}
              </>
            ) : (
              <span>Browsing saved results</span>
            )}
          </p>
        </div>
        <div className="dw-results-tools">
          <div className="dw-search">
            <Search size={15} className="dw-search__icon" />
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search files, folders or keywords..."
              aria-label="Search found files"
            />
            {search && (
              <button
                type="button"
                className="dw-search__clear"
                onClick={() => setSearch("")}
                aria-label="Clear search"
              >
                <X size={12} />
              </button>
            )}
          </div>
          <div className="dw-view-toggle" role="group" aria-label="View mode">
            <button type="button" className="is-on" title="List view" aria-pressed="true">
              <LayoutGrid size={15} />
            </button>
            <button type="button" title="Details view" aria-pressed="false" disabled>
              <List size={15} />
            </button>
          </div>
        </div>
      </div>

      {/* ── The searching strip (live ticker while scanning) ──────── */}
      <AnimatePresence>
        {scanning && progress && (
          <motion.div
            className="dw-scan-strip dw-results-strip"
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
                {Math.max(1, Math.round(progress.remainingSec))}s remaining
              </span>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* ── Category tabs + filter ─────────────────────────────────── */}
      <div className="dw-tab-row">
        <div className="dw-cat-tabs" role="tablist" aria-label="File categories">
          {primaryChips.map((chip) => (
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
          {otherChips.length > 0 && (
            <OthersMenu
              chips={otherChips}
              activeCategory={activeCategory}
              setCategory={setCategory}
            />
          )}
        </div>
        <div className="dw-tab-spacer" />
        <button
          type="button"
          className={`dw-btn dw-btn--secondary ${filterOpen ? "is-on" : ""}`}
          onClick={() => setFilterOpen(!filterOpen)}
          aria-expanded={filterOpen}
        >
          <Filter size={14} />
          Filter
          {filterIsActive(filter) && <span className="dw-filter-dot" aria-label="Filter active" />}
        </button>
      </div>

      {/* ── Advanced filter bar (+ the essential/complete switch) ───── */}
      <AnimatePresence>
        {filterOpen && (
          <FilterBar key="filterbar">
            <div className="dw-mode-switch" role="group" aria-label="Results detail">
              {(["essential", "complete"] as const).map((mode) => (
                <button
                  key={mode}
                  type="button"
                  className={resultsMode === mode ? "is-on" : ""}
                  onClick={() => setResultsMode(mode)}
                  title={
                    mode === "essential"
                      ? "Filtered view — duplicates and nonessential results hidden"
                      : "Every found file, unfiltered"
                  }
                >
                  {mode === "essential" ? "Essential results" : "Complete results"}
                </button>
              ))}
            </div>
          </FilterBar>
        )}
      </AnimatePresence>

      {/* ── 3-pane body ────────────────────────────────────────────── */}
      <div className="dw-results-body">
        <TreePanel />
        <FileTable files={visibleFiles} loading={loading || !!scanning} />
        <PreviewPanel />
      </div>

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

/* ── The advanced filter bar ─────────────────────────────────────────── */

const SIZE_OPTIONS: { label: string; min: number | null; max: number | null }[] = [
  { label: "Any size", min: null, max: null },
  { label: "< 512 KB", min: null, max: 512 * 1024 },
  { label: "512 KB – 1 MB", min: 512 * 1024, max: 1024 * 1024 },
  { label: "1 MB – 512 MB", min: 1024 * 1024, max: 512 * 1024 * 1024 },
  { label: "> 512 MB", min: 512 * 1024 * 1024, max: null },
];

function FilterBar({ children }: { children?: React.ReactNode }) {
  const filter = useResultsStore((s) => s.filter);
  const patch = useResultsStore((s) => s.patchFilter);
  const reset = useResultsStore((s) => s.resetFilter);
  const sizeIdx = SIZE_OPTIONS.findIndex((o) => o.min === filter.sizeMin && o.max === filter.sizeMax);

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
          <option key={o.label} value={i}>
            {o.label}
          </option>
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
        <input
          type="checkbox"
          checked={filter.hideEmpty}
          onChange={(e) => patch({ hideEmpty: e.target.checked })}
        />
        Hide 0 KB files
      </label>
      <label className="dw-check">
        <input
          type="checkbox"
          checked={filter.hideJunk}
          onChange={(e) => patch({ hideJunk: e.target.checked })}
        />
        Hide system &amp; temp files
      </label>
      <button type="button" className="dw-link" onClick={reset}>
        Reset Filter
      </button>
      {children}
    </motion.div>
  );
}

/** The "Others ⌄" tab (the reference): hosts the low-count categories
 * (Email, Others) — selecting one highlights the tab. */
function OthersMenu({
  chips,
  activeCategory,
  setCategory,
}: {
  chips: { id: FileCategory; label: string; count: number }[];
  activeCategory: FileCategory | "all";
  setCategory: (c: FileCategory | "all") => void;
}) {
  const active = chips.find((c) => c.id === activeCategory);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return undefined;
    const onDown = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("pointerdown", onDown);
    return () => window.removeEventListener("pointerdown", onDown);
  }, [open]);

  return (
    <div className="dw-menu-anchor dw-others-anchor" ref={ref}>
      <button
        type="button"
        role="tab"
        aria-selected={!!active}
        aria-haspopup="menu"
        aria-expanded={open}
        className={`dw-cat-tab ${active ? "is-active" : ""}`}
        onClick={() => setOpen((v) => !v)}
      >
        {active ? active.label : "Others"}
        <ChevronDown size={13} strokeWidth={2} />
      </button>
      {open && (
        <div className="dw-menu" role="menu" aria-label="Other categories">
          {chips.map((c) => (
            <button
              key={c.id}
              type="button"
              role="menuitem"
              className={`dw-menu__item ${c.id === activeCategory ? "is-current" : ""}`}
              onClick={() => {
                setCategory(c.id);
                setOpen(false);
              }}
            >
              <span className="dw-menu__icon">
                <CategoryIcon category={c.id} size={14} />
              </span>
              {c.label}
              <span className="dw-others-count dw-num">{formatCountFull(c.count)}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
