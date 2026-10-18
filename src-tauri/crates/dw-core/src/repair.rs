//! Boot-sector repair — the RawFixer pre-scan model (SDR + damaged
//! volumes + lost-partition restoration).
//!
//! Research: `artifacts/PartitionRecovery/rawfixer-fs-repair.md` —
//! the vendor repairs FS metadata IN PLACE (transactionally, every
//! fixer implements Undo) so a structured scan can proceed; only
//! volumes that resist repair fall through to pure RAW carving.
//!
//! Strategy (the evidence-decoded check→fix→update→verify chain):
//!
//! 1. `diagnose`: read the boot sector; a parseable FAT/exFAT/NTFS
//!    BPB is Healthy. Otherwise check the FS's backup boot sector —
//!    FAT32 sector 6, exFAT sector 12 (CRC32 self-verifying, the
//!    only fixer with a checksummed boot sector — `ChecksumDBR`),
//!    NTFS the volume's last sector (the NTFS mirror convention).
//! 2. A valid backup → `FixFromDBR`: plan restoring the primary
//!    from the backup (`UpdateDBR` after `CopySectors` snapshot).
//! 3. No valid backup → `FixByScanSector`: deep-scan the first
//!    256 sectors for a valid boot-sector image with geometry that
//!    fits the device.
//! 4. `apply` writes the planned sectors through a caller-supplied
//!    writer (the platform layer opens a WRITE-ENABLED handle only
//!    for this user-confirmed repair — the scan path stays
//!    read-only); every overwritten sector lands in the undo log.
//! 5. exFAT repairs also re-stamp the boot checksum (sector 11).
//!
//! SDR (SD Card Rescue) = this engine + the volume-targeting flow:
//! "first attempts to restore ACCESS to the card; continues with
//! file recovery when repair is not applicable."

use crate::blockdev::{BlockDevice, BlockRead};
use crate::error::{CoreError, CoreResult};
use crate::fs::{exfat, fat};
use crate::types::FsKind;

/// How deep the boot-sector scan searches (FixByScanSector bound —
/// the vendor's sector-scan fallback; 256 sectors covers every
/// real-world misaligned-image case at trivial cost).
pub const SCAN_SECTORS: u64 = 256;
/// exFAT backup boot sector LBA.
const EXFAT_BACKUP_LBA: u64 = 12;
/// FAT32 backup boot sector LBA.
const FAT32_BACKUP_LBA: u64 = 6;
/// exFAT boot checksum sector LBA (CRC32 of sectors 0..=10).
const EXFAT_CHECKSUM_LBA: u64 = 11;

/// The diagnosis (CRawFixer::CheckPartition's verdict shape).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagnosis {
    /// The boot sector parses; nothing to repair.
    Healthy(FsKind),
    /// The primary is broken but the backup is valid.
    FixableFromBackup { fs: FsKind, backup_lba: u64 },
    /// No valid backup, but a scan found a valid boot image.
    FixableByScan { fs: FsKind, source_lba: u64 },
    /// Nothing repairable — the volume still carves (RAW path).
    CarveOnly,
}

/// A planned repair: the sectors to write, what they replace (the
/// undo log), and a human description (the UI's repair preview).
#[derive(Debug, Clone)]
pub struct RepairPlan {
    pub writes: Vec<(u64, Vec<u8>)>,
    pub undo: Vec<(u64, Vec<u8>)>,
    pub description: String,
    pub fs: FsKind,
}

/// Read one sector.
fn read_sector(dev: &dyn BlockDevice, lba: u64) -> Option<Vec<u8>> {
    let ss = dev.sector_size() as usize;
    let mut buf = vec![0u8; ss];
    dev.read_sectors(lba, &mut buf).ok()?;
    Some(buf)
}

/// Does this 512-byte image parse as a known boot sector with
/// geometry that fits the device?
fn classify_boot(sector: &[u8], dev_sectors: u64) -> Option<FsKind> {
    if sector.len() < 512 {
        return None;
    }
    // FAT family (label variants).
    if let Ok(boot) = fat::FatBoot::parse(sector) {
        let total: u64 = boot.total_sectors;
        if total <= dev_sectors.saturating_add(64) {
            return Some(boot.variant);
        }
    }
    if let Ok(boot) = exfat::ExFatBoot::parse(sector)
        && boot.total_sectors <= dev_sectors.saturating_add(64)
    {
        return Some(FsKind::ExFat);
    }
    // NTFS: OEM + BPB sanity (the engine's full parse needs the
    // volume; here the signature + geometry gates suffice).
    if &sector[3..11] == b"NTFS    " && sector[510] == 0x55 && sector[511] == 0xAA {
        let total = u64::from(u32::from_le_bytes([
            sector[0x28],
            sector[0x29],
            sector[0x2A],
            sector[0x2B],
        ]));
        if total == 0 || total <= dev_sectors.saturating_add(64) {
            return Some(FsKind::Ntfs);
        }
    }
    None
}

