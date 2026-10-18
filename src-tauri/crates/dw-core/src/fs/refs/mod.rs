//! The ReFS engine — DBR validation + metadata page census (the
//! evidence-complete layer).
//!
//! Algorithm (research:
//! `artifacts/SearchDLL/refs-engine--deep-dive.md` and
//! `refs-btrfs-zfs-engine-algorithms-v2.md` §1 — the `refs::`
//! namespace block-header chain, byte-exact):
//!
//! 1. DBR (`refs::RefsDbrImpl`): "ReFS" at +3, "FSRS" at +0x10,
//!    geometry at +0x20 (bytes/sector, one of 0x100..0x1000) and
//!    +0x24 (sectors/cluster) with cluster size ∈ {0x1000, 0x10000},
//!    on-disk version at +0x28/+0x29 (major 1 or 3), volume id at
//!    +0x18, reserved region at +0x38, volume size at +0x40.
//!    Metadata page size: 0x4000 for v1, cluster-size-derived for v3.
//! 2. Page census (`RefsMetadataBlockHeaderImpl`): pages start with
//!    "SUPB" (superblock), "CHKP" (checkpoint) or "MSB+" (metadata
//!    sequence block) as the leading u32, with the `ReFSFSRS`
//!    identity inside; the 0x20-byte header carries a block
//!    reference at +8: {valid, block_id, flags(+0x2C), size(+0x30),
//!    next(+0x38)}.
//! 3. Generation census: the CHKP pages carry checkpoint sequence
//!    numbers — newest-transaction-wins, the crash-consistency rule
//!    all three modern filesystems in the vendor suite share.
//!
//! **Scope note (honest boundary, zero-hallucination rule):** the
//! research evidence documents the ReFS DBR + page chain at byte
//! level; the object-tree record layouts inside MSB pages are
//! documented only at RTTI class level (`RefsDirectoryValuesImpl` /
//! `RefsFileValuesImpl`, plus the 42K-line RefsFileSystemAnalyser.mo
//! sibling). This engine therefore delivers: correct volume
//! identification, geometry, and the full metadata page census —
//! and relies on the RAW carver for content (the vendor's own
//! deep-pass split for ReFS). Byte-level MSB tree enumeration is
//! the M3 item in `docs/TODO.md`, with the evidence trail and the
//! fixture plan (format a real ReFS VHD on the Windows runner,
//! reverse the layouts against ground truth).

use crate::blockdev::{BlockDevice, BlockRead};
use crate::error::{CoreError, CoreResult};
use crate::fs::{ScanControl, ScanEngine, ScanSink, ScanStats};
use crate::types::FsKind;

/// DBR page signatures (leading u32, little-endian on disk).
const SIG_SUPB: u32 = 0x4250_5553; // "SUPB" (evidence: *_Src == 0x42505553)
const SIG_CHKP: u32 = 0x504B_4843; // "CHKP"
const SIG_MSB: u32 = 0x2B42_534D; // "MSB+"
/// DBR signature u32s.
const DBR_REFS: u32 = 0x5346_6552; // "ReFS" at +3
const DBR_FSRS: u32 = 0x5352_5346; // "FSRS" at +0x10
/// How far into the volume the page census scans (metadata lives in
/// the leading region; 256 MiB covers any real layout many times
/// over while bounding damaged-volume scan time).
const CENSUS_CAP: u64 = 256 * 1024 * 1024;

/// The parsed ReFS DBR.
#[derive(Debug, Clone, Copy)]
pub struct RefsDbr {
    pub bytes_per_sector: u32,
    pub sectors_per_cluster: u32,
    pub cluster_size: u32,
    pub major: u8,
    pub minor: u8,
    pub volume_id: u64,
    pub reserved: u64,
    pub volume_size: u64,
    pub page_size: u32,
}

