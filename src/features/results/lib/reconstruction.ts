/**
 * The reconstruction model — partition-wise recovery structure derived
 * ENTIRELY client-side from the frozen snapshot wire format (no core
 * contract changes; the previous session's wire extension was reverted
 * and this module is the lesson learned).
 *
 * The real core's whole-disk scans root their trees at per-partition
 * branches ("Partition 1", "Lost Partition 1" — see dw-core tree.rs and
 * the Recovery Lab evidence); volume scans root at the volume itself.
 * This module detects that shape from the node list and aggregates:
 *
 *   - per-partition file counts + bytes (roll-up)
 *   - per-partition category mix
 *   - the special carve branches (File Path Lost / Tags Extracted)
 *   - lost partitions (the deep scan's recovered volumes)
 *
 * Pure functions only — unit-tested against synthetic whole-disk and
 * single-volume snapshots, including edge cases (empty, no dirs, deep
 * chains, renamed roots).
 */
import type { FileCategory, FileNode, NodeOrigin } from "../../../types";

/** One partition-level group in the reconstruction tree. */
export interface ReconstructionPartition {
  /** Stable id = the top-level directory node id (or 0 for root). */
  id: number;
  label: string;
  /** volume = the scanned target itself; partition = a table entry;
   * lost = a partition the deep scan carved back. */
  kind: "volume" | "partition" | "lost-partition";
  isLost: boolean;
  fileCount: number;
  bytes: number;
  categories: Partial<Record<FileCategory, { count: number; size: number }>>;
  origins: Partial<Record<NodeOrigin, number>>;
  /** Direct child dir names (the reconstructed top-level structure). */
  children: { id: number; name: string; fileCount: number; bytes: number }[];
}

export interface ReconstructionModel {
  partitions: ReconstructionPartition[];
  /** The derivation basis (for empty states). */
  shape: "empty" | "single-volume" | "multi-partition";
}

const LOST_PARTITION_RE = /^lost[\s_-]*partition/i;
const PARTITION_RE = /^partition[\s_-]*\d+/i;

/** The carve/special branches the tree builder creates. */
export const RECON_BRANCH_LABELS = [
  "File Path Lost",
  "File Name Lost",
  "Tags Extracted",
  "Other deleted files",
] as const;

function isBranchLabel(name: string): boolean {
  return RECON_BRANCH_LABELS.some((b) => name.toLowerCase() === b.toLowerCase());
}

