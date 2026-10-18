//! The exFAT engine — entry-set walk (0x85 File + 0xC0 Stream +
//! 0xC1 Name), FAT chains, deleted sets.
//!
//! Algorithm (research: `artifacts/SearchDLL/exfat-engine--deep-dive.md`
//! + `fat-exfat-engine-algorithms-v2.md` — "InUse-mask triples"):
//!
//! 1. Boot region: `EXFAT` signature, shift-encoded geometry
//!    (BytesPerSectorShift / SectorsPerClusterShift), FAT offset /
//!    length, cluster-heap offset, root cluster.
//! 2. Directory walk from the root cluster chain: 32-byte entries;
//!    allocation = EntryType bit 7. A File Set = primary 0x85 (attrs,
//!    timestamps) + secondaries: 0xC0 Stream (name len/hash, valid
//!    size, first cluster, data length) + N × 0xC1 Name (15 UTF-16
//!    chars each).
//! 3. Deleted sets (bit 7 clear on the 0x85: type 0x05) still parse —
//!    the InUse triples (name hash + length + cluster) re-validate
//!    them; chains re-walked where the FAT survives.
//! 4. FAT: u32 entries masked 0x0FFFFFFF; BAD 0x0FFFFFF7; EOC ≥
//!    0x0FFFFFF8; cycle-guarded chain walks.

use std::collections::HashSet;
use std::sync::Arc;

use crate::blockdev::{BlockDevice, BlockRead};
use crate::error::{CoreError, CoreResult};
use crate::fs::{ScanControl, ScanEngine, ScanSink, ScanStats};
use crate::types::{Extent, FileAttrs, FoundEntry, FsKind, NodeOrigin, Timestamps};

/// Entry type codes (allocated forms; bit 7 = allocation).
const ET_FILE: u8 = 0x85;
const ET_STREAM: u8 = 0xC0;
const ET_NAME: u8 = 0xC1;
const ET_ALLOC: u8 = 0x80;

/// The parsed exFAT boot region.
#[derive(Debug, Clone, Copy)]
pub struct ExFatBoot {
    pub bytes_per_sector: u32,
    pub sectors_per_cluster: u32,
    pub total_sectors: u64,
    pub fat_offset: u32,
    pub fat_length: u32,
    pub cluster_heap_offset: u32,
    pub cluster_count: u32,
    pub root_cluster: u32,
}

impl ExFatBoot {
    /// Parse from the boot sector.
    ///
    /// # Errors
    /// [`CoreError::Corrupt`] on signature/geometry failure.
    pub fn parse(sector: &[u8]) -> CoreResult<Self> {
        if sector.len() < 512 || &sector[3..11] != b"EXFAT   " {
            return Err(CoreError::Corrupt {
                structure: "exFAT boot",
                detail: "missing EXFAT signature".into(),
            });
        }
        let volume_length = u64::from(u32::from_le_bytes([
            sector[0x0F],
            sector[0x10],
            sector[0x11],
            sector[0x12],
        ]));
        let fat_offset =
            u32::from_le_bytes([sector[0x13], sector[0x14], sector[0x15], sector[0x16]]);
        let fat_length =
            u32::from_le_bytes([sector[0x17], sector[0x18], sector[0x19], sector[0x1A]]);
        let heap = u32::from_le_bytes([sector[0x1B], sector[0x1C], sector[0x1D], sector[0x1E]]);
        let cluster_count =
            u32::from_le_bytes([sector[0x1F], sector[0x20], sector[0x21], sector[0x22]]);
        let root_cluster =
            u32::from_le_bytes([sector[0x23], sector[0x24], sector[0x25], sector[0x26]]);
        let bps_shift = sector[0x2F];
        let spc_shift = sector[0x30];
        if !(9..=12).contains(&bps_shift) || spc_shift > 25 - bps_shift {
            return Err(CoreError::Corrupt {
                structure: "exFAT BPB",
                detail: format!("bad shifts bps={bps_shift} spc={spc_shift}"),
            });
        }
        let bytes_per_sector = 1u32 << bps_shift;
        let sectors_per_cluster = 1u32 << spc_shift;
        if fat_offset == 0 || fat_length == 0 || cluster_count < 2 || heap == 0 {
            return Err(CoreError::Corrupt {
                structure: "exFAT BPB",
                detail: "missing layout fields".into(),
            });
        }
        Ok(Self {
            bytes_per_sector,
            sectors_per_cluster,
            total_sectors: volume_length,
            fat_offset,
            fat_length,
            cluster_heap_offset: heap,
            cluster_count,
            root_cluster,
        })
    }

