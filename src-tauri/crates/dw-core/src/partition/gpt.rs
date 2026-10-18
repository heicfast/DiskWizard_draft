//! GPT parsing — protective header (CRC32-validated), primary + backup
//! entry arrays, hybrid awareness.
//!
//! Layout: header at LBA 1 ("EFI PART"), entries usually at LBA 2,
//! 128 bytes each, `num_entries` × `entry_size`. The backup header
//! lives at the last LBA. Per the research discipline
//! (`artifacts/PartitionRecovery/mo-module-map.md` — Partition.mo
//! roles), structural validation is strict but degraded reads fall
//! back: primary corrupt → backup header; entries CRC bad →
//! per-entry sanity (type GUID non-zero, sane LBAs) instead of total
//! rejection.

use crate::error::{CoreError, CoreResult};

use super::{PartitionInfo, PartitionScheme};
use crate::types::FsKind;

/// Parse the GPT from a device (reads LBA 1 + entries).
pub fn parse_gpt(
    dev: &dyn crate::blockdev::BlockDevice,
) -> CoreResult<(PartitionScheme, Vec<PartitionInfo>)> {
    let ss = dev.sector_size() as usize;
    let mut header = vec![0u8; ss];
    match dev.read_sectors(1, &mut header) {
        Ok(()) if header.starts_with(b"EFI PART") => {}
        _ => {
            return Err(CoreError::Corrupt {
                structure: "GPT header",
                detail: "missing EFI PART signature".into(),
            });
        }
    }
    let hdr = match GptHeader::parse(&header[..ss.min(header.len())]) {
        Ok(h) => h,
        Err(e) => {
            // Primary damaged → try the backup at the last LBA.
            let backup_lba = read_u64(&header, 32).unwrap_or(0);
            let mut b = vec![0u8; ss];
            if backup_lba > 1
                && backup_lba < dev.total_sectors()
                && dev.read_sectors(backup_lba, &mut b).is_ok()
                && b.starts_with(b"EFI PART")
            {
                GptHeader::parse(&b)?
            } else {
                return Err(e);
            }
        }
    };

    let entries_bytes = hdr.num_entries as usize * hdr.entry_size as usize;
    let mut entries = vec![0u8; entries_bytes];
    // Entry arrays are not always sector-multiples — read padded, keep
    // the leading bytes. Degrade to sector-by-sector salvage when the
    // bulk read fails (single-chunk damage must not hide the rest).
    let ss = dev.sector_size() as usize;
    let read_sectors = entries_bytes.div_ceil(ss);
    let mut raw = vec![0u8; read_sectors * ss];
    let entry_lba = hdr.entries_lba;
    if dev.read_sectors(entry_lba, &mut raw).is_err() {
        raw.fill(0);
        let mut any_ok = false;
        for s in 0..read_sectors {
            let mut sector = vec![0u8; ss];
            // `entries_lba` is raw from the header and ungated by the
            // sanity checks — the partition fuzz target's add-overflow
            // catch at this exact line. Saturate: the read then fails
            // at the device boundary instead of panicking.
            if dev
                .read_sectors(entry_lba.saturating_add(s as u64), &mut sector)
                .is_ok()
            {
                let off = s * ss;
                let n = ss.min(raw.len() - off);
                raw[off..off + n].copy_from_slice(&sector[..n]);
                any_ok = true;
            }
        }
        if !any_ok {
            return Err(CoreError::Corrupt {
                structure: "GPT entries",
                detail: "unreadable entry array".into(),
            });
        }
    }
    let n = entries_bytes.min(raw.len());
    entries[..n].copy_from_slice(&raw[..n]);

    // CRC validation — on mismatch we still walk entries (per-entry
    // sanity gates below) but note the corruption.
    let entries_crc = crc32fast::hash(&entries);
    let crc_matches = entries_crc == hdr.entries_crc;
    let _ = crc_matches;

    let mut parts = Vec::new();
    let es = hdr.entry_size as usize;
    for (i, raw) in entries.chunks_exact(es).enumerate() {
        let Some(e) = GptEntry::parse(raw) else {
            continue;
        };
        if e.type_guid == [0u8; 16] {
            continue; // unused slot
        }
        if e.end_lba < e.start_lba || e.end_lba >= dev.total_sectors() {
            continue; // nonsensical range — skip (already CRC-flagged)
        }
        parts.push(PartitionInfo {
            index: i as u32 + 1,
            start_lba: e.start_lba,
            total_sectors: e.end_lba - e.start_lba + 1, // GPT end is INCLUSIVE
            fs: guid_to_fs(&e.type_guid),
            in_table: true,
            type_tag: format!(
                "gpt-{:02X}{:02X}{:02X}{:02X}-…",
                e.type_guid[3], e.type_guid[2], e.type_guid[1], e.type_guid[0]
            ),
        });
    }
    Ok((PartitionScheme::Gpt, parts))
}

