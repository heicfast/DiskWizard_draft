/**
 * The left tree panel — two modes (Folders = the path tree with the
 * special carve branches; File reconstruction = the partition-aware
 * recovery structure. It replaces the generic "File Types" tab because a
 * recovery tool needs to explain where a result came from, not just its
 * extension.
 */
import { useMemo, useState } from "react";
import { ChevronDownIcon, ChevronRightIcon, FolderIcon } from "lucide-react";
import { formatBytes, formatCount } from "../../lib/format";
import { useResultsStore } from "../../state/results";
import { CATEGORY_LABELS, CATEGORY_ORDER, type FileCategory, type FileNode } from "../../types";
import { CategoryIcon, categoryTone } from "./categoryIcon";

const LOST_BRANCH_LABELS = new Set(["File Path Lost", "Tags Extracted", "Reconstructed"]);

export function TreePanel() {
  const treeMode = useResultsStore((s) => s.treeMode);
  const setTreeMode = useResultsStore((s) => s.setTreeMode);
  const snapshot = useResultsStore((s) => s.snapshot);
  const activeCategory = useResultsStore((s) => s.activeCategory);
  const setCategory = useResultsStore((s) => s.setCategory);
  const search = useResultsStore((s) => s.search);

  /* Reconstruction lens: category rollups stay under the scanned partition,
   * while special branches expose files whose original structure could not
   * be preserved. This follows the research corpus' volume / File Path Lost
   * / Reconstructed model without inventing partitions the core has not
   * reported. */
  const reconstruction = useMemo(() => {
    if (!snapshot) return { categories: [], pathLost: 0, reconstructed: 0 };
    const total = snapshot.categories.reduce((a, c) => a + c.count, 0);
    const categories = [
      { key: "all" as const, label: "All recovered files", count: total, size: snapshot.categories.reduce((a, c) => a + c.size, 0) },
      ...CATEGORY_ORDER.map((c) => {
        const hit = snapshot.categories.find((x) => x.category === c);
        return { key: c, label: CATEGORY_LABELS[c], count: hit?.count ?? 0, size: hit?.size ?? 0 };
      }).filter((r) => r.count > 0),
    ];
    const allFiles = snapshot.nodes.filter((node) => !node.isDir);
    const pathLost = allFiles.filter((node) => node.origin === "lost-path").length;
    // The core identifies a name-rebuilt RAW result as `lost-name`; do not
    // mislabel metadata-tag results as reconstructed content.
    const reconstructed = allFiles.filter((node) => node.origin === "lost-name").length;
    return { categories, pathLost, reconstructed };
  }, [snapshot]);

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
          aria-selected={treeMode === "reconstruction"}
          className={treeMode === "reconstruction" ? "is-on" : ""}
          onClick={() => setTreeMode("reconstruction")}
        >
          File reconstruction
        </button>
      </div>

      {treeMode === "reconstruction" ? (
        <div className="dw-reconstruction-tree" role="list">
          <div className="dw-reconstruction-note">
            <strong>{snapshot?.targetName ?? "Selected partition"}</strong>
            <span>Recovered structure</span>
          </div>
          <div className="dw-type-tree" role="list" aria-label="Recovered file categories">
          {reconstruction.categories.map((row) => (
            <button
              key={row.key}
              type="button"
              role="listitem"
              className={`dw-type-row ${activeCategory === row.key ? "is-active" : ""}`}
              onClick={() => setCategory(row.key)}
            >
              <span className={`dw-type-icon ${row.key === "all" ? "" : categoryTone(row.key)}`}>
                <CategoryIcon category={row.key === "all" ? "other" : row.key} size={15} />
              </span>
              <span className="dw-truncate">{row.label}</span>
              <span className="dw-type-count dw-num">{formatCount(row.count)}</span>
            </button>
          ))}
          </div>
          <div className="dw-reconstruction-branches" aria-label="Reconstruction branches">
            <ReconstructionBranch label="File Path Lost" count={reconstruction.pathLost} tone="lost" />
            <ReconstructionBranch label="Reconstructed" count={reconstruction.reconstructed} tone="reconstructed" />
          </div>
          <p className="dw-tree-rootnote">
            Lost partitions appear here only when the scan engine identifies and validates them.
          </p>
        </div>
      ) : (
        <FolderTree
          snapshot={snapshot?.nodes ?? []}
          rootName={snapshot?.targetName ?? "Scan"}
          search={search}
          activeCategory={activeCategory}
        />
      )}
    </aside>
  );
}

function ReconstructionBranch({
  label,
  count,
  tone,
}: {
  label: string;
  count: number;
  tone: "lost" | "reconstructed";
}) {
  return (
    <div className={`dw-reconstruction-branch is-${tone}`} role="listitem">
      <span className="dw-reconstruction-branch-icon"><FolderIcon size={14} strokeWidth={1.8} /></span>
      <span className="dw-truncate">{label}</span>
      <span className="dw-type-count dw-num">{formatCount(count)}</span>
    </div>
  );
}

/** The path tree: root + collapsible folders + the carve branches. */
function FolderTree({
  snapshot,
  rootName,
  activeCategory,
}: {
  snapshot: FileNode[];
  rootName: string;
  search: string;
  activeCategory: FileCategory | "all";
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
      <div className="dw-tree-rootnote" data-cat={activeCategory}>
        {rootName} — folders are rebuilt as the scan digs; carved files land under the lost branches.
      </div>
      {renderNode(root, 0)}
    </div>
  );
}