    #[must_use]
    pub fn cluster_size(&self) -> u64 {
        u64::from(self.bytes_per_sector) * u64::from(self.sectors_per_cluster)
    }

    /// Byte offset of cluster `n` (n ≥ 2).
    #[must_use]
    pub fn cluster_offset(&self, n: u32) -> u64 {
        (u64::from(self.cluster_heap_offset)
            + u64::from(n.saturating_sub(2)) * u64::from(self.sectors_per_cluster))
            * u64::from(self.bytes_per_sector)
    }
}

/// One assembled entry set (file or directory).
struct EntrySet {
    name: String,
    is_dir: bool,
    attrs: u16,
    times: Timestamps,
    first_cluster: u32,
    data_len: u64,
    valid_len: u64,
    allocated: bool,
}

/// The engine.
pub struct ExFatEngine {
    dev: Arc<dyn BlockDevice>,
    boot: ExFatBoot,
    fat: Vec<u8>,
}

impl ExFatEngine {
    /// Open + parse + load the FAT.
    ///
    /// # Errors
    /// [`CoreError::Corrupt`] when the volume isn't parseable exFAT.
    pub fn new(dev: Arc<dyn BlockDevice>) -> CoreResult<Self> {
        let sector = dev.read_struct_at::<512>(0)?;
        let boot = ExFatBoot::parse(&sector)?;
        let fat_bytes = u64::from(boot.fat_length) * u64::from(boot.bytes_per_sector);
        let mut fat = vec![0u8; fat_bytes as usize];
        dev.read_bytes(
            u64::from(boot.fat_offset) * u64::from(boot.bytes_per_sector),
            &mut fat,
        )?;
        Ok(Self { dev, boot, fat })
    }

    #[must_use]
    fn fat_entry(n: u32, fat: &[u8]) -> u32 {
        let off = n as usize * 4;
        if off + 4 > fat.len() {
            return 0xFFFF_FFFF;
        }
        u32::from_le_bytes([fat[off], fat[off + 1], fat[off + 2], fat[off + 3]]) & 0x0FFF_FFFF
    }

    fn chain_extents(&self, first: u32) -> Vec<Extent> {
        let mut out: Vec<Extent> = Vec::new();
        let mut visited = HashSet::new();
        let cs = self.boot.cluster_size();
        let mut cluster = first;
        while (2..0x0FFF_FFF7).contains(&cluster)
            && cluster <= self.boot.cluster_count.saturating_add(1)
            && visited.insert(cluster)
        {
            let off = self.boot.cluster_offset(cluster);
            match out.last_mut() {
                Some(last) if last.end() == off => last.len += cs,
                _ => out.push(Extent::new(off, cs)),
            }
            let next = Self::fat_entry(cluster, &self.fat);
            if next >= 0x0FFF_FFF8 {
                break;
            }
            cluster = next;
        }
        out
    }

    /// Read one directory chain's bytes (bounded).
    fn read_dir_data(&self, first: u32) -> Vec<u8> {
        let extents = self.chain_extents(first);
        let total: u64 = extents.iter().map(|e| e.len).sum();
        if total == 0 || total > 16 * 1024 * 1024 {
            return Vec::new();
        }
        let mut buf = vec![0u8; total as usize];
        let mut pos = 0usize;
        for e in extents {
            let _ = self
                .dev
                .read_bytes(e.offset, &mut buf[pos..pos + e.len as usize]);
            pos += e.len as usize;
        }
        buf
    }