/// exFAT boot-checksum verify: sector 11 repeats the CRC32 of
/// sectors 0..=10 (BootChecksum, the `ChecksumDBR` evidence).
fn exfat_boot_checksum_ok(dev: &dyn BlockDevice) -> bool {
    let ss = dev.sector_size() as usize;
    let mut boot_region = vec![0u8; ss * 11];
    if dev.read_bytes(0, &mut boot_region).is_err() {
        return false;
    }
    let Some(ck_sector) = read_sector(dev, EXFAT_CHECKSUM_LBA) else {
        return false;
    };
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&boot_region);
    let crc = hasher.finalize();
    // The checksum sector: the u32 LE CRC repeated to fill.
    let want = crc.to_le_bytes();
    ck_sector
        .as_chunks::<4>()
        .0
        .iter()
        .take(8)
        .all(|c| *c == want)
}

/// Diagnose a volume's boot region.
#[must_use]
pub fn diagnose(dev: &dyn BlockDevice) -> Diagnosis {
    let Some(boot) = read_sector(dev, 0) else {
        return Diagnosis::CarveOnly;
    };
    let dev_sectors = dev.total_sectors();
    if let Some(fs) = classify_boot(&boot, dev_sectors) {
        // exFAT: the checksum gate too (a tampered-but-parseable
        // boot region is not healthy).
        if fs == FsKind::ExFat && !exfat_boot_checksum_ok(dev) {
            // Primary parses but fails the checksum — the backup
            // with a VALID checksum is the repair source.
            if let Some(Diagnosis::FixableFromBackup { fs, backup_lba }) =
                backup_diagnosis(dev, FsKind::ExFat, dev_sectors)
            {
                return Diagnosis::FixableFromBackup { fs, backup_lba };
            }
        }
        return Diagnosis::Healthy(fs);
    }
    // Primary broken: try the FS-specific backups, then the scan.
    for fs in [FsKind::Fat32, FsKind::ExFat, FsKind::Ntfs] {
        if let Some(d) = backup_diagnosis(dev, fs, dev_sectors) {
            return d;
        }
    }
    // FixByScanSector: any lba in 1..SCAN_SECTORS (skip the known
    // backup slots — a broken backup there must not win over a
    // genuine image elsewhere).
    let skip = |lba: u64| {
        lba == FAT32_BACKUP_LBA || (EXFAT_BACKUP_LBA..=EXFAT_CHECKSUM_LBA + 12).contains(&lba)
    };
    for lba in 1..SCAN_SECTORS.min(dev_sectors) {
        if skip(lba) {
            continue;
        }
        if let Some(sector) = read_sector(dev, lba)
            && let Some(fs) = classify_boot(&sector, dev_sectors)
        {
            return Diagnosis::FixableByScan {
                fs,
                source_lba: lba,
            };
        }
    }
    Diagnosis::CarveOnly
}

/// The backup-boot check for one FS family.
fn backup_diagnosis(dev: &dyn BlockDevice, fs: FsKind, dev_sectors: u64) -> Option<Diagnosis> {
    let (backup_lba, expect) = match fs {
        FsKind::Fat32 | FsKind::Fat16 | FsKind::Fat12 => {
            // FAT12/16 carry no backup; FAT32's backup is sector 6.
            (FAT32_BACKUP_LBA, FsKind::Fat32)
        }
        FsKind::ExFat => (EXFAT_BACKUP_LBA, FsKind::ExFat),
        FsKind::Ntfs => (dev_sectors.saturating_sub(1), FsKind::Ntfs),
        _ => return None,
    };
    if backup_lba == 0 {
        return None;
    }
    let sector = read_sector(dev, backup_lba)?;
    let kind = classify_boot(&sector, dev_sectors)?;
    // The backup must be the SAME family we expected (an exFAT
    // image in the NTFS backup slot is a coincidence, not a backup).
    if matches!(kind, FsKind::Fat12 | FsKind::Fat16 | FsKind::Fat32)
        && matches!(expect, FsKind::Fat32)
    {
        return Some(Diagnosis::FixableFromBackup {
            fs: kind,
            backup_lba,
        });
    }
    if kind == expect {
        return Some(Diagnosis::FixableFromBackup {
            fs: kind,
            backup_lba,
        });
    }
    None
}

