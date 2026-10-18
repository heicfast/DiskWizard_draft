//! Raw-IO block devices — `\\.\PhysicalDriveN` and volume handles.
//!
//! The user-mode production path (ADR-1 in `docs/CORE-PLAN.md`):
//! whole-disk access via `\\.\PhysicalDriveN` (share-read/write — the
//! OS owns the disk while we read sectors beneath the volume stack),
//! volume access via `\\.\X:` with full sharing. The engine-side
//! contract is [`dw_core::blockdev::BlockDevice`]; this module maps
//! Windows handles onto it (the EPMNtDrv driver contract from the
//! research remains a drop-in alternative behind the same trait).

use crate::win_len;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::sync::Arc;

use dw_core::blockdev::{BlockDevice, check_buf};
use dw_core::error::{CoreError, CoreResult};

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_NO_BUFFERING, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    DISK_GEOMETRY_EX, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, IOCTL_STORAGE_GET_DEVICE_NUMBER,
    STORAGE_DEVICE_NUMBER,
};

/// Open flags for raw devices: heavy sharing (we coexist with the OS)
/// plus no buffering (we do our own caching) — Overlapped required
/// with NO_BUFFERING for reads >64 KB alignment paths.
const RAW_SHARE: u32 = FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0;

/// A whole physical disk (`\\.\PhysicalDriveN`) or volume (`\\.\X:`)
/// handle wrapped as a [`BlockDevice`].
pub struct RawHandleDevice {
    file: File,
    sector_size: u32,
    total_sectors: u64,
    label: String,
}

impl RawHandleDevice {
    /// The raw Win32 handle (IOCTL callers).
    #[must_use]
    pub fn as_file_handle(&self) -> *mut core::ffi::c_void {
        self.file.as_raw_handle()
    }

    /// Open `\\.\PhysicalDriveN` (index-based) with geometry probed.
    ///
    /// # Errors
    /// [`CoreError::OpenFailed`] when the device can't be opened;
    /// geometry failures degrade to 512-byte sectors + size from the
    /// drive layout (we never refuse to scan because geometry IOCTls
    /// failed).
    pub fn open_physical(index: u32) -> CoreResult<Self> {
        let path = Path::new(&format!("\\\\.\\PhysicalDrive{index}")).to_path_buf();
        Self::open(&path, &format!("PhysicalDrive{index}"))
    }

    /// Open a volume by letter (`\\.\X:`).
    ///
    /// # Errors
    /// Same contract as [`RawHandleDevice::open_physical`].
    pub fn open_volume(letter: char) -> CoreResult<Self> {
        let up = letter.to_ascii_uppercase();
        if !up.is_ascii_alphabetic() {
            return Err(CoreError::OpenFailed(format!(
                "bad volume letter {letter:?}"
            )));
        }
        let path = Path::new(&format!("\\\\.\\{up}:")).to_path_buf();
        Self::open(&path, &format!("Volume {up}:"))
    }

    /// Open any raw path (physical drive, volume, or file image).
    fn open(path: &Path, label: &str) -> CoreResult<Self> {
        let file = OpenOptions::new()
            .read(true)
            .share_mode(RAW_SHARE)
            .custom_flags(FILE_FLAG_NO_BUFFERING.0 | FILE_FLAG_OVERLAPPED.0)
            .open(path)
            .or_else(|_| {
                // Retry without NO_BUFFERING (some filter stacks reject
                // it on volumes; correctness is identical, just slower).
                OpenOptions::new()
                    .read(true)
                    .share_mode(RAW_SHARE)
                    .open(path)
            })
            .map_err(|e| CoreError::OpenFailed(format!("{}: {e}", path.display())))?;
        let (sector_size, total_sectors) = probe_geometry(&file, path);
        Ok(Self {
            file,
            sector_size,
            total_sectors,
            label: label.to_string(),
        })
    }

    /// The Windows storage device number (disk index + partition
    /// index) — the identity the device enumerator joins by.
    ///
    /// # Errors
    /// [`CoreError::Corrupt`] when the IOCTL fails (files/volumes
    /// without a storage stack).
    pub fn device_number(&self) -> CoreResult<(u32, u32)> {
        let mut num = STORAGE_DEVICE_NUMBER::default();
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                HANDLE(self.file.as_raw_handle()),
                IOCTL_STORAGE_GET_DEVICE_NUMBER,
                None,
                0,
                Some((&raw mut num).cast()),
                win_len(std::mem::size_of::<STORAGE_DEVICE_NUMBER>()),
                Some(&raw mut returned),
                None,
            )
        };
        if ok.is_err() {
            return Err(CoreError::Corrupt {
                structure: "STORAGE_DEVICE_NUMBER",
                detail: "IOCTL failed".into(),
            });
        }
        Ok((num.DeviceNumber, num.PartitionNumber))
    }
}

/// Probe sector size + capacity (geometry for drives; length fallback
/// for files/volumes).
fn probe_geometry(file: &File, path: &Path) -> (u32, u64) {
    let mut geo_out = vec![0u8; std::mem::size_of::<DISK_GEOMETRY_EX>() + 4096];
    let ok = unsafe {
        DeviceIoControl(
            HANDLE(file.as_raw_handle()),
            IOCTL_DISK_GET_DRIVE_GEOMETRY_EX,
            None,
            0,
            Some(geo_out.as_mut_ptr().cast()),
            win_len(geo_out.len()),
            None,
            None,
        )
    };
    if ok.is_ok() {
        // DISK_GEOMETRY_EX: DISK_GEOMETRY (40 bytes, sector size at
        // offset 8) then LARGE_INTEGER DiskSize at 40 then
        // PARTITION_INFORMATION_EX... total capacity = DiskSize for
        // drives. Layout offsets are ABI-stable.
        let bytes: [u8; 8] = geo_out[40..48].try_into().unwrap_or([0; 8]);
        let disk_size = u64::from_le_bytes(bytes);
        let ss: [u8; 4] = geo_out[8..12].try_into().unwrap_or([0x00, 0x02, 0, 0]);
        let ss = u32::from_le_bytes(ss);
        let ss = if ss == 0 { 512 } else { ss };
        if disk_size > 0 {
            return (ss, disk_size / u64::from(ss));
        }
    }
    // Fallback: file length.
    match file.metadata() {
        Ok(m) if m.len() > 0 => {
            let ss = 512;
            (ss, m.len() / u64::from(ss))
        }
        _ => {
            let _ = path;
            (512, 0)
        }
    }
}

impl BlockDevice for RawHandleDevice {
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
        let mut f = &self.file;
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
            "raw:{} ({}x{})",
            self.label, self.total_sectors, self.sector_size
        )
    }
}

/// Open a device and wrap it in the engine's read-ahead cache — the
/// standard construction for scan targets.
///
/// # Errors
/// [`CoreError::OpenFailed`] when the device can't be opened.
pub fn open_cached_physical(index: u32) -> CoreResult<Arc<dw_core::blockdev::CachedDevice>> {
    let dev = RawHandleDevice::open_physical(index)?;
    Ok(Arc::new(dw_core::blockdev::CachedDevice::new(Arc::new(
        dev,
    ))))
}