    /// Parse raw directory bytes into assembled entry sets.
    fn parse_dir(data: &[u8]) -> Vec<EntrySet> {
        let mut out: Vec<EntrySet> = Vec::new();
        let mut i = 0usize;
        while i + 32 <= data.len() {
            let et = data[i];
            if et == 0x00 {
                i += 32;
                continue; // never-used slot
            }
            let base_type = et & 0x7F;
            if base_type != ET_FILE & 0x7F {
                // Volume label (0x83/0x03), bitmap (0x81/0x01),
                // upcase (0x82/0x02) — skip one entry.
                i += 32;
                continue;
            }
            // File set: 0x85 (or deleted 0x05) + secondaries.
            let allocated = et & ET_ALLOC != 0;
            let secondary_count = data[i + 1] as usize;
            let attrs = u16::from_le_bytes([data[i + 8], data[i + 9]]);
            // Timestamps: create/modify/access as (date<<16)|time u32s.
            let t = |off: usize| {
                let v = u32::from_le_bytes([
                    data[i + off],
                    data[i + off + 1],
                    data[i + off + 2],
                    data[i + off + 3],
                ]);
                crate::fs::fat::fat_datetime((v >> 16) as u16, (v & 0xFFFF) as u16)
            };
            let times = Timestamps {
                created: Some(t(0x0C)),
                modified: Some(t(0x10)),
                accessed: Some(t(0x14)),
                changed: None,
                deleted: None,
            };
            // Walk secondaries.
            let mut name = String::new();
            let mut first_cluster = 0u32;
            let mut data_len = 0u64;
            let mut valid_len = 0u64;
            let mut set_len = 1 + secondary_count;
            for s in 1..=secondary_count {
                let soff = i + s * 32;
                if soff + 32 > data.len() {
                    set_len = s;
                    break;
                }
                let st = data[soff];
                match st & 0x7F {
                    x if x == ET_STREAM & 0x7F => {
                        let name_len = data[soff + 3] as usize;
                        let _ = name_len;
                        valid_len = u64::from_le_bytes({
                            let mut b = [0u8; 8];
                            b.copy_from_slice(&data[soff + 0x08..soff + 0x10]);
                            b
                        });
                        first_cluster = u32::from_le_bytes([
                            data[soff + 0x14],
                            data[soff + 0x15],
                            data[soff + 0x16],
                            data[soff + 0x17],
                        ]);
                        data_len = u64::from_le_bytes({
                            let mut b = [0u8; 8];
                            b.copy_from_slice(&data[soff + 0x18..soff + 0x20]);
                            b
                        });
                    }
                    x if x == ET_NAME & 0x7F => {
                        for c in data[soff + 2..soff + 32].chunks_exact(2) {
                            let u = u16::from_le_bytes([c[0], c[1]]);
                            if u == 0 {
                                break;
                            }
                            name.push(char::from_u32(u32::from(u)).unwrap_or('\u{FFFD}'));
                        }
                    }
                    _ => {}
                }
            }
            if !name.is_empty() && name != "." && name != ".." {
                out.push(EntrySet {
                    name,
                    is_dir: attrs & 0x0010 != 0,
                    attrs,
                    times,
                    first_cluster,
                    data_len,
                    valid_len,
                    allocated,
                });
            }
            i += set_len * 32;
        }
        out
    }

