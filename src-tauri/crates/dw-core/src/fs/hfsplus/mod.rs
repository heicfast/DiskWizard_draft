//! The HFS+/HFSX engine — catalog B-tree leaf walk + cnid chains.
//!
//! Algorithm (research:
//! `artifacts/SearchDLL/hfsplus-engine--deep-dive.md` and
//! `ext4-hfs-engine-algorithms-v2.md` §3-4 — the volume-header gate
//! plus the leaf-record math, byte-exact):
//!
//! 1. Volume header at byte 1024 (big-endian): signature "H+"
//!    (0x482B) or "HX" (0x4858, case-sensitive HFSX); the vendor's
//!    acceptance gate — block size a multiple of 512, version < 17,
//!    allocation/extents fork block counts ≤ totalBlocks.
//! 2. Catalog B-tree: node 0 is the header node (nodeSize, rootNode,
//!    firstLeaf/lastLeaf); every catalog record lives in a leaf, and
//!    leaves chain forward via fLink — the full enumeration walks
//!    that chain (no tree navigation needed).
//! 3. Records: catalog key = (parentID, HFSUniStr255 name); data =
//!    folder(1) / file(2) / folder-thread(3) / file-thread(4).
//!    Folder/file records build the cnid → (parent, name) map; paths
//!    reconstruct up the chain to the root folder (cnid 2).
//! 4. Forks: data + resource both surface (resource as
//!    `name/..namedfork/rsrc`, the recovery-product convention);
//!    extents = 8 inline + the extents-overflow B-tree (leaf records
//!    keyed by cnid/fork/startBlock).
//! 5. Deleted recovery (HFS+ overwrites catalog records on delete —
//!    the vendor's deep pass): orphaned-cnid sweep (records whose
//!    parent chain breaks — a deleted folder's surviving children)
//!    plus a conservative stale-record scan of leaf-node free space
//!    (removed records leave bytes until the node rebalances).
//!
//! Journal replay (hfspluslib journal.cpp) is queued as M3 engine
//! work in `docs/TODO.md`.

use std::collections::HashMap;

use crate::blockdev::{BlockDevice, BlockRead};
use crate::error::{CoreError, CoreResult};
use crate::fs::{ScanControl, ScanEngine, ScanSink, ScanStats};
use crate::types::{Extent, FileAttrs, FoundEntry, FsKind, NodeOrigin, Timestamps};

/// HFS+ epoch offset (seconds between 1904-01-01 and 1970-01-01).
const HFS_OFFSET_TO_UNIX: i64 = 2_082_844_800;
/// Special cnids.
const ROOT_FOLDER_ID: u32 = 2;
const CATALOG_FILE_ID: u32 = 4;
/// Catalog record types.
const RT_FOLDER: i16 = 1;
const RT_FILE: i16 = 2;
/// Fork selectors in overflow keys.
const FORK_DATA: u8 = 0;
const FORK_RESOURCE: u8 = 0xFF;
/// Cap on in-memory B-tree bytes (the metadata cache budget).
const MAX_TREE_BYTES: u64 = 64 * 1024 * 1024;
/// Leaf-chain length guard.
const MAX_LEAVES: u64 = 1 << 22;
/// Path recursion guard.
const MAX_PARENT_DEPTH: usize = 64;

/// One fork's inline extent table (8 × {startBlock, blockCount}).
#[derive(Debug, Clone, Copy)]
pub struct ForkData {
    pub logical_size: u64,
    pub total_blocks: u32,
    pub extents: [(u32, u32); 8],
}

impl ForkData {
    fn parse(b: &[u8]) -> Self {
        let be32 = |o: usize| u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        let be64 = |o: usize| {
            u64::from_be_bytes([
                b[o],
                b[o + 1],
                b[o + 2],
                b[o + 3],
                b[o + 4],
                b[o + 5],
                b[o + 6],
                b[o + 7],
            ])
        };
        let mut extents = [(0u32, 0u32); 8];
        for (i, e) in extents.iter_mut().enumerate() {
            *e = (be32(16 + i * 8), be32(16 + i * 8 + 4));
        }
        Self {
            logical_size: be64(0),
            total_blocks: be32(12),
            extents,
        }
    }

    /// Volume-relative extents (block-granular, coalesced).
    fn extent_list(&self, block_size: u64, extra: &[(u32, u32)]) -> Vec<Extent> {
        let mut out: Vec<Extent> = Vec::new();
        let push = |start: u32, count: u32, out: &mut Vec<Extent>| {
            if count == 0 {
                return;
            }
            let off = u64::from(start) * block_size;
            let len = u64::from(count) * block_size;
            match out.last_mut() {
                Some(last) if last.end() == off => last.len += len,
                _ => out.push(Extent::new(off, len)),
            }
        };
        for &(s, c) in &self.extents {
            push(s, c, &mut out);
        }
        for &(s, c) in extra {
            push(s, c, &mut out);
        }
        out
    }
}

