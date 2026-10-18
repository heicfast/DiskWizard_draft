//! MBR parsing — classic partition entries, protective MBR (GPT), and
//! type-byte → [`FsKind`] mapping.

use crate::error::{CoreError, CoreResult};
use crate::types::FsKind;

use super::{PartitionInfo, PartitionScheme, has_dos_signature};

/// Parse an MBR from the 512-byte sector.
///
/// Handles: protective MBR (type 0xEE → reports GPT scheme upstream),
/// extended partitions (0x05/0x0F — chain walked by the caller through
/// [`super::parse_ebr_chain`]), and the 0x00 empty entries. Entries
/// with LBA/length past the device end are kept but flagged corrupt
/// via a truncated length (they still become scan targets — the
/// salvage-over-purity rule).
pub fn parse_mbr(
    sector: &[u8],
    device_sectors: u64,
) -> CoreResult<(PartitionScheme, Vec<PartitionInfo>)> {
    if !has_dos_signature(sector) {
        return Err(CoreError::Corrupt {
            structure: "MBR",
            detail: "missing 0x55AA signature".into(),
        });
    }
    let mut parts = Vec::new();
    let mut scheme = PartitionScheme::Mbr;
    let mut index = 0u32;
    for i in 0..4u32 {
        let entry = &sector[0x1BE + (i as usize) * 16..0x1BE + (i as usize) * 16 + 16];
        let ptype = entry[4];
        let start = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as u64;
        let len = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as u64;
        if ptype == 0x00 || len == 0 {
            continue;
        }
        if ptype == 0xEE {
            // Protective MBR — the real table is GPT.
            scheme = PartitionScheme::Gpt;
            continue;
        }
        if ptype == 0x05 || ptype == 0x0F {
            // Extended container — children resolved by the caller.
            continue;
        }
        index += 1;
        parts.push(PartitionInfo {
            index,
            start_lba: start,
            total_sectors: len.min(device_sectors.saturating_sub(start).max(1)),
            fs: mbr_type_to_fs(ptype),
            in_table: true,
            type_tag: format!("mbr-0x{ptype:02X}"),
        });
    }
    if scheme == PartitionScheme::Gpt {
        return Ok((scheme, Vec::new()));
    }
    if parts.is_empty() {
        // Signature present, no entries — treat as superfloppy host.
        return Ok((PartitionScheme::Superfloppy, parts));
    }
    Ok((scheme, parts))
}

/// The type-byte map (the practical subset; unknowns → Unknown and
/// still scan-eligible).
#[must_use]
pub fn mbr_type_to_fs(t: u8) -> FsKind {
    match t {
        0x01 => FsKind::Fat12,
        0x04 | 0x06 => FsKind::Fat16,
        0x0B => FsKind::Fat32,
        0x0C => FsKind::Fat32, // FAT32 LBA
        0x07 => FsKind::Ntfs,  // also HPFS/exFAT — probe decides
        0x0E => FsKind::Fat16, // FAT16 LBA
        0x82 => FsKind::Ext2,  // also swap — probe decides
        0x83 => FsKind::Ext4,
        0x86 | 0x87 => FsKind::Ntfs, // NTFS mirror/striped
        _ => FsKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(ptype: u8, start: u32, len: u32) -> [u8; 16] {
        let mut e = [0u8; 16];
        e[0] = 0x80;
        e[4] = ptype;
        e[8..12].copy_from_slice(&start.to_le_bytes());
        e[12..16].copy_from_slice(&len.to_le_bytes());
        e
    }

    fn mbr_with(entries: [[u8; 16]; 4]) -> Vec<u8> {
        let mut s = vec![0u8; 512];
        for (i, e) in entries.iter().enumerate() {
            s[0x1BE + i * 16..0x1BE + i * 16 + 16].copy_from_slice(e);
        }
        s[510] = 0x55;
        s[511] = 0xAA;
        s
    }

    #[test]
    fn parses_two_primary_entries() {
        let mbr = mbr_with([
            entry(0x07, 2048, 1_000_000),
            entry(0x0B, 1_002_048, 500_000),
            [0u8; 16],
            [0u8; 16],
        ]);
        let (scheme, parts) = parse_mbr(&mbr, 10_000_000).expect("parse");
        assert_eq!(scheme, PartitionScheme::Mbr);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].fs, FsKind::Ntfs);
        assert_eq!(parts[1].fs, FsKind::Fat32);
        assert_eq!(parts[0].start_lba, 2048);
        assert_eq!(parts[1].total_sectors, 500_000);
    }

    #[test]
    fn protective_mbr_yields_gpt_scheme() {
        let mbr = mbr_with([entry(0xEE, 1, 100), [0u8; 16], [0u8; 16], [0u8; 16]]);
        let (scheme, parts) = parse_mbr(&mbr, 10_000_000).expect("parse");
        assert_eq!(scheme, PartitionScheme::Gpt);
        assert_eq!(parts.len(), 0);
    }

    #[test]
    fn oversize_entry_clamps_to_device() {
        let mbr = mbr_with([entry(0x83, 900, 1_000_000), [0u8; 16], [0u8; 16], [0u8; 16]]);
        let (_, parts) = parse_mbr(&mbr, 1_000).expect("parse");
        assert_eq!(parts[0].total_sectors, 100);
    }

    #[test]
    fn rejects_missing_signature() {
        let bad = vec![0u8; 512];
        let err = parse_mbr(&bad, 1_000).expect_err("must reject");
        assert!(matches!(
            err,
            CoreError::Corrupt {
                structure: "MBR",
                ..
            }
        ));
    }

    #[test]
    fn empty_table_is_superfloppy() {
        let mbr = mbr_with([[0u8; 16]; 4]);
        let (scheme, parts) = parse_mbr(&mbr, 1_000).expect("parse");
        assert_eq!(scheme, PartitionScheme::Superfloppy);
        assert_eq!(parts.len(), 0);
    }
}