    fn walk_dir(
        &self,
        cluster: u32,
        path: &str,
        sink: &mut dyn ScanSink,
        ctrl: &ScanControl,
        stats: &mut ScanStats,
        depth: usize,
    ) -> CoreResult<()> {
        if depth > 32 {
            return Ok(());
        }
        ctrl.checkpoint()?;
        if cluster < 2 {
            return Ok(());
        }
        let data = self.read_dir_data(cluster);
        for e in Self::parse_dir(&data) {
            let child_path = if path.is_empty() {
                e.name.clone()
            } else {
                format!("{path}\\{}", e.name)
            };
            let origin = if e.allocated {
                NodeOrigin::Existing
            } else {
                NodeOrigin::Deleted
            };
            let extents = self.chain_extents(e.first_cluster);
            let mut entry = FoundEntry::structured(e.name.clone(), e.is_dir, FsKind::ExFat);
            entry.parent_hint = Some(path.to_string());
            entry.attrs = FileAttrs(u32::from(e.attrs));
            entry.timestamps = e.times;
            entry.origin = origin;
            entry.size = if e.is_dir {
                0
            } else {
                e.data_len.min(e.valid_len.max(e.data_len))
            };
            entry.extents = if e.is_dir { Vec::new() } else { extents };
            entry.confidence = if e.allocated { 100 } else { 75 };
            stats.entries += u64::from(!e.is_dir);
            stats.bytes += entry.size;
            sink.on_entry(entry);
            if e.is_dir {
                self.walk_dir(e.first_cluster, &child_path, sink, ctrl, stats, depth + 1)?;
            }
        }
        Ok(())
    }
}

impl ScanEngine for ExFatEngine {
    fn kind(&self) -> FsKind {
        FsKind::ExFat
    }

