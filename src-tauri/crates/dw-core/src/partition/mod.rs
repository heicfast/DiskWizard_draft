//! Partition tables — MBR (classic + protective), EBR chains, GPT
//! (CRC-validated), and the lost-partition sweep.
//!
//! Output: [`PartitionInfo`] rows per device — mounted-letter mapping
//! is the platform layer's job (dw-win joins by start LBA/size); the
//! core decides STRUCTURE: what tables say, what's unallocated, what
//! boot signatures live in the gaps (lost partitions → scan targets,
//! the "lost partitions are displayed here" home-screen contract).
//!
//! Research: `architecture/04-recovery-workflow.md` §3 ("Insert lost
//! partition %s, %d, StartSector=%I64d, TotalSector=%I64d"),
//! `artifacts/PartitionRecovery/mo-module-map.md` (Partition.mo /
//! PartitionRecoveryD roles), `artifacts/MOM-modules/Partition.md`.

mod ebr;
mod gpt;
mod lost;
mod mbr;

pub use ebr::parse_ebr_chain;
pub use gpt::parse_gpt;
pub use lost::{LostPartitionHint, sweep_for_lost_partitions};
pub use mbr::{mbr_type_to_fs, parse_mbr};

use serde::{Deserialize, Serialize};

use crate::types::FsKind;

/// One partition row (table-stated or lost-detected).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionInfo {
    /// 1-based index in enumeration order (stable per device boot).
    pub index: u32,
    /// Absolute start LBA on the device.
    pub start_lba: u64,
    /// Length in sectors.
    pub total_sectors: u64,
    /// FS hint (from the table type byte/GUID, or probed from the
    /// boot sector for lost rows).
    pub fs: FsKind,
    /// True when this row came from a table entry.
    pub in_table: bool,
    /// GPT type GUID (pretty) or MBR type byte description.
    pub type_tag: String,
}

impl PartitionInfo {
    #[must_use]
    pub fn end_lba(&self) -> u64 {
        self.start_lba + self.total_sectors
    }
}

/// The full table picture for one device.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PartitionLayout {
    pub scheme: PartitionScheme,
    pub partitions: Vec<PartitionInfo>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PartitionScheme {
    #[default]
    None,
    Mbr,
    /// GPT (possibly hybrid with a protective MBR).
    Gpt,
    /// Superfloppy — filesystem starts at LBA 0 (common on SD cards).
    Superfloppy,
}

impl PartitionScheme {
    #[must_use]
    pub fn wire(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Mbr => "mbr",
            Self::Gpt => "gpt",
            Self::Superfloppy => "superfloppy",
        }
    }
}

/// Sector-0 signature check shared by the parsers.
fn has_dos_signature(sector: &[u8]) -> bool {
    sector.len() >= 512 && sector[510] == 0x55 && sector[511] == 0xAA
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_wire_names() {
        assert_eq!(PartitionScheme::Gpt.wire(), "gpt");
        assert_eq!(PartitionScheme::Superfloppy.wire(), "superfloppy");
    }
}
