//! The ext2/ext3/ext4 engine — superblock + GDT + inode-table walk.
//!
//! Algorithm (research: `artifacts/SearchDLL/ext4-engine--deep-dive.md`
//! and `ext4-hfs-engine-algorithms-v2.md` — the FUN_180004710
//! validation set and FUN_1800047c0 geometry init, byte-exact):
//!
//! 1. Superblock at byte 1024: magic 0xEF53 (+0x38), the vendor's
//!    field gates (inodes/blocks/per-group counts non-zero, the two
//!    first_data_block cross-checks), `block_size = 1024 << log`,
//!    64-bit block count (hi at +0x150), `desc_size` at +0xFE when
//!    the 64BIT feature is set (32- or 64-byte GDT entries).
//! 2. Group descriptor table → per-group inode-table blocks.
//! 3. Directory walk from inode 2 (linear dirent arrays inside each
//!    directory block — htree (INDEX) directories keep the same
//!    linear layout in their leaf blocks, so the block walk covers
//!    both). Deleted-name recovery follows the two real kernel
//!    deletion shapes (fs/ext4/namei.c `ext4_generic_delete_entry`):
//!    - merged: the predecessor's rec_len absorbs the deleted entry —
//!      the full dirent (inode, name) survives in that slack;
//!    - zeroed: the entry's inode is cleared but rec_len + name stay
//!      walkable — the name survives, the inode link does not.
//! 4. Orphan inode sweep across every group (the vendor's inode-table
//!    walk): inodes with `dtime != 0` (or `links == 0`) that no
//!    directory names → `LostPath` entries, content still resolved
//!    from extents/indirects.
//! 5. Data resolution: ext4 extents trees (`i_flags & 0x80000`,
//!    header magic 0xF30A, bounded depth) or ext2/3 indirect blocks
//!    (direct 12 + single/double/triple, cycle-guarded).
//!
//! Version discrimination: extents feature (incompat 0x40) → ext4;
//! journal feature (compat 0x4) → ext3; else ext2.
//!
//! Journal-walk reanimation (the vendor's TraverseJournalDir path)
//! is queued as engine-scale M3 work in `docs/TODO.md`.

use std::collections::HashSet;
use std::sync::Arc;

use crate::blockdev::{BlockDevice, BlockRead};
use crate::error::{CoreError, CoreResult};
use crate::fs::{ScanControl, ScanEngine, ScanSink, ScanStats};
use crate::types::{Extent, FileAttrs, FoundEntry, FsKind, NodeOrigin, Timestamps};

/// ext superblock magic.
const EXT_MAGIC: u16 = 0xEF53;
/// `i_flags`: extent tree present in `i_block`.
const F_EXTENTS: u32 = 0x0008_0000;
/// `s_feature_compat`: journal present.
const FC_HAS_JOURNAL: u32 = 0x0000_0004;
/// `s_feature_incompat`: 64-bit GDT/block numbers.
const FI_64BIT: u32 = 0x0000_0080;
/// `s_feature_incompat`: extents supported.
const FI_EXTENTS: u32 = 0x0000_0040;
/// ext4 extent-header magic.
const EXT_MAGIC_EXTENT: u16 = 0xF30A;
/// Reserved inode numbers (< first non-reserved).
const FIRST_NON_RESERVED_INO: u32 = 11;
/// Root directory inode.
const ROOT_INO: u32 = 2;
/// Cap on a directory's total blocks (deep-walk guard).
const MAX_DIR_BYTES: u64 = 16 * 1024 * 1024;
/// Directory recursion depth guard.
const MAX_DIR_DEPTH: usize = 48;
/// Extent-tree depth guard (spec max is 5).
const MAX_EXTENT_DEPTH: u8 = 5;

/// The parsed + validated ext superblock.
#[derive(Debug, Clone)]
pub struct ExtSuper {
    pub inodes_count: u32,
    pub blocks_count: u64,
    pub first_data_block: u32,
    pub block_size: u32,
    pub blocks_per_group: u32,
    pub inodes_per_group: u32,
    pub inode_size: u16,
    pub feature_compat: u32,
    pub feature_incompat: u32,
    pub volume_name: String,
    pub desc_size: u16,
}

impl ExtSuper {
    /// Parse from raw superblock bytes (any length ≥ 264).
    ///
    /// # Errors
    /// [`CoreError::Corrupt`] on any vendor-validation failure.
    pub fn parse(sb: &[u8]) -> CoreResult<Self> {
        if sb.len() < 264 {
            return Err(CoreError::Corrupt {
                structure: "ext superblock",
                detail: "short read".into(),
            });
        }
        let u16at = |o: usize| u16::from_le_bytes([sb[o], sb[o + 1]]);
        let u32at = |o: usize| u32::from_le_bytes([sb[o], sb[o + 1], sb[o + 2], sb[o + 3]]);
        let magic = u16at(0x38);
        if magic != EXT_MAGIC {
            return Err(CoreError::Corrupt {
                structure: "ext superblock",
                detail: format!("magic {magic:#06x} != 0xEF53"),
            });
        }
        let inodes_count = u32at(0x00);
        let blocks_lo = u32at(0x04);
        let first_data_block = u32at(0x14);
        let log_block_size = u32at(0x18);
        let blocks_per_group = u32at(0x20);
        let inodes_per_group = u32at(0x28);
        // The vendor's non-zero gates (FUN_180004710).
        if inodes_count == 0 || blocks_lo == 0 || blocks_per_group == 0 || inodes_per_group == 0 {
            return Err(CoreError::Corrupt {
                structure: "ext superblock",
                detail: "zero geometry field".into(),
            });
        }
        // The two kernel cross-checks: 1 KiB blocks have
        // first_data_block 1 (block 0 is the boot area), larger blocks
        // have 0.
        if (first_data_block == 0 && log_block_size == 0)
            || (first_data_block == 1 && log_block_size != 0)
        {
            return Err(CoreError::Corrupt {
                structure: "ext superblock",
                detail: format!(
                    "first_data_block {first_data_block} vs log_block_size {log_block_size}"
                ),
            });
        }
        if log_block_size > 6 {
            return Err(CoreError::Corrupt {
                structure: "ext superblock",
                detail: format!("log_block_size {log_block_size} > 6"),
            });
        }
        let block_size = 1024u32 << log_block_size;
        let blocks_hi = if sb.len() >= 0x154 { u32at(0x150) } else { 0 };
        let blocks_count = (u64::from(blocks_hi) << 32) | u64::from(blocks_lo);
        let groups = blocks_count.div_ceil(u64::from(blocks_per_group));
        if groups == 0 || groups > blocks_count {
            return Err(CoreError::Corrupt {
                structure: "ext superblock",
                detail: format!("group count {groups} vs blocks {blocks_count}"),
            });
        }
        // Rev-1 field block (kernel `ext4_super_block`): s_rev_level
        // 0x4C, s_def_resuid/resgid 0x50/0x52, s_first_ino 0x54,
        // s_inode_size 0x58, s_block_group_nr 0x5A, feature flags
        // 0x5C/0x60/0x64. Byte-verified against real mkfs.ext2 1.47.4
        // output — the CI fixture gate caught the old offsets, which
        // were shifted +4 (missing the resuid/resgid pair) and made
        // every inode read land mid-slot on real images.
        let rev_level = u32at(0x4C);
        let inode_size = if rev_level >= 1 { u16at(0x58) } else { 128 };
        if !(128..=1024).contains(&inode_size) || inode_size % 4 != 0 {
            return Err(CoreError::Corrupt {
                structure: "ext superblock",
                detail: format!("inode size {inode_size}"),
            });
        }
        let feature_compat = u32at(0x5C);
        let feature_incompat = u32at(0x60);
        let desc_size = if feature_incompat & FI_64BIT != 0 && sb.len() >= 0x100 {
            u16at(0xFE).max(32)
        } else {
            32
        };
        let volume_name = sb[0x78..0x88]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as char)
            .collect();
        Ok(Self {
            inodes_count,
            blocks_count,
            first_data_block,
            block_size,
            blocks_per_group,
            inodes_per_group,
            inode_size,
            feature_compat,
            feature_incompat,
            volume_name,
            desc_size,
        })
    }

    /// Byte offset of an absolute block number. Saturating: fuzzed
    /// GDT/inode block numbers can overflow the scale — a saturated
    /// offset simply fails at the device boundary instead of panicking
    /// under overflow checks (the fuzz suite's discipline).
    #[must_use]
    pub fn block_offset(&self, block: u64) -> u64 {
        block.saturating_mul(u64::from(self.block_size))
    }

    /// The refined family member.
    #[must_use]
    pub fn kind(&self) -> FsKind {
        if self.feature_incompat & FI_EXTENTS != 0 {
            FsKind::Ext4
        } else if self.feature_compat & FC_HAS_JOURNAL != 0 {
            FsKind::Ext3
        } else {
            FsKind::Ext2
        }
    }
}

