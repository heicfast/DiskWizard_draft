//! Filesystem scan engines — the SearchDLL family, in Rust.
//!
//! Contract (research: `architecture/05-scan-engine-architecture.md`,
//! `artifacts/SearchDLL/searchdll-family--architecture.md`): each
//! engine scans one volume behind the plugin seam — the vendor's
//! `EUCreate<FS>Searcher` factories become [`create_engine`]; the
//! quick/complete controller pair becomes [`ScanEngine::quick_scan`]
//! / [`ScanEngine::deep_scan`]; results flow through the
//! [`ScanSink`] into the tree builder (the controller=tree-builder
//! role of the original, split cleanly here).
//!
//! Every engine is a pure consumer of [`BlockDevice`] — no OS
//! dependencies — which keeps the whole family unit-testable on any
//! host (images built by the test suite itself).

pub mod exfat;
pub mod ext;
pub mod fat;
pub mod hfsplus;
pub mod ntfs;
pub mod refs;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::blockdev::{BlockDevice, BlockRead};
use crate::error::{CoreError, CoreResult};
use crate::types::{FoundEntry, FsKind};

/// Where engines send found entries + progress (the tree builder
/// implements this; the app layer wraps it for IPC).
pub trait ScanSink: Send {
    /// One found file/dir (paths as hints; tree building assembles).
    fn on_entry(&mut self, entry: FoundEntry);
    /// Progress in engine-specific units (records walked / clusters /
    /// sectors); the controller normalizes.
    fn on_progress(&mut self, done: u64, total: u64) {
        let _ = (done, total);
    }
}

/// Cooperative scan control: cancel + pause, checked per record batch
/// (the research's per-record cancel contract).
#[derive(Clone, Default)]
pub struct ScanControl {
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
}

impl ScanControl {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Request cancellation.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Is cancellation requested?
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Set/clear pause.
    pub fn set_paused(&self, paused: bool) {
        self.pause.store(paused, Ordering::Relaxed);
    }

    /// Engine checkpoint: Err(Cancelled) when cancelled; parks while
    /// paused (bounded sleep loop — the phase machine owns resume).
    ///
    /// # Errors
    /// [`CoreError::Cancelled`] when the scan was cancelled.
    pub fn checkpoint(&self) -> CoreResult<()> {
        if self.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        while self.pause.load(Ordering::Relaxed) {
            if self.is_cancelled() {
                return Err(CoreError::Cancelled);
            }
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
        Ok(())
    }
}

/// Aggregate stats an engine reports per phase.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanStats {
    pub entries: u64,
    pub bytes: u64,
}

/// One filesystem engine instance (created per scan target).
pub trait ScanEngine: Send {
    fn kind(&self) -> FsKind;

    /// The metadata pass (e.g. NTFS MFT walk, FAT chain walk).
    ///
    /// # Errors
    /// [`CoreError`] per the typed taxonomy; never panics on
    /// untrusted input.
    fn quick_scan(&mut self, sink: &mut dyn ScanSink, ctrl: &ScanControl) -> CoreResult<ScanStats>;

    /// The deep/complete pass. Default: NO-OP — the quick pass of
    /// every same-filesystem engine already walks the full metadata
    /// stream INCLUDING deleted records (the NTFS MFT walk visits
    /// every record; the og's complete searcher adds only USN
    /// cross-check + orphan sweeps, which the tree's pass-2 fixpoint
    /// provides here). Re-running quick would re-read the entire MFT
    /// for zero new entries — the session-9 "scan is very very slow"
    /// defect. Engines with REAL deep work (ext/refs/hfs+ orphan
    /// passes, lost-filesystem probes) override this.
    ///
    /// # Errors
    /// Same contract as [`ScanEngine::quick_scan`].
    fn deep_scan(&mut self, sink: &mut dyn ScanSink, ctrl: &ScanControl) -> CoreResult<ScanStats> {
        ctrl.checkpoint()?;
        let _ = sink; // no re-emission: quick's entries stand
        Ok(ScanStats::default())
    }
}

