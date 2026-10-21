/**
 * ReconstructionTree — the File Reconstruction tab's body: the
 * partition-wise recovery view. Every partition the scan resolved
 * (existing volumes first, then the lost partitions the deep pass
 * carved back), each expandable into its category mix and top-level
 * reconstructed folders. Clicking a partition scopes the file table.
 */
import { useMemo, useState } from "react";
import {
  ChevronDownIcon,
  ChevronRightIcon,
  FolderIcon,
} from "lucide-react";
import { HardDriveIcon, ShieldAlertIcon } from "../../../shell/icons";
import { formatBytes, formatCount } from "../../../lib/format";
import { useResultsStore } from "../../../state/results";
import { CATEGORY_LABELS, CATEGORY_ORDER, type FileCategory } from "../../../types";
import {
  buildReconstructionModel,
  type ReconstructionPartition,
} from "../lib/reconstruction";

export function ReconstructionTree() {
  const snapshot = useResultsStore((s) => s.snapshot);
  const activePartition = useResultsStore((s) => s.activePartition);
  const setPartition = useResultsStore((s) => s.setPartition);
  const [expanded, setExpanded] = useState<Set<number>>(() => new Set());

  const model = useMemo(
    () => buildReconstructionModel(snapshot?.nodes ?? []),
    [snapshot],
  );

  const toggle = (id: number) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  if (model.shape === "empty") {
    return (
      <div className="dw-tree-empty">
        The partition map appears here as the scan reconstructs what it
        finds — existing volumes first, lost partitions as the deep pass
        digs them out.
      </div>
    );
  }

  const lost = model.partitions.filter((p) => p.isLost);
  const healthy = model.partitions.filter((p) => !p.isLost);

  return (
    <div role="list" aria-label="Partition-wise reconstruction">
      <div className="dw-recon-note">
        Partition-wise reconstruction of everything recovered. Lost
        partitions the deep scan finds are listed first — click one to
        scope the file list.
      </div>

      {healthy.map((p) => (
        <PartitionRow
          key={p.id}
          partition={p}
          expanded={expanded.has(p.id)}
          onToggle={() => toggle(p.id)}
          active={activePartition === p.label}
          onSelect={() => setPartition(activePartition === p.label ? null : p.label)}
        />
      ))}

      {lost.length > 0 && (
        <>
          <div className="dw-recon-section">
            <ShieldAlertIcon size={12} />
            Lost Partitions ({lost.length})
          </div>
          {lost.map((p) => (
            <PartitionRow
              key={p.id}
              partition={p}
              expanded={expanded.has(p.id)}
              onToggle={() => toggle(p.id)}
              active={activePartition === p.label}
              onSelect={() => setPartition(activePartition === p.label ? null : p.label)}
            />
          ))}
        </>
      )}

      {activePartition && (
        <button
          type="button"
          className="dw-tree-row"
          onClick={() => setPartition(null)}
          style={{ color: "var(--ink-deep)" }}
        >
          <span className="dw-tree-icon is-root">
            <HardDriveIcon size={13} />
          </span>
          Show all partitions
        </button>
      )}
    </div>
  );
}

function PartitionRow({
  partition,
  expanded,
  onToggle,
  active,
  onSelect,
}: {
  partition: ReconstructionPartition;
  expanded: boolean;
  onToggle: () => void;
  active: boolean;
  onSelect: () => void;
}) {
  const categories = CATEGORY_ORDER.filter(
    (c) => (partition.categories[c]?.count ?? 0) > 0,
  );

  return (
    <div>
      <div className={`dw-tree-row ${active ? "is-active" : ""}`}>
        <button
          type="button"
          className="dw-tree-chev"
          aria-label={expanded ? `Collapse ${partition.label}` : `Expand ${partition.label}`}
          aria-expanded={expanded}
          onClick={onToggle}
          style={{ display: "inline-flex", padding: 0 }}
        >
          {expanded ? <ChevronDownIcon size={13} /> : <ChevronRightIcon size={13} />}
        </button>
        <span className={`dw-tree-icon ${partition.isLost ? "is-lost-branch" : "is-partition"}`}>
          {partition.isLost ? <ShieldAlertIcon size={13} /> : <HardDriveIcon size={13} />}
        </span>
        <button
          type="button"
          className="dw-tree-label"
          onClick={onSelect}
          title={`${partition.label} — ${formatCount(partition.fileCount)} files · ${formatBytes(partition.bytes)}`}
          style={{ textAlign: "left", display: "block", padding: 0 }}
        >
          {partition.label}
        </button>
        {partition.isLost && <span className="dw-tree-badge">Lost</span>}
        <span className="dw-tree-count dw-num">{formatCount(partition.fileCount)}</span>
      </div>

      {expanded && (
        <div style={{ paddingLeft: 26 }}>
          {categories.map((c) => (
            <div key={c} className="dw-tree-row is-child" role="listitem">
              <span className={`dw-tree-icon is-${c}`}>
                <FolderIcon size={12} />
              </span>
              <span className="dw-tree-label">{CATEGORY_LABELS[c as FileCategory]}</span>
              <span className="dw-tree-count dw-num">
                {formatCount(partition.categories[c]?.count ?? 0)}
              </span>
            </div>
          ))}
          {partition.children.map((ch) => (
            <div key={ch.id} className="dw-tree-row is-child" role="listitem">
              <span className="dw-tree-icon">
                <FolderIcon size={12} />
              </span>
              <span className="dw-tree-label dw-truncate" title={ch.name}>
                {ch.name}
              </span>
              <span className="dw-tree-count dw-num">{formatCount(ch.fileCount)}</span>
            </div>
          ))}
          {categories.length === 0 && partition.children.length === 0 && (
            <div className="dw-tree-row is-child">
              <span className="dw-tree-label">No files reconstructed yet…</span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
