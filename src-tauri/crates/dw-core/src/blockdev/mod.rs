//! Sector-level device abstraction — the volume-IO seam.
//!
//! Everything the engines know about storage goes through
//! [`BlockDevice`]: quick engines read mounted-healthy volumes, deep
//! scans walk damaged/lost ones, and tests feed crafted images. The
//! Windows implementations (dw-win: `\\.\PhysicalDriveN`, volume
//! handles) live outside this crate; here we ship the trait, the
//! byte-range helpers engines actually call, the file-backed image
//! device (any OS), the partition sub-device and a read-ahead cache.
//!
//! Research lineage: `architecture/05-scan-engine-architecture.md` §8
//! (volume IO layers), `artifacts/PartitionRecovery/
//! eprdtdrv-sys--driver-internals.md` (the driver contract preserved
//! as a future impl behind this same trait — ADR-1 in
//! `docs/CORE-PLAN.md`).

mod cached;
mod image;
mod sub;

pub use cached::CachedDevice;
pub use image::{ImageFile, SectorSizeMode};
pub use sub::SubDevice;

use crate::error::{CoreError, CoreResult};

/// A random-access sector device.
///
/// Contract:
/// * `read_sectors` fills `buf` completely or fails; `buf.len()` must
///   be a multiple of [`BlockDevice::sector_size`].
/// * Implementations are `Send + Sync` — engines may parallelize.
/// * Sector sizes are LOGICAL (512..4096); physical geometry is the
///   platform layer's concern.
/// * Read failures carry offset+len (grading marks extents Poor, the
///   scan itself survives — the CCheckNodeThread degradation model).
/// * All reads are POSITIONED (address + length carry the offset);
///   implementations never rely on a shared file pointer, so
///   concurrent calls race nowhere.
pub trait BlockDevice: Send + Sync {
    /// Logical sector size in bytes (512, 4096, …).
    fn sector_size(&self) -> u32;

    /// Total addressable sectors.
    fn total_sectors(&self) -> u64;

    /// Read `buf.len() / sector_size` sectors starting at `start_lba`.
    fn read_sectors(&self, start_lba: u64, buf: &mut [u8]) -> CoreResult<()>;

    /// Best-effort read for bulk passes (carving): salvage whatever
    /// sub-ranges are readable, zero-fill the gaps, and return the
    /// unreadable `(offset_in_buf, len)` byte ranges. Never fails for
    /// media reasons (programming errors still can); a fully dead
    /// range reports itself instead of erroring.
    ///
    /// Default: strict read; media damage maps to one whole-buffer
    /// bad range (still zeroes `buf` so callers see deterministic
    /// content).
    fn read_sectors_sparse(&self, start_lba: u64, buf: &mut [u8]) -> CoreResult<Vec<(u64, u64)>> {
        match self.read_sectors(start_lba, buf) {
            Ok(()) => Ok(Vec::new()),
            Err(e) if e.is_media_damage() => {
                buf.fill(0);
                Ok(vec![(0, buf.len() as u64)])
            }
            Err(e) => Err(e),
        }
    }

    /// Preferred buffer ADDRESS alignment (power of two) for large
    /// reads. Unbuffered (O_DIRECT-class) devices require
    /// sector-aligned buffers; the carving windows allocate
    /// accordingly. Default: the logical sector size.
    #[must_use]
    fn preferred_read_alignment(&self) -> u32 {
        self.sector_size().max(1)
    }

    /// Device identity for logs/diagnostics (model/serial/path).
    fn describe(&self) -> String {
        String::new()
    }
}

/// Blanket byte-range helpers every engine uses.
pub trait BlockRead: BlockDevice {
    /// Read an arbitrary byte range (handles alignment internally via
    /// a stack bounce buffer — hot paths should prefer aligned reads
    /// through the cache layer).
    fn read_bytes(&self, offset: u64, out: &mut [u8]) -> CoreResult<()> {
        let ss = u64::from(self.sector_size());
        if ss == 0 {
            return Err(CoreError::Corrupt {
                structure: "BlockDevice",
                detail: "sector size is zero".into(),
            });
        }
        let first_lba = offset / ss;
        let last_end = offset + out.len() as u64;
        let last_lba_excl = last_end.div_ceil(ss);
        let n_sectors = (last_lba_excl - first_lba) as usize;
        let mut bounce = vec![0u8; n_sectors * ss as usize];
        self.read_sectors(first_lba, &mut bounce)?;
        let skip = (offset % ss) as usize;
        out.copy_from_slice(&bounce[skip..skip + out.len()]);
        Ok(())
    }

    /// Read a fixed-size struct at a byte offset — the parser
    /// primitive (all on-disk little-endian structs flow through
    /// `from_le_bytes` on the caller side).
    fn read_struct_at<const N: usize>(&self, offset: u64) -> CoreResult<[u8; N]> {
        let mut buf = [0u8; N];
        self.read_bytes(offset, &mut buf)?;
        Ok(buf)
    }

