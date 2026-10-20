//! The FAT engine — FAT12/16/32 cluster chains, directory-entry walk,
//! LFN reassembly, deleted reanimation.
//!
//! Algorithm (research: `artifacts/SearchDLL/fat-engine--deep-dive.md`
//! and `fat-exfat-engine-algorithms-v2.md` — the "FAT table
//! polymorphism + cycle-guard + 0xE5×6 sites" write-up):
//!
//! 1.  BPB parse → FAT variant by cluster-count math (the canonical
//!     discriminator: < 4085 = FAT12, < 65525 = FAT16, else FAT32).
//! 2.  Directory tree walk from the root (FAT12/16: the fixed root
//!     region; FAT32: the root cluster chain).
//! 3.  Per directory: cluster-chain walk with a visited-set cycle
//!     guard; 32-byte entry parse; LFN fragments reassemble ahead of
//!     their 8.3 anchor; `0xE5` first bytes mark deleted entries.
//! 4.  Deleted files keep their cluster start (byte 1 of the name
//!     survives) — chains re-walked; `0x00` first-byte terminates the
//!     live region, then a deep pass continues into the erased region.
//! 5.  BAD clusters (0xFF7/0xFFF7/0xFFFFFF7) and EOC markers gate the
//!     chain walker; orphan chains (chain not referenced by any entry)
//!     are a deep-pass salvage.

use std::collections::HashSet;
use std::sync::Arc;

use crate::blockdev::{BlockDevice, BlockRead};
use crate::error::{CoreError, CoreResult};
use crate::fs::{ScanControl, ScanEngine, ScanSink, ScanStats};
use crate::types::{Extent, FileAttrs, FoundEntry, FsKind, NodeOrigin, Timestamps};

/// FAT directory attributes.
const ATTR_LFN: u8 = 0x0F;
const ATTR_DIR: u8 = 0x10;

/// The parsed BPB.
#[derive(Debug, Clone, Copy)]
pub struct FatBoot {
    pub variant: FsKind,
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub fat_count: u8,
    pub root_entry_count: u16,
    pub total_sectors: u64,
    pub sectors_per_fat: u32,
    /// FAT32: the root's first cluster; FAT12/16: unused (2).
    pub root_cluster: u32,
}

impl FatBoot {
    /// Parse + classify from the boot sector.
    ///
    /// # Errors
    /// [`CoreError::Corrupt`] on missing signature/geometry.
    pub fn parse(sector: &[u8]) -> CoreResult<Self> {
        let dos_ok = sector.len() >= 512 && sector[510] == 0x55 && sector[511] == 0xAA;
        let bps = u16::from_le_bytes([sector[0x0B], sector[0x0C]]);
        let spc = sector[0x0D];
        if !(dos_ok && bps != 0) || bps & (bps - 1) != 0 || spc == 0 || spc & (spc - 1) != 0 {
            return Err(CoreError::Corrupt {
                structure: "FAT BPB",
                detail: format!("bad geometry sig={dos_ok} bps={bps} spc={spc}"),
            });
        }
        let reserved = u16::from_le_bytes([sector[0x0E], sector[0x0F]]);
        let fats = sector[0x10];
        let root_entries = u16::from_le_bytes([sector[0x11], sector[0x12]]);
        let total16 = u16::from_le_bytes([sector[0x13], sector[0x14]]) as u64;
        let total32 =
            u32::from_le_bytes([sector[0x20], sector[0x21], sector[0x22], sector[0x23]]) as u64;
        let total = if total32 > 0 { total32 } else { total16 };
        let spf16 = u16::from_le_bytes([sector[0x16], sector[0x17]]) as u32;
        let spf32 = u32::from_le_bytes([sector[0x24], sector[0x25], sector[0x26], sector[0x27]]);
        let root_cluster =
            u32::from_le_bytes([sector[0x2C], sector[0x2D], sector[0x2E], sector[0x2F]]);
        if fats == 0 || total == 0 || (spf16 == 0 && spf32 == 0) {
            return Err(CoreError::Corrupt {
                structure: "FAT BPB",
                detail: "missing FAT geometry".into(),
            });
        }
        let sectors_per_fat = if spf16 > 0 { spf16 } else { spf32 };
        let fat32 = spf32 > 0 && spf16 == 0;
        // Cluster-count classification.
        let root_dir_sectors = if fat32 {
            0
        } else {
            (u64::from(root_entries) * 32).div_ceil(u64::from(bps))
        };
        let data_sectors = total.saturating_sub(
            u64::from(reserved) + u64::from(fats) * u64::from(sectors_per_fat) + root_dir_sectors,
        );
        let cluster_count = data_sectors / u64::from(spc);
        let variant = if fat32 || cluster_count >= 65_525 {
            FsKind::Fat32
        } else if cluster_count >= 4085 {
            FsKind::Fat16
        } else {
            FsKind::Fat12
        };
        Ok(Self {
            variant,
            bytes_per_sector: bps,
            sectors_per_cluster: spc,
            reserved_sectors: reserved,
            fat_count: fats,
            root_entry_count: root_entries,
            total_sectors: total,
            sectors_per_fat,
            root_cluster: if fat32 && root_cluster >= 2 {
                root_cluster
            } else {
                2
            },
        })
    }