/// Build the repair plan for a diagnosis (FixFromDBR /
/// FixByScanSector → UpdateDBR with the CopySectors undo snapshot).
///
/// # Errors
/// [`CoreError::Corrupt`] when the diagnosis has nothing to plan.
pub fn plan(dev: &dyn BlockDevice, d: &Diagnosis) -> CoreResult<RepairPlan> {
    let source_lba = match d {
        Diagnosis::Healthy(_) => {
            return Err(CoreError::Corrupt {
                structure: "repair plan",
                detail: "volume is healthy — nothing to repair".into(),
            });
        }
        Diagnosis::FixableFromBackup { backup_lba, .. } => *backup_lba,
        Diagnosis::FixableByScan { source_lba, .. } => *source_lba,
        Diagnosis::CarveOnly => {
            return Err(CoreError::Corrupt {
                structure: "repair plan",
                detail: "no valid boot image found — carve-only volume".into(),
            });
        }
    };
    let fs = match d {
        Diagnosis::FixableFromBackup { fs, .. } | Diagnosis::FixableByScan { fs, .. } => *fs,
        _ => unreachable!("handled above"),
    };
    let Some(boot) = read_sector(dev, source_lba) else {
        return Err(CoreError::Corrupt {
            structure: "repair plan",
            detail: format!("repair source sector {source_lba} unreadable"),
        });
    };
    let Some(undo0) = read_sector(dev, 0) else {
        return Err(CoreError::Corrupt {
            structure: "repair plan",
            detail: "boot sector unreadable".into(),
        });
    };
    let mut writes = vec![(0u64, boot.clone())];
    let mut undo = vec![(0u64, undo0)];
    let mut description = match d {
        Diagnosis::FixableFromBackup { backup_lba, .. } => {
            format!("Restore the boot sector from the backup copy (sector {backup_lba}).")
        }
        Diagnosis::FixableByScan { source_lba, .. } => {
            format!("Restore the boot sector from a valid copy found at sector {source_lba}.")
        }
        _ => String::new(),
    };
    // exFAT: also restore the backup region + re-stamp the checksum
    // (sectors 0..=11 from the backup region at 12..=23).
    if fs == FsKind::ExFat
        && let Diagnosis::FixableFromBackup { .. } = d
    {
        let ss = dev.sector_size() as usize;
        let mut region = vec![0u8; ss * 12];
        if dev
            .read_bytes(EXFAT_BACKUP_LBA * dev.sector_size() as u64, &mut region)
            .is_ok()
        {
            let mut hasher = crc32fast::Hasher::new();
            hasher.update(&region[..ss * 11]);
            let crc = hasher.finalize();
            for (i, b) in crc.to_le_bytes().iter().enumerate() {
                region[ss * 11 + i] = *b;
            }
            for s in 0..12u64 {
                let Some(old) = read_sector(dev, s) else {
                    continue;
                };
                undo.push((s, old));
                writes.push((s, region[s as usize * ss..(s as usize + 1) * ss].to_vec()));
            }
            description.push_str(" The exFAT boot checksum is re-stamped (sector 11).");
        }
    }
    Ok(RepairPlan {
        writes,
        undo,
        description,
        fs,
    })
}

/// Apply a plan through a caller-supplied sector writer (the
/// platform layer's WRITE-ENABLED handle — the scan path never
/// writes). Returns the undo log for [`undo`].
///
/// # Errors
/// Propagates the writer's errors; a failed apply leaves the undo
/// log valid for whatever sectors DID land (partial-repair
/// recovery).
pub fn apply(
    plan: &RepairPlan,
    mut write_sector: impl FnMut(u64, &[u8]) -> std::io::Result<()>,
) -> CoreResult<Vec<(u64, Vec<u8>)>> {
    for (lba, data) in &plan.writes {
        write_sector(*lba, data).map_err(|e| CoreError::ReadFailed {
            offset: *lba,
            len: data.len() as u64,
            source: e,
        })?;
    }
    Ok(plan.undo.clone())
}

