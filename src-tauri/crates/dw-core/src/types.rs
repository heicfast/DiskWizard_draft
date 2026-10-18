//! Core domain types — the engine-side model that the app layer
//! serializes into the frozen IPC shapes (`src/types.ts` mirrors).
//!
//! Contract notes (frozen per `docs/ARCHITECTURE.md`):
//! * node ids are DENSE indexes into the results snapshot — the arena
//!   in [`crate::tree`] assigns them sequentially per scan generation.
//! * `Category` maps 1:1 to the UI's `FileCategory` (8 top categories
//!   per FileFilter.xml — Graphics/Video/Audio/Document/Email/Archive/
//!   Folders/Other; we surface 7 + fold Folders into the tree).
//! * `NodeOrigin` reproduces the results-tree special branches
//!   (`artifacts/UIUX/09-results-phase-deep-dive.md`): existing /
//!   deleted / lost-path (carved, path unreconstructable) / lost-name
//!   (carved, synthesized name) / tags (metadata mining) / version.

use serde::{Deserialize, Serialize};

/// Filesystem kinds the engines cover (`FileSystemKind` in the UI
/// contract; codes follow the vendor FS-code table where sensible).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum FsKind {
    Fat12,
    Fat16,
    Fat32,
    ExFat,
    Ntfs,
    Ntfs5,
    Ext2,
    Ext3,
    Ext4,
    HfsPlus,
    Refs,
    /// Raw / unrecognized volume — carving target (SDR pre-phase).
    Raw,
    Unknown,
}

impl FsKind {
    /// The wire string used by the frozen IPC contract
    /// (`FileSystemKind` in `src/types.ts`).
    #[must_use]
    pub fn wire(self) -> &'static str {
        match self {
            Self::Fat12
            | Self::Fat16
            | Self::Fat32
            | Self::ExFat
            | Self::Ntfs
            | Self::Ntfs5
            | Self::Ext2
            | Self::Ext3
            | Self::Ext4
            | Self::HfsPlus
            | Self::Refs
            | Self::Raw => {
                match self {
                    Self::Fat12 => "FAT16", // FAT12 surfaces as FAT16 in the UI contract
                    Self::Fat16 => "FAT16",
                    Self::Fat32 => "FAT32",
                    Self::ExFat => "exFAT",
                    Self::Ntfs | Self::Ntfs5 => "NTFS",
                    Self::Ext2 | Self::Ext3 | Self::Ext4 => "ext4",
                    Self::HfsPlus => "HFS+",
                    Self::Refs => "ReFS",
                    Self::Unknown => "RAW",
                    Self::Raw => "RAW",
                }
            }
            Self::Unknown => "RAW",
        }
    }
}

/// Result categories (FileFilter.xml's 8 minus Folders).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Images,
    Videos,
    Audio,
    Documents,
    Archives,
    Email,
    Other,
}

impl Category {
    /// The wire string (`FileCategory` in the UI contract).
    #[must_use]
    pub fn wire(self) -> &'static str {
        match self {
            Self::Images => "images",
            Self::Videos => "videos",
            Self::Audio => "audio",
            Self::Documents => "documents",
            Self::Archives => "archives",
            Self::Email => "email",
            Self::Other => "other",
        }
    }
}

/// A contiguous data run on the source volume, in bytes relative to
/// the volume start (the datarun model; `artifacts/SaveScan/
/// ifileproperties-part1--ntfs-fat-exfat.md` — DataRunList persistence
/// proves the extent-vector contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extent {
    /// Byte offset relative to the volume/device start.
    pub offset: u64,
    /// Length in bytes.
    pub len: u64,
}

impl Extent {
    #[must_use]
    pub const fn new(offset: u64, len: u64) -> Self {
        Self { offset, len }
    }

    #[must_use]
    pub const fn end(&self) -> u64 {
        self.offset + self.len
    }
}

/// DOS-style attributes (mirrors $FILE_NAME flags / FAT dir attrs).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileAttrs(pub u32);

impl FileAttrs {
    pub const READONLY: u32 = 0x0000_0001;
    pub const HIDDEN: u32 = 0x0000_0002;
    pub const SYSTEM: u32 = 0x0000_0004;
    pub const DIRECTORY: u32 = 0x0000_0010;
    pub const ARCHIVE: u32 = 0x0000_0020;
    pub const DEVICE: u32 = 0x0000_0040;
    pub const COMPRESSED: u32 = 0x0000_0800;
    pub const ENCRYPTED: u32 = 0x0000_4000;
    pub const SPARSE: u32 = 0x0000_0200;

    #[must_use]
    pub const fn has(self, bit: u32) -> bool {
        self.0 & bit != 0
    }
}

