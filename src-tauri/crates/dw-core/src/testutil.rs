//! Shared test doubles (crate-internal, tests-only).
#![allow(dead_code)]

use crate::blockdev::{BlockDevice, check_buf};
use crate::error::{CoreError, CoreResult};

/// 512-byte-sector RAM device (test double shared across modules).
pub struct RamDevice {
    pub data: Vec<u8>,
    pub sector_size: u32,
    /// When set, reads touching this LBA fail (media-damage paths).
    pub bad_lba: Option<u64>,
}

impl RamDevice {
    pub fn new(len_bytes: usize, sector_size: u32) -> Self {
        Self {
            data: vec![0; len_bytes],
            sector_size,
            bad_lba: None,
        }
    }
}

impl BlockDevice for RamDevice {
    fn sector_size(&self) -> u32 {
        self.sector_size
    }
    fn total_sectors(&self) -> u64 {
        (self.data.len() / self.sector_size as usize) as u64
    }
    fn read_sectors(&self, start_lba: u64, buf: &mut [u8]) -> CoreResult<()> {
        if let Some(e) = check_buf(buf, self.sector_size) {
            return Err(e);
        }
        let ss = self.sector_size as usize;
        let start = start_lba as usize * ss;
        let end = start + buf.len();
        if end > self.data.len() {
            return Err(CoreError::ReadFailed {
                offset: start_lba * ss as u64,
                len: buf.len() as u64,
                source: std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "past end"),
            });
        }
        if let Some(bad) = self.bad_lba {
            let sectors = (buf.len() / ss) as u64;
            if start_lba <= bad && bad < start_lba + sectors {
                return Err(CoreError::ReadFailed {
                    offset: start_lba * ss as u64,
                    len: buf.len() as u64,
                    source: std::io::Error::other("simulated media damage"),
                });
            }
        }
        buf.copy_from_slice(&self.data[start..end]);
        Ok(())
    }
    fn describe(&self) -> String {
        format!("ram://{}x{}", self.total_sectors(), self.sector_size)
    }
}