/// One group-descriptor entry (what we use of it).
#[derive(Debug, Clone, Copy)]
struct GroupDesc {
    inode_table: u64,
}

/// A parsed inode.
#[derive(Debug, Clone)]
struct ExtInode {
    mode: u16,
    size: u64,
    links: u16,
    flags: u32,
    dtime: i64,
    atime: i64,
    ctime: i64,
    mtime: i64,
    crtime: i64,
    block: [u8; 60],
}

impl ExtInode {
    fn parse(raw: &[u8]) -> Self {
        let u16at = |o: usize| u16::from_le_bytes([raw[o], raw[o + 1]]);
        let u32at = |o: usize| u32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]]);
        let i32at = |o: usize| i32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]]);
        let mode = u16at(0x00);
        let size_lo = u64::from(u32at(0x04));
        let size_hi = if mode & 0xF000 == 0x8000 {
            u64::from(u32at(0x6C))
        } else {
            0
        };
        let mut block = [0u8; 60];
        block.copy_from_slice(&raw[0x28..0x64]);
        let crtime = if raw.len() >= 0x90 {
            i32at(0x8C) as i64
        } else {
            0
        };
        Self {
            mode,
            size: size_lo | (size_hi << 32),
            links: u16at(0x1A),
            flags: u32at(0x20),
            dtime: i32at(0x14) as i64,
            atime: i32at(0x08) as i64,
            ctime: i32at(0x0C) as i64,
            mtime: i32at(0x10) as i64,
            crtime,
            block,
        }
    }

    fn is_dir(&self) -> bool {
        self.mode & 0xF000 == 0x4000
    }

    fn is_regular(&self) -> bool {
        self.mode & 0xF000 == 0x8000
    }

    /// Fast symlink: content inline in i_block.
    fn is_symlink_fast(&self) -> bool {
        self.mode & 0xF000 == 0xA000 && self.size <= 60
    }

    fn is_deleted(&self) -> bool {
        self.dtime != 0 || self.links == 0
    }
}

/// The engine.
pub struct ExtEngine {
    dev: Arc<dyn BlockDevice>,
    sb: ExtSuper,
    gdt: Vec<GroupDesc>,
    kind: FsKind,
}

impl ExtEngine {
    /// Open + parse + load the GDT.
    ///
    /// # Errors
    /// [`CoreError::Corrupt`] when the volume isn't parseable ext.
    pub fn new(dev: Arc<dyn BlockDevice>) -> CoreResult<Self> {
        let mut sbbuf = [0u8; 1024];
        dev.read_bytes(1024, &mut sbbuf)?;
        let sb = ExtSuper::parse(&sbbuf)?;
        let dev_blocks = dev.capacity_bytes() / u64::from(sb.block_size);
        if sb.blocks_count > dev_blocks + 64 {
            // Superblock claims far more blocks than the device holds
            // (a backup-superblock find on a shifted volume is legal —
            // tolerate small slack, reject garbage).
            return Err(CoreError::Corrupt {
                structure: "ext superblock",
                detail: format!(
                    "blocks_count {} exceeds device {dev_blocks} blocks",
                    sb.blocks_count
                ),
            });
        }
        let groups = sb.blocks_count.div_ceil(u64::from(sb.blocks_per_group)) as usize;
        // GDT: right after the superblock block (byte 2048 for 1 KiB
        // blocks; block 1 for larger). Bounded read.
        let gdt_start = if sb.block_size == 1024 {
            2 * u64::from(sb.block_size)
        } else {
            u64::from(sb.block_size)
        };
        let gdt_bytes = groups * sb.desc_size as usize;
        if gdt_bytes > 16 * 1024 * 1024 {
            return Err(CoreError::Corrupt {
                structure: "ext GDT",
                detail: format!("GDT size {gdt_bytes} exceeds bound"),
            });
        }
        let mut raw = vec![0u8; gdt_bytes];
        dev.read_bytes(gdt_start, &mut raw)?;
        let gdt: Vec<GroupDesc> = raw
            .chunks(sb.desc_size as usize)
            .map(|g| {
                let lo = u32::from_le_bytes([g[8], g[9], g[10], g[11]]);
                let hi = if g.len() >= 0x2C {
                    u32::from_le_bytes([g[0x28], g[0x29], g[0x2A], g[0x2B]])
                } else {
                    0
                };
                u64::from(lo) | (u64::from(hi) << 32)
            })
            .filter(|table| *table != 0)
            .map(|inode_table| GroupDesc { inode_table })
            .collect();
        if gdt.is_empty() {
            return Err(CoreError::Corrupt {
                structure: "ext GDT",
                detail: "no valid group descriptors".into(),
            });
        }
        let kind = sb.kind();
        Ok(Self { dev, sb, gdt, kind })
    }

    /// Read one inode by number (1-based).
    fn read_inode(&self, ino: u32) -> CoreResult<Option<ExtInode>> {
        if ino == 0 || ino > self.sb.inodes_count {
            return Ok(None);
        }
        let group = ((ino - 1) / self.sb.inodes_per_group) as usize;
        let idx = (ino - 1) % self.sb.inodes_per_group;
        let Some(g) = self.gdt.get(group) else {
            return Ok(None);
        };
        let off = self
            .sb
            .block_offset(g.inode_table)
            .saturating_add(u64::from(idx) * u64::from(self.sb.inode_size));
        let mut raw = vec![0u8; self.sb.inode_size as usize];
        self.dev.read_bytes(off, &mut raw)?;
        // Zero mode = never-used slot (inodes keep mode through
        // unlink — only never-used or wiped slots read as 0).
        if raw[0] == 0 && raw[1] == 0 {
            return Ok(None);
        }
        Ok(Some(ExtInode::parse(&raw)))
    }

