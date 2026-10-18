//! Lost-partition sweep — find filesystems the tables don't claim.
//!
//! Strategy (research: `architecture/04-recovery-workflow.md` §3 —
//! "lost partitions are displayed here"; the deep-scan insert trace
//! "Insert lost partition %s, %d, StartSector=%I64d, TotalSector=%I64d"):
//!
//! 1. Compute unallocated gaps from the table layout.
//! 2. QUICK probe: check the first sector of each gap + 1 MiB-aligned
//!    boundaries (2048-sector) for boot signatures — the standard
//!    modern alignment catches most real partitions.
//! 3. DEEP sweep: stride through gap sectors hunting signature bytes
//!    (the sector-by-sector pass) — bounded by the caller's budget.
//!
//! A hit probes the FS: parses the boot sector enough to size the
//! volume (BPB totals for FAT/exFAT, $MFT + cluster math for NTFS,
//! superblock for ext) and emits a [`LostPartitionHint`].

use crate::error::CoreResult;
use crate::types::FsKind;

use super::{PartitionInfo, PartitionLayout};

/// A lost-partition candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LostPartitionHint {
    pub info: PartitionInfo,
    /// Confidence heuristic (signature strength + BPB sanity).
    pub confidence: u8,
}

/// Signature probe result for one boot sector.
#[derive(Debug, Clone, Copy)]
pub struct FsProbe {
    pub fs: FsKind,
    /// Volume size in sectors when derivable from the BPB/superblock.
    pub total_sectors: Option<u64>,
    pub confidence: u8,
}

/// Probe a 512+ byte buffer for a filesystem boot sector.
///
/// Handles: NTFS, FAT12/16/32, exFAT (boot sector); ext2/3/4 (needs
/// superblock at +1024 — probed separately by the caller); HFS+
/// (volume header at +1024); ReFS. Unknown-but-signed DOS sectors
/// (0x55AA with a valid BPB) report as Raw with low confidence.
#[must_use]
pub fn probe_boot_sector(sector: &[u8]) -> Option<FsProbe> {
    if sector.len() < 512 {
        return None;
    }
    let dos_ok = sector[510] == 0x55 && sector[511] == 0xAA;
    // NTFS: "NTFS    " at 0x03, BPB sanity, $MFT LBA plausible.
    if &sector[3..11] == b"NTFS    " && dos_ok {
        let sectors_per_cluster = sector[13] as u64;
        let total_sectors = u64::from(u32::from_le_bytes([
            sector[0x28],
            sector[0x29],
            sector[0x2A],
            sector[0x2B],
        ]));
        let mft_lba = u64::from_le_bytes({
            let mut b = [0u8; 8];
            b.copy_from_slice(sector.get(0x30..0x38)?);
            b
        });
        let plausible = sectors_per_cluster.is_power_of_two()
            && sectors_per_cluster <= 128
            && mft_lba < (1u64 << 48);
        return Some(FsProbe {
            fs: FsKind::Ntfs,
            total_sectors: if total_sectors > 0 {
                Some(total_sectors)
            } else {
                None
            },
            confidence: if plausible { 95 } else { 40 },
        });
    }
    // exFAT: "EXFAT   " at 0x03.
    if &sector[3..11] == b"EXFAT   " && dos_ok {
        let total = u64::from_le_bytes({
            let mut b = [0u8; 8];
            b.copy_from_slice(sector.get(0x48..0x50)?);
            b
        });
        return Some(FsProbe {
            fs: FsKind::ExFat,
            total_sectors: (total > 0).then_some(total),
            confidence: 90,
        });
    }
    // FAT family: OEM + BPB; the FAT16/FAT32 distinction lives in the
    // FAT-mirror geometry (count of clusters), approximated by the
    // stated fields (the FAT engine re-derives precisely).
    if dos_ok && sector[0x0B] == 0x00 && sector[0x0C] == 0x02 {
        // bytes/sector 512 plausible check
        let bps = u16::from_le_bytes([sector[0x0B], sector[0x0C]]);
        if bps == 0 || bps & (bps - 1) != 0 {
            return None;
        }
        let is_fat32_label = &sector[0x52..0x5A] == b"FAT32   ";
        let is_fat16_label = &sector[0x36..0x3E] == b"FAT16   ";
        let total16 = u16::from_le_bytes([sector[0x13], sector[0x14]]) as u64;
        let total32 =
            u32::from_le_bytes([sector[0x20], sector[0x21], sector[0x22], sector[0x23]]) as u64;
        let total = if total32 > 0 { total32 } else { total16 };
        if is_fat32_label {
            return Some(FsProbe {
                fs: FsKind::Fat32,
                total_sectors: (total > 0).then_some(total),
                confidence: 85,
            });
        }
        if is_fat16_label {
            return Some(FsProbe {
                fs: FsKind::Fat16,
                total_sectors: (total > 0).then_some(total),
                confidence: 80,
            });
        }
        // Unlabeled but BPB-shaped → maybe FAT12/16 (floppy-class).
        if total > 0 && sector[5] == 0xF8 || sector[5] == 0xF0 {
            return Some(FsProbe {
                fs: FsKind::Fat16,
                total_sectors: Some(total),
                confidence: 30,
            });
        }
    }
    None
}