    /// Byte offset of the data area start (cluster 2).
    #[must_use]
    pub fn data_start(&self) -> u64 {
        let root_dir_sectors =
            (u64::from(self.root_entry_count) * 32).div_ceil(u64::from(self.bytes_per_sector));
        (u64::from(self.reserved_sectors)
            + u64::from(self.fat_count) * u64::from(self.sectors_per_fat)
            + root_dir_sectors)
            * u64::from(self.bytes_per_sector)
    }

    /// Byte offset of the FAT12/16 fixed root directory.
    #[must_use]
    pub fn root_dir_offset(&self) -> u64 {
        (u64::from(self.reserved_sectors)
            + u64::from(self.fat_count) * u64::from(self.sectors_per_fat))
            * u64::from(self.bytes_per_sector)
    }

    #[must_use]
    pub fn cluster_size(&self) -> u64 {
        u64::from(self.bytes_per_sector) * u64::from(self.sectors_per_cluster)
    }

    /// Byte offset of cluster `n` (n ≥ 2).
    #[must_use]
    pub fn cluster_offset(&self, n: u32) -> u64 {
        self.data_start() + u64::from(n.saturating_sub(2)) * self.cluster_size()
    }
}

/// One parsed directory entry (the 8.3 anchor; LFN pre-assembled).
struct DirEntry {
    name: String,
    is_dir: bool,
    attrs: u8,
    first_cluster: u32,
    size: u64,
    times: Timestamps,
    deleted: bool,
}

/// The engine.
pub struct FatEngine {
    dev: Arc<dyn BlockDevice>,
    boot: FatBoot,
    /// The FAT table (loaded whole — a 1 GiB FAT32 is 1 MiB; FAT16
    /// tiny; acceptable and simple; chunked walk is M2 hardening).
    fat: Vec<u8>,
}

impl FatEngine {
    /// Open + parse + load the FAT.
    ///
    /// # Errors
    /// [`CoreError::Corrupt`] when the volume isn't parseable FAT.
    pub fn new(dev: Arc<dyn BlockDevice>) -> CoreResult<Self> {
        let sector = dev.read_struct_at::<512>(0)?;
        let boot = FatBoot::parse(&sector)?;
        let fat_bytes = u64::from(boot.sectors_per_fat) * u64::from(boot.bytes_per_sector);
        // The FAT lives ON the volume: a size exceeding the device is
        // corrupt metadata, not a multi-GiB allocation to attempt (the
        // fuzz suite's 12-GiB-malloc catch — allocate AFTER the guard,
        // never before the read that would reject the region).
        if fat_bytes == 0 || fat_bytes > dev.capacity_bytes() {
            return Err(CoreError::Corrupt {
                structure: "fat boot",
                detail: format!("FAT bytes {fat_bytes} exceed the volume"),
            });
        }
        let mut fat = vec![0u8; fat_bytes as usize];
        dev.read_bytes(
            u64::from(boot.reserved_sectors) * u64::from(boot.bytes_per_sector),
            &mut fat,
        )?;
        Ok(Self { dev, boot, fat })
    }