    /// Resolve an inode's data extents to volume-relative runs.
    ///
    /// ext4: extent tree (bounded depth). ext2/3: indirect blocks.
    fn resolve_extents(&self, ino: &ExtInode) -> Vec<Extent> {
        if ino.is_symlink_fast() || ino.size == 0 || !ino.is_regular() {
            return Vec::new();
        }
        let bs = u64::from(self.sb.block_size);
        let max_blocks = ino.size.div_ceil(bs).max(1);
        let mut out = Vec::new();
        if ino.flags & F_EXTENTS != 0 {
            let mut visited = HashSet::new();
            self.walk_extent_node(&ino.block, 0, max_blocks, &mut visited, &mut out);
            out.sort_by_key(|e| e.offset);
        } else {
            let mut visited = HashSet::new();
            self.walk_indirect(&ino.block, 0, max_blocks, &mut visited, &mut out);
        }
        out
    }

    /// Recurse one extent-tree node: `node` is the 60-byte i_block
    /// (root) or a full metadata block (children).
    fn walk_extent_node(
        &self,
        node: &[u8],
        depth_budget: u8,
        max_blocks: u64,
        visited: &mut HashSet<u64>,
        out: &mut Vec<Extent>,
    ) {
        if node.len() < 24 || depth_budget > MAX_EXTENT_DEPTH {
            return;
        }
        let magic = u16::from_le_bytes([node[0], node[1]]);
        if magic != EXT_MAGIC_EXTENT {
            return;
        }
        let entries = u16::from_le_bytes([node[2], node[3]]) as usize;
        let depth = u16::from_le_bytes([node[6], node[7]]) as u8;
        let cap = (node.len() - 12) / 12;
        let bs = u64::from(self.sb.block_size);
        for i in 0..entries.min(cap) {
            let e = &node[12 + i * 12..12 + i * 12 + 12];
            if depth == 0 {
                let len = u64::from(u16::from_le_bytes([e[4], e[5]]) & 0x7FFF).min(max_blocks);
                let phys = u64::from(u32::from_le_bytes([e[8], e[9], e[10], e[11]]))
                    | (u64::from(u16::from_le_bytes([e[6], e[7]])) << 32);
                if len == 0 || phys == 0 || phys >= self.sb.blocks_count {
                    continue;
                }
                out.push(Extent::new(phys.saturating_mul(bs), len.saturating_mul(bs)));
            } else {
                let child = u64::from(u32::from_le_bytes([e[4], e[5], e[6], e[7]]))
                    | (u64::from(u16::from_le_bytes([e[8], e[9]])) << 32);
                if child == 0 || child >= self.sb.blocks_count || !visited.insert(child) {
                    continue;
                }
                let mut raw = vec![0u8; bs as usize];
                if self
                    .dev
                    .read_bytes(self.sb.block_offset(child), &mut raw)
                    .is_err()
                {
                    continue;
                }
                self.walk_extent_node(&raw, depth_budget + 1, max_blocks, visited, out);
            }
        }
    }

    /// ext2/3 indirect walk: 12 direct + single/double/triple.
    fn walk_indirect(
        &self,
        block: &[u8; 60],
        _depth: u8,
        max_blocks: u64,
        visited: &mut HashSet<u64>,
        out: &mut Vec<Extent>,
    ) {
        let bs = u64::from(self.sb.block_size);
        let per_block = bs / 4;
        let ptr = |b: &[u8], i: usize| {
            u64::from(u32::from_le_bytes([
                b[i * 4],
                b[i * 4 + 1],
                b[i * 4 + 2],
                b[i * 4 + 3],
            ]))
        };
        let push = |out: &mut Vec<Extent>, p: u64| {
            let off = p.saturating_mul(bs);
            match out.last_mut() {
                Some(last) if last.end() == off => last.len += bs,
                _ => out.push(Extent::new(off, bs)),
            }
        };
        // Direct entries (block[0..12]).
        for i in 0..12 {
            let p = ptr(block, i);
            if p == 0 || p >= self.sb.blocks_count {
                continue;
            }
            if out.len() as u64 >= max_blocks {
                break;
            }
            push(out, p);
        }
        // Indirect roots (block[12]=single, [13]=double, [14]=triple).
        for root_slot in 12..15 {
            let root = ptr(block, root_slot);
            if root == 0 || root >= self.sb.blocks_count {
                continue;
            }
            let level = (root_slot - 11) as u8; // 1, 2, 3
            self.walk_indirect_level(root, level, per_block, max_blocks, visited, out, &push);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_indirect_level(
        &self,
        node_block: u64,
        level: u8,
        per_block: u64,
        max_blocks: u64,
        visited: &mut HashSet<u64>,
        out: &mut Vec<Extent>,
        push: &impl Fn(&mut Vec<Extent>, u64),
    ) {
        if level == 0 || out.len() as u64 >= max_blocks || !visited.insert(node_block) {
            return;
        }
        let bs = u64::from(self.sb.block_size);
        let mut raw = vec![0u8; bs as usize];
        if self
            .dev
            .read_bytes(self.sb.block_offset(node_block), &mut raw)
            .is_err()
        {
            return;
        }
        for i in 0..per_block {
            let p = u64::from(u32::from_le_bytes([
                raw[i as usize * 4],
                raw[i as usize * 4 + 1],
                raw[i as usize * 4 + 2],
                raw[i as usize * 4 + 3],
            ]));
            if p == 0 || p >= self.sb.blocks_count {
                continue;
            }
            if out.len() as u64 >= max_blocks {
                return;
            }
            if level == 1 {
                push(out, p);
            } else {
                self.walk_indirect_level(p, level - 1, per_block, max_blocks, visited, out, push);
            }
        }
    }

    /// Read a directory's content blocks (bounded).
    fn read_dir_data(&self, ino: &ExtInode) -> Vec<u8> {
        if !ino.is_dir() {
            return Vec::new();
        }
        let bs = u64::from(self.sb.block_size);
        let mut extents = Vec::new();
        let mut visited = HashSet::new();
        let max_blocks = (ino.size.min(MAX_DIR_BYTES)).div_ceil(bs).max(1);
        if ino.flags & F_EXTENTS != 0 {
            self.walk_extent_node(&ino.block, 0, max_blocks, &mut visited, &mut extents);
            extents.sort_by_key(|e| e.offset);
        } else {
            self.walk_indirect(&ino.block, 0, max_blocks, &mut visited, &mut extents);
        }
        let total: u64 = extents
            .iter()
            .map(|e| e.len)
            .sum::<u64>()
            .min(MAX_DIR_BYTES);
        if total == 0 {
            return Vec::new();
        }
        let mut buf = vec![0u8; total as usize];
        let mut pos = 0usize;
        for e in extents {
            if pos >= buf.len() {
                break;
            }
            let end = (pos + e.len as usize).min(buf.len());
            let _ = self.dev.read_bytes(e.offset, &mut buf[pos..end]);
            pos = end;
        }
        buf
    }

    /// Walk one directory block: live entries + the two deleted-entry
    /// shapes (merged-rec_len slack with intact dirent; zeroed-inode
    /// entry with walkable rec_len). Returns (inode, name, is_deleted)
    /// triples — inode 0 marks a name-only recovery.
    fn parse_dirents(data: &[u8], inodes_count: u32) -> Vec<(u32, String, bool)> {
        let mut out: Vec<(u32, String, bool)> = Vec::new();
        let mut off = 0usize;
        while off + 8 <= data.len() {
            let ino = u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]);
            let rec_len = u16::from_le_bytes([data[off + 4], data[off + 5]]) as usize;
            let name_len = data[off + 6] as usize;
            if rec_len < 8 || off + rec_len > data.len() || name_len + 8 > rec_len {
                break; // corrupt chain — stop at this block
            }
            let entry_end = off + rec_len;
            if ino != 0 && ino <= inodes_count && name_len > 0 {
                let name = String::from_utf8_lossy(&data[off + 8..off + 8 + name_len]).to_string();
                if !name.is_empty() && name != "." && name != ".." {
                    out.push((ino, name, false));
                }
                // Merged-deletion slack: [align4(name end) .. rec_len
                // end) can hold the deleted dirent whole.
                let slack_start = (off + 8 + name_len + 3) & !3;
                if slack_start + 8 <= entry_end {
                    Self::scan_slack(&data[slack_start..entry_end], inodes_count, &mut out);
                }
            } else if ino == 0 && name_len > 0 {
                // Zeroed-inode deletion (or never-used slot with stale
                // name): the name itself is recoverable.
                let name = String::from_utf8_lossy(&data[off + 8..off + 8 + name_len]).to_string();
                if is_plausible_name(&name) {
                    out.push((0, name, true));
                }
                let slack_start = (off + 8 + name_len + 3) & !3;
                if slack_start + 8 <= entry_end {
                    Self::scan_slack(&data[slack_start..entry_end], inodes_count, &mut out);
                }
            } else {
                // Garbage/free region: scan it whole for remnants.
                Self::scan_slack(&data[off..entry_end], inodes_count, &mut out);
            }
            off = entry_end;
        }
        out
    }

