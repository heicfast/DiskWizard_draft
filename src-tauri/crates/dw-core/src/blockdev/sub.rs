//! Partition sub-device — an offset/length view over a parent device.
//!
//! Engines scan VOLUMES; the platform layer and the lost-partition
//! path hand us regions inside a whole disk. `SubDevice` implements
//! the CPartitionVirtualVolumeDevice role for those regions: LBA 0 of
//! the sub-device is the region start (the "virtual volume" that
//! works for partitions the OS can't see — research:
//! `architecture/05-scan-engine-architecture.md` §4 item 2).

use std::sync::Arc;

use super::{check_buf, BlockDevice};
use crate::error::{CoreError, CoreResult};

/// A sector-range view over `parent`.
#[derive(Clone)]
pub struct SubDevice {
    parent: Arc<dyn BlockDevice>,
    start_lba: u64,
    len_sectors: u64,
    label: String,
}

impl SubDevice {
    /// Create the view. `start_lba + len_sectors` must fit the parent
    /// (clamped defensively — a truncated tail partition still scans).
    pub fn new(
        parent: Arc<dyn BlockDevice>,
        start_lba: u64,
        len_sectors: u64,
        label: impl Into<String>,
    ) -> Self {
        let max = parent.total_sectors();
        let len = len_sectors.min(max.saturating_sub(start_lba));
        Self {
            parent,
            start_lba,
            len_sectors: len,
            label: label.into(),
        }
    }

    /// Region start on the parent (absolute LBA).
    #[must_use]
    pub fn start_lba(&self) -> u64 {
        self.start_lba
    }

    /// Region length in sectors.
    #[must_use]
    pub fn len_sectors(&self) -> u64 {
        self.len_sectors
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len_sectors == 0
    }
}

impl BlockDevice for SubDevice {
    fn sector_size(&self) -> u32 {
        self.parent.sector_size()
    }

    fn total_sectors(&self) -> u64 {
        self.len_sectors
    }

    fn read_sectors(&self, start_lba: u64, buf: &mut [u8]) -> CoreResult<()> {
        if let Some(e) = check_buf(buf, self.sector_size()) {
            return Err(e);
        }
        if start_lba + (buf.len() / self.sector_size() as usize) as u64 > self.len_sectors {
            return Err(CoreError::ReadFailed {
                offset: start_lba * u64::from(self.sector_size()),
                len: buf.len() as u64,
                source: std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "read past partition end",
                ),
            });
        }
        self.parent.read_sectors(self.start_lba + start_lba, buf)
    }

    fn describe(&self) -> String {
        format!(
            "{}@{}+{}x{}",
            self.label,
            self.start_lba,
            self.len_sectors,
            self.sector_size()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::RamDevice;

    fn ram_with_pattern() -> Arc<RamDevice> {
        let mut ram = RamDevice::new(512 * 100, 512);
        for (i, b) in ram.data.iter_mut().enumerate() {
            *b = (i / 512 % 251) as u8; // sector index as pattern
        }
        Arc::new(ram)
    }

    #[test]
    fn sub_device_remaps_lba_zero_to_region_start() {
        let ram = ram_with_pattern();
        let sub = SubDevice::new(ram, 10, 20, "p1");
        let mut buf = [0u8; 512];
        sub.read_sectors(0, &mut buf).expect("read");
        assert_eq!(buf[0], 10); // parent sector 10
        sub.read_sectors(5, &mut buf).expect("read");
        assert_eq!(buf[0], 15);
    }

    #[test]
    fn reads_beyond_region_fail() {
        let ram = ram_with_pattern();
        let sub = SubDevice::new(ram, 10, 20, "p1");
        let mut buf = [0u8; 512];
        assert!(sub.read_sectors(19, &mut buf).is_ok()); // last sector
        assert!(sub.read_sectors(20, &mut buf).is_err()); // one past
        let mut big = [0u8; 1024];
        assert!(sub.read_sectors(19, &mut big).is_err()); // spans past end
    }

    #[test]
    fn oversize_regions_clamp_to_parent() {
        let ram = ram_with_pattern();
        let sub = SubDevice::new(ram, 90, 1000, "tail");
        assert_eq!(sub.total_sectors(), 10);
    }
}