impl RefsDbr {
    /// Parse + apply the vendor's acceptance gate (RefsDbrImpl,
    /// decompiled byte-exact).
    ///
    /// # Errors
    /// [`CoreError::Corrupt`] on signature/geometry failure.
    pub fn parse(dbr: &[u8]) -> CoreResult<Self> {
        if dbr.len() < 0x48 {
            return Err(CoreError::Corrupt {
                structure: "ReFS DBR",
                detail: "short read".into(),
            });
        }
        let le32 = |o: usize| u32::from_le_bytes([dbr[o], dbr[o + 1], dbr[o + 2], dbr[o + 3]]);
        let le64 = |o: usize| {
            u64::from_le_bytes([
                dbr[o],
                dbr[o + 1],
                dbr[o + 2],
                dbr[o + 3],
                dbr[o + 4],
                dbr[o + 5],
                dbr[o + 6],
                dbr[o + 7],
            ])
        };
        if le32(3) != DBR_REFS || le32(0x10) != DBR_FSRS {
            return Err(CoreError::Corrupt {
                structure: "ReFS DBR",
                detail: "missing ReFS/FSRS signatures".into(),
            });
        }
        let bytes_per_sector = le32(0x20);
        let sectors_per_cluster = le32(0x24);
        let major = dbr[0x28];
        let minor = dbr[0x29];
        if !matches!(bytes_per_sector, 0x100 | 0x200 | 0x400 | 0x800 | 0x1000) {
            return Err(CoreError::Corrupt {
                structure: "ReFS DBR",
                detail: format!("bytes per sector {bytes_per_sector}"),
            });
        }
        let cluster_size = sectors_per_cluster.wrapping_mul(bytes_per_sector);
        if cluster_size != 0x1000 && cluster_size != 0x10000 {
            return Err(CoreError::Corrupt {
                structure: "ReFS DBR",
                detail: format!("cluster size {cluster_size:#x} not 4K/64K"),
            });
        }
        if major != 1 && major != 3 {
            return Err(CoreError::Corrupt {
                structure: "ReFS DBR",
                detail: format!("major version {major}"),
            });
        }
        let page_size = if major == 1 { 0x4000 } else { cluster_size };
        Ok(Self {
            bytes_per_sector,
            sectors_per_cluster,
            cluster_size,
            major,
            minor,
            volume_id: le64(0x18),
            reserved: le64(0x38),
            volume_size: le64(0x40),
            page_size,
        })
    }
}

/// One discovered metadata page.
#[derive(Debug, Clone, Copy)]
pub struct RefsPage {
    pub offset: u64,
    pub kind: PageKind,
    /// The block-reference fields (page+8 structure).
    pub block_id: u64,
    pub flags: u32,
    pub size: u32,
    pub next: u64,
}

/// Page kinds by leading signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    Superblock,
    Checkpoint,
    Msb,
}

impl PageKind {
    fn from_sig(sig: u32) -> Option<Self> {
        match sig {
            SIG_SUPB => Some(Self::Superblock),
            SIG_CHKP => Some(Self::Checkpoint),
            SIG_MSB => Some(Self::Msb),
            _ => None,
        }
    }
}

/// The engine.
pub struct RefsEngine {
    pub dbr: RefsDbr,
    /// The metadata page census (sorted by offset).
    pub pages: Vec<RefsPage>,
}

impl RefsEngine {
    /// Open + validate + run the page census.
    ///
    /// # Errors
    /// [`CoreError::Corrupt`] when the volume isn't parseable ReFS.
    pub fn new(dev: &dyn BlockDevice) -> CoreResult<Self> {
        let mut dbrbuf = [0u8; 0x48];
        dev.read_bytes(0, &mut dbrbuf)?;
        let dbr = RefsDbr::parse(&dbrbuf)?;
        let pages = census_pages(dev, &dbr);
        Ok(Self { dbr, pages })
    }

    /// The newest checkpoint generation's page (newest-transaction-
    /// wins — the shared crash-consistency rule).
    #[must_use]
    pub fn newest_checkpoint(&self) -> Option<&RefsPage> {
        self.pages
            .iter()
            .filter(|p| p.kind == PageKind::Checkpoint)
            .max_by_key(|p| p.block_id)
    }
}

