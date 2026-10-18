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
pub mod ntfs;

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

    /// The sector-by-sector extension (unallocated regions, orphan
    /// chains, deleted reanimation). Default: quick + RAW carving is
    /// the deep phase for engines without a distinct deep pass.
    ///
    /// # Errors
    /// Same contract as [`ScanEngine::quick_scan`].
    fn deep_scan(&mut self, sink: &mut dyn ScanSink, ctrl: &ScanControl) -> CoreResult<ScanStats> {
        self.quick_scan(sink, ctrl)
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
                let compat = u32::from_le_bytes([sb[0x60], sb[0x61], sb[0x62], sb[0x63]]);
                let incompat = u32::from_le_bytes([sb[0x64], sb[0x65], sb[0x66], sb[0x67]]);
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
/// [`CoreError::Unsupported`] when the kind has no engine yet
/// (ReFS/ext/HFS+ land in M2 — the registry grows monotonically).
pub fn create_engine(kind: FsKind, dev: Arc<dyn BlockDevice>) -> CoreResult<Box<dyn ScanEngine>> {
    match kind {
        FsKind::Ntfs | FsKind::Ntfs5 => Ok(Box::new(ntfs::NtfsEngine::new(dev)?)),
        FsKind::Fat12 | FsKind::Fat16 | FsKind::Fat32 => Ok(Box::new(fat::FatEngine::new(dev)?)),
        FsKind::ExFat => Ok(Box::new(exfat::ExFatEngine::new(dev)?)),
        FsKind::Ext2 | FsKind::Ext3 | FsKind::Ext4 => Ok(Box::new(ext::ExtEngine::new(dev)?)),
        other => Err(CoreError::Unsupported(other)),
    }
}