    /// Total capacity in bytes (saturating).
    #[must_use]
    fn capacity_bytes(&self) -> u64 {
        self.total_sectors()
            .saturating_mul(u64::from(self.sector_size()))
    }
}

impl<T: BlockDevice + ?Sized> BlockRead for T {}

/// A heap buffer whose ADDRESS is aligned to a power of two — the
/// requirement unbuffered (O_DIRECT-class) device reads impose and a
/// straight win for large scan windows everywhere else.
///
/// Implemented in pure safe Rust by over-allocating `align - 1` bytes
/// and slicing from the first aligned interior offset (waste is
/// bounded by the clamp below — at most 4 KiB per window).
pub struct AlignedBuf {
    storage: Vec<u8>,
    offset: usize,
    len: usize,
}

impl AlignedBuf {
    /// Allocate `len` zeroed bytes aligned to `align` (power of two).
    ///
    /// # Panics
    /// On allocation failure (propagated by `Vec`, i.e. abort).
    #[must_use]
    pub fn zeroed(len: usize, align: u32) -> Self {
        let align = (u64::from(align.max(16))).min(4096).next_power_of_two() as usize;
        let storage = vec![0u8; len + align];
        let addr = storage.as_ptr().addr();
        let offset = (align - (addr % align)) % align;
        Self {
            storage,
            offset,
            len,
        }
    }

    /// The usable length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the usable length is zero.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl std::ops::Deref for AlignedBuf {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.storage[self.offset..self.offset + self.len]
    }
}

impl std::ops::DerefMut for AlignedBuf {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.storage[self.offset..self.offset + self.len]
    }
}

/// Validate that `buf` is sector-multiple sized (impl helper).
#[must_use]
pub fn check_buf(buf: &[u8], sector_size: u32) -> Option<CoreError> {
    if sector_size == 0 || !buf.len().is_multiple_of(sector_size as usize) {
        Some(CoreError::Corrupt {
            structure: "BlockDevice",
            detail: format!(
                "buffer len {} not a multiple of sector size {sector_size}",
                buf.len()
            ),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::RamDevice;

    #[test]
    fn read_bytes_handles_unaligned_ranges() {
        let mut ram = RamDevice::new(8192, 512);
        for (i, b) in ram.data.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        let mut out = [0u8; 137];
        ram.read_bytes(1000, &mut out).expect("read");
        assert_eq!(&out[..], &ram.data[1000..1137]);
    }

    #[test]
    fn read_bytes_reports_media_damage() {
        let mut ram = RamDevice::new(8192, 512);
        for (i, b) in ram.data.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        ram.bad_lba = Some(1); // the LBA this read touches
        let mut out = [0u8; 128];
        let err = ram.read_bytes(512, &mut out).expect_err("must fail");
        assert!(err.is_media_damage());
        assert_eq!(err.code(), "read_failed");
    }

    #[test]
    fn read_struct_at_matches_manual() {
        let mut ram = RamDevice::new(1024, 512);
        ram.data[100..104].copy_from_slice(&0xDEAD_BEEF_u32.to_le_bytes());
        let b = ram.read_struct_at::<4>(100).expect("read");
        assert_eq!(u32::from_le_bytes(b), 0xDEAD_BEEF);
    }

    #[test]
    fn capacity_saturates() {
        let ram = RamDevice::new(512 * 10, 512);
        assert_eq!(ram.capacity_bytes(), 5120);
    }

    #[test]
    fn aligned_buf_honors_power_of_two_alignments() {
        for align in [512u32, 4096] {
            let mut buf = AlignedBuf::zeroed(64 * 1024, align);
            assert_eq!(buf.len(), 64 * 1024);
            assert!(buf.as_ptr().addr().is_multiple_of(align as usize));
            assert!(buf.iter().all(|&b| b == 0));
            // DerefMut writes through the aligned view.
            buf[..4].copy_from_slice(&[1, 2, 3, 4]);
            assert_eq!(&buf[..4], &[1, 2, 3, 4]);
        }
    }

    #[test]
    fn aligned_buf_zero_len_is_usable() {
        let buf = AlignedBuf::zeroed(0, 512);
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn sparse_default_maps_damage_to_whole_range() {
        let mut ram = RamDevice::new(8192, 512);
        for (i, b) in ram.data.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        ram.bad_lba = Some(4);
        let mut out = vec![0xFFu8; 1024];
        let bad = ram.read_sectors_sparse(4, &mut out).expect("sparse ok");
        assert_eq!(bad, vec![(0, 1024)]);
        assert!(out.iter().all(|&b| b == 0), "zero-filled on damage");
        // Healthy read: no bad ranges, content intact.
        let mut ok = vec![0u8; 512];
        let bad = ram.read_sectors_sparse(2, &mut ok).expect("sparse ok");
        assert!(bad.is_empty());
        assert_eq!(ok[0], ((2 * 512) % 251) as u8);
    }
}