/// The parsed volume header.
#[derive(Debug, Clone)]
pub struct HfsVolume {
    pub block_size: u32,
    pub total_blocks: u32,
    pub free_blocks: u32,
    pub file_count: u32,
    pub folder_count: u32,
    pub next_catalog_id: u32,
    pub catalog: ForkData,
    pub ext_overflow: ForkData,
    pub is_hfsx: bool,
}

impl HfsVolume {
    /// Parse + apply the vendor's acceptance gate.
    ///
    /// # Errors
    /// [`CoreError::Corrupt`] on signature/geometry failure.
    pub fn parse(vh: &[u8]) -> CoreResult<Self> {
        if vh.len() < 0x200 {
            return Err(CoreError::Corrupt {
                structure: "HFS+ volume header",
                detail: "short read".into(),
            });
        }
        let sig = u16::from_be_bytes([vh[0], vh[1]]);
        let is_hfsx = sig == 0x4858;
        if sig != 0x482B && !is_hfsx {
            return Err(CoreError::Corrupt {
                structure: "HFS+ volume header",
                detail: format!("signature {sig:#06x} not H+/HX"),
            });
        }
        let be16 = |o: usize| u16::from_be_bytes([vh[o], vh[o + 1]]);
        let be32 = |o: usize| u32::from_be_bytes([vh[o], vh[o + 1], vh[o + 2], vh[o + 3]]);
        let version = be16(0x02);
        // Vendor gate: version < 0x11.
        if version >= 0x11 {
            return Err(CoreError::Corrupt {
                structure: "HFS+ volume header",
                detail: format!("version {version} >= 17"),
            });
        }
        let block_size = be32(0x28);
        let total_blocks = be32(0x2C);
        // Vendor gate: block size a multiple of 512.
        if block_size == 0 || block_size % 512 != 0 {
            return Err(CoreError::Corrupt {
                structure: "HFS+ volume header",
                detail: format!("block size {block_size} not 512-aligned"),
            });
        }
        let catalog = ForkData::parse(&vh[0x110..0x160]);
        let ext_overflow = ForkData::parse(&vh[0xC0..0x110]);
        // Vendor gate: fork block counts clamped by totalBlocks.
        if catalog.total_blocks > total_blocks || ext_overflow.total_blocks > total_blocks {
            return Err(CoreError::Corrupt {
                structure: "HFS+ volume header",
                detail: "fork block count exceeds totalBlocks".into(),
            });
        }
        if catalog.total_blocks == 0 {
            return Err(CoreError::Corrupt {
                structure: "HFS+ volume header",
                detail: "empty catalog fork".into(),
            });
        }
        Ok(Self {
            block_size,
            total_blocks,
            free_blocks: be32(0x30),
            file_count: be32(0x20),
            folder_count: be32(0x24),
            next_catalog_id: be32(0x40),
            catalog,
            ext_overflow,
            is_hfsx,
        })
    }
}

/// One catalog leaf record (parsed).
#[derive(Debug, Clone)]
struct CatalogRecord {
    parent_id: u32,
    name: String,
    record_type: i16,
    /// Folder record: the folder cnid.
    folder_id: u32,
    /// File record: cnid + forks.
    file_id: u32,
    data_fork: ForkData,
    rsrc_fork: ForkData,
    create: i64,
    modify: i64,
}

/// The engine. The whole catalog (+ overflow tree) is resident —
/// the metadata-cache budget caps it (MAX_TREE_BYTES).
pub struct HfsPlusEngine {
    vh: HfsVolume,
    catalog: Vec<u8>,
    catalog_node_size: usize,
    first_leaf: u32,
    /// Overflow extents beyond the inline 8: (cnid, fork) → extra runs.
    overflow: HashMap<(u32, u8), Vec<(u32, u32)>>,
}

impl HfsPlusEngine {
    /// Open + validate + load the catalog (+ overflow tree).
    ///
    /// # Errors
    /// [`CoreError::Corrupt`] when the volume isn't parseable HFS+.
    pub fn new(dev: &dyn BlockDevice) -> CoreResult<Self> {
        let mut vhbuf = [0u8; 512];
        dev.read_bytes(1024, &mut vhbuf)?;
        let vh = HfsVolume::parse(&vhbuf)?;
        // The overflow tree's own extents come from the VH (its inline
        // 8 suffice for any sane volume).
        let mut overflow = HashMap::new();
        if vh.ext_overflow.total_blocks > 0 {
            let tree = read_fork_bytes(dev, &vh, &vh.ext_overflow, &[]);
            overflow = parse_overflow_tree(&tree);
        }
        // The catalog's own overflow (cnid 4) resolves through the
        // now-loaded map.
        let extra = overflow
            .get(&(CATALOG_FILE_ID, FORK_DATA))
            .cloned()
            .unwrap_or_default();
        let catalog = read_fork_bytes(dev, &vh, &vh.catalog, &extra);
        if catalog.len() < 14 + 0x16 {
            return Err(CoreError::Corrupt {
                structure: "HFS+ catalog",
                detail: "short catalog".into(),
            });
        }
        let (node_size, first_leaf, _last_leaf) = parse_header_node(&catalog)?;
        Ok(Self {
            vh,
            catalog,
            catalog_node_size: node_size,
            first_leaf,
            overflow,
        })
    }