    /// Read the FAT entry for cluster `n` (variant-polymorphic read:
    /// 12-bit packed / 16 / 32).
    #[must_use]
    fn fat_entry(&self, n: u32) -> u32 {
        match self.boot.variant {
            FsKind::Fat12 => {
                let off = n as usize * 3 / 2;
                if off + 2 > self.fat.len() {
                    return 0;
                }
                let v = u16::from_le_bytes([self.fat[off], self.fat[off + 1]]);
                if n.is_multiple_of(2) {
                    u32::from(v & 0x0FFF)
                } else {
                    u32::from(v >> 4)
                }
            }
            FsKind::Fat16 => {
                let off = n as usize * 2;
                if off + 2 > self.fat.len() {
                    return 0;
                }
                u32::from(u16::from_le_bytes([self.fat[off], self.fat[off + 1]]))
            }
            _ => {
                let off = n as usize * 4;
                if off + 4 > self.fat.len() {
                    return 0;
                }
                u32::from_le_bytes([
                    self.fat[off],
                    self.fat[off + 1],
                    self.fat[off + 2],
                    self.fat[off + 3],
                ]) & 0x0FFF_FFFF
            }
        }
    }

    fn is_eoc(variant: FsKind, v: u32) -> bool {
        match variant {
            FsKind::Fat12 => v >= 0xFF8,
            FsKind::Fat16 => v >= 0xFFF8,
            _ => v >= 0x0FFF_FFF8,
        }
    }

    fn is_bad(variant: FsKind, v: u32) -> bool {
        match variant {
            FsKind::Fat12 => v == 0xFF7,
            FsKind::Fat16 => v == 0xFFF7,
            _ => v == 0x0FFF_FFF7,
        }
    }

    /// Walk a cluster chain with the cycle guard → extents.
    fn chain_extents(&self, first: u32) -> Vec<Extent> {
        let mut out: Vec<Extent> = Vec::new();
        let mut visited = HashSet::new();
        let cs = self.boot.cluster_size();
        let mut cluster = first;
        while cluster >= 2
            && !Self::is_eoc(self.boot.variant, cluster)
            && !Self::is_bad(self.boot.variant, cluster)
            && visited.insert(cluster)
        {
            let off = self.boot.cluster_offset(cluster);
            // Coalesce adjacent clusters into one extent.
            match out.last_mut() {
                Some(last) if last.end() == off => last.len += cs,
                _ => out.push(Extent::new(off, cs)),
            }
            cluster = self.fat_entry(cluster);
        }
        out
    }

    /// Read a directory's raw sectors (its cluster chain).
    fn read_dir_data(&self, first_cluster: u32) -> Vec<u8> {
        let extents = self.chain_extents(first_cluster);
        let total: u64 = extents.iter().map(|e| e.len).sum();
        if total == 0 || total > 16 * 1024 * 1024 {
            return Vec::new(); // sanity cap: 16 MiB directory
        }
        let mut buf = vec![0u8; total as usize];
        let mut pos = 0usize;
        for e in extents {
            if self
                .dev
                .read_bytes(e.offset, &mut buf[pos..pos + e.len as usize])
                .is_err()
            {
                // Damaged cluster: leave zeros, keep walking (deep
                // entries after the hole survive).
                pos += e.len as usize;
                continue;
            }
            pos += e.len as usize;
        }
        buf
    }