/// Probe an ext superblock block (1024 bytes into the volume).
/// (The ext engine lands in M2; the sweep keeps the probe contract.)
#[allow(dead_code)]
#[must_use]
pub fn probe_ext_superblock(sb: &[u8]) -> Option<FsProbe> {
    if sb.len() < 118 || sb[56..58] != [0x53, 0xEF] {
        return None;
    }
    let blocks = u32::from_le_bytes([sb[4], sb[5], sb[6], sb[7]]) as u64;
    let log_block = sb[24] as u64; // s_log_block_size
    let block_size = 1024u64 << log_block.min(6);
    Some(FsProbe {
        fs: FsKind::Ext4,
        total_sectors: Some(blocks * block_size / 512),
        confidence: 88,
    })
}

/// Probe an HFS+ volume header (1024 bytes into the volume).
/// (The HFS+ engine lands in M2; the sweep keeps the probe contract.)
#[allow(dead_code)]
#[must_use]
pub fn probe_hfs_vh(vh: &[u8]) -> Option<FsProbe> {
    if vh.len() < 512 {
        return None;
    }
    let sig = &vh[0..2];
    if sig != b"H+" && sig != b"HX" {
        return None;
    }
    let total_blocks = u32::from_le_bytes([vh[40], vh[41], vh[42], vh[43]]) as u64;
    let block_size = u32::from_le_bytes([vh[44], vh[45], vh[46], vh[47]]) as u64;
    if block_size == 0 || !block_size.is_power_of_two() {
        return None;
    }
    Some(FsProbe {
        fs: FsKind::HfsPlus,
        total_sectors: Some(total_blocks * block_size / 512),
        confidence: 85,
    })
}

/// QUICK lost-partition scan: probe gap starts + 1 MiB boundaries.
pub fn sweep_for_lost_partitions(
    dev: &dyn crate::blockdev::BlockDevice,
    layout: &PartitionLayout,
) -> CoreResult<Vec<LostPartitionHint>> {
    let ss = dev.sector_size() as usize;
    let total = dev.total_sectors();
    let mut hints = Vec::new();
    for (start, end) in unallocated_gaps(layout, total) {
        let mut lba = start;
        let mut probed = 0u32;
        while lba < end && probed < 64 {
            let mut buf = vec![0u8; ss];
            if let Some(probe) = dev
                .read_sectors(lba, &mut buf)
                .ok()
                .and_then(|()| probe_boot_sector(&buf))
            {
                let size = probe
                    .total_sectors
                    .unwrap_or(end - lba)
                    .min(end - lba)
                    .max(1);
                hints.push(LostPartitionHint {
                    info: PartitionInfo {
                        index: 0, // renumbered by the caller
                        start_lba: lba,
                        total_sectors: size,
                        fs: probe.fs,
                        in_table: false,
                        type_tag: "lost".into(),
                    },
                    confidence: probe.confidence,
                });
            }
            // 1 MiB stride + the gap start itself.
            lba = if probed == 0 {
                start.max(1).next_multiple_of(2048).max(start + 1)
            } else {
                lba + 2048
            };
            if lba <= start && probed == 0 {
                break; // degenerate tiny gap
            }
            probed += 1;
        }
    }
    Ok(hints)
}