    /// 4-aligned scan of a slack region for deleted dirents — both
    /// kernel deletion shapes: intact dirent (merged-rec_len) and
    /// zeroed-inode remnant with a plausible name.
    fn scan_slack(slack: &[u8], inodes_count: u32, out: &mut Vec<(u32, String, bool)>) {
        let mut i = 0usize;
        while i + 8 <= slack.len() {
            let ino = u32::from_le_bytes([slack[i], slack[i + 1], slack[i + 2], slack[i + 3]]);
            let name_len = slack[i + 6] as usize;
            let has_room = name_len > 0 && name_len <= 255 && i + 8 + name_len <= slack.len();
            if has_room {
                if ino != 0 && ino <= inodes_count {
                    let name = String::from_utf8_lossy(&slack[i + 8..i + 8 + name_len]).to_string();
                    if is_plausible_name(&name) {
                        out.push((ino, name, true));
                        i += 8 + name_len;
                        continue;
                    }
                } else if ino == 0 && name_len >= 2 {
                    // Zeroed-inode remnant: name survives, link lost
                    // (min length 2 — single-char names are noise
                    // risk in binary slack).
                    let name = String::from_utf8_lossy(&slack[i + 8..i + 8 + name_len]).to_string();
                    if is_plausible_name(&name) {
                        out.push((0, name, true));
                        i += 8 + name_len;
                        continue;
                    }
                }
            }
            i += 4; // dirent alignment
        }
    }

    /// Recursive directory walk: emits entries + collects named inodes.
    #[allow(clippy::too_many_arguments)]
    fn walk_dir(
        &self,
        ino: u32,
        inode: &ExtInode,
        path: &str,
        sink: &mut dyn ScanSink,
        ctrl: &ScanControl,
        stats: &mut ScanStats,
        named: &mut HashSet<u32>,
        depth: usize,
    ) -> CoreResult<()> {
        if depth > MAX_DIR_DEPTH {
            return Ok(());
        }
        ctrl.checkpoint()?;
        // The cycle/explosion guard (the FAT fuzz target's 11-second
        // lesson): a directory inode reachable by two paths — or wired
        // into itself — re-walks once per PATH without this check
        // (2^depth walks). One inode, one walk; the orphan sweep's
        // `named` bookkeeping is unchanged (the inode is in the set
        // either way).
        if !named.insert(ino) {
            return Ok(());
        }
        let data = self.read_dir_data(inode);
        for (child_ino, name, deleted) in Self::parse_dirents(&data, self.sb.inodes_count) {
            let child_path = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}\\{name}")
            };
            if child_ino == 0 {
                // Name-only deletion: the inode link is gone — emit
                // the name (metadata-only; content may surface via the
                // orphan sweep as inode_N).
                let mut entry = FoundEntry::structured(name, false, self.kind);
                entry.parent_hint = Some(path.to_string());
                entry.origin = NodeOrigin::Deleted;
                entry.confidence = 40;
                sink.on_entry(entry);
                continue;
            }
            named.insert(child_ino);
            let Some(child) = self.read_inode(child_ino)? else {
                continue;
            };
            let is_deleted_entry = deleted || child.is_deleted();
            let extents = if child.is_dir() || child.is_symlink_fast() {
                Vec::new()
            } else {
                self.resolve_extents(&child)
            };
            let mut entry = FoundEntry::structured(name.clone(), child.is_dir(), self.kind);
            entry.parent_hint = Some(path.to_string());
            entry.origin = if is_deleted_entry {
                NodeOrigin::Deleted
            } else {
                NodeOrigin::Existing
            };
            entry.size = if child.is_dir() { 0 } else { child.size };
            entry.extents = extents;
            entry.confidence = if deleted { 70 } else { 100 };
            // Unix-seconds inode times → the millisecond wire
            // (×1000; ext4's nanosecond fields are a future
            // refinement — second precision rides as :000).
            entry.timestamps = Timestamps {
                modified: nonzero(child.mtime * 1000),
                accessed: nonzero(child.atime * 1000),
                changed: nonzero(child.ctime * 1000),
                created: nonzero(child.crtime * 1000),
                deleted: nonzero(child.dtime * 1000),
            };
            entry.attrs = FileAttrs(if child.is_dir() {
                FileAttrs::DIRECTORY
            } else {
                0
            });
            stats.entries += u64::from(!child.is_dir());
            stats.bytes += entry.size;
            sink.on_entry(entry);
            if child.is_dir() && !is_deleted_entry {
                self.walk_dir(
                    child_ino,
                    &child,
                    &child_path,
                    sink,
                    ctrl,
                    stats,
                    named,
                    depth + 1,
                )?;
            }
        }
        Ok(())
    }
}

/// unix-epoch helper (0 = unset in ext).
fn nonzero(t: i64) -> Option<i64> {
    (t > 0).then_some(t)
}

/// Filename plausibility gate (deleted-name validation).
fn is_plausible_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.len() <= 255
        && name
            .chars()
            .all(|c| c == ' ' || c == '.' || c == '-' || c == '_' || c.is_ascii_alphanumeric())
}

impl ScanEngine for ExtEngine {
    fn kind(&self) -> FsKind {
        self.kind
    }

