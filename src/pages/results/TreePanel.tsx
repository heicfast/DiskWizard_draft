/**
 * The left tree panel — two modes:
 *
 *  - Folders        the path tree with the special carve branches
 *                   (File Path Lost / Tags Extracted — the research's
 *                   CLeftTreeView contract).
 *  - Reconstruction the partition-wise view of everything recovered:
 *                   one section per partition (existing volumes first,
 *                   then lost partitions surfaced by the deep scan —
 *                   the "Smart Partition Scanning" presentation), each
 *                   expandable into its reconstructed folder structure
 *                   and clickable to scope the file table.
 */
import { useMemo, useState } from "react";
import { ChevronDownIcon, ChevronRightIcon, FolderIcon } from "lucide-react";
import { formatBytes, formatCount } from "../../lib/format";
import { useResultsStore } from "../../state/results";
import type { FileNode } from "../../types";
import { ReconstructionTree } from "./ReconstructionTree";

export function TreePanel() {
  const treeMode = useResultsStore((s) => s.treeMode);
  const setTreeMode = useResultsStore((s) => s.setTreeMode);
  const snapshot = useResultsStore((s) => s.snapshot);
  const search = useResultsStore((s) => s.search);

  return (
    <aside className="dw-tree-panel dw-scroll" aria-label="Results tree">
      <div className="dw-segmented dw-tree-modes" role="tablist" aria-label="Tree mode">
        <button
          type="button"
          role="tab"
          aria-selected={treeMode === "folders"}
          className={treeMode === "folders" ? "is-on" : ""}
          onClick={() => setTreeMode("folders")}
        >
          Folders
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={treeMode === "recon"}
          className={treeMode === "recon" ? "is-on" : ""}
          onClick={() => setTreeMode("recon")}
        >
          Reconstruction
        </button>
      </div>

      {treeMode === "recon" ? (
        <ReconstructionTree
          nodes={snapshot?.nodes ?? []}
          partitions={snapshot?.partitions ?? []}
        />
      ) : (
        <FolderTree
          snapshot={snapshot?.nodes ?? []}
          rootName={snapshot?.targetName ?? "Scan"}
          search={search}
        />
      )}
    </aside>
  );
}

/** The path tree: root + collapsible folders + the carve branches. */
function FolderTree({
  snapshot,
  rootName,
}: {
  snapshot: FileNode[];
  rootName: string;
  search: string;
}) {
  const openPreview = useResultsStore((s) => s.openPreview);
  const [collapsed, setCollapsed] = useState<Set<number>>(new Set());

  /* Children by parent, precomputed once per snapshot. */
  const children = useMemo(() => {
    const map = new Map<number | null, FileNode[]>();
    for (const n of snapshot) {
      const list = map.get(n.parentId) ?? [];
      list.push(n);
      map.set(n.parentId, list);
    }
    for (const list of map.values()) list.sort((a, b) => (a.isDir === b.isDir ? a.name.localeCompare(b.name) : a.isDir ? -1 : 1));
    return map;
  }, [snapshot]);

  const root = snapshot.length > 0 ? snapshot[0] : null;

  const renderNode = (node: FileNode, depth: number): React.ReactNode => {
    if (!node.isDir) return null;
    const kids = children.get(node.id) ?? [];
    const dirKids = kids.filter((k) => k.isDir);
    const fileCount = kids.filter((k) => !k.isDir).length;
    const isCollapsed = collapsed.has(node.id);
    const isLost = LOST_BRANCH_LABELS.has(node.name);
    return (
      <div key={node.id} className="dw-tree-branch">
        <button
          type="button"
          className={`dw-tree-row ${isLost ? "is-lost" : ""}`}
          style={{ paddingLeft: 6 + depth * 14 }}
          onClick={() => {
            const next = new Set(collapsed);
            if (next.has(node.id)) next.delete(node.id);
            else next.add(node.id);
            setCollapsed(next);
            openPreview(null);
          }}
        >
          {isCollapsed ? <ChevronRightIcon size={13} className="dw-tree-chev" /> : <ChevronDownIcon size={13} className="dw-tree-chev" />}
          <span className={`dw-tree-folder ${isLost ? "is-lost" : ""}`}>
            <FolderIcon size={14} strokeWidth={1.8} />
          </span>
          <span className="dw-truncate">{node.name}</span>
          <span className="dw-tree-count dw-num" title={`${formatCount(fileCount)} files · ${formatBytes(node.size)}`}>
            {formatCount(fileCount)}
          </span>
        </button>
        {!isCollapsed &&
          dirKids.map((k) => renderNode(k, depth + 1))}
      </div>
    );
  };

  if (!root) {
    return <div className="dw-tree-empty">Building tree…</div>;
  }
  return (
    <div className="dw-folder-tree">
      <div className="dw-tree-rootnote">
        {rootName} — folders are rebuilt as the scan digs; carved files land under the lost branches.
      </div>
      {renderNode(root, 0)}
    </div>
  );
}

const LOST_BRANCH_LABELS = new Set(["File Path Lost", "Tags Extracted"]);