/// Normalized timestamps (unix seconds — the UI formats).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timestamps {
    /// Last modification (content).
    pub modified: Option<i64>,
    /// Creation.
    pub created: Option<i64>,
    /// Last access.
    pub accessed: Option<i64>,
    /// Record change (NTFS $STANDARD_INFORMATION / MFT time).
    pub changed: Option<i64>,
    /// Deletion time when known (FAT/inode dtime; UI date-deleted
    /// bucket).
    pub deleted: Option<i64>,
}

/// Where a found node came from — the special-branch taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeOrigin {
    /// Still linked on disk.
    Existing,
    /// FS record present but unlinked (deleted).
    Deleted,
    /// Carved by the RAW engine; path unreconstructable.
    LostPath,
    /// Carved; name synthesized from content signature.
    LostName,
    /// Identified via EXIF/metadata tag mining.
    Tags,
    /// Older on-disk version of an existing path (NTFS-recorded).
    Version,
}

impl NodeOrigin {
    /// The wire string (`NodeOrigin` in the UI contract).
    #[must_use]
    pub fn wire(self) -> &'static str {
        match self {
            Self::Existing => "existing",
            Self::Deleted => "deleted",
            Self::LostPath => "lost-path",
            Self::LostName => "lost-name",
            Self::Tags => "tags",
            Self::Version => "version",
        }
    }
}

/// Recoverability grading — the "file recoverability indicators"
/// release-note feature; the UI's Excellent/Good/Poor chips.
///
/// Grading inputs (the CCheckNodeThread analog): extent readability
/// probe, deletion damage, structural sanity. Carved files cap at
/// Good (mock realism contract preserved).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Recoverability {
    Excellent,
    Good,
    Poor,
}

impl Recoverability {
    #[must_use]
    pub fn wire(self) -> &'static str {
        match self {
            Self::Excellent => "excellent",
            Self::Good => "good",
            Self::Poor => "poor",
        }
    }
}

/// A candidate file found by an engine — the unit the tree builder
/// consumes (path is a hint; carved entries carry `None`).
#[derive(Debug, Clone)]
pub struct FoundEntry {
    /// Best-known name (carved: synthesized from format + index).
    pub name: String,
    /// Parent directory chain BELOW the volume root ("" = root);
    /// engines supply what they know — tree building tolerates gaps.
    pub parent_hint: Option<String>,
    pub is_dir: bool,
    pub size: u64,
    pub attrs: FileAttrs,
    pub timestamps: Timestamps,
    pub origin: NodeOrigin,
    /// Content extents in volume-relative bytes (empty = metadata-only
    /// or not-yet-resolved; recovery resolves lazily where possible).
    pub extents: Vec<Extent>,
    /// Engine confidence 0..=100 feeding the grade (carvers set from
    /// signature confidence; structured engines start at 100).
    pub confidence: u8,
    /// Category hint already known (extension-based); tree building
    /// re-classifies as the authority.
    pub category_hint: Option<Category>,
    /// Image dimensions when cheaply known (carvers parse headers).
    pub dimensions: Option<(u32, u32)>,
    /// Engine that produced the entry (diagnostics + merge policy).
    pub source: FsKind,
}

impl FoundEntry {
    /// A structured (non-carved) entry with full confidence.
    #[must_use]
    pub fn structured(name: String, is_dir: bool, source: FsKind) -> Self {
        Self {
            name,
            parent_hint: None,
            is_dir,
            size: 0,
            attrs: FileAttrs::default(),
            timestamps: Timestamps::default(),
            origin: NodeOrigin::Existing,
            extents: Vec::new(),
            confidence: 100,
            category_hint: None,
            dimensions: None,
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fs_kind_wire_matches_ui_contract() {
        assert_eq!(FsKind::Ntfs.wire(), "NTFS");
        assert_eq!(FsKind::Ntfs5.wire(), "NTFS");
        assert_eq!(FsKind::Fat32.wire(), "FAT32");
        assert_eq!(FsKind::Fat12.wire(), "FAT16");
        assert_eq!(FsKind::ExFat.wire(), "exFAT");
        assert_eq!(FsKind::Refs.wire(), "ReFS");
        assert_eq!(FsKind::Ext4.wire(), "ext4");
        assert_eq!(FsKind::HfsPlus.wire(), "HFS+");
        assert_eq!(FsKind::Raw.wire(), "RAW");
        assert_eq!(FsKind::Unknown.wire(), "RAW");
    }

    #[test]
    fn origin_and_grade_wire() {
        assert_eq!(NodeOrigin::LostPath.wire(), "lost-path");
        assert_eq!(NodeOrigin::Version.wire(), "version");
        assert_eq!(Recoverability::Excellent.wire(), "excellent");
        assert_eq!(Recoverability::Poor.wire(), "poor");
        assert_eq!(Category::Email.wire(), "email");
    }

    #[test]
    fn extent_end_wraps_correctly() {
        assert_eq!(Extent::new(100, 28).end(), 128);
    }
}
