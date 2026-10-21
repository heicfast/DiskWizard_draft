/**
 * FileTable — the virtualized results list: sortable columns, rounded
 * pink checkboxes, per-row category icon, origin markers, row menu.
 * Column geometry lives in one CSS grid (pages/table.css) so header
 * and rows can never drift.
 */
import { useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Download, MoreHorizontal } from "lucide-react";
import { formatBytes, formatDateTime } from "../../../lib/format";
import { useResultsStore } from "../../../state/results";
import { useViewStore } from "../../../state/view";
import type { FileNode } from "../../../types";
import { CheckSquare } from "../../../components/ui";
import { CategoryIcon, categoryTone } from "./categoryIcon";
import { Menu } from "../../../components/ui";

type SortKey = "name" | "size" | "type" | "modified";

const ROW_H = 44;

export function FileTable({ files, loading }: { files: FileNode[]; loading: boolean }) {
  const selectedIds = useResultsStore((s) => s.selectedIds);
  const toggleSelected = useResultsStore((s) => s.toggleSelected);
  const selectAll = useResultsStore((s) => s.selectAll);
  const clearSelection = useResultsStore((s) => s.clearSelection);
  const openPreview = useResultsStore((s) => s.openPreview);
  const previewId = useResultsStore((s) => s.previewId);
  const showToast = useViewStore((s) => s.showToast);
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
      const v =
        k === "name"
          ? a.name.localeCompare(b.name)
          : k === "type"
            ? a.ext.localeCompare(b.ext)
            : k === "size"
              ? a.size - b.size
              : a.modified - b.modified;
      return v * sort.dir;
    });
    return list;
  }, [files, sort]);

  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: sorted.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_H,
    overscan: 16,
  });

  const onHeader = (key: SortKey) =>
    setSort((s) => (s.key === key ? { key, dir: s.dir === 1 ? -1 : 1 } : { key, dir: 1 }));

  return (
    <section className="dw-file-table" aria-label="Found files">
      <div className="dw-ft-head dw-ft-grid" role="row">
        <span className="dw-ft-cell dw-ft-check" role="columnheader" aria-label="Select all">
          <CheckSquare
            checked={allOn}
            mixed={someOn}
            onClick={onToggleAll}
            aria-label={allOn ? "Deselect all files in view" : "Select all files in view"}
            disabled={files.length === 0}
          />
        </span>
        <span className="dw-ft-cell dw-ft-icon" aria-label="Type" />
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
            <span className="dw-ft-sort" aria-hidden="true">
              {sort.key === key ? (sort.dir === 1 ? "▲" : "▼") : ""}
            </span>
          </button>
        ))}
        <span className="dw-ft-cell dw-ft-more" aria-label="Actions" />
      </div>

      <div className={`dw-ft-body dw-scroll ${loading ? "is-loading" : ""}`} ref={parentRef}>
        {sorted.length === 0 && !loading ? (
          <div className="dw-ft-empty">
            <div className="dw-ft-empty-title">No files match this view</div>
            <div className="dw-ft-empty-sub">
              Try another category, clear the filter or switch to complete results.
            </div>
          </div>
        ) : (
          <div style={{ height: virtualizer.getTotalSize(), position: "relative", width: "100%" }}>
            {virtualizer.getVirtualItems().map((v) => {
              const f = sorted[v.index];
              const checked = selectedIds.has(f.id);
              return (
                <div
                  key={f.id}
                  className={`dw-file-row dw-ft-grid ${checked ? "is-selected" : ""} ${previewId === f.id ? "is-preview" : ""}`}
                  style={{ position: "absolute", top: v.start, height: ROW_H, width: "100%" }}
                  role="row"
                  aria-selected={checked}
                >
                  <span className="dw-ft-cell dw-ft-check">
                    <CheckSquare
                      checked={checked}
                      onClick={() => toggleSelected(f.id)}
                      aria-label={checked ? `Deselect ${f.name}` : `Select ${f.name}`}
                    />
                  </span>
                  <span className={`dw-ft-cell dw-ft-icon ${categoryTone(f.category)}`}>
                    <span className="dw-ft-icon-tile">
                      <CategoryIcon category={f.category} size={14} />
                    </span>
                  </span>
                  <button
                    type="button"
                    className="dw-ft-cell dw-ft-name dw-truncate"
                    onClick={() => toggleSelected(f.id)}
                    onDoubleClick={() => openPreview(f.id)}
                    title={f.path}
                  >
                    <span className="dw-truncate">{f.name}</span>
                    {f.origin === "deleted" && <span className="dw-origin-chip">Deleted</span>}
                    {(f.origin === "lost-path" || f.origin === "lost-name") && (
                      <span className="dw-origin-chip dw-origin-chip--lost">Path lost</span>
                    )}
                    {f.origin === "version" && (
                      <span className="dw-origin-chip dw-origin-chip--soft">Version</span>
                    )}
                  </button>
                  <span className="dw-ft-cell dw-ft-size dw-num">{formatBytes(f.size)}</span>
                  <span className="dw-ft-cell dw-ft-type">{f.ext ? f.ext.toUpperCase() : "—"}</span>
                  <span className="dw-ft-cell dw-ft-modified dw-num">{formatDateTime(f.modified)}</span>
                  <span className="dw-ft-cell dw-ft-more">
                    <Menu
                      label={`Actions for ${f.name}`}
                      trigger={(open, toggleMenu, id) => (
                        <button
                          type="button"
                          className={`dw-row-more ${open ? "is-open" : ""}`}
                          aria-haspopup="menu"
                          aria-expanded={open}
                          aria-controls={id}
                          aria-label={`Actions for ${f.name}`}
                          onClick={toggleMenu}
                        >
                          <MoreHorizontal size={15} />
                        </button>
                      )}
                      items={[
                        {
                          id: "preview",
                          label: "Preview",
                          onSelect: () => openPreview(f.id),
                        },
                        {
                          id: "select",
                          label: checked ? "Deselect" : "Select",
                          onSelect: () => toggleSelected(f.id),
                        },
                        {
                          id: "copy-path",
                          label: "Copy full path",
                          onSelect: () => {
                            void navigator.clipboard?.writeText(f.path).catch(() => undefined);
                            showToast("Full path copied to the clipboard", "check");
                          },
                        },
                        {
                          id: "sep",
                          label: "",
                          onSelect: () => undefined,
                        },
                        {
                          id: "recover",
                          label: "Recover this file",
                          icon: <Download size={14} />,
                          onSelect: () => {
                            if (!checked) toggleSelected(f.id);
                            openPreview(f.id);
                          },
                        },
                      ]}
                    />
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