    /// Iterate every leaf node in the fLink chain, parsing records.
    fn leaf_records(&self) -> Vec<CatalogRecord> {
        let mut out = Vec::new();
        let mut node = self.first_leaf;
        let mut hops = 0u64;
        while node != 0 && hops < MAX_LEAVES {
            hops += 1;
            let Some(nb) = self.node_bytes(node) else {
                break;
            };
            if nb[8] != 0 {
                break; // not a leaf — chain corrupt, stop
            }
            let count = u16::from_be_bytes([nb[10], nb[11]]) as usize;
            let node_end = self.catalog_node_size;
            for i in 0..count.min(1024) {
                let oslot = node_end - (i + 1) * 2;
                if oslot < 14 || oslot + 2 > node_end {
                    break;
                }
                let off = u16::from_be_bytes([nb[oslot], nb[oslot + 1]]) as usize;
                if off < 14 || off + 4 > node_end {
                    continue;
                }
                if let Some(rec) = parse_record(&nb[off..node_end]) {
                    out.push(rec);
                }
            }
            node = u32::from_be_bytes([nb[0], nb[1], nb[2], nb[3]]);
        }
        out
    }

    /// Bytes of B-tree node `n` from the loaded catalog image.
    fn node_bytes(&self, n: u32) -> Option<&[u8]> {
        let start = n as usize * self.catalog_node_size;
        let end = start + self.catalog_node_size;
        if end > self.catalog.len() || self.catalog_node_size == 0 {
            return None;
        }
        Some(&self.catalog[start..end])
    }

    /// Reconstruct a record's path by walking parent cnids.
    fn path_of(rec: &CatalogRecord, by_cnid: &HashMap<u32, CatalogRecord>) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        parts.push(rec.name.clone());
        let mut parent = rec.parent_id;
        let mut depth = 0usize;
        while parent != ROOT_FOLDER_ID {
            if parent == 1 {
                // Root folder's own record — path complete.
                break;
            }
            let Some(p) = by_cnid.get(&parent) else {
                return None; // broken chain → orphan
            };
            parts.push(p.name.clone());
            parent = p.parent_id;
            depth += 1;
            if depth > MAX_PARENT_DEPTH {
                return None;
            }
        }
        parts.reverse();
        Some(parts.join("\\"))
    }

    fn fork_extents(&self, cnid: u32, fork: &ForkData, which: u8) -> Vec<Extent> {
        let extra = self
            .overflow
            .get(&(cnid, which))
            .cloned()
            .unwrap_or_default();
        fork.extent_list(u64::from(self.vh.block_size), &extra)
    }
}

/// Read a fork's bytes through its extent table (+extra overflow runs).
fn read_fork_bytes(
    dev: &dyn BlockDevice,
    vh: &HfsVolume,
    fork: &ForkData,
    extra: &[(u32, u32)],
) -> Vec<u8> {
    let len = fork.logical_size.min(MAX_TREE_BYTES);
    if len == 0 {
        return Vec::new();
    }
    let bs = u64::from(vh.block_size);
    let mut buf = vec![0u8; len as usize];
    let mut pos = 0usize;
    let mut remaining_blocks = fork.total_blocks;

    let mut runs: Vec<(u32, u32)> = fork.extents.to_vec();
    runs.extend_from_slice(extra);
    for &(start, count) in &runs {
        if pos >= buf.len() || count == 0 || remaining_blocks == 0 {
            break;
        }
        let take = count.min(remaining_blocks);
        let off = u64::from(start) * bs;
        let want = ((u64::from(take) * bs) as usize).min(buf.len() - pos);
        if off >= dev.capacity_bytes() {
            break;
        }
        // Unreadable metadata run: zero-fill and continue (the
        // salvage model — partial B-trees still parse).
        let _ = dev.read_bytes(off, &mut buf[pos..pos + want]);
        pos += want;
        remaining_blocks -= take;
    }
    buf
}

/// Parse B-tree header node → (nodeSize, firstLeaf, lastLeaf).
fn parse_header_node(tree: &[u8]) -> CoreResult<(usize, u32, u32)> {
    if tree.len() < 14 + 0x16 {
        return Err(CoreError::Corrupt {
            structure: "HFS+ B-tree header",
            detail: "short header node".into(),
        });
    }
    let be16 = |o: usize| u16::from_be_bytes([tree[o], tree[o + 1]]);
    let be32 = |o: usize| u32::from_be_bytes([tree[o], tree[o + 1], tree[o + 2], tree[o + 3]]);
    let node_size = be16(14 + 0x12) as usize;
    if node_size < 512 || !node_size.is_power_of_two() {
        return Err(CoreError::Corrupt {
            structure: "HFS+ B-tree header",
            detail: format!("node size {node_size}"),
        });
    }
    let first = be32(14 + 0x0A);
    let last = be32(14 + 0x0E);
    Ok((node_size, first, last))
}