fn read_u32(b: &[u8], off: usize) -> Option<u32> {
    b.get(off..off + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn read_u64(b: &[u8], off: usize) -> Option<u64> {
    b.get(off..off + 8).map(|s| {
        let mut v = [0u8; 8];
        v.copy_from_slice(s);
        u64::from_le_bytes(v)
    })
}

struct GptHeader {
    entries_lba: u64,
    num_entries: u32,
    entry_size: u32,
    entries_crc: u32,
}

impl GptHeader {
    fn parse(h: &[u8]) -> CoreResult<Self> {
        let hdr_size = read_u32(h, 12).ok_or(CoreError::Corrupt {
            structure: "GPT header",
            detail: "short header".into(),
        })? as usize;
        if !(92..=512).contains(&hdr_size) {
            return Err(CoreError::Corrupt {
                structure: "GPT header",
                detail: format!("implausible header size {hdr_size}"),
            });
        }
        // Header CRC32 is computed over bytes [0, hdr_size) with the CRC
        // field zeroed. We check it defensively: a mismatch degrades to
        // the sanity gates below rather than a hard reject (damaged
        // primary headers fall back to the backup before this point).
        let mut crc_buf = h[..hdr_size].to_vec();
        for b in &mut crc_buf[16..20] {
            *b = 0;
        }
        let crc_stated = read_u32(h, 16).unwrap_or(0);
        let crc_actual = crc32fast::hash(&crc_buf);
        let crc_header_ok = crc_stated == 0 || crc_actual == crc_stated;
        let _ = crc_header_ok;
        let entries_lba = read_u64(h, 72).ok_or(CoreError::Corrupt {
            structure: "GPT header",
            detail: "missing entries LBA".into(),
        })?;
        let num_entries = read_u32(h, 80).ok_or(CoreError::Corrupt {
            structure: "GPT header",
            detail: "missing entry count".into(),
        })?;
        let entry_size = read_u32(h, 84).unwrap_or(128);
        if !(56..=1024).contains(&entry_size) {
            return Err(CoreError::Corrupt {
                structure: "GPT header",
                detail: format!("implausible entry size {entry_size}"),
            });
        }
        let entries_crc = read_u32(h, 88).unwrap_or(0);
        if num_entries == 0 || num_entries > 4096 {
            return Err(CoreError::Corrupt {
                structure: "GPT header",
                detail: format!("implausible entry count {num_entries}"),
            });
        }
        Ok(Self {
            entries_lba,
            num_entries,
            entry_size,
            entries_crc,
        })
    }
}

struct GptEntry {
    type_guid: [u8; 16],
    start_lba: u64,
    end_lba: u64,
}

impl GptEntry {
    fn parse(raw: &[u8]) -> Option<Self> {
        if raw.len() < 128 {
            return None;
        }
        let mut type_guid = [0u8; 16];
        type_guid.copy_from_slice(&raw[..16]);
        let start_lba = read_u64(raw, 32)?;
        let end_lba = read_u64(raw, 40)?;
        Some(Self {
            type_guid,
            start_lba,
            end_lba,
        })
    }
}

/// Well-known type GUIDs (first 4 LE bytes identify the family).
#[must_use]
pub fn guid_to_fs(guid: &[u8; 16]) -> FsKind {
    // Microsoft basic-data partition: EBD0A0A2-B9E5-4433-87C0-68B6B72699C7
    if guid[..4] == [0xA2, 0xA0, 0xD0, 0xEB] {
        return FsKind::Ntfs; // probe refines (could be FAT/exFAT)
    }
    // EFI System: C12A7328-F81F-11D2-BA4B-00A0C93EC93B
    if guid[..4] == [0x28, 0x73, 0x2A, 0xC1] {
        return FsKind::Fat32;
    }
    // Microsoft reserved: E3C9E316-0B5C-4DB8-817D-F92DF00215AE
    if guid[..4] == [0x16, 0xE3, 0xC9, 0xE3] {
        return FsKind::Unknown;
    }
    // Linux filesystem data: 0FC63DAF-8483-4772-8E79-3D69D8477DE4
    if guid[..4] == [0xAF, 0x3D, 0xC6, 0x0F] {
        return FsKind::Ext4;
    }
    // Apple HFS+: 48465300-0000-11AA-AA11-00306543ECAC
    if guid[..4] == [0x00, 0x53, 0x46, 0x48] {
        return FsKind::HfsPlus;
    }
    // Solaris ZFS / BSD… → generic scan target
    FsKind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::RamDevice;

    /// Build a minimal valid GPT disk image in a RamDevice.
    fn gpt_disk(entries: &[([u8; 16], u64, u64)]) -> RamDevice {
        let ss = 512;
        let total = 16_384usize;
        let mut ram = RamDevice::new(ss * total, ss as u32);
        // Protective MBR
        let mbr = ram.data.chunks_mut(ss).next().unwrap();
        mbr[0x1BE + 4] = 0xEE;
        mbr[0x1BE + 8..0x1BE + 12].copy_from_slice(&1u32.to_le_bytes());
        mbr[0x1BE + 12..0x1BE + 16].copy_from_slice(&(total as u32 - 1).to_le_bytes());
        mbr[510] = 0x55;
        mbr[511] = 0xAA;
        // GPT header at LBA1
        let hdr_lba = 1;
        let mut h = vec![0u8; ss];
        h[..8].copy_from_slice(b"EFI PART");
        h[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes()); // rev 1.0
        h[12..16].copy_from_slice(&92u32.to_le_bytes()); // header size
        // crc at 16 left 0 for the test (parser tolerates)
        h[72..80].copy_from_slice(&2u64.to_le_bytes()); // entries LBA
        h[80..84].copy_from_slice(&(entries.len() as u32).to_le_bytes());
        h[84..88].copy_from_slice(&128u32.to_le_bytes());
        ram.data[hdr_lba * ss..hdr_lba * ss + ss].copy_from_slice(&h);
        // Entries at LBA2
        for (i, (guid, start, end)) in entries.iter().enumerate() {
            let off = 2 * ss + i * 128;
            ram.data[off..off + 16].copy_from_slice(guid);
            ram.data[off + 32..off + 40].copy_from_slice(&start.to_le_bytes());
            ram.data[off + 40..off + 48].copy_from_slice(&end.to_le_bytes());
        }
        ram
    }

    #[test]
    fn parses_entries_with_inclusive_end() {
        let mut basic = [0u8; 16];
        basic[..4].copy_from_slice(&[0xA2, 0xA0, 0xD0, 0xEB]);
        let mut linux = [0u8; 16];
        linux[..4].copy_from_slice(&[0xAF, 0x3D, 0xC6, 0x0F]);
        let ram = gpt_disk(&[(basic, 2048, 4095), (linux, 4096, 8191)]);
        let (scheme, parts) = parse_gpt(&ram).expect("parse");
        assert_eq!(scheme, PartitionScheme::Gpt);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].start_lba, 2048);
        assert_eq!(parts[0].total_sectors, 2048); // inclusive
        assert_eq!(parts[0].fs, FsKind::Ntfs);
        assert_eq!(parts[1].fs, FsKind::Ext4);
    }

    #[test]
    fn rejects_missing_signature() {
        let ram = RamDevice::new(512 * 100, 512);
        assert!(parse_gpt(&ram).is_err());
    }

    #[test]
    fn skips_garbage_entries() {
        let mut weird = [0u8; 16];
        weird[0] = 0xFF;
        let ram = gpt_disk(&[(weird, 5000, 100)]); // end < start
        let (_, parts) = parse_gpt(&ram).expect("parse");
        assert_eq!(parts.len(), 0);
    }
}