/// Detect the filesystem of a volume from its first sectors (the
/// vendor FS-probe role: boot-sector sniff + superblock magics).
///
/// # Errors
/// [`CoreError::Corrupt`] when the volume can't be read at all.
pub fn probe_filesystem(dev: &dyn BlockDevice) -> CoreResult<FsKind> {
    let sector = dev.read_struct_at::<512>(0)?;
    let dos_ok = sector[510] == 0x55 && sector[511] == 0xAA;
    if dos_ok || dev.total_sectors() > 0 {
        if &sector[3..11] == b"NTFS    " {
            return Ok(FsKind::Ntfs);
        }
        if &sector[3..11] == b"EXFAT   " {
            return Ok(FsKind::ExFat);
        }
        if &sector[0x52..0x5A] == b"FAT32   " {
            return Ok(FsKind::Fat32);
        }
        if &sector[0x36..0x3E] == b"FAT16   " {
            return Ok(FsKind::Fat16);
        }
        if &sector[0x36..0x3E] == b"FAT   " {
            return Ok(FsKind::Fat12);
        }
        // ext: superblock at 1024, magic 0xEF53 at +56; the
        // feature flags discriminate ext2/3/4 (compat journal →
        // ext3, incompat extents → ext4).
        if dev.capacity_bytes() >= 1176 {
            let sb = dev.read_struct_at::<110>(1024)?;
            if sb[56..58] == [0x53, 0xEF] {
                // Kernel `ext4_super_block`: s_feature_compat 0x5C,
                // s_feature_incompat 0x60 (byte-verified against real
                // mkfs output; the ext2 CI fixture gate caught the old
                // swapped 0x60/0x64 reads, which mislabeled ext3).
                let compat = u32::from_le_bytes([sb[0x5C], sb[0x5D], sb[0x5E], sb[0x5F]]);
                let incompat = u32::from_le_bytes([sb[0x60], sb[0x61], sb[0x62], sb[0x63]]);
                if incompat & 0x40 != 0 {
                    return Ok(FsKind::Ext4);
                }
                if compat & 0x4 != 0 {
                    return Ok(FsKind::Ext3);
                }
                return Ok(FsKind::Ext2);
            }
        }
        // HFS+: volume header at 1024.
        if dev.capacity_bytes() >= 1026 {
            let vh = dev.read_struct_at::<2>(1024)?;
            if &vh == b"H+" || &vh == b"HX" {
                return Ok(FsKind::HfsPlus);
            }
        }
        // ReFS: "ReFS" at offset 0x200 of the boot region… and the
        // identifier string inside the superblock page.
        if dev.capacity_bytes() >= 0x800 {
            let b = dev.read_struct_at::<8>(0x200)?;
            if &b[..4] == b"ReFS" {
                return Ok(FsKind::Refs);
            }
        }
        // Unlabeled-but-valid BPB → FAT family (real-world mkfs writes
        // the label, but damaged/floppy images skip it).
        if let Ok(boot) = crate::fs::fat::FatBoot::parse(&sector) {
            return Ok(boot.variant);
        }
    }
    Ok(FsKind::Raw)
}

/// The engine factory (the `EUCreate<FS>Searcher` seam): build the
/// matching engine for a probed kind on the given device.
///
/// # Errors
/// [`CoreError::Unsupported`] when the kind has no engine (every
/// Section 6.3 filesystem now has one; the registry grows
/// monotonically).
pub fn create_engine(kind: FsKind, dev: Arc<dyn BlockDevice>) -> CoreResult<Box<dyn ScanEngine>> {
    match kind {
        FsKind::Ntfs | FsKind::Ntfs5 => Ok(Box::new(ntfs::NtfsEngine::new(dev)?)),
        FsKind::Fat12 | FsKind::Fat16 | FsKind::Fat32 => Ok(Box::new(fat::FatEngine::new(dev)?)),
        FsKind::ExFat => Ok(Box::new(exfat::ExFatEngine::new(dev)?)),
        FsKind::Ext2 | FsKind::Ext3 | FsKind::Ext4 => Ok(Box::new(ext::ExtEngine::new(dev)?)),
        FsKind::HfsPlus => Ok(Box::new(hfsplus::HfsPlusEngine::new(dev.as_ref())?)),
        FsKind::Refs => Ok(Box::new(refs::RefsEngine::new(dev.as_ref())?)),
        other => Err(CoreError::Unsupported(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::RamDevice;

    /// A true-layout (kernel `ext4_super_block` offsets) superblock
    /// at byte 1024 on an otherwise-zero device — the probe path
    /// (magic + compat/incompat discrimination).
    fn ext_device(compat: u32, incompat: u32) -> RamDevice {
        let mut ram = RamDevice::new(2 * 1024 * 1024, 512);
        let sb = &mut ram.data[1024..2048];
        sb[0x00..0x04].copy_from_slice(&4096u32.to_le_bytes()); // inodes
        sb[0x04..0x08].copy_from_slice(&2048u32.to_le_bytes()); // blocks
        sb[0x14..0x18].copy_from_slice(&1u32.to_le_bytes()); // first_data_block
        sb[0x18..0x1C].copy_from_slice(&0u32.to_le_bytes()); // log_block_size
        sb[0x20..0x24].copy_from_slice(&8192u32.to_le_bytes()); // blocks/group
        sb[0x28..0x2C].copy_from_slice(&2048u32.to_le_bytes()); // inodes/group
        sb[0x38..0x3A].copy_from_slice(&0xEF53u16.to_le_bytes()); // magic
        sb[0x4C..0x50].copy_from_slice(&1u32.to_le_bytes()); // rev level
        sb[0x58..0x5A].copy_from_slice(&256u16.to_le_bytes()); // inode size
        sb[0x5C..0x60].copy_from_slice(&compat.to_le_bytes());
        sb[0x60..0x64].copy_from_slice(&incompat.to_le_bytes());
        ram
    }

    /// The ext2 CI fixture gate caught the old swapped feature-flag
    /// reads (compat at 0x60, incompat at 0x64): ext3 volumes were
    /// mislabeled ext2 (journal flag read from the wrong field) and
    /// ext4 detection depended on the ro-compat bits happening to
    /// include 0x40. True offsets: compat 0x5C, incompat 0x60.
    #[test]
    fn ext_probe_discriminates_true_layout() {
        // Real mkfs.ext2 shape: compat ext_attr|resize|dir_index,
        // incompat filetype.
        let ext2 = ext_device(0x38, 0x02);
        assert_eq!(probe_filesystem(&ext2).unwrap(), FsKind::Ext2);
        // ext3 = ext2 + HAS_JOURNAL in compat.
        let ext3 = ext_device(0x38 | 0x4, 0x02);
        assert_eq!(probe_filesystem(&ext3).unwrap(), FsKind::Ext3);
        // ext4 = EXTENTS in incompat.
        let ext4 = ext_device(0x38, 0x02 | 0x40);
        assert_eq!(probe_filesystem(&ext4).unwrap(), FsKind::Ext4);
    }
}