    /// Parse raw directory bytes into entries (LFN reassembly; both
    /// live and 0xE5-deleted).
    fn parse_dir(&self, data: &[u8]) -> Vec<DirEntry> {
        let mut out = Vec::new();
        let mut lfn_parts: Vec<(u8, Vec<u16>)> = Vec::new(); // (seq, chars)
        let mut live_region = true;
        let (entries, _) = data.as_chunks::<32>();
        for entry in entries {
            let first = entry[0];
            if first == 0x00 {
                live_region = false; // end of allocated entries — the
                // erased region may still hold recoverable 0xE5s
                // (keep scanning: deep behavior per the research).
                continue;
            }
            let attrs = entry[0x0B];
            if attrs == ATTR_LFN {
                // LFN fragment: seq at 0, chars spread across 3 spans.
                let seq = if first == 0xE5 { 0 } else { first & 0x3F };
                if seq == 0 {
                    lfn_parts.clear();
                    continue;
                }
                let mut chars = Vec::with_capacity(13);
                for (start, len) in [(0x01usize, 10usize), (0x0E, 12), (0x1C, 4)] {
                    let (units, _) = entry[start..start + len].as_chunks::<2>();
                    for c in units {
                        let u = u16::from_le_bytes(*c);
                        if u == 0x0000 || u == 0xFFFF {
                            continue;
                        }
                        chars.push(u);
                    }
                }
                lfn_parts.push((seq, chars));
                continue;
            }
            // 8.3 anchor.
            let deleted = first == 0xE5;
            if deleted {
                live_region = false;
            }
            let name = if lfn_parts.is_empty() {
                short_name(entry)
            } else {
                // Reassemble LFN fragments in sequence order.
                lfn_parts.sort_by_key(|(s, _)| *s);
                let mut units = Vec::new();
                for (_, chars) in &lfn_parts {
                    units.extend_from_slice(chars);
                }
                lfn_parts.clear();
                let n = String::from_utf16_lossy(&units);
                if n.is_empty() { short_name(entry) } else { n }
            };
            if name == "." || name == ".." {
                continue;
            }
            let cluster_lo = u16::from_le_bytes([entry[0x1A], entry[0x1B]]) as u32;
            let cluster_hi = if self.boot.variant == FsKind::Fat32 {
                u16::from_le_bytes([entry[0x14], entry[0x15]]) as u32
            } else {
                0
            };
            let first_cluster = cluster_hi << 16 | cluster_lo;
            let size =
                u32::from_le_bytes([entry[0x1C], entry[0x1D], entry[0x1E], entry[0x1F]]) as u64;
            let write_date = u16::from_le_bytes([entry[0x18], entry[0x19]]);
            let write_time = u16::from_le_bytes([entry[0x16], entry[0x17]]);
            let create_date = u16::from_le_bytes([entry[0x10], entry[0x11]]);
            let create_time = u16::from_le_bytes([entry[0x0E], entry[0x0F]]);
            let access_date = u16::from_le_bytes([entry[0x12], entry[0x13]]);
            out.push(DirEntry {
                name,
                is_dir: attrs & ATTR_DIR != 0 && !deleted,
                attrs,
                first_cluster,
                size,
                times: Timestamps {
                    modified: Some(fat_datetime(write_date, write_time)),
                    created: Some(fat_datetime(create_date, create_time)),
                    accessed: Some(fat_date(access_date)),
                    changed: None,
                    deleted: None,
                },
                deleted,
            });
        }
        let _ = live_region;
        out
    }

    /// Recursive tree walk.
    #[allow(clippy::too_many_arguments)] // Tree-walk recursion signature; a params struct hurts here.
    fn walk_dir(
        &self,
        cluster: Option<u32>,
        root_region: Option<(u64, u64)>,
        path: &str,
        sink: &mut dyn ScanSink,
        ctrl: &ScanControl,
        stats: &mut ScanStats,
        depth: usize,
        visited: &mut HashSet<u32>,
    ) -> CoreResult<()> {
        if depth > 32 {
            return Ok(()); // pathological depth guard
        }
        ctrl.checkpoint()?;
        // The GLOBAL directory-cluster set: a fuzzed FAT can wire
        // directory entries into each other, and without this set the
        // same cluster re-walks once per PATH — 2^depth walks of the
        // same bytes (the fat fuzz target's 11-second timeout on a
        // 514-byte input). One cluster, one walk: identical results,
        // linear cost. It also de-duplicates legitimately-reachable-
        // twice directories (their entries emitted once, not twice).
        if let Some(c) = cluster
            && c >= 2
            && !visited.insert(c)
        {
            return Ok(());
        }
        let data = match (cluster, root_region) {
            (Some(c), _) if c >= 2 => self.read_dir_data(c),
            (None, Some((off, len))) => {
                let mut buf = vec![0u8; len as usize];
                if self.dev.read_bytes(off, &mut buf).is_err() {
                    return Ok(());
                }
                buf
            }
            _ => return Ok(()),
        };
        for e in self.parse_dir(&data) {
            let child_path = if path.is_empty() {
                e.name.clone()
            } else {
                format!("{path}\\{}", e.name)
            };
            let origin = if e.deleted {
                NodeOrigin::Deleted
            } else {
                NodeOrigin::Existing
            };
            // Every entry resolves its chain (live files recover from
            // their extents; deleted entries re-walk what survives).
            let extents = self.chain_extents(e.first_cluster);
            let mut entry = FoundEntry::structured(e.name.clone(), e.is_dir, self.boot.variant);
            entry.parent_hint = Some(path.to_string());
            entry.attrs = FileAttrs(u32::from(e.attrs));
            entry.timestamps = e.times;
            entry.origin = origin;
            entry.size = if e.is_dir { 0 } else { e.size };
            entry.extents = if e.is_dir { Vec::new() } else { extents };
            if e.deleted {
                entry.confidence = 80;
            }
            stats.entries += u64::from(!e.is_dir);
            stats.bytes += entry.size;
            sink.on_entry(entry);
            if e.is_dir {
                self.walk_dir(
                    Some(e.first_cluster),
                    None,
                    &child_path,
                    sink,
                    ctrl,
                    stats,
                    depth + 1,
                    visited,
                )?;
            }
        }
        Ok(())
    }
}