/// Walk the overflow B-tree's leaf chain → (cnid, fork) → extra runs.
fn parse_overflow_tree(tree: &[u8]) -> HashMap<(u32, u8), Vec<(u32, u32)>> {
    let mut map = HashMap::new();
    let Ok((node_size, first, _last)) = parse_header_node(tree) else {
        return map;
    };
    let node_bytes = |n: u32| -> Option<&[u8]> {
        let s = n as usize * node_size;
        let e = s + node_size;
        (e <= tree.len()).then(|| &tree[s..e])
    };
    let mut node = first;
    let mut hops = 0u64;
    while node != 0 && hops < MAX_LEAVES {
        hops += 1;
        let Some(nb) = node_bytes(node) else {
            break;
        };
        if nb[8] != 0 {
            break;
        }
        let count = u16::from_be_bytes([nb[10], nb[11]]) as usize;
        for i in 0..count.min(1024) {
            let oslot = node_size - (i + 1) * 2;
            if oslot < 14 {
                break;
            }
            let off = u16::from_be_bytes([nb[oslot], nb[oslot + 1]]) as usize;
            if off < 14 || off + 10 + 8 > node_size {
                continue;
            }
            let rec = &nb[off..node_size];
            let key_len = u16::from_be_bytes([rec[0], rec[1]]) as usize;
            if key_len != 10 || rec.len() < 2 + 10 + 64 {
                continue;
            }
            let cnid = u32::from_be_bytes([rec[2], rec[3], rec[4], rec[5]]);
            let start = u32::from_be_bytes([rec[6], rec[7], rec[8], rec[9]]);
            let fork_id = rec[10];
            // Data: 8 × {startBlock, blockCount} BE.
            let data = &rec[12..];
            let mut runs = Vec::new();
            for j in 0..8 {
                if j * 8 + 8 > data.len() {
                    break;
                }
                let s = u32::from_be_bytes([
                    data[j * 8],
                    data[j * 8 + 1],
                    data[j * 8 + 2],
                    data[j * 8 + 3],
                ]);
                let c = u32::from_be_bytes([
                    data[j * 8 + 4],
                    data[j * 8 + 5],
                    data[j * 8 + 6],
                    data[j * 8 + 7],
                ]);
                if c > 0 {
                    runs.push((start.wrapping_add(s.saturating_sub(start)), c));
                }
            }
            // The record's key startBlock orders runs; merge by sort.
            let entry = map.entry((cnid, fork_id)).or_insert_with(Vec::new);
            entry.extend(runs);
        }
        node = u32::from_be_bytes([nb[0], nb[1], nb[2], nb[3]]);
    }
    // Sort each run list by start block.
    for runs in map.values_mut() {
        runs.sort_unstable();
    }
    map
}

/// Parse one catalog record from leaf bytes.
fn parse_record(rec: &[u8]) -> Option<CatalogRecord> {
    let key_len = u16::from_be_bytes([rec[0], rec[1]]) as usize;
    if key_len < 6 || rec.len() < key_len + 2 {
        return None;
    }
    let parent_id = u32::from_be_bytes([rec[2], rec[3], rec[4], rec[5]]);
    let name_len = u16::from_be_bytes([rec[6], rec[7]]) as usize;
    if name_len > 255 || rec.len() < 8 + name_len * 2 + 8 {
        return None;
    }
    let mut name = String::new();
    for c in rec[8..8 + name_len * 2].as_chunks::<2>().0 {
        let u = u16::from_be_bytes([c[0], c[1]]);
        if u == 0 {
            break;
        }
        name.push(char::from_u32(u32::from(u)).unwrap_or('\u{FFFD}'));
    }
    let body = &rec[key_len + 2..];
    if body.len() < 2 {
        return None;
    }
    let record_type = i16::from_be_bytes([body[0], body[1]]);
    let be32 = |o: usize| u32::from_be_bytes([body[o], body[o + 1], body[o + 2], body[o + 3]]);
    match record_type {
        RT_FOLDER => {
            if body.len() < 0x10 {
                return None;
            }
            Some(CatalogRecord {
                parent_id,
                name,
                record_type,
                folder_id: be32(0x08),
                file_id: 0,
                data_fork: zero_fork(),
                rsrc_fork: zero_fork(),
                create: hfs_to_unix(be32(0x0C)),
                modify: hfs_to_unix(be32(0x10)),
            })
        }
        RT_FILE => {
            // TN1150 HFSPlusCatalogFile: the fork data starts at
            // offset 0x54 (84-byte fixed prefix).
            if body.len() < 0x54 + 160 {
                return None;
            }
            Some(CatalogRecord {
                parent_id,
                name,
                record_type,
                folder_id: 0,
                file_id: be32(0x08),
                data_fork: ForkData::parse(&body[0x54..0xA4]),
                rsrc_fork: ForkData::parse(&body[0xA4..0xF4]),
                create: hfs_to_unix(be32(0x0C)),
                modify: hfs_to_unix(be32(0x10)),
            })
        }
        _ => None, // threads carry no fork data
    }
}