    fn quick_scan(&mut self, sink: &mut dyn ScanSink, ctrl: &ScanControl) -> CoreResult<ScanStats> {
        let mut stats = ScanStats::default();
        // Pass A: the directory tree from inode 2.
        let mut named: HashSet<u32> = HashSet::new();
        if let Some(root) = self.read_inode(ROOT_INO)?.filter(ExtInode::is_dir) {
            self.walk_dir(ROOT_INO, &root, "", sink, ctrl, &mut stats, &mut named, 0)?;
        }
        // Pass B: orphan sweep — every inode table; deleted or
        // unnamed-but-contentful inodes the tree didn't name (the
        // vendor's inode-table walk).
        let groups = self.gdt.len() as u64;
        for (gi, g) in self.gdt.iter().enumerate() {
            ctrl.checkpoint()?;
            sink.on_progress(gi as u64, groups);
            let table_off = self.sb.block_offset(g.inode_table);
            let table_bytes = u64::from(self.sb.inodes_per_group) * u64::from(self.sb.inode_size);
            if table_bytes == 0 || table_bytes > 64 * 1024 * 1024 {
                continue; // hostile geometry — skip this group
            }
            let mut raw = vec![0u8; table_bytes as usize];
            if self.dev.read_bytes(table_off, &mut raw).is_err() {
                continue; // unreadable group — salvage continues
            }
            for idx in 0..self.sb.inodes_per_group {
                let ino = gi as u32 * self.sb.inodes_per_group + idx + 1;
                if ino < FIRST_NON_RESERVED_INO
                    || ino > self.sb.inodes_count
                    || named.contains(&ino)
                {
                    continue;
                }
                let off = idx as usize * self.sb.inode_size as usize;
                let end = off + self.sb.inode_size as usize;
                let inode = ExtInode::parse(&raw[off..end]);
                if !inode.is_regular() || inode.size == 0 {
                    continue; // nameless dirs/symlinks: not usefully recoverable
                }
                let extents = self.resolve_extents(&inode);
                let deleted = inode.is_deleted();
                let mut entry = FoundEntry::structured(format!("inode_{ino}"), false, self.kind);
                entry.origin = NodeOrigin::LostPath;
                entry.size = inode.size;
                entry.extents = extents;
                entry.confidence = if deleted { 65 } else { 70 };
                entry.timestamps = Timestamps {
                    modified: nonzero(inode.mtime * 1000),
                    accessed: nonzero(inode.atime * 1000),
                    changed: nonzero(inode.ctime * 1000),
                    created: nonzero(inode.crtime * 1000),
                    deleted: nonzero(inode.dtime * 1000),
                };
                stats.entries += 1;
                stats.bytes += entry.size;
                sink.on_entry(entry);
            }
        }
        Ok(stats)
    }