    fn quick_scan(&mut self, sink: &mut dyn ScanSink, ctrl: &ScanControl) -> CoreResult<ScanStats> {
        let mut stats = ScanStats::default();
        self.walk_dir(self.boot.root_cluster, "", sink, ctrl, &mut stats, 0)?;
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::RamDevice;

    /// Build a tiny exFAT image: 512 B sectors, 1 sector/cluster,
    /// FAT at sector 1 (1 sector), heap at sector 3, root = cluster 2,
    /// one file "notes.txt" (cluster 5, 40 bytes) + a deleted file.
    fn exfat_image() -> RamDevice {
        let mut s = vec![0u8; 512];
        s[..3].copy_from_slice(b"\xEB\x76\x90");
        s[3..11].copy_from_slice(b"EXFAT   ");
        s[0x0B..0x0F].copy_from_slice(&0u32.to_le_bytes()); // partition offset
        s[0x0F..0x13].copy_from_slice(&64u32.to_le_bytes()); // total sectors
        s[0x13..0x17].copy_from_slice(&1u32.to_le_bytes()); // FAT offset
        s[0x17..0x1B].copy_from_slice(&1u32.to_le_bytes()); // FAT length
        s[0x1B..0x1F].copy_from_slice(&3u32.to_le_bytes()); // heap offset
        s[0x1F..0x23].copy_from_slice(&10u32.to_le_bytes()); // cluster count
        s[0x23..0x27].copy_from_slice(&2u32.to_le_bytes()); // root cluster
        s[0x2F] = 9; // 512 bytes/sector
        s[0x30] = 0; // 1 sector/cluster
        s[510] = 0x55;
        s[511] = 0xAA;
        let mut ram = RamDevice::new(512 * 64, 512);
        ram.data[..512].copy_from_slice(&s);
        // FAT at sector 1.
        let set_fat = |ram: &mut RamDevice, n: u32, v: u32| {
            let off = 512 + n as usize * 4;
            ram.data[off..off + 4].copy_from_slice(&v.to_le_bytes());
        };
        set_fat(&mut ram, 0, 0xFFFFFFF8);
        set_fat(&mut ram, 1, 0xFFFFFFFF);
        set_fat(&mut ram, 2, 0xFFFFFFFF); // root dir: cluster 2
        set_fat(&mut ram, 5, 0xFFFFFFFF); // notes.txt: cluster 5
        set_fat(&mut ram, 6, 0xFFFFFFFF); // deleted.bin: cluster 6
                                          // Root dir at cluster 2 → sector 3.
        let root = 3 * 512;
        // notes.txt: File set = 0x85 (1 secondary) + 0xC0 + 0xC1.
        let name_u16: Vec<u16> = "notes.txt".encode_utf16().collect();
        let mut set = Vec::new();
        let mut f = [0u8; 32];
        f[0] = 0x85;
        f[1] = 2; // 2 secondaries
        f[8..10].copy_from_slice(&0x20u16.to_le_bytes()); // archive
        f[0x0C..0x10].copy_from_slice(
            &((((2026 - 1980) as u32) << 25) | (10 << 21) | (17 << 16)).to_le_bytes(),
        );
        set.extend_from_slice(&f);
        let mut st = [0u8; 32];
        st[0] = 0xC0;
        st[1] = 1; // allocation possible
        st[3] = name_u16.len() as u8;
        st[8..16].copy_from_slice(&40u64.to_le_bytes()); // valid size
        st[0x14..0x18].copy_from_slice(&5u32.to_le_bytes()); // first cluster
        st[0x18..0x20].copy_from_slice(&40u64.to_le_bytes()); // data length
        set.extend_from_slice(&st);
        let mut nm = [0u8; 32];
        nm[0] = 0xC1;
        for (i, u) in name_u16.iter().enumerate() {
            nm[2 + i * 2..4 + i * 2].copy_from_slice(&u.to_le_bytes());
        }
        set.extend_from_slice(&nm);
        // Deleted file "gone.tmp": 0x05 primary (bit 7 clear) + set.
        let gname: Vec<u16> = "gone.tmp".encode_utf16().collect();
        let mut f = [0u8; 32];
        f[0] = 0x05; // deleted File
        f[1] = 2;
        f[8..10].copy_from_slice(&0x20u16.to_le_bytes());
        set.extend_from_slice(&f);
        let mut st = [0u8; 32];
        st[0] = 0x40; // deleted stream
        st[3] = gname.len() as u8;
        st[8..16].copy_from_slice(&7u64.to_le_bytes());
        st[0x14..0x18].copy_from_slice(&6u32.to_le_bytes());
        st[0x18..0x20].copy_from_slice(&7u64.to_le_bytes());
        set.extend_from_slice(&st);
        let mut nm = [0u8; 32];
        nm[0] = 0x41; // deleted name
        for (i, u) in gname.iter().enumerate() {
            nm[2 + i * 2..4 + i * 2].copy_from_slice(&u.to_le_bytes());
        }
        set.extend_from_slice(&nm);
        ram.data[root..root + set.len()].copy_from_slice(&set);
        ram
    }

    #[test]
    fn exfat_walk_finds_live_and_deleted() {
        let ram = exfat_image();
        let dev: Arc<dyn BlockDevice> = Arc::new(ram);
        let mut engine = ExFatEngine::new(dev).expect("engine");
        struct Collector(Vec<FoundEntry>);
        impl crate::fs::ScanSink for Collector {
            fn on_entry(&mut self, e: FoundEntry) {
                self.0.push(e);
            }
        }
        let mut sink = Collector(Vec::new());
        let ctrl = ScanControl::new();
        let stats = engine.quick_scan(&mut sink, &ctrl).expect("scan");
        assert!(stats.entries >= 2);
        let notes = sink
            .0
            .iter()
            .find(|e| e.name == "notes.txt")
            .expect("notes");
        assert_eq!(notes.origin, NodeOrigin::Existing);
        assert_eq!(notes.size, 40);
        assert_eq!(notes.extents.len(), 1);
        // Cluster 5 = heap sector 3 + 3 = sector 6.
        assert_eq!(notes.extents[0].offset, 6 * 512);
        let gone = sink.0.iter().find(|e| e.name == "gone.tmp").expect("gone");
        assert_eq!(gone.origin, NodeOrigin::Deleted);
        assert_eq!(gone.size, 7);
        assert_eq!(gone.confidence, 75);
    }

    #[test]
    fn boot_rejects_bad_signature() {
        assert!(ExFatBoot::parse(&[0u8; 512]).is_err());
        let mut s = [0u8; 512];
        s[3..11].copy_from_slice(b"EXFAT   ");
        s[0x2F] = 9;
        assert!(ExFatBoot::parse(&s).is_err()); // missing layout
    }
}