export function buildReconstructionModel(nodes: FileNode[]): ReconstructionModel {
  if (nodes.length === 0) {
    return { partitions: [], shape: "empty" };
  }

  // Children-by-parent + the node index.
  const byId = new Map<number, FileNode>();
  const children = new Map<number | null, FileNode[]>();
  for (const n of nodes) {
    byId.set(n.id, n);
    const list = children.get(n.parentId) ?? [];
    list.push(n);
    children.set(n.parentId, list);
  }

  // Top-level dirs: nodes whose parent is null, or (single-root case)
  // the children of the sole root dir.
  const roots = (children.get(null) ?? []).filter((n) => n.isDir);
  let topLevel: FileNode[];
  let shape: ReconstructionModel["shape"];

  if (roots.length === 0) {
    return { partitions: [], shape: "empty" };
  }
  if (roots.length === 1) {
    const kids = (children.get(roots[0].id) ?? []).filter((n) => n.isDir);
    const partitionShaped = kids.some(
      (k) => PARTITION_RE.test(k.name) || LOST_PARTITION_RE.test(k.name),
    );
    if (partitionShaped) {
      topLevel = kids;
      shape = "multi-partition";
    } else {
      topLevel = [roots[0]];
      shape = "single-volume";
    }
  } else {
    topLevel = roots;
    shape = "multi-partition";
  }

  // Partition buckets. A bucket = top-level dir; every file maps to
  // exactly one bucket by walking up to the top level.
  const bucketOf = new Map<number, number>(); // any dir node id → bucket root id
  for (const t of topLevel) bucketOf.set(t.id, t.id);
  // Propagate bucket ownership down (BFS per top-level tree).
  for (const t of topLevel) {
    const stack = [...(children.get(t.id) ?? []).filter((n) => n.isDir)];
    while (stack.length > 0) {
      const d = stack.pop()!;
      if (bucketOf.has(d.id)) continue;
      bucketOf.set(d.id, t.id);
      for (const k of children.get(d.id) ?? []) if (k.isDir) stack.push(k);
    }
  }

  interface Agg {
    fileCount: number;
    bytes: number;
    categories: Partial<Record<FileCategory, { count: number; size: number }>>;
    origins: Partial<Record<NodeOrigin, number>>;
    childAgg: Map<number, { fileCount: number; bytes: number }>;
  }
  const aggs = new Map<number, Agg>();
  for (const t of topLevel) {
    aggs.set(t.id, {
      fileCount: 0,
      bytes: 0,
      categories: {},
      origins: {},
      childAgg: new Map(),
    });
  }

  for (const n of nodes) {
    if (n.isDir) continue;
    // Walk up to the owning bucket (files always have a dir ancestor;
    // tolerate orphans by skipping them — a torn snapshot mid-append).
    let cursor: FileNode | undefined = n;
    let bucketId: number | undefined;
    let hops = 0;
    while (cursor && hops < 4096) {
      const owner = bucketOf.get(cursor.id);
      if (owner !== undefined) {
        bucketId = owner;
        break;
      }
      cursor = cursor.parentId === null ? undefined : byId.get(cursor.parentId);
      hops += 1;
    }
    if (bucketId === undefined) continue;
    const agg = aggs.get(bucketId);
    if (!agg) continue;

    agg.fileCount += 1;
    agg.bytes += n.size;
    const cat = agg.categories[n.category] ?? { count: 0, size: 0 };
    cat.count += 1;
    cat.size += n.size;
    agg.categories[n.category] = cat;
    agg.origins[n.origin] = (agg.origins[n.origin] ?? 0) + 1;

    // Direct-child roll-up (files one level under a top dir).
    if (n.parentId !== null) {
      const parent = byId.get(n.parentId);
      if (parent && bucketOf.get(parent.id) === bucketId && parent.id !== bucketId) {
        const ca = agg.childAgg.get(parent.id) ?? { fileCount: 0, bytes: 0 };
        ca.fileCount += 1;
        ca.bytes += n.size;
        agg.childAgg.set(parent.id, ca);
      }
    }
  }

  const partitions: ReconstructionPartition[] = topLevel.map((t) => {
    const agg = aggs.get(t.id)!;
    const isLost = LOST_PARTITION_RE.test(t.name);
    return {
      id: t.id,
      label: t.name,
      kind: isLost ? "lost-partition" : shape === "multi-partition" ? "partition" : "volume",
      isLost,
      fileCount: agg.fileCount,
      bytes: agg.bytes,
      categories: agg.categories,
      origins: agg.origins,
      children: (children.get(t.id) ?? [])
        .filter((k) => k.isDir && !isBranchLabel(k.name))
        .map((k) => ({
          id: k.id,
          name: k.name,
          fileCount: agg.childAgg.get(k.id)?.fileCount ?? 0,
          bytes: agg.childAgg.get(k.id)?.bytes ?? 0,
        }))
        .sort((a, b) => b.fileCount - a.fileCount)
        .slice(0, 24),
    };
  });

  // Lost partitions surface first (they're the headline), then by size.
  partitions.sort((a, b) => {
    if (a.isLost !== b.isLost) return a.isLost ? -1 : 1;
    return b.bytes - a.bytes;
  });

  return { partitions, shape };
}

/** Does a node belong to the partition bucket with this label? (the
 * table's scope filter — path-prefix based, the core's own naming). */
export function nodeInPartition(node: FileNode, partitionLabel: string): boolean {
  const prefix = `${partitionLabel}\\`;
  return node.path === partitionLabel || node.path.startsWith(prefix);
}
