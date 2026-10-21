/**
 * The Reconstruction tree — the partition-wise view of the recovery:
 * every partition the scan resolved (existing volumes, then the lost
 * partitions the deep pass carved back from unallocated space), each
 * row expandable into that partition's reconstructed folder structure.
 *
 * Clicking a partition scopes the file table to its files (the store's
 * activePartition filter); counts update live as the scan streams in
 * (progressive fill regenerates the snapshot with fresh roll-ups).
 */
import { useMemo, useState } from "react";
import { ChevronDownIcon, ChevronRightIcon, FolderIcon } from "lucide-react";
import { HardDriveIcon } from "../../shell/icons";
import { formatBytes, formatCount } from "../../lib/format";
import { useResultsStore } from "../../state/results";
import type { FileNode, PartitionInfo } from "../../types";

export function ReconstructionTree({
  nodes,
  partitions,
}: {
  nodes: FileNode[];
  partitions: PartitionInfo[];
}) {
  const activePartition = useResultsStore((s) => s.activePartition);
  const setPartition = useResultsStore((s) => s.setPartition);
  const openPreview = useResultsStore((s) => s.openPreview);
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());

  /* Children-by-parent map, computed once per snapshot change. */
  const children = useMemo(() => {
    const map = new Map<number | null, FileNode[]>();
    for (const n of nodes) {
      const list = map.get(n.parentId) ?? [];
      list.push(n);
      map.set(n.parentId, list);
    }
    for (const list of map.values()) {
      list.sort((a, b) => (a.isDir === b.isDir ? a.name.localeCompare(b.name) : a.isDir ? -1 : 1));
    }
    return map;
  }, [nodes]);

  /* Directory → partitions with files somewhere beneath it: built
   * bottom-up from every FILE (files carry the partition id; dirs
   * don't). A shared folder name shows under every partition that
   * owns files inside it — exactly what “partition-wise
   * reconstruction” means when two volumes reuse folder names. */
  const dirPartitions = useMemo(() => {
    const owner = new Map<number, Set<string>>();
    const nodeById = new Map<number, FileNode>();
    for (const n of nodes) nodeById.set(n.id, n);
    const claim = (dirId: number | null, part: string): void => {
      let cur = dirId;
      while (cur !== null) {
        let set = owner.get(cur);
        if (!set) {
          set = new Set();
          owner.set(cur, set);
        }
        set.add(part);
        const d = nodeById.get(cur);
        cur = d?.parentId ?? null;
      }
    };
    for (const n of nodes) {
      if (!n.isDir && n.partition) claim(n.parentId, n.partition);
    }
    return owner;
  }, [nodes]);

  const toggle = (id: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  if (partitions.length === 0) {
    return (
      <div className="dw-tree-empty">
        {nodes.length === 0
          ? "Partitions appear as the scan resolves them…"
          : "No partition layout was resolved for this target."}
      </div>
    );
  }

  const renderDirs = (parent: number | null, depth: number, partId: string): React.ReactNode => {
    const kids = (children.get(parent) ?? []).filter(
      (k) => k.isDir && (dirPartitions.get(k.id)?.has(partId) ?? false),
    );
    return kids.map((k) => {
      const fileCount = (children.get(k.id) ?? []).filter(
        (c) => !c.isDir && c.partition === partId,
      ).length;
      const subDirs = (children.get(k.id) ?? []).filter(
        (c) => c.isDir && (dirPartitions.get(c.id)?.has(partId) ?? false),
      );
      const isOpen = expanded.has(`${partId}:${k.id}`);
      return (
        <div key={`${partId}:${k.id}`}>
          <button
            type="button"
            className="dw-recon-row"
            style={{ paddingLeft: 8 + depth * 14 }}
            onClick={() => {
              toggle(`${partId}:${k.id}`);
              openPreview(null);
            }}
            title={`${k.path} · ${formatCount(fileCount)} files · ${formatBytes(k.size)}`}
          >
            {subDirs.length > 0 ? (
              isOpen ? (
                <ChevronDownIcon size={12} className="dw-tree-chev" />
              ) : (
                <ChevronRightIcon size={12} className="dw-tree-chev" />
              )
            ) : (
              <span className="dw-tree-chev" style={{ width: 12 }} />
            )}
            <span className="dw-tree-folder">
              <FolderIcon size={13} strokeWidth={1.8} />
            </span>
            <span className="dw-truncate">{k.name}</span>
            <span className="dw-recon-count dw-num">{formatCount(fileCount)}</span>
          </button>
          {isOpen && renderDirs(k.id, depth + 1, partId)}
        </div>
      );
    });
  };

  return (
    <div className="dw-recon-tree" role="list" aria-label="Partition reconstruction">
      {partitions.map((p) => {
        const isActive = activePartition === p.id;
        const isOpen = expanded.has(`part:${p.id}`);
        return (
          <div key={p.id} className="dw-recon-partition" role="listitem">
            <button
              type="button"
              className={`dw-recon-row dw-recon-head ${isActive ? "is-active" : ""}`}
              onClick={() => {
                // Row click: expand + scope the file table to this
                // partition (the scope chip in the toolbar clears it).
                toggle(`part:${p.id}`);
                if (!isActive) setPartition(p.id);
              }}
              title={
                isActive
                  ? `${p.label} — scoped to the file list (clear via the chip on the toolbar)`
                  : `Scope the file list to ${p.label}`
              }
            >
              <span className="dw-recon-chevron" aria-hidden="true">
                {isOpen ? <ChevronDownIcon size={13} /> : <ChevronRightIcon size={13} />}
              </span>
              <span className={`dw-recon-icon ${p.isLost ? "is-lost" : ""}`}>
                <HardDriveIcon size={13} strokeWidth={1.8} />
              </span>
              <span className="dw-recon-name">
                <span className="dw-recon-title dw-truncate">
                  {p.label}
                  {p.isLost && <span className="dw-recon-lost-chip">Lost</span>}
                </span>
                <span className="dw-recon-sub dw-num">
                  {p.fileSystem} · {formatBytes(p.sizeBytes)}
                </span>
              </span>
              <span className="dw-recon-count dw-num" title={`${formatCount(p.fileCount)} files recovered · ${formatBytes(p.bytesFound)}`}>
                {formatCount(p.fileCount)}
              </span>
            </button>
            {isOpen && (
              <div className="dw-recon-partition-body">
                {renderDirs(nodes.length > 0 ? nodes[0].id : null, 0, p.id)}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
