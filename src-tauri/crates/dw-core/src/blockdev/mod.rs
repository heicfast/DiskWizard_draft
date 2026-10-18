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
pub use image::ImageFile;
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
pub trait BlockDevice: Send + Sync {
    /// Logical sector size in bytes (512, 4096, …).
    fn sector_size(&self) -> u32;

    /// Total addressable sectors.
    fn total_sectors(&self) -> u64;

    /// Read `buf.len() / sector_size` sectors starting at `start_lba`.
    fn read_sectors(&self, start_lba: u64, buf: &mut [u8]) -> CoreResult<()>;

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
}