impl ScanEngine for FatEngine {
    fn kind(&self) -> FsKind {
        self.boot.variant
    }

    fn quick_scan(&mut self, sink: &mut dyn ScanSink, ctrl: &ScanControl) -> CoreResult<ScanStats> {
        let mut stats = ScanStats::default();
        let mut visited = HashSet::new();
        if self.boot.variant == FsKind::Fat32 {
            self.walk_dir(
                Some(self.boot.root_cluster),
                None,
                "",
                sink,
                ctrl,
                &mut stats,
                0,
                &mut visited,
            )
        } else {
            let len = u64::from(self.boot.root_entry_count) * 32;
            self.walk_dir(
                None,
                Some((self.boot.root_dir_offset(), len)),
                "",
                sink,
                ctrl,
                &mut stats,
                0,
                &mut visited,
            )
        }?;
        Ok(stats)
    }
}

/// The 8.3 name formatter — case preserved, spaces trimmed. A
/// deleted entry's first byte (0xE5) lost the real first character,
/// so the marker becomes '_' (the honest recovery convention; the old
/// unconditional `~name` lower-case mangling corrupted LIVE names
/// too).
fn short_name(entry: &[u8]) -> String {
    let deleted_marker = entry[0] == 0xE5;
    let mut base: String = entry[0..8]
        .iter()
        .take_while(|&&b| b != b' ')
        .map(|&b| b as char)
        .collect();
    if deleted_marker {
        // The 0xE5 first byte decodes as 'å' through `as char` —
        // replace that first CHAR on its UTF-8 boundary.
        let first_len = base.chars().next().map_or(0, char::len_utf8);
        base.replace_range(..first_len, "_");
    }
    let ext: String = entry[8..11]
        .iter()
        .take_while(|&&b| b != b' ')
        .map(|&b| b as char)
        .collect();
    if ext.is_empty() {
        base
    } else {
        format!("{base}.{ext}")
    }
}

/// FAT date/time → unix seconds.
#[must_use]
pub fn fat_datetime(date: u16, time: u16) -> i64 {
    let year = 1980 + (date >> 9) as i64;
    let month = ((date >> 5) & 0x0F) as i64;
    let day = (date & 0x1F) as i64;
    let hour = (time >> 11) as i64;
    let min = ((time >> 5) & 0x3F) as i64;
    let sec = (time & 0x1F) as i64 * 2;
    civil_to_unix(year, month.max(1), day.max(1), hour, min, sec)
}

/// FAT date-only → unix seconds.
#[must_use]
pub fn fat_date(date: u16) -> i64 {
    fat_datetime(date, 0)
}