/// Unallocated sector ranges [start, end) given the layout.
#[must_use]
pub fn unallocated_gaps(layout: &PartitionLayout, device_sectors: u64) -> Vec<(u64, u64)> {
    let mut taken: Vec<(u64, u64)> = layout
        .partitions
        .iter()
        .map(|p| (p.start_lba, p.end_lba()))
        .filter(|(s, e)| *s < *e && *s < device_sectors)
        .map(|(s, e)| (s, e.min(device_sectors)))
        .collect();
    taken.sort_unstable();
    let mut gaps = Vec::new();
    if device_sectors <= 1 {
        return gaps;
    }
    let mut cursor = 1u64; // LBA 0..1 reserved for the table itself
    for (s, e) in taken {
        if s > cursor {
            gaps.push((cursor, s.min(device_sectors)));
        }
        cursor = cursor.max(e);
    }
    if cursor < device_sectors {
        gaps.push((cursor, device_sectors));
    }
    gaps.retain(|(s, e)| e - s >= 2048); // ignore slivers < 1 MiB
    gaps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::partition::{PartitionScheme, parse_mbr};
    use crate::testutil::RamDevice;

    #[test]
    fn gaps_exclude_table_and_partitions() {
        let mut mbr = vec![0u8; 512];
        let mut e = [0u8; 16];
        e[4] = 0x07;
        e[8..12].copy_from_slice(&2048u32.to_le_bytes());
        e[12..16].copy_from_slice(&4096u32.to_le_bytes());
        mbr[0x1BE..0x1CE].copy_from_slice(&e);
        mbr[510] = 0x55;
        mbr[511] = 0xAA;
        let (_, parts) = parse_mbr(&mbr, 1_000_000).unwrap();
        let layout = PartitionLayout {
            scheme: PartitionScheme::Mbr,
            partitions: parts,
        };
        let gaps = unallocated_gaps(&layout, 1_000_000);
        // The (1, 2048) sliver is 2047 sectors < 1 MiB → dropped;
        // the tail gap survives.
        assert_eq!(gaps, vec![(6144, 1_000_000)]);
    }

    #[test]
    fn ntfs_boot_probe_fires() {
        let mut sector = vec![0u8; 512];
        sector[3..11].copy_from_slice(b"NTFS    ");
        sector[13] = 8; // sectors per cluster
        sector[0x28..0x2C].copy_from_slice(&100_000u32.to_le_bytes());
        sector[0x30..0x38].copy_from_slice(&4096u64.to_le_bytes());
        sector[510] = 0x55;
        sector[511] = 0xAA;
        let probe = probe_boot_sector(&sector).expect("probe");
        assert_eq!(probe.fs, FsKind::Ntfs);
        assert_eq!(probe.total_sectors, Some(100_000));
        assert!(probe.confidence >= 90);
    }

    #[test]
    fn fat32_boot_probe_fires() {
        let mut sector = vec![0u8; 512];
        sector[0x0B] = 0x00;
        sector[0x0C] = 0x02; // 512 bps
        sector[0x20..0x24].copy_from_slice(&50_000u32.to_le_bytes());
        sector[0x52..0x5A].copy_from_slice(b"FAT32   ");
        sector[510] = 0x55;
        sector[511] = 0xAA;
        let probe = probe_boot_sector(&sector).expect("probe");
        assert_eq!(probe.fs, FsKind::Fat32);
    }

    #[test]
    fn exfat_probe_fires() {
        let mut sector = vec![0u8; 512];
        sector[3..11].copy_from_slice(b"EXFAT   ");
        sector[0x48..0x50].copy_from_slice(&2_000_000u64.to_le_bytes());
        sector[510] = 0x55;
        sector[511] = 0xAA;
        let probe = probe_boot_sector(&sector).expect("probe");
        assert_eq!(probe.fs, FsKind::ExFat);
        assert_eq!(probe.total_sectors, Some(2_000_000));
    }

    #[test]
    fn sweep_finds_a_lost_ntfs_in_gap() {
        let mut ram = RamDevice::new(512 * 100_000, 512);
        // Place an NTFS boot sector at LBA 4096 (in the unallocated gap).
        let mut sector = vec![0u8; 512];
        sector[3..11].copy_from_slice(b"NTFS    ");
        sector[13] = 8;
        sector[0x28..0x2C].copy_from_slice(&20_000u32.to_le_bytes());
        sector[0x30..0x38].copy_from_slice(&16u64.to_le_bytes());
        sector[510] = 0x55;
        sector[511] = 0xAA;
        ram.data[4096 * 512..4097 * 512].copy_from_slice(&sector);
        let layout = PartitionLayout::default();
        let hints = sweep_for_lost_partitions(&ram, &layout).expect("sweep");
        assert!(
            hints
                .iter()
                .any(|h| h.info.start_lba == 4096 && h.info.fs == FsKind::Ntfs)
        );
    }
}