    fn deep_scan(&mut self, sink: &mut dyn ScanSink, ctrl: &ScanControl) -> CoreResult<ScanStats> {
        // The deleted reanimation IS the quick pass here (dtime +
        // slack names + orphans); the RAW carver sweep that follows in
        // the controller covers the unstructured remainder.
        self.quick_scan(sink, ctrl)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::testutil::RamDevice;

    /// In-code ext image builder (the fixture pattern shared with the
    /// NTFS/FAT engine tests — ground truth we control byte-exact).
    pub struct ExtBuilder {
        pub data: Vec<u8>,
        pub block_size: u32,
        pub inodes_per_group: u32,
        pub inode_size: u16,
        pub ext4: bool,
        next_free_block: u32,
        next_free_ino: u32,
    }

    /// Fixed test timestamp (2023-11-14T22:13:20Z).
    const T: u32 = 1_700_000_000;

    impl ExtBuilder {
        /// 1 MiB image, 1 KiB blocks, 32 inodes; ext4 (extents) or ext2.
        pub fn new(ext4: bool) -> Self {
            let bs = 1024u32;
            let blocks = 1024u32;
            let inodes_per_group = 32u32;
            let inode_size = 128u16;
            let mut data = vec![0u8; bs as usize * blocks as usize];
            // Superblock at 1024.
            let sb = &mut data[1024..2048];
            let put32 = |sb: &mut [u8], off: usize, v: u32| {
                sb[off..off + 4].copy_from_slice(&v.to_le_bytes());
            };
            let put16 = |sb: &mut [u8], off: usize, v: u16| {
                sb[off..off + 2].copy_from_slice(&v.to_le_bytes());
            };
            put32(sb, 0x00, inodes_per_group); // s_inodes_count
            put32(sb, 0x04, blocks); // s_blocks_count_lo
            put32(sb, 0x14, 1); // s_first_data_block (1 KiB blocks)
            put32(sb, 0x18, 0); // s_log_block_size
            put32(sb, 0x20, 8192); // s_blocks_per_group
            put32(sb, 0x28, inodes_per_group); // s_inodes_per_group
            put16(sb, 0x38, EXT_MAGIC); // s_magic
            put32(sb, 0x4C, 1); // s_rev_level (true offset; 0x50 is resuid)
            put16(sb, 0x58, inode_size); // s_inode_size (0x58; 0x5C is compat)
            let compat = if ext4 { FC_HAS_JOURNAL } else { 0 };
            let mut incompat = 0x2u32; // FILETYPE
            if ext4 {
                incompat |= FI_EXTENTS;
                put32(sb, 0xE0, 8); // s_journal_inum (0xE0; 0xD8 is inside journal_uuid)
            }
            put32(sb, 0x5C, compat);
            put32(sb, 0x60, incompat);
            // GDT at block 2: block bitmap @3, inode bitmap @4, inode
            // table @5.
            let gdt = &mut data[2048..2048 + 32];
            gdt[0..4].copy_from_slice(&3u32.to_le_bytes());
            gdt[4..8].copy_from_slice(&4u32.to_le_bytes());
            gdt[8..12].copy_from_slice(&5u32.to_le_bytes());
            // Block bitmap (block 3): blocks 0..=9 used.
            let bbm = &mut data[3 * bs as usize..4 * bs as usize];
            for b in 0..=9u32 {
                bbm[(b / 8) as usize] |= 1 << (b % 8);
            }
            // Inode bitmap (block 4): inodes 1..=10 used.
            let ibm = &mut data[4 * bs as usize..5 * bs as usize];
            for i in 1..=10u32 {
                ibm[(i / 8) as usize] |= 1 << (i % 8);
            }
            // Root dir inode 2; root dir content block = 9.
            write_inode(&mut data, 2, inode_size, |raw| {
                raw[0..2].copy_from_slice(&0o040_755u16.to_le_bytes()); // dir
                put_size(raw, bs as u64);
                raw[0x1A..0x1C].copy_from_slice(&3u16.to_le_bytes()); // links
                set_times(raw);
                if ext4 {
                    raw[0x20..0x24].copy_from_slice(&F_EXTENTS.to_le_bytes());
                    write_extent_header(raw, 1);
                    write_extent_entry(raw, 0, 0, 1, 9); // logical 0 → block 9
                } else {
                    raw[0x28..0x2C].copy_from_slice(&9u32.to_le_bytes());
                }
            });
            // Root dir block: "." + ".." entries.
            let dir = &mut data[9 * bs as usize..10 * bs as usize];
            dir[0..4].copy_from_slice(&2u32.to_le_bytes()); // . → inode 2
            dir[4..6].copy_from_slice(&12u16.to_le_bytes());
            dir[6] = 1;
            dir[7] = 2; // DT_DIR
            dir[8] = b'.';
            dir[12..16].copy_from_slice(&2u32.to_le_bytes()); // .. → inode 2
            dir[16..18].copy_from_slice(&12u16.to_le_bytes());
            dir[18] = 2;
            dir[19] = 2;
            dir[20] = b'.';
            dir[21] = b'.';
            Self {
                data,
                block_size: bs,
                inodes_per_group,
                inode_size,
                ext4,
                next_free_block: 10,
                next_free_ino: 11,
            }
        }

        /// Add a file at the root with content; returns its inode.
        pub fn add_file(&mut self, name: &str, content: &[u8]) -> u32 {
            let ino = self.next_free_ino;
            self.next_free_ino += 1;
            let bs = self.block_size;
            let nblocks = content.len().div_ceil(bs as usize).max(1) as u32;
            let first = self.next_free_block;
            self.next_free_block += nblocks;
            for b in 0..nblocks {
                let block = first + b;
                self.data[3 * bs as usize..4 * bs as usize][(block / 8) as usize] |=
                    1 << (block % 8);
                let s = b as usize * bs as usize;
                let e = (s + bs as usize).min(content.len());
                self.data[block as usize * bs as usize + s..block as usize * bs as usize + e]
                    .copy_from_slice(&content[s..e]);
            }
            write_inode(&mut self.data, ino, self.inode_size, |raw| {
                raw[0..2].copy_from_slice(&0o100_644u16.to_le_bytes()); // regular
                put_size(raw, content.len() as u64);
                raw[0x1A..0x1C].copy_from_slice(&1u16.to_le_bytes()); // links
                set_times(raw);
                if self.ext4 {
                    raw[0x20..0x24].copy_from_slice(&F_EXTENTS.to_le_bytes());
                    write_extent_header(raw, 1);
                    write_extent_entry(raw, 0, 0, nblocks, first);
                } else {
                    for b in 0..nblocks {
                        raw[0x28 + b as usize * 4..0x2C + b as usize * 4]
                            .copy_from_slice(&(first + b).to_le_bytes());
                    }
                }
            });
            // Inode bitmap bit.
            self.data[4 * bs as usize..5 * bs as usize][((ino - 1) / 8) as usize] |=
                1 << ((ino - 1) % 8);
            // Root dir entry appended after "." and ".." (offset 24).
            let dir = &mut self.data[9 * bs as usize..10 * bs as usize];
            let name_off = 24usize;
            let rec_len = (8 + name.len()).div_ceil(4) * 4;
            dir[name_off..name_off + 4].copy_from_slice(&ino.to_le_bytes());
            dir[name_off + 4..name_off + 6].copy_from_slice(&(rec_len as u16).to_le_bytes());
            dir[name_off + 6] = name.len() as u8;
            dir[name_off + 7] = 1; // DT_REG
            dir[name_off + 8..name_off + 8 + name.len()].copy_from_slice(name.as_bytes());
            ino
        }

        /// Kernel-style delete (fs/ext4/namei.c): the predecessor's
        /// rec_len absorbs the entry (the common shape — the dirent
        /// survives whole in the slack); dtime + links 0 on the inode.
        pub fn delete_file(&mut self, name: &str) {
            let bs = self.block_size as usize;
            let mut target = None;
            {
                let dir = &mut self.data[9 * bs..10 * bs];
                let mut off = 0usize;
                let mut pde_off = None;
                while off + 8 <= bs {
                    let ino =
                        u32::from_le_bytes([dir[off], dir[off + 1], dir[off + 2], dir[off + 3]]);
                    let rec_len = u16::from_le_bytes([dir[off + 4], dir[off + 5]]) as usize;
                    let name_len = dir[off + 6] as usize;
                    if rec_len < 8 || off + rec_len > bs {
                        break;
                    }
                    if ino != 0
                        && name_len == name.len()
                        && &dir[off + 8..off + 8 + name_len] == name.as_bytes()
                    {
                        target = Some(ino);
                        match pde_off {
                            Some(p) => {
                                // Kernel shape A (fs/ext4/namei.c):
                                // merge rec_len into the predecessor —
                                // the dirent (incl. inode bytes)
                                // survives whole in the slack.
                                let prev_rl = u16::from_le_bytes([dir[p + 4], dir[p + 5]]) as usize;
                                let merged = (prev_rl + rec_len).min(u16::MAX as usize);
                                dir[p + 4..p + 6].copy_from_slice(&(merged as u16).to_le_bytes());
                            }
                            None => {
                                // Kernel shape B: first entry — zero
                                // the inode, rec_len stays walkable.
                                dir[off..off + 4].copy_from_slice(&0u32.to_le_bytes());
                            }
                        }
                        break;
                    }
                    pde_off = Some(off);
                    off += rec_len;
                }
            }
            if let Some(ino) = target {
                let base = 5 * bs + (ino as usize - 1) * self.inode_size as usize;
                self.data[base + 0x14..base + 0x18].copy_from_slice(&T.to_le_bytes()); // dtime
                self.data[base + 0x1A..base + 0x1C].copy_from_slice(&0u16.to_le_bytes()); // links 0
            }
        }

        /// Hard delete: dirent fully zeroed (no name), dtime + links 0.
        pub fn hard_delete(&mut self, ino: u32) {
            let bs = self.block_size as usize;
            for b in self.data[9 * bs..10 * bs].iter_mut().skip(24).take(32) {
                *b = 0;
            }
            let base = 5 * bs + (ino as usize - 1) * self.inode_size as usize;
            self.data[base + 0x14..base + 0x18].copy_from_slice(&T.to_le_bytes());
            self.data[base + 0x1A..base + 0x1C].copy_from_slice(&0u16.to_le_bytes());
        }
    }

    fn put_size(raw: &mut [u8], size: u64) {
        raw[0x04..0x08].copy_from_slice(&(size as u32).to_le_bytes());
        raw[0x6C..0x70].copy_from_slice(&((size >> 32) as u32).to_le_bytes());
    }

    fn set_times(raw: &mut [u8]) {
        for off in [0x08, 0x0C, 0x10] {
            raw[off..off + 4].copy_from_slice(&T.to_le_bytes());
        }
    }

    fn write_inode(data: &mut [u8], ino: u32, inode_size: u16, f: impl FnOnce(&mut [u8])) {
        let off = 5 * 1024 + (ino as usize - 1) * inode_size as usize;
        let mut tmp = vec![0u8; inode_size as usize];
        f(&mut tmp);
        data[off..off + inode_size as usize].copy_from_slice(&tmp);
    }

    fn write_extent_header(raw: &mut [u8], entries: u16) {
        raw[0x28..0x2A].copy_from_slice(&EXT_MAGIC_EXTENT.to_le_bytes());
        raw[0x2A..0x2C].copy_from_slice(&entries.to_le_bytes());
        raw[0x2E..0x30].copy_from_slice(&0u16.to_le_bytes()); // depth 0
    }

    fn write_extent_entry(raw: &mut [u8], idx: usize, logical: u32, len: u32, phys: u32) {
        let off = 0x34 + idx * 12;
        raw[off..off + 4].copy_from_slice(&logical.to_le_bytes());
        raw[off + 4..off + 6].copy_from_slice(&(len as u16).to_le_bytes());
        raw[off + 8..off + 12].copy_from_slice(&phys.to_le_bytes());
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
    fn ext4_superblock_validates() {
        let dev = ram(ExtBuilder::new(true).data);
        let engine = ExtEngine::new(dev).expect("engine opens");
        assert_eq!(engine.kind(), FsKind::Ext4);
        assert_eq!(engine.sb.block_size, 1024);
        assert_eq!(engine.gdt.len(), 1);
    }

    #[test]
    fn ext2_discriminates() {
        let dev = ram(ExtBuilder::new(false).data);
        let engine = ExtEngine::new(dev).expect("engine opens");
        assert_eq!(engine.kind(), FsKind::Ext2);
    }

    /// The regression for the CI fixture-gate failure: the rev-1
    /// superblock field block was parsed +4 bytes off (missing the
    /// s_def_resuid/s_def_resgid pair), so real mkfs images (inode
    /// size 256) had every inode read land mid-slot and the walk
    /// found nothing. These are the REAL first 0x160 superblock
    /// bytes of `mkfs.ext2 -q -F` on a 16 MiB image (e2fsprogs
    /// 1.47.4, the same fixture CI builds) — byte-level ground truth.
    #[test]
    fn superblock_offsets_match_real_mkfs() {
        const REAL_SB: [u8; 0x160] = [
            0x00, 0x10, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x33, 0x03, 0x00, 0x00, 0x6a, 0x3b,
            0x00, 0x00, 0xf4, 0x0f, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x08,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xe5, 0xdb, 0xd4, 0x6a, 0x00, 0x00, 0xff, 0xff,
            0x53, 0xef, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0xe5, 0xdb, 0xd4, 0x6a, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x0b, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x38, 0x00, 0x00, 0x00, 0x02, 0x00,
            0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xbc, 0xf7, 0xc0, 0x25, 0xbc, 0x15, 0x43, 0xb9,
            0xaf, 0x37, 0x8f, 0x83, 0x67, 0xf8, 0xbd, 0x58, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3f, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xfe, 0x9e,
            0x93, 0xed, 0x86, 0x7b, 0x4e, 0xe8, 0x8c, 0x7a, 0xbb, 0xa1, 0xac, 0xe9, 0x35, 0xe9,
            0x01, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xe5, 0xdb,
            0xd4, 0x6a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00,
            0x20, 0x00,
        ];
        let sb = ExtSuper::parse(&REAL_SB).expect("real mkfs superblock parses");
        // Geometry (dumpe2fs ground truth for this image).
        assert_eq!(sb.inodes_count, 4096);
        assert_eq!(sb.blocks_count, 16384);
        assert_eq!(sb.block_size, 1024);
        assert_eq!(sb.blocks_per_group, 8192);
        assert_eq!(sb.inodes_per_group, 2048);
        assert_eq!(sb.first_data_block, 1);
        // The fields the old +4-shifted reads destroyed:
        // s_inode_size 0x58 = 256 (was read from 0x5C = compat flags).
        assert_eq!(sb.inode_size, 256, "s_inode_size lives at 0x58");
        // s_feature_compat 0x5C = ext_attr|resize_inode|dir_index.
        assert_eq!(sb.feature_compat, 0x38, "s_feature_compat lives at 0x5C");
        // s_feature_incompat 0x60 = filetype.
        assert_eq!(
            sb.feature_incompat, 0x02,
            "s_feature_incompat lives at 0x60"
        );
        assert_eq!(sb.kind(), FsKind::Ext2);
        assert_eq!(sb.desc_size, 32);
    }

    #[test]
    fn bad_magic_rejected() {
        let mut b = ExtBuilder::new(true);
        b.data[1024 + 0x38] = 0x00; // break magic
        assert!(ExtEngine::new(ram(b.data)).is_err());
    }

    #[test]
    fn first_data_block_crosscheck_rejects() {
        let mut b = ExtBuilder::new(true);
        // 1 KiB blocks with first_data_block = 0 → invalid.
        b.data[1024 + 0x14..1024 + 0x18].copy_from_slice(&0u32.to_le_bytes());
        assert!(ExtEngine::new(ram(b.data)).is_err());
    }

    #[test]
    fn ext4_walk_finds_files_with_extents() {
        let mut b = ExtBuilder::new(true);
        let content: Vec<u8> = (0..4096u32).map(|i| (i % 251) as u8).collect();
        b.add_file("data.bin", &content);
        let mut engine = ExtEngine::new(ram(b.data)).expect("engine");
        let mut sink = CollectSink(Vec::new());
        let stats = engine
            .quick_scan(&mut sink, &ScanControl::new())
            .expect("scan");
        let f = sink.0.iter().find(|e| e.name == "data.bin").expect("found");
        assert_eq!(f.size, 4096);
        assert_eq!(f.origin, NodeOrigin::Existing);
        let total: u64 = f.extents.iter().map(|e| e.len).sum();
        assert_eq!(total, 4096, "extent bytes cover the file");
        assert!(stats.entries >= 1);
    }

    #[test]
    fn ext2_walk_finds_files_with_indirects() {
        let mut b = ExtBuilder::new(false);
        let content: Vec<u8> = (0..2048u32).map(|i| (i % 249) as u8).collect();
        b.add_file("old.txt", &content);
        let mut engine = ExtEngine::new(ram(b.data)).expect("engine");
        assert_eq!(engine.kind(), FsKind::Ext2);
        let mut sink = CollectSink(Vec::new());
        engine
            .quick_scan(&mut sink, &ScanControl::new())
            .expect("scan");
        let f = sink.0.iter().find(|e| e.name == "old.txt").expect("found");
        assert_eq!(f.size, 2048);
        let total: u64 = f.extents.iter().map(|e| e.len).sum();
        assert_eq!(total, 2048);
    }

    #[test]
    fn deleted_file_found_via_merged_slack() {
        let mut b = ExtBuilder::new(true);
        let content: Vec<u8> = (0..1024u32).map(|i| (i % 253) as u8).collect();
        b.add_file("gone.bin", &content);
        b.delete_file("gone.bin"); // kernel-shape: merged rec_len + dtime
        let mut engine = ExtEngine::new(ram(b.data)).expect("engine");
        let mut sink = CollectSink(Vec::new());
        engine
            .quick_scan(&mut sink, &ScanControl::new())
            .expect("scan");
        let f = sink
            .0
            .iter()
            .find(|e| e.name == "gone.bin")
            .expect("deleted file found by slack name");
        assert_eq!(f.origin, NodeOrigin::Deleted);
        assert_eq!(f.size, 1024);
        assert_eq!(f.timestamps.deleted, Some(T as i64 * 1000));
        let total: u64 = f.extents.iter().map(|e| e.len).sum();
        assert_eq!(total, 1024, "content extents intact");
    }

    #[test]
    fn zeroed_inode_deletion_recovers_name() {
        let mut b = ExtBuilder::new(true);
        let content: Vec<u8> = (0..768u32).map(|i| (i % 251) as u8).collect();
        let ino = b.add_file("zeroed.bin", &content);
        // Zeroed-inode shape: inode field cleared, rec_len intact.
        let bs = b.block_size as usize;
        let dir = &mut b.data[9 * bs..10 * bs];
        dir[24..28].copy_from_slice(&0u32.to_le_bytes());
        let base = 5 * bs + (ino as usize - 1) * b.inode_size as usize;
        b.data[base + 0x14..base + 0x18].copy_from_slice(&T.to_le_bytes());
        b.data[base + 0x1A..base + 0x1C].copy_from_slice(&0u16.to_le_bytes());
        let mut engine = ExtEngine::new(ram(b.data)).expect("engine");
        let mut sink = CollectSink(Vec::new());
        engine
            .quick_scan(&mut sink, &ScanControl::new())
            .expect("scan");
        // The name survives (metadata-only) AND the content survives
        // via the orphan sweep.
        let name = sink
            .0
            .iter()
            .find(|e| e.name == "zeroed.bin")
            .expect("name recovered");
        assert_eq!(name.origin, NodeOrigin::Deleted);
        let orphan = sink
            .0
            .iter()
            .find(|e| e.name == format!("inode_{ino}"))
            .expect("content swept as orphan");
        assert_eq!(orphan.origin, NodeOrigin::LostPath);
        assert_eq!(orphan.size, 768);
        let total: u64 = orphan.extents.iter().map(|e| e.len).sum();
        assert_eq!(total, 1024, "extents are block-rounded (1 block)");
    }

    #[test]
    fn orphan_sweep_finds_unnamed_deleted() {
        let mut b = ExtBuilder::new(true);
        let content: Vec<u8> = (0..512u32).map(|i| (i % 247) as u8).collect();
        let ino = b.add_file("never-named.bin", &content);
        b.hard_delete(ino); // dirent fully gone
        let mut engine = ExtEngine::new(ram(b.data)).expect("engine");
        let mut sink = CollectSink(Vec::new());
        engine
            .quick_scan(&mut sink, &ScanControl::new())
            .expect("scan");
        let f = sink
            .0
            .iter()
            .find(|e| e.name == format!("inode_{ino}"))
            .expect("orphan swept");
        assert_eq!(f.origin, NodeOrigin::LostPath);
        assert_eq!(f.size, 512);
        let total: u64 = f.extents.iter().map(|e| e.len).sum();
        assert_eq!(total, 1024, "extents are block-rounded (1 block)");
    }

    #[test]
    fn multi_block_extents_coalesce() {
        let mut b = ExtBuilder::new(true);
        let content: Vec<u8> = (0..5000u32).map(|i| (i % 253) as u8).collect();
        b.add_file("big.bin", &content);
        let mut engine = ExtEngine::new(ram(b.data)).expect("engine");
        let mut sink = CollectSink(Vec::new());
        engine
            .quick_scan(&mut sink, &ScanControl::new())
            .expect("scan");
        let f = sink.0.iter().find(|e| e.name == "big.bin").expect("found");
        assert_eq!(f.size, 5000);
        // 5 blocks → one coalesced extent (blocks are contiguous).
        assert_eq!(f.extents.len(), 1, "contiguous blocks coalesce");
        assert_eq!(f.extents[0].len, 5 * 1024);
        assert_eq!(f.extents[0].offset, 10 * 1024); // first free block
    }

    #[test]
    fn corrupted_dirent_chain_stops_cleanly() {
        let mut b = ExtBuilder::new(true);
        let content: Vec<u8> = (0..256u32).map(|i| i as u8).collect();
        b.add_file("ok.bin", &content);
        // Corrupt rec_len at the first entry → chain breaks, no panic.
        let bs = b.block_size as usize;
        b.data[9 * bs + 4] = 0xFF;
        b.data[9 * bs + 5] = 0xFF;
        let mut engine = ExtEngine::new(ram(b.data)).expect("engine");
        let mut sink = CollectSink(Vec::new());
        let _ = engine.quick_scan(&mut sink, &ScanControl::new());
        // No panic = pass; the orphan sweep may still find content.
    }

    // ── REAL-filesystem fixture (M2 exit criterion: engine vs
    // ground-truth mkfs output, not hand-built structures) ──────────
    //
    // CI (ci.yml rust-core job) generates a fresh mkfs.ext2 image,
    // writes a deterministic seed file via debugfs, and runs this test
    // with DW_EXT2_FIXTURE=<path>. Soft-skips when the env var is
    // unset (local runs without the fixture step).

    /// The seed content the CI fixture writes (i*7+3 mod 251, 1000 B).
    fn ext2_fixture_seed() -> Vec<u8> {
        (0..1000u32).map(|i| ((i * 7 + 3) % 251) as u8).collect()
    }

    /// In-memory destination (the recover tests' RamDest pattern).
    struct MemDest {
        files: std::collections::HashMap<String, Vec<u8>>,
    }
    impl crate::recover::DestWriter for MemDest {
        fn mkdir(&mut self, _path: &str) -> Result<(), String> {
            Ok(())
        }
        fn write_file(
            &mut self,
            path: &str,
            read: &mut dyn FnMut(&mut [u8]) -> usize,
        ) -> Result<(), String> {
            let mut out = Vec::new();
            let mut buf = vec![0u8; 4096];
            loop {
                let n = read(&mut buf);
                if n == 0 {
                    break;
                }
                out.extend_from_slice(&buf[..n]);
            }
            self.files.insert(path.to_string(), out);
            Ok(())
        }
        fn free_bytes(&self) -> Result<u64, String> {
            Ok(u64::MAX / 4)
        }
    }

    #[test]
    #[ignore = "CI fixture test: run with DW_EXT2_FIXTURE=<mkfs.ext2 image>"]
    fn ext2_real_mkfs_fixture_scan_and_recover() {
        let Ok(path) = std::env::var("DW_EXT2_FIXTURE") else {
            return; // fixture not provided — soft skip (CI provides it)
        };
        let data = std::fs::read(&path).expect("fixture image readable");
        let mut ram = RamDevice::new(data.len(), 512);
        ram.data = data;
        let dev: Arc<dyn BlockDevice> = Arc::new(ram);

        // Full pipeline: the controller must detect the ext2
        // superblock on the raw image (no partition table), run the
        // engine, and surface the seed file.
        let mut obs = crate::scan::NoObserver;
        let (tree, report) = crate::scan::run_scan(
            Arc::clone(&dev) as Arc<dyn BlockDevice>,
            "ext2-fixture",
            &mut obs,
        )
        .expect("scan completes");
        assert!(
            !report.stopped,
            "fixture scan must run to completion (fs: {:?})",
            report.fs
        );
        let node = tree
            .nodes()
            .iter()
            .find(|n| n.name == "seed.txt")
            .expect("the mkfs seed file is found (detection + walk + naming)");
        let seed = ext2_fixture_seed();
        assert_eq!(node.size, seed.len() as u64, "exact logical size");

        // Byte-truth recovery through the real queue/extent path.
        let ids = [node.id];
        let queue = crate::recover::build_queue(tree.nodes(), &ids);
        assert!(!queue.is_empty(), "the seed node is recoverable");
        let mut dest = MemDest {
            files: std::collections::HashMap::new(),
        };
        let ctrl = ScanControl::new();
        let result = crate::recover::run_recovery(&*dev, &queue, &mut dest, &ctrl, |_| {})
            .expect("recovery");
        assert_eq!(result.done_files, 1);
        assert_eq!(result.failed_files, 0);
        let recovered = dest.files.values().next().expect("one recovered file");
        assert_eq!(recovered.len(), seed.len());
        assert_eq!(recovered, &seed, "recovered bytes are byte-identical");
    }
}