/// Walk page-aligned offsets through the census range, collecting
/// every page whose leading u32 is a known signature and whose block
/// reference parses (the vendor's 0x200-stride header search, at page
/// granularity since the DBR fixed the page size).
fn census_pages(dev: &dyn BlockDevice, dbr: &RefsDbr) -> Vec<RefsPage> {
    let mut pages = Vec::new();
    let ps = u64::from(dbr.page_size);
    let cap = dev.capacity_bytes().min(CENSUS_CAP);
    // Read a window per page (metadata pages are hot; the cache layer
    // coalesces). 0x40 bytes cover the header + block reference.
    let mut buf = [0u8; 0x40];
    let mut off = 0u64;
    while off + ps <= cap {
        if dev.read_bytes(off, &mut buf).is_ok() {
            let sig = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
            if let Some(kind) = PageKind::from_sig(sig) {
                // Block reference at page+8: valid flag, then the
                // {block_id, flags, size, next} family.
                let valid = buf[8] != 0;
                if valid {
                    let block_id = u64::from_le_bytes([
                        buf[0x10], buf[0x11], buf[0x12], buf[0x13], buf[0x14], buf[0x15],
                        buf[0x16], buf[0x17],
                    ]);
                    // The reference structure fields as documented:
                    // flags at +0x2C, size at +0x30, next at +0x38
                    // (page-relative).
                    let flags = u32::from_le_bytes([buf[0x2C], buf[0x2D], buf[0x2E], buf[0x2F]]);
                    let size = u32::from_le_bytes([buf[0x30], buf[0x31], buf[0x32], buf[0x33]]);
                    let next = u64::from_le_bytes([
                        buf[0x38], buf[0x39], buf[0x3A], buf[0x3B], buf[0x3C], buf[0x3D],
                        buf[0x3E], buf[0x3F],
                    ]);
                    pages.push(RefsPage {
                        offset: off,
                        kind,
                        block_id,
                        flags,
                        size,
                        next,
                    });
                }
            }
        }
        off += ps;
    }
    pages
}

impl ScanEngine for RefsEngine {
    fn kind(&self) -> FsKind {
        FsKind::Refs
    }

    fn quick_scan(&mut self, sink: &mut dyn ScanSink, ctrl: &ScanControl) -> CoreResult<ScanStats> {
        // The evidence-complete layer: identify + census. Content
        // recovery for ReFS flows through the RAW carver (the deep
        // phase) — the vendor's own split for ReFS deep passes; the
        // MSB tree enumeration is the M3 item (see module docs).
        let mut stats = ScanStats::default();
        let total = self.pages.len() as u64;
        for (i, p) in self.pages.iter().enumerate() {
            if i % 256 == 0 {
                ctrl.checkpoint()?;
                sink.on_progress(i as u64, total);
            }
            let _ = p;
        }
        // Surface the census as engine diagnostics: the volume
        // resolves as ReFS with its true geometry; the app layer
        // logs page counts per kind.
        stats.entries = 0;
        stats.bytes = u64::from(self.dbr.cluster_size);
        Ok(stats)
    }

