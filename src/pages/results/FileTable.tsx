/**
 * The file table — virtualized (2.5k+ rows stream in during a scan),
 * sortable, with per-row recoverability indicators (the "file
 * recoverability indicators" feature: Excellent / Good / Poor chips),
 * origin badges (Deleted / Path Lost) and row selection synced to the
 * global results store.
 */
import { useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { CheckIcon, MoreIcon } from "../../shell/icons";
import { formatBytes, formatDateTime } from "../../lib/format";
import { useResultsStore } from "../../state/results";
import type { FileNode } from "../../types";
import { CategoryIcon, categoryTone } from "./categoryIcon";

type SortKey = "name" | "size" | "type" | "modified";

/** Reference row height — generous spacing, ~13 rows visible on a
 * 1080p results body (the reference capture's density). */
const ROW_H = 48;

export function FileTable({ files, loading }: { files: FileNode[]; loading: boolean }) {
  const selectedIds = useResultsStore((s) => s.selectedIds);
  const toggleSelected = useResultsStore((s) => s.toggleSelected);
  const selectAll = useResultsStore((s) => s.selectAll);
  const clearSelection = useResultsStore((s) => s.clearSelection);
  const openPreview = useResultsStore((s) => s.openPreview);
  const previewId = useResultsStore((s) => s.previewId);
  const [sort, setSort] = useState<{ key: SortKey; dir: 1 | -1 }>({ key: "name", dir: 1 });

  const allOn = files.length > 0 && files.every((f) => selectedIds.has(f.id));
  const someOn = !allOn && files.some((f) => selectedIds.has(f.id));
  const onToggleAll = () => {
    if (allOn) clearSelection();
    else selectAll(files.map((f) => f.id));
  };

  const sorted = useMemo(() => {
    const list = [...files];
    list.sort((a, b) => {
      const k = sort.key;
      const v = k === "name" ? a.name.localeCompare(b.name) : k === "type" ? a.ext.localeCompare(b.ext) : k === "size" ? a.size - b.size : a.modified - b.modified;
      return v * sort.dir;
    });
    return list;
  }, [files, sort]);

  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: sorted.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_H,
    overscan: 14,
  });

  const onHeader = (key: SortKey) =>
    setSort((s) => (s.key === key ? { key, dir: s.dir === 1 ? -1 : 1 } : { key, dir: 1 }));

  return (
    <section className="dw-file-table" aria-label="Found files">
      <div className="dw-ft-head" role="row">
        <span className="dw-ft-cell dw-ft-check" role="columnheader" aria-label="Select all">
          <button
            type="button"
            className={`dw-check-square ${allOn ? "is-on" : ""} ${someOn ? "is-mixed" : ""}`}
            onClick={onToggleAll}
            aria-label={allOn ? "Deselect all files in view" : "Select all files in view"}
            aria-pressed={allOn}
            disabled={files.length === 0}
          >
            {allOn && <CheckIcon size={12} strokeWidth={3.2} />}
          </button>
        </span>
        <span className="dw-ft-cell dw-ft-icon-h" aria-label="Type" />
        {(
          [
            ["name", "Name"],
            ["size", "Size"],
            ["type", "Type"],
            ["modified", "Date Modified"],
          ] as [SortKey, string][]
        ).map(([key, label]) => (
          <button
            key={key}
            type="button"
            className={`dw-ft-cell dw-ft-col dw-ft-${key} ${sort.key === key ? "is-sorted" : ""}`}
            onClick={() => onHeader(key)}
            aria-sort={sort.key === key ? (sort.dir === 1 ? "ascending" : "descending") : "none"}
          >
            {label}
            <span className="dw-ft-sort" aria-hidden="true">{sort.key === key ? (sort.dir === 1 ? "▲" : "▼") : ""}</span>
          </button>
        ))}
        <span className="dw-ft-cell dw-ft-grade-h">Condition</span>
        <span className="dw-ft-cell dw-ft-more-h" aria-label="Actions" />
      </div>

      <div className={`dw-ft-body dw-scroll ${loading ? "is-loading" : ""}`} ref={parentRef}>
        {sorted.length === 0 && !loading ? (
          <div className="dw-ft-empty">
            <div className="dw-ft-empty-title">No files match this view</div>
            <div className="dw-ft-empty-sub">Try another category, clear the filter or switch to complete results.</div>
          </div>
        ) : (
          <div style={{ height: virtualizer.getTotalSize(), position: "relative", width: "100%" }}>
            {virtualizer.getVirtualItems().map((v) => {
              const f = sorted[v.index];
              const checked = selectedIds.has(f.id);
              return (
                <div
                  key={f.id}
                  className={`dw-file-row ${checked ? "is-selected" : ""} ${previewId === f.id ? "is-preview" : ""}`}
                  style={{ position: "absolute", top: v.start, height: ROW_H, width: "100%" }}
                  role="row"
                  aria-selected={checked}
                >
                  <span className="dw-ft-cell dw-ft-check">
                    <button
                      type="button"
                      className={`dw-check-square ${checked ? "is-on" : ""}`}
                      onClick={() => toggleSelected(f.id)}
                      aria-label={checked ? `Deselect ${f.name}` : `Select ${f.name}`}
                      aria-pressed={checked}
                    >
                      {checked && <CheckIcon size={11} strokeWidth={3.2} />}
                    </button>
                  </span>
                  <span className={`dw-ft-cell dw-ft-icon ${categoryTone(f.category)}`}>
                    <CategoryIcon category={f.category} size={16} />
                  </span>
                  <button
                    type="button"
                    className="dw-ft-cell dw-ft-name dw-truncate"
                    onClick={() => toggleSelected(f.id)}
                    onDoubleClick={() => openPreview(f.id)}
                    title={f.path}
                  >
                    {f.name}
                    {f.origin === "deleted" && <span className="dw-origin-chip">Deleted</span>}
                    {(f.origin === "lost-path" || f.origin === "lost-name") && (
                      <span className="dw-origin-chip is-lost">Path lost</span>
                    )}
                    {f.origin === "version" && <span className="dw-origin-chip is-soft">Version</span>}
                  </button>
                  <span className="dw-ft-cell dw-ft-size dw-num">{formatBytes(f.size)}</span>
                  <span className="dw-ft-cell dw-ft-type">{f.ext ? f.ext.toUpperCase() : "—"}</span>
                  <span className="dw-ft-cell dw-ft-modified dw-num">{formatDateTime(f.modified)}</span>
                  <span className="dw-ft-cell dw-ft-grade">
                    <GradeChip grade={f.recoverability} />
                  </span>
                  <span className="dw-ft-cell dw-ft-more">
                    <button type="button" className="dw-row-more" aria-label={`Actions for ${f.name}`}>
                      <MoreIcon size={14} />
                    </button>
                  </span>
                </div>
              );
            })}
          </div>
        )}
        {loading && sorted.length > 0 && (
          <div className="dw-ft-loading-row">
            <span className="dw-scan-strip-ticker" aria-hidden="true" />
            More files streaming in…
          </div>
        )}
      </div>
    </section>
  );
}

export function GradeChip({ grade }: { grade: FileNode["recoverability"] }) {
  return (
    <span className={`dw-grade dw-grade-${grade}`}>
      <span className="dw-grade-dot" aria-hidden="true" />
      {grade === "excellent" ? "Excellent" : grade === "good" ? "Good" : "Poor"}
    </span>
  );
}