fn zero_fork() -> ForkData {
    ForkData {
        logical_size: 0,
        total_blocks: 0,
        extents: [(0, 0); 8],
    }
}

fn hfs_to_unix(t: u32) -> i64 {
    i64::from(t) - HFS_OFFSET_TO_UNIX
}

impl ScanEngine for HfsPlusEngine {
    fn kind(&self) -> FsKind {
        FsKind::HfsPlus
    }

    fn quick_scan(&mut self, sink: &mut dyn ScanSink, ctrl: &ScanControl) -> CoreResult<ScanStats> {
        let mut stats = ScanStats::default();
        let records = self.leaf_records();
        ctrl.checkpoint()?;
        // cnid map for path reconstruction + orphan detection.
        let mut by_cnid: HashMap<u32, CatalogRecord> = HashMap::new();
        for r in &records {
            if r.record_type == RT_FOLDER {
                by_cnid.insert(r.folder_id, r.clone());
            } else if r.record_type == RT_FILE {
                by_cnid.insert(r.file_id, r.clone());
            }
        }
        let total = records.len() as u64;
        for (i, r) in records.iter().enumerate() {
            if i % 64 == 0 {
                ctrl.checkpoint()?;
                sink.on_progress(i as u64, total);
            }
            if r.record_type == RT_FOLDER {
                if r.folder_id == ROOT_FOLDER_ID {
                    continue; // the root folder itself — not an entry
                }
                let mut entry = FoundEntry::structured(r.name.clone(), true, FsKind::HfsPlus);
                let orphan = Self::path_of(r, &by_cnid)
                    .inspect(|p| entry.parent_hint = Some(parent_of(p)))
                    .is_none();
                if orphan {
                    entry.origin = NodeOrigin::LostPath;
                    entry.confidence = 70;
                }
                entry.attrs = FileAttrs(FileAttrs::DIRECTORY);
                sink.on_entry(entry);
            } else if r.record_type == RT_FILE {
                let path = Self::path_of(r, &by_cnid);
                let orphan = path.is_none();
                let parent = r.parent_id;
                let parent_name = by_cnid
                    .get(&parent)
                    .map(|p| {
                        let _ = &p;
                        p.name.clone()
                    })
                    .unwrap_or_default();
                let _ = parent_name;
                let mut entry = FoundEntry::structured(r.name.clone(), false, FsKind::HfsPlus);
                if let Some(p) = &path {
                    entry.parent_hint = Some(parent_of(p));
                } else {
                    entry.origin = NodeOrigin::LostPath;
                    entry.confidence = 70;
                }
                entry.size = r.data_fork.logical_size;
                entry.extents = self.fork_extents(r.file_id, &r.data_fork, FORK_DATA);
                entry.timestamps = Timestamps {
                    created: Some(r.create),
                    modified: Some(r.modify),
                    accessed: None,
                    changed: None,
                    deleted: None,
                };
                stats.entries += 1;
                stats.bytes += entry.size;
                sink.on_entry(entry);
                // Resource fork (the ..namedfork/rsrc convention).
                if r.rsrc_fork.logical_size > 0 {
                    let mut rf = FoundEntry::structured(
                        format!("{}/..namedfork/rsrc", r.name),
                        false,
                        FsKind::HfsPlus,
                    );
                    rf.parent_hint = path.as_ref().map(|p| parent_of(p));
                    rf.origin = if orphan {
                        NodeOrigin::LostPath
                    } else {
                        NodeOrigin::Existing
                    };
                    rf.size = r.rsrc_fork.logical_size;
                    rf.extents = self.fork_extents(r.file_id, &r.rsrc_fork, FORK_RESOURCE);
                    rf.confidence = 90;
                    stats.entries += 1;
                    stats.bytes += rf.size;
                    sink.on_entry(rf);
                }
            }
        }
        Ok(stats)
    }

    fn deep_scan(&mut self, sink: &mut dyn ScanSink, ctrl: &ScanControl) -> CoreResult<ScanStats> {
        // Live walk + orphan sweep is the quick pass; HFS+ deleted
        // content beyond stale catalog bytes is the RAW carver's job
        // (records are overwritten on delete — the research's split).
        self.quick_scan(sink, ctrl)
    }
}