/// Civil → unix (days-from-civil: Howard Hinnant's algorithm).
#[must_use]
fn civil_to_unix(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + hour * 3_600 + minute * 60 + second
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::RamDevice;

    /// Build a FAT16 volume image with a root file + a subdirectory.
    fn fat16_image() -> RamDevice {
        let total_sectors = 8192u64;
        let mut s = vec![0u8; 512];
        s[..3].copy_from_slice(b"\xEB\x3C\x90");
        s[0x0B..0x0D].copy_from_slice(&512u16.to_le_bytes());
        s[0x0D] = 1; // 1 sector/cluster
        s[0x0E..0x10].copy_from_slice(&1u16.to_le_bytes()); // reserved
        s[0x10] = 1; // FAT count
        s[0x11..0x13].copy_from_slice(&16u16.to_le_bytes()); // root entries
        s[0x13..0x15].copy_from_slice(&(total_sectors as u16).to_le_bytes());
        s[0x16..0x18].copy_from_slice(&1u16.to_le_bytes()); // sectors/FAT
        s[510] = 0x55;
        s[511] = 0xAA;
        let mut ram = RamDevice::new(512 * total_sectors as usize, 512);
        ram.data[..512].copy_from_slice(&s);
        // FAT at sector 1: clusters 2,3 used (files), 4 used (dir), EOC chains.
        let fat_base = 512usize;
        let set16 = |ram: &mut RamDevice, n: u32, v: u16| {
            let off = fat_base + n as usize * 2;
            ram.data[off..off + 2].copy_from_slice(&v.to_le_bytes());
        };
        set16(&mut ram, 0, 0xFFF8);
        set16(&mut ram, 1, 0xFFFF);
        set16(&mut ram, 2, 0xFFFF); // readme.txt: single cluster
        set16(&mut ram, 3, 0xFFFF); // gone.log (deleted)
        set16(&mut ram, 4, 0xFFFF); // docs dir: single cluster
        // Root dir at sector 2 (reserved 1 + FAT 1): 16 entries × 32 B.
        let root = 2 * 512;
        let put = |ram: &mut RamDevice, slot: usize, entry: &[u8; 32]| {
            let off = root + slot * 32;
            ram.data[off..off + 32].copy_from_slice(entry);
        };
        // readme.txt (live) → cluster 2, 100 bytes.
        let mut e = [0u8; 32];
        e[..11].copy_from_slice(b"README  TXT");
        e[0x0B] = 0x20; // archive
        e[0x1A..0x1C].copy_from_slice(&2u16.to_le_bytes());
        e[0x1C..0x20].copy_from_slice(&100u32.to_le_bytes());
        e[0x18..0x1A]
            .copy_from_slice(&(((2026 - 1980) << 0x9 | 10 << 0x5 | 0x11) as u16).to_le_bytes());
        put(&mut ram, 0, &e);
        // GONE.LOG (deleted 0xE5) → cluster 3, 40 bytes.
        let mut e = [0u8; 32];
        e[..11].copy_from_slice(b"\xE5ONE    LOG");
        e[0x0B] = 0x20;
        e[0x1A..0x1C].copy_from_slice(&3u16.to_le_bytes());
        e[0x1C..0x20].copy_from_slice(&40u32.to_le_bytes());
        put(&mut ram, 1, &e);
        // DOCS dir (live) → cluster 4.
        let mut e = [0u8; 32];
        e[..11].copy_from_slice(b"DOCS       ");
        e[0x0B] = 0x10;
        e[0x1A..0x1C].copy_from_slice(&4u16.to_le_bytes());
        put(&mut ram, 2, &e);
        // Inside DOCS (cluster 4 = sector 5): a LFN file "Report Final.txt" on cluster 5.
        set16(&mut ram, 5, 0xFFFF);
        let dir_base = 5 * 512;
        // LFN fragments (2 fragments, 16 chars): seq 1 + seq 0x42.
        let name_u16: Vec<u16> = "Report Final.txt".encode_utf16().collect();
        assert_eq!(name_u16.len(), 16);
        // LFN entry seq1 = chars 0..13 (5 + 6 + 2 spans).
        let mut lfn1 = [0u8; 32];
        lfn1[0] = 0x01;
        lfn1[0x0B] = 0x0F;
        for (i, u) in name_u16.iter().take(5).enumerate() {
            lfn1[1 + i * 2..3 + i * 2].copy_from_slice(&u.to_le_bytes());
        }
        for (i, u) in name_u16.iter().skip(5).take(6).enumerate() {
            lfn1[0x0E + i * 2..0x10 + i * 2].copy_from_slice(&u.to_le_bytes());
        }
        for (i, u) in name_u16.iter().skip(11).take(2).enumerate() {
            lfn1[0x1C + i * 2..0x1E + i * 2].copy_from_slice(&u.to_le_bytes());
        }
        // LFN entry seq2 (last, 0x42) = chars 13..16.
        let mut lfn2 = [0u8; 32];
        lfn2[0] = 0x42;
        lfn2[0x0B] = 0x0F;
        for (i, u) in name_u16.iter().skip(13).take(5).enumerate() {
            lfn2[1 + i * 2..3 + i * 2].copy_from_slice(&u.to_le_bytes());
        }
        // Anchor: REPORTFIXT → short name REPORT~1 TXT, cluster 5.
        let mut e3 = [0u8; 32];
        e3[..11].copy_from_slice(b"REPORT  TXT");
        e3[0x0B] = 0x20;
        e3[0x1A..0x1C].copy_from_slice(&5u16.to_le_bytes());
        e3[0x1C..0x20].copy_from_slice(&1000u32.to_le_bytes());
        let mut off = dir_base;
        ram.data[off..off + 32].copy_from_slice(&lfn1);
        off += 32;
        ram.data[off..off + 32].copy_from_slice(&lfn2);
        off += 32;
        ram.data[off..off + 32].copy_from_slice(&e3);
        ram
    }

    /// The fat fuzz target's catch (run 39139452642, 12-GiB malloc):
    /// `sectors_per_fat × bytes_per_sector` was allocated BEFORE the
    /// read that would reject the region. An over-volume FAT size is
    /// corrupt metadata — guard, never allocate.
    #[test]
    fn implausible_fat_size_is_corrupt_not_oom() {
        let mut ram = fat16_image();
        // 65535 sectors/FAT × 512 B = 33.5 MiB on a 4 MiB image.
        ram.data[0x16..0x18].copy_from_slice(&0xFFFFu16.to_le_bytes());
        let dev: Arc<dyn BlockDevice> = Arc::new(ram);
        assert!(
            FatEngine::new(dev).is_err(),
            "an over-volume FAT size is corrupt metadata, never an allocation"
        );
    }

    #[test]
    fn fat16_walk_finds_files_dirs_deleted_lfn() {
        let ram = fat16_image();
        let dev: Arc<dyn BlockDevice> = Arc::new(ram);
        let mut engine = FatEngine::new(dev).expect("engine");
        assert_eq!(engine.boot.variant, FsKind::Fat16);
        struct Collector(Vec<FoundEntry>);
        impl crate::fs::ScanSink for Collector {
            fn on_entry(&mut self, e: FoundEntry) {
                self.0.push(e);
            }
        }
        let mut sink = Collector(Vec::new());
        let ctrl = ScanControl::new();
        let stats = engine.quick_scan(&mut sink, &ctrl).expect("scan");
        assert!(stats.entries >= 3);
        // Live root file.
        let readme = sink
            .0
            .iter()
            .find(|e| e.name == "README.TXT")
            .expect("readme");
        assert_eq!(readme.origin, NodeOrigin::Existing);
        assert_eq!(readme.size, 100);
        assert_eq!(readme.extents.len(), 1);
        // Deleted file reanimated with its chain.
        let gone = sink
            .0
            .iter()
            .find(|e| e.name.starts_with('_') && e.name.to_lowercase().contains("one"))
            .expect("deleted gone.log");
        assert_eq!(gone.origin, NodeOrigin::Deleted);
        assert_eq!(gone.size, 40);
        assert_eq!(gone.confidence, 80);
        // Directory recursed.
        assert!(sink.0.iter().any(|e| e.is_dir && e.name == "DOCS"));
        // LFN reassembled.
        assert!(sink.0.iter().any(|e| e.name == "Report Final.txt"));
    }

    #[test]
    fn lfn_and_short_names() {
        // Unit-check the short-name formatter.
        let mut e = [0u8; 32];
        e[..11].copy_from_slice(b"ABC     TXT");
        assert_eq!(short_name(&e), "ABC.TXT");
    }

    #[test]
    fn fat_times_convert() {
        // 2026-10-17 00:00:00 ≈ 1792243200.
        let date = ((2026 - 1980) << 0x9) | (10 << 0x5) | 0x11;
        let t = fat_datetime(date, 0);
        assert!(t > 1_790_000_000 && t < 1_800_000_000);
    }

    #[test]
    fn cycle_guard_terminates_on_loops() {
        let mut ram = fat16_image();
        // Corrupt: cluster 5 → cluster 6 → cluster 5 (loop).
        let fat_base = 512usize;
        ram.data[fat_base + 5 * 2..fat_base + 5 * 2 + 2].copy_from_slice(&6u16.to_le_bytes());
        ram.data[fat_base + 6 * 2..fat_base + 6 * 2 + 2].copy_from_slice(&5u16.to_le_bytes());
        let dev: Arc<dyn BlockDevice> = Arc::new(ram);
        let engine = FatEngine::new(dev).expect("engine");
        let ext = engine.chain_extents(5);
        // Loop broken: 5 then 6 then stop (6 → 5 already visited).
        assert!(ext.len() <= 2);
    }

    #[test]
    fn boot_rejects_bad_signature() {
        assert!(FatBoot::parse(&[0u8; 512]).is_err());
    }
}