/// Roll a repair back.
///
/// # Errors
/// Propagates the writer's errors.
pub fn undo(
    log: &[(u64, Vec<u8>)],
    mut write_sector: impl FnMut(u64, &[u8]) -> std::io::Result<()>,
) -> CoreResult<()> {
    for (lba, data) in log {
        write_sector(*lba, data).map_err(|e| CoreError::ReadFailed {
            offset: *lba,
            len: data.len() as u64,
            source: e,
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::RamDevice;

    /// A minimal valid FAT32 boot sector (the engine's own parser is
    /// the ground truth — reuse its fields).
    fn fat32_boot(total_sectors: u32) -> Vec<u8> {
        let mut s = vec![0u8; 512];
        s[0] = 0xEB;
        s[1] = 0x58;
        s[2] = 0x90;
        s[3..11].copy_from_slice(b"MSDOS5.0");
        // BPB: 512 bps, 8 spc, 32 reserved, 2 fats, 512 root entries
        // → FAT32 by geometry.
        s[11..13].copy_from_slice(&512u16.to_le_bytes());
        s[13] = 8;
        s[14..16].copy_from_slice(&32u16.to_le_bytes());
        s[16] = 2;
        s[17..19].copy_from_slice(&0u16.to_le_bytes()); // root entries 0 → FAT32
        s[19..21].copy_from_slice(&0u16.to_le_bytes()); // total16 0 → use 32
        s[21] = 0xF8;
        // spf16 (0x16) = 0 + spf32 (0x24) > 0 → the FAT32 signature.
        s[0x24..0x28].copy_from_slice(&32u32.to_le_bytes());
        s[24] = 0x29;
        s[54..62].copy_from_slice(b"FAT32   ");
        s[510] = 0x55;
        s[511] = 0xAA;
        // Total sectors 32-bit at 0x20.
        s[0x20..0x24].copy_from_slice(&total_sectors.to_le_bytes());
        s
    }

    fn ram_with_boot(boot: &[u8], total_sectors: u64) -> RamDevice {
        let mut ram = RamDevice::new(total_sectors as usize * 512, 512);
        ram.data[..512].copy_from_slice(boot);
        ram
    }

    #[test]
    fn healthy_volume_diagnoses_clean() {
        let ram = ram_with_boot(&fat32_boot(1024), 1024);
        assert_eq!(diagnose(&ram), Diagnosis::Healthy(FsKind::Fat32));
    }

    #[test]
    fn corrupt_primary_repairs_from_fat32_backup() {
        #[allow(unused_mut)]
        let mut ram = ram_with_boot(&fat32_boot(2048), 2048);
        // Valid backup at sector 6, then trash the primary.
        ram.data[6 * 512..7 * 512].copy_from_slice(&fat32_boot(2048));
        for b in &mut ram.data[..512] {
            *b = 0;
        }
        assert_eq!(
            diagnose(&ram),
            Diagnosis::FixableFromBackup {
                fs: FsKind::Fat32,
                backup_lba: 6
            }
        );
        // Plan + apply through an in-memory writer; the FS parses again.
        let d = diagnose(&ram);
        let plan = plan(&ram, &d).expect("plan");
        assert_eq!(plan.writes.len(), 1);
        assert_eq!(plan.undo.len(), 1);
        let log = apply(&plan, |lba, data| {
            ram.data[lba as usize * 512..lba as usize * 512 + data.len()].copy_from_slice(data);
            Ok(())
        })
        .expect("apply");
        assert_eq!(diagnose(&ram), Diagnosis::Healthy(FsKind::Fat32));
        // Undo restores the corruption (transactionality).
        undo(&log, |lba, data| {
            ram.data[lba as usize * 512..lba as usize * 512 + data.len()].copy_from_slice(data);
            Ok(())
        })
        .expect("undo");
        assert_eq!(
            diagnose(&ram),
            Diagnosis::FixableFromBackup {
                fs: FsKind::Fat32,
                backup_lba: 6
            }
        );
    }

    #[test]
    fn no_backup_falls_back_to_scan() {
        let mut ram = ram_with_boot(&fat32_boot(4096), 4096);
        for b in &mut ram.data[..512] {
            *b = 0;
        }
        // No backup at 6 — but a valid image at sector 32 (a stale
        // copy from a previous format).
        ram.data[32 * 512..33 * 512].copy_from_slice(&fat32_boot(4096));
        assert_eq!(
            diagnose(&ram),
            Diagnosis::FixableByScan {
                fs: FsKind::Fat32,
                source_lba: 32
            }
        );
    }

    #[test]
    fn dead_volume_is_carve_only() {
        let ram = RamDevice::new(1024 * 512, 512);
        assert_eq!(diagnose(&ram), Diagnosis::CarveOnly);
        assert!(plan(&ram, &Diagnosis::CarveOnly).is_err());
    }

    #[test]
    fn healthy_plan_is_rejected() {
        let ram = ram_with_boot(&fat32_boot(1024), 1024);
        assert!(plan(&ram, &Diagnosis::Healthy(FsKind::Fat32)).is_err());
    }

    #[test]
    fn oversized_boot_image_rejected_by_geometry_gate() {
        // A boot image claiming more sectors than the device has is
        // a stale/foreign image, not a repair source.
        let ram = ram_with_boot(&fat32_boot(999_999), 1024);
        assert_eq!(diagnose(&ram), Diagnosis::CarveOnly);
    }
}