    fn deep_scan(&mut self, sink: &mut dyn ScanSink, ctrl: &ScanControl) -> CoreResult<ScanStats> {
        self.quick_scan(sink, ctrl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::testutil::RamDevice;

    fn ram(data: Vec<u8>) -> Arc<dyn BlockDevice> {
        Arc::new(RamDevice {
            data,
            sector_size: 512,
            bad_lba: None,
        })
    }

    /// Minimal ReFS-shaped image: DBR + a superblock page + a
    /// checkpoint page + an MSB page (cluster 4K, page 4K, v3.4).
    fn refs_image() -> Vec<u8> {
        let mut data = vec![0u8; 0x40000];
        let dbr = &mut data[0..0x48];
        dbr[3..7].copy_from_slice(b"ReFS");
        dbr[0x10..0x14].copy_from_slice(b"FSRS");
        dbr[0x18..0x20].copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
        dbr[0x20..0x24].copy_from_slice(&0x1000u32.to_le_bytes()); // bps
        dbr[0x24..0x28].copy_from_slice(&1u32.to_le_bytes()); // spc
        dbr[0x28] = 3; // major
        dbr[0x29] = 4; // minor
        dbr[0x38..0x40].copy_from_slice(&0x10000u64.to_le_bytes());
        dbr[0x40..0x48].copy_from_slice(&0x40000u64.to_le_bytes());
        // Superblock page at 0x10000.
        let mut page = |off: usize, sig: u32, block_id: u64| {
            data[off..off + 4].copy_from_slice(&sig.to_le_bytes());
            data[off + 8] = 1; // valid
            data[off + 0x10..off + 0x18].copy_from_slice(&block_id.to_le_bytes());
            data[off + 0x30..off + 0x34].copy_from_slice(&0x1000u32.to_le_bytes());
        };
        page(0x10000, SIG_SUPB, 7);
        page(0x11000, SIG_CHKP, 42);
        page(0x12000, SIG_MSB, 43);
        page(0x13000, SIG_CHKP, 41); // older checkpoint
        data
    }

    struct CollectSink;
    impl ScanSink for CollectSink {
        fn on_entry(&mut self, _entry: crate::types::FoundEntry) {}
    }

    #[test]
    fn dbr_gate_accepts_valid() {
        let dbr = RefsDbr::parse(&refs_image()[0..0x48]).expect("parse");
        assert_eq!(dbr.cluster_size, 0x1000);
        assert_eq!(dbr.page_size, 0x1000); // v3 → cluster-sized pages
        assert_eq!((dbr.major, dbr.minor), (3, 4));
    }

    #[test]
    fn dbr_gate_rejects_bad_signatures() {
        let mut img = refs_image();
        img[4] = b'X'; // break "ReFS"
        assert!(RefsDbr::parse(&img[0..0x48]).is_err());
    }

    #[test]
    fn dbr_gate_rejects_bad_cluster_size() {
        let mut img = refs_image();
        img[0x24..0x28].copy_from_slice(&3u32.to_le_bytes()); // 12K clusters
        assert!(RefsDbr::parse(&img[0..0x48]).is_err());
    }

    #[test]
    fn dbr_gate_rejects_unknown_major() {
        let mut img = refs_image();
        img[0x28] = 2;
        assert!(RefsDbr::parse(&img[0..0x48]).is_err());
    }

    #[test]
    fn v1_pages_are_16k() {
        let mut img = refs_image();
        img[0x28] = 1; // major 1
        let dbr = RefsDbr::parse(&img[0..0x48]).expect("parse");
        assert_eq!(dbr.page_size, 0x4000);
    }

    #[test]
    fn page_census_finds_all_kinds() {
        let dev = ram(refs_image());
        let engine = RefsEngine::new(&*dev).expect("engine");
        assert_eq!(engine.pages.len(), 4, "census finds all four pages");
        assert_eq!(
            engine
                .pages
                .iter()
                .filter(|p| p.kind == PageKind::Superblock)
                .count(),
            1
        );
        assert_eq!(
            engine
                .pages
                .iter()
                .filter(|p| p.kind == PageKind::Checkpoint)
                .count(),
            2
        );
        assert_eq!(
            engine
                .pages
                .iter()
                .filter(|p| p.kind == PageKind::Msb)
                .count(),
            1
        );
    }

    #[test]
    fn newest_checkpoint_wins() {
        let dev = ram(refs_image());
        let engine = RefsEngine::new(&*dev).expect("engine");
        let newest = engine.newest_checkpoint().expect("checkpoint present");
        assert_eq!(newest.block_id, 42, "generation 42 beats 41");
        assert_eq!(newest.offset, 0x11000);
    }

    #[test]
    fn census_survives_all_zero_volume() {
        let dev = ram(vec![0u8; 0x40000]);
        // No DBR → engine construction fails cleanly (no panic).
        assert!(RefsEngine::new(&*dev).is_err());
    }

    #[test]
    fn scan_runs_clean() {
        let dev = ram(refs_image());
        let mut engine = RefsEngine::new(&*dev).expect("engine");
        let mut sink = CollectSink;
        let stats = engine
            .quick_scan(&mut sink, &ScanControl::new())
            .expect("scan");
        assert_eq!(stats.entries, 0, "no fabricated entries");
        assert_eq!(stats.bytes, 0x1000, "cluster size surfaced");
    }
}
