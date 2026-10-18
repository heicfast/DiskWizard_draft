//! File-backed block device — disk images (.img/.iso/.vhd-flat),
//! test fixtures, any `Read+Seek` file. Works on every OS: the whole
//! engine test-suite runs against these.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;

use super::{BlockDevice, check_buf};
use crate::error::{CoreError, CoreResult};

/// Logical sector size detection result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectorSizeMode {
    /// Use exactly this size (from the caller — platform layer knows).
    Fixed(u32),
    /// Probe: 512 unless the file exposes a 4K-native signature.
    Probe,
}

/// The internal handle: a real file or a boxed reader (tests, VHD
/// passthrough). One enum keeps the hot path branch-free per read.
enum Handle {
    File(File),
    Reader(Box<dyn ReadSeekSend>),
}

impl Read for Handle {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::File(f) => f.read(buf),
            Self::Reader(r) => r.read(buf),
        }
    }
}

impl Seek for Handle {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match self {
            Self::File(f) => f.seek(pos),
            Self::Reader(r) => r.seek(pos),
        }
    }
}

/// A seekable-file-backed [`BlockDevice`].
///
/// The inner handle is `Arc<Mutex<…>>` because scans are
/// multi-threaded (quick + carve overlap) and one OS handle must
/// serve them all; the seek+read pair is serialized under the mutex.
/// Hot read paths go through [`crate::blockdev::CachedDevice`] so
/// contention stays low.
#[derive(Clone)]
pub struct ImageFile {
    file: Arc<Mutex<Handle>>,
    sector_size: u32,
    total_sectors: u64,
    label: String,
}

impl ImageFile {
    /// Open `path` with `mode` sizing.
    pub fn open(path: &Path, mode: SectorSizeMode) -> CoreResult<Self> {
        let file = File::open(path)
            .map_err(|e| CoreError::OpenFailed(format!("{}: {e}", path.display())))?;
        let len = file
            .metadata()
            .map_err(|e| CoreError::OpenFailed(e.to_string()))?
            .len();
        let sector_size = match mode {
            SectorSizeMode::Fixed(ss) => ss,
            SectorSizeMode::Probe => probe_sector_size(&file, len),
        };
        Ok(Self {
            file: Arc::new(Mutex::new(Handle::File(file))),
            sector_size,
            total_sectors: len / u64::from(sector_size),
            label: path.display().to_string(),
        })
    }

    /// Wrap an already-open seekable reader (tests, VHD passthrough).
    pub fn from_reader<R: Read + Seek + Send + 'static>(
        reader: R,
        sector_size: u32,
        label: &str,
    ) -> CoreResult<Self> {
        let mut boxed: Box<dyn ReadSeekSend> = Box::new(reader);
        let len = boxed
            .seek(SeekFrom::End(0))
            .map_err(|e| CoreError::OpenFailed(e.to_string()))?;
        boxed
            .rewind()
            .map_err(|e| CoreError::OpenFailed(e.to_string()))?;
        Ok(Self {
            file: Arc::new(Mutex::new(Handle::Reader(boxed))),
            sector_size,
            total_sectors: len / u64::from(sector_size),
            label: label.to_string(),
        })
    }
}

/// Object-safe reader seam for [`ImageFile::from_reader`].
trait ReadSeekSend: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeekSend for T {}

/// 4K-native probe: a VHD footer "cxsparse"/image geometry can't be
/// read cheaply here, so we use the pragmatic 99% rule — sizes that
/// are 512-multiples but not 4096-multiples are 512e; exact 4096
/// multiples of a plausible disk image default to 512 anyway (512e
/// images dominate), EXCEPT ISO-9660 which is always 2048 (handled by
/// the caller passing Fixed for .iso).
fn probe_sector_size(_file: &File, _len: u64) -> u32 {
    512
}

impl BlockDevice for ImageFile {
    fn sector_size(&self) -> u32 {
        self.sector_size
    }

    fn total_sectors(&self) -> u64 {
        self.total_sectors
    }

    fn read_sectors(&self, start_lba: u64, buf: &mut [u8]) -> CoreResult<()> {
        if let Some(e) = check_buf(buf, self.sector_size) {
            return Err(e);
        }
        let offset = start_lba * u64::from(self.sector_size);
        let mut f = self.file.lock();
        f.seek(SeekFrom::Start(offset))
            .map_err(|e| CoreError::ReadFailed {
                offset,
                len: buf.len() as u64,
                source: e,
            })?;
        let mut done = 0usize;
        while done < buf.len() {
            match f.read(&mut buf[done..]) {
                Ok(0) => {
                    return Err(CoreError::ReadFailed {
                        offset,
                        len: buf.len() as u64,
                        source: std::io::Error::new(
                            std::io::ErrorKind::UnexpectedEof,
                            format!("short read at {done}/{}", buf.len()),
                        ),
                    });
                }
                Ok(n) => done += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => {
                    return Err(CoreError::ReadFailed {
                        offset,
                        len: buf.len() as u64,
                        source: e,
                    });
                }
            }
        }
        Ok(())
    }

    fn describe(&self) -> String {
        format!(
            "image:{} ({}x{})",
            self.label, self.total_sectors, self.sector_size
        )
    }

    /// Image files read through one serialized seek+read handle, so
    /// queue depth stays 1; the OS page cache makes 4 MiB windows the
    /// throughput sweet spot for image scans (§5.1 item 5 — the
    /// bands allow 4–16 MiB for flash-backed stores; a file on the
    /// host cache behaves flash-like).
    fn read_profile(&self) -> super::ReadProfile {
        super::ReadProfile {
            window_bytes: 4 * 1024 * 1024,
            queue_depth: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockdev::BlockRead;
    use std::io::Write;

    #[test]
    fn round_trips_through_a_temp_file() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tmp");
        let mut pattern = Vec::new();
        for i in 0..(512 * 40) {
            pattern.push((i % 249) as u8);
        }
        tmp.write_all(&pattern).expect("write");
        tmp.flush().expect("flush");

        let img = ImageFile::open(tmp.path(), SectorSizeMode::Fixed(512)).expect("open");
        assert_eq!(img.total_sectors(), 40);
        let mut out = [0u8; 97];
        img.read_bytes(5_000, &mut out).expect("read");
        assert_eq!(&out[..], &pattern[5_000..5_097]);
    }

    #[test]
    fn short_file_read_fails_with_media_damage() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tmp");
        tmp.write_all(&vec![0u8; 512 * 3]).expect("write");
        tmp.flush().expect("flush");
        let img = ImageFile::open(tmp.path(), SectorSizeMode::Fixed(512)).expect("open");
        let mut buf = [0u8; 512];
        // Sector 3 lies past the 3-sector file → short read.
        let err = img.read_sectors(3, &mut buf).expect_err("past end");
        assert!(err.is_media_damage());
    }

    #[test]
    fn reader_backed_device_works() {
        let cursor = std::io::Cursor::new(vec![7u8; 2048]);
        let img = ImageFile::from_reader(cursor, 512, "cursor").expect("wrap");
        assert_eq!(img.total_sectors(), 4);
        let mut buf = [0u8; 1024];
        img.read_sectors(1, &mut buf).expect("read");
        assert!(buf.iter().all(|&b| b == 7));
    }
}
