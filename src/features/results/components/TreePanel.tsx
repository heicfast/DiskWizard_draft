/**
 * TreePanel — the results left pane with two tabs (the reference's
 * "Folders" bar, with File Types replaced by FILE RECONSTRUCTION per
 * the owner directive):
 *
 *   Folders            the path tree with the special carve branches
 *   File Reconstruction the partition-wise recovery view (lost
 *                      partitions included)
 */
import { useMemo, useState } from "react";
import { ChevronDownIcon, ChevronRightIcon, FolderIcon } from "lucide-react";
import { formatBytes, formatCount } from "../../../lib/format";
import { useResultsStore } from "../../../state/results";
import type { FileNode } from "../../../types";
import { Segmented } from "../../../components/ui";
import { ReconstructionTree } from "./ReconstructionTree";

const LOST_BRANCH_LABELS = new Set(["File Path Lost", "Tags Extracted", "File Name Lost"]);

export function TreePanel() {
  const treeMode = useResultsStore((s) => s.treeMode);
  const setTreeMode = useResultsStore((s) => s.setTreeMode);
  const snapshot = useResultsStore((s) => s.snapshot);

  return (
    <aside className="dw-tree-panel" aria-label="Results tree">
      <div className="dw-tree-modes">
        <Segmented
          ariaLabel="Tree mode"
          value={treeMode}
          onChange={setTreeMode}
          options={[
            { value: "folders", label: "Folders" },
            { value: "reconstruction", label: "File Reconstruction" },
          ]}
        />
      </div>
      <div className="dw-tree-scroll dw-scroll">
        {treeMode === "reconstruction" ? (
          <ReconstructionTree />
        ) : (
          <FolderTree
            nodes={snapshot?.nodes ?? []}
            rootName={snapshot?.targetName ?? "Scan"}
          />
        )}
      </div>
    </aside>
  );
}

/** The path tree: root + collapsible folders + the carve branches. */
function FolderTree({ nodes, rootName }: { nodes: FileNode[]; rootName: string }) {
  const activeCategory = useResultsStore((s) => s.activeCategory);
  const setCategory = useResultsStore((s) => s.setCategory);
  const [collapsed, setCollapsed] = useState<Set<number>>(new Set());

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

  const root = nodes.length > 0 ? nodes[0] : null;

  const toggle = (id: number) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const renderNode = (node: FileNode, depth: number): React.ReactNode => {
    if (!node.isDir) return null;
    const kids = children.get(node.id) ?? [];
    const dirKids = kids.filter((k) => k.isDir);
    const fileCount = kids.filter((k) => !k.isDir).length;
    const isCollapsed = collapsed.has(node.id);
    const isLost = LOST_BRANCH_LABELS.has(node.name);
    return (
      <div key={node.id}>
        <button
          type="button"
          className="dw-tree-row"
          style={{ paddingLeft: 4 + depth * 14 }}
          onClick={() => toggle(node.id)}
          aria-expanded={!isCollapsed}
        >
          {isCollapsed ? (
            <ChevronRightIcon size={13} className="dw-tree-chev" />
          ) : (
            <ChevronDownIcon size={13} className="dw-tree-chev" />
          )}
          <span className={`dw-tree-icon ${isLost ? "is-lost-branch" : ""}`}>
            <FolderIcon size={13} />
          </span>
          <span className="dw-tree-label dw-truncate">{node.name}</span>
          <span
            className="dw-tree-count dw-num"
            title={`${formatCount(fileCount)} files · ${formatBytes(node.size)}`}
          >
            {formatCount(fileCount)}
          </span>
        </button>
        {!isCollapsed && dirKids.map((k) => renderNode(k, depth + 1))}
      </div>
    );
  };

  if (!root) {
    return <div className="dw-tree-empty">Building tree…</div>;
  }
  return (
    <div role="list" aria-label="Folder tree">
      <button
        type="button"
        className={`dw-tree-row ${activeCategory === "all" ? "is-active" : ""}`}
        onClick={() => setCategory("all")}
      >
        <span className="dw-tree-icon is-root">
          <FolderIcon size={13} />
        </span>
        <span className="dw-tree-label">{rootName}</span>
        <span className="dw-tree-count dw-num">
          {formatCount(
            nodes.filter((n) => !n.isDir).length,
          )}
        </span>
      </button>
      {(children.get(root.id) ?? [])
        .filter((k) => k.isDir)
        .map((k) => renderNode(k, 1))}
    </div>
  );
}