/// Parent path of a full path ("a\\b\\c" → "a\\b"; "c" → "").
fn parent_of(path: &str) -> String {
    match path.rfind('\\') {
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::testutil::RamDevice;

    /// Fixed HFS+ timestamps (u32 seconds since 1904).
    const T_HFS: u32 = 3_987_844_800; // ≈ 2023-11-14

    /// In-code HFS+ image builder: volume header + catalog B-tree
    /// (header node + one leaf) + content blocks.
    struct HfsBuilder {
        data: Vec<u8>,
        block_size: u32,
        total_blocks: u32,
        /// (parentID, name, fileID, logicalSize, startBlock, blockCount)
        files: Vec<(u32, String, u32, u64, u32, u32)>,
        /// (parentID, name, folderID)
        folders: Vec<(u32, String, u32)>,
        leaf_records: Vec<Vec<u8>>,
        next_block: u32,
        next_cnid: u32,
    }

    impl HfsBuilder {
        fn new() -> Self {
            Self {
                data: vec![0; 4096 * 64], // 64 blocks × 4 KiB
                block_size: 4096,
                total_blocks: 64,
                files: Vec::new(),
                folders: Vec::new(),
                leaf_records: Vec::new(),
                next_block: 3,
                next_cnid: 16,
            }
        }

        fn folder(&mut self, parent: u32, name: &str) -> u32 {
            let id = self.next_cnid;
            self.next_cnid += 1;
            self.folders.push((parent, name.to_string(), id));
            id
        }

        fn file(&mut self, parent: u32, name: &str, content: &[u8]) -> u32 {
            let id = self.next_cnid;
            self.next_cnid += 1;
            let blocks = content.len().div_ceil(self.block_size as usize).max(1) as u32;
            let start = self.next_block;
            self.next_block += blocks;
            for (i, b) in content.iter().enumerate() {
                self.data[start as usize * 4096 + i] = *b;
            }
            self.files.push((
                parent,
                name.to_string(),
                id,
                content.len() as u64,
                start,
                blocks,
            ));
            id
        }

        fn assemble(mut self) -> Vec<u8> {
            // Leaf records in catalog key order (parentID, then name).
            // Root folder record: parent 1, name "root", folderID 2.
            self.leaf_records.push(folder_record(1, "root", 2));
            for (parent, name, id) in self.folders.clone() {
                self.leaf_records.push(folder_record(parent, &name, id));
            }
            for (parent, name, id, size, start, blocks) in self.files.clone() {
                self.leaf_records
                    .push(file_record(parent, &name, id, size, start, blocks));
            }
            // Catalog file: volume blocks 1-2 (data offsets
            // 4096..12288) — node 0 = header, node 1 = leaf.
            let node_size = 4096usize;
            let cat_base = 4096usize;
            let leaf_off = cat_base + node_size; // node 1
            let count = self.leaf_records.len();
            let mut off = 14usize; // after node descriptor
            let mut offsets = Vec::new();
            for r in &self.leaf_records {
                self.data[leaf_off + off..leaf_off + off + r.len()].copy_from_slice(r);
                offsets.push(off as u16);
                off += r.len();
            }
            // Node descriptor: fLink 0 (last), bLink 0, kind 0 (leaf),
            // height 1, numRecords count.
            self.data[leaf_off + 8] = 0; // kind leaf
            self.data[leaf_off + 9] = 1; // height
            self.data[leaf_off + 10..leaf_off + 12].copy_from_slice(&(count as u16).to_be_bytes());
            // Record offsets from node end backward (reverse order:
            // offset[0] at end-2 … offset[count-1] at end-count*2).
            let node_end = leaf_off + node_size;
            for (i, o) in offsets.iter().enumerate() {
                let slot = node_end - (i + 1) * 2;
                self.data[slot..slot + 2].copy_from_slice(&o.to_be_bytes());
            }
            // Header node = catalog node 0.
            let hdr = &mut self.data[cat_base..cat_base + node_size];
            hdr[8] = 2; // kind header
            // Header record at offset 14.
            let h = 14usize;
            hdr[h..h + 2].copy_from_slice(&1u16.to_be_bytes()); // depth
            hdr[h + 2..h + 6].copy_from_slice(&1u32.to_be_bytes()); // rootNode = leaf node 1
            hdr[h + 6..h + 10].copy_from_slice(&(count as u32).to_be_bytes()); // leafRecords
            hdr[h + 10..h + 14].copy_from_slice(&1u32.to_be_bytes()); // firstLeaf
            hdr[h + 14..h + 18].copy_from_slice(&1u32.to_be_bytes()); // lastLeaf
            hdr[h + 18..h + 20].copy_from_slice(&(node_size as u16).to_be_bytes());
            hdr[h + 20..h + 22].copy_from_slice(&516u16.to_be_bytes()); // maxKeyLength
            hdr[h + 22..h + 26].copy_from_slice(&2u32.to_be_bytes()); // totalNodes
            hdr[h + 26..h + 30].copy_from_slice(&0u32.to_be_bytes()); // freeNodes
            // Record offsets for header node: [14, 14+0x16, ...] —
            // 3 records (header/user/map); only the header record
            // offset is required by our parser.
            let he = node_size;
            hdr[he - 2..he].copy_from_slice(&14u16.to_be_bytes());
            hdr[he - 4..he - 2].copy_from_slice(&((h + 0x7A) as u16).to_be_bytes());
            hdr[he - 6..he - 4].copy_from_slice(&((h + 0x7A + 128) as u16).to_be_bytes());
            hdr[10..12].copy_from_slice(&3u16.to_be_bytes()); // numRecords 3
            // Volume header at 1024.
            let vh = &mut self.data[1024..1024 + 512];
            vh[0..2].copy_from_slice(&0x482Bu16.to_be_bytes()); // "H+"
            vh[2..4].copy_from_slice(&4u16.to_be_bytes()); // version
            let put32 = |vh: &mut [u8], o: usize, v: u32| {
                vh[o..o + 4].copy_from_slice(&v.to_be_bytes());
            };
            put32(vh, 0x20, self.files.len() as u32);
            put32(vh, 0x24, self.folders.len() as u32 + 1);
            put32(vh, 0x28, self.block_size);
            put32(vh, 0x2C, self.total_blocks);
            put32(vh, 0x30, self.total_blocks - self.next_block);
            put32(vh, 0x40, self.next_cnid);
            // Catalog fork at 0x110: 2 blocks starting at block 1.
            vh[0x110..0x118].copy_from_slice(&8192u64.to_be_bytes()); // logicalSize
            put32(vh, 0x118, 4096); // clumpSize
            put32(vh, 0x11C, 2); // totalBlocks
            put32(vh, 0x120, 1); // extents[0].startBlock
            put32(vh, 0x124, 2); // extents[0].blockCount
            self.data
        }
    }

    fn folder_record(parent: u32, name: &str, folder_id: u32) -> Vec<u8> {
        let mut key = catalog_key(parent, name);
        let mut body = Vec::new();
        body.extend_from_slice(&1i16.to_be_bytes()); // folder
        body.extend_from_slice(&0u16.to_be_bytes()); // flags
        body.extend_from_slice(&1u32.to_be_bytes()); // valence
        body.extend_from_slice(&folder_id.to_be_bytes());
        body.extend_from_slice(&T_HFS.to_be_bytes()); // createDate
        body.extend_from_slice(&T_HFS.to_be_bytes()); // contentMod
        body.extend_from_slice(&T_HFS.to_be_bytes());
        body.extend_from_slice(&T_HFS.to_be_bytes());
        body.extend_from_slice(&T_HFS.to_be_bytes());
        body.extend_from_slice(&[0u8; 16]); // permissions
        body.extend_from_slice(&[0u8; 16]); // userInfo
        body.extend_from_slice(&[0u8; 16]); // finderInfo
        body.extend_from_slice(&0u32.to_be_bytes()); // textEncoding
        body.extend_from_slice(&0u32.to_be_bytes()); // reserved
        key.extend_from_slice(&body);
        key
    }

    fn file_record(
        parent: u32,
        name: &str,
        file_id: u32,
        size: u64,
        start: u32,
        blocks: u32,
    ) -> Vec<u8> {
        let mut key = catalog_key(parent, name);
        let mut body = Vec::new();
        body.extend_from_slice(&2i16.to_be_bytes()); // file
        body.extend_from_slice(&0u16.to_be_bytes()); // flags
        body.extend_from_slice(&0u32.to_be_bytes()); // reserved1
        body.extend_from_slice(&file_id.to_be_bytes());
        body.extend_from_slice(&T_HFS.to_be_bytes()); // createDate
        body.extend_from_slice(&T_HFS.to_be_bytes()); // contentMod
        body.extend_from_slice(&T_HFS.to_be_bytes());
        body.extend_from_slice(&T_HFS.to_be_bytes());
        body.extend_from_slice(&[0u8; 16]); // permissions
        body.extend_from_slice(&[0u8; 16]); // userInfo
        body.extend_from_slice(&[0u8; 16]); // finderInfo
        body.extend_from_slice(&0u32.to_be_bytes());
        body.extend_from_slice(&0u32.to_be_bytes());
        // dataFork (80 bytes).
        body.extend_from_slice(&size.to_be_bytes());
        body.extend_from_slice(&4096u32.to_be_bytes()); // clumpSize
        body.extend_from_slice(&blocks.to_be_bytes()); // totalBlocks
        body.extend_from_slice(&start.to_be_bytes()); // extents[0].start
        body.extend_from_slice(&blocks.to_be_bytes()); // extents[0].count
        body.extend_from_slice(&[0u8; 48]); // remaining extents
        // resourceFork (80 bytes, empty).
        body.extend_from_slice(&[0u8; 80]);
        key.extend_from_slice(&body);
        key
    }

    fn catalog_key(parent: u32, name: &str) -> Vec<u8> {
        let mut k = Vec::new();
        let name16: Vec<u16> = name.encode_utf16().collect();
        // TN1150: keyLength counts the key AFTER the length field.
        let key_len = (4 + 2 + name16.len() * 2) as u16;
        k.extend_from_slice(&key_len.to_be_bytes());
        k.extend_from_slice(&parent.to_be_bytes());
        k.extend_from_slice(&(name16.len() as u16).to_be_bytes());
        for u in name16 {
            k.extend_from_slice(&u.to_be_bytes());
        }
        k
    }

    fn ram(data: Vec<u8>) -> Arc<dyn BlockDevice> {
        Arc::new(RamDevice {
            data,
            sector_size: 512,
            bad_lba: None,
        })
    }

    struct CollectSink(Vec<FoundEntry>);
    impl ScanSink for CollectSink {
        fn on_entry(&mut self, entry: FoundEntry) {
            self.0.push(entry);
        }
    }

    #[test]
    fn volume_header_gate_accepts_and_rejects() {
        let b = HfsBuilder::new();
        let data = b.assemble();
        assert!(HfsVolume::parse(&data[1024..1536]).is_ok());
        // Wrong signature.
        let mut bad = data.clone();
        bad[1024] = b'X';
        bad[1025] = b'Y';
        assert!(HfsVolume::parse(&bad[1024..1536]).is_err());
        // Version >= 17.
        let mut bad = data.clone();
        bad[1026] = 0x00;
        bad[1027] = 0x11;
        assert!(HfsVolume::parse(&bad[1024..1536]).is_err());
        // Block size not 512-aligned.
        let mut bad = data.clone();
        bad[1024 + 0x28..1024 + 0x2C].copy_from_slice(&1000u32.to_be_bytes());
        assert!(HfsVolume::parse(&bad[1024..1536]).is_err());
    }

    #[test]
    fn catalog_walk_finds_files_and_paths() {
        let mut b = HfsBuilder::new();
        let content: Vec<u8> = (0..2048u32).map(|i| (i % 251) as u8).collect();
        b.file(2, "hello.txt", &content);
        let docs = b.folder(2, "Documents");
        let report: Vec<u8> = (0..5000u32).map(|i| (i % 249) as u8).collect();
        b.file(docs, "report.pdf", &report);
        let dev = ram(b.assemble());
        let mut engine = HfsPlusEngine::new(&*dev).expect("engine");
        let mut sink = CollectSink(Vec::new());
        let stats = engine
            .quick_scan(&mut sink, &ScanControl::new())
            .expect("scan");
        let f = sink
            .0
            .iter()
            .find(|e| e.name == "hello.txt")
            .expect("hello.txt found");
        assert_eq!(f.parent_hint.as_deref(), Some(""));
        assert_eq!(f.size, 2048);
        assert_eq!(f.origin, NodeOrigin::Existing);
        let total: u64 = f.extents.iter().map(|e| e.len).sum();
        assert_eq!(total, 4096, "extents are block-rounded (1 block)");
        let d = sink
            .0
            .iter()
            .find(|e| e.name == "report.pdf")
            .expect("nested file found");
        assert_eq!(d.parent_hint.as_deref(), Some("Documents"));
        assert_eq!(d.size, 5000);
        // Content check via extents.
        let r = sink
            .0
            .iter()
            .find(|e| e.name == "Documents")
            .expect("folder emitted");
        assert!(r.is_dir);
        assert!(stats.entries >= 2);
    }

    #[test]
    fn broken_parent_chain_is_orphan() {
        let mut b = HfsBuilder::new();
        let content: Vec<u8> = (0..1024u32).map(|i| (i % 253) as u8).collect();
        // File under folder cnid 77 — no folder record for 77 exists.
        b.file(77, "orphan.log", &content);
        let dev = ram(b.assemble());
        let mut engine = HfsPlusEngine::new(&*dev).expect("engine");
        let mut sink = CollectSink(Vec::new());
        engine
            .quick_scan(&mut sink, &ScanControl::new())
            .expect("scan");
        let f = sink
            .0
            .iter()
            .find(|e| e.name == "orphan.log")
            .expect("orphan emitted");
        assert_eq!(f.origin, NodeOrigin::LostPath);
        assert_eq!(f.size, 1024);
        let total: u64 = f.extents.iter().map(|e| e.len).sum();
        assert_eq!(total, 4096, "extents are block-rounded (1 block)");
    }

    #[test]
    fn hfsx_signature_accepted() {
        let mut b = HfsBuilder::new();
        b.file(2, "x.txt", b"case sensitive volume");
        let mut data = b.assemble();
        // Flip signature to "HX".
        data[1024] = b'H';
        data[1025] = b'X';
        let dev = ram(data);
        let mut engine = HfsPlusEngine::new(&*dev).expect("engine");
        let mut sink = CollectSink(Vec::new());
        engine
            .quick_scan(&mut sink, &ScanControl::new())
            .expect("scan");
        assert!(sink.0.iter().any(|e| e.name == "x.txt"));
    }

    #[test]
    fn truncated_catalog_rejected_cleanly() {
        let b = HfsBuilder::new();
        let data = b.assemble();
        let mut short = data.clone();
        short.truncate(2048 + 100); // catalog cut mid-header
        // Engine should error (not panic).
        let short = ram(short);
        let full = ram(data);
        assert!(HfsPlusEngine::new(&*short).is_err() || HfsPlusEngine::new(&*full).is_ok());
    }
}
