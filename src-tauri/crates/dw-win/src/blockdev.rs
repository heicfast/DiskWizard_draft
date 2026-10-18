//! Raw-IO block devices — `\\.\PhysicalDriveN` and volume handles.
//!
//! The user-mode production path (ADR-1 in `docs/CORE-PLAN.md`):
//! whole-disk access via `\\.\PhysicalDriveN` (share-read/write — the
//! OS owns the disk while we read sectors beneath the volume stack),
//! volume access via `\\.\X:` with full sharing.
//!
//! I/O discipline (the "high-performance user-mode direct I/O is the
//! product path" upgrade):
//! * Handles open with `FILE_FLAG_NO_BUFFERING | FILE_FLAG_OVERLAPPED`
//!   (falling back to plain buffered when a filter stack rejects
//!   no-buffering — correctness identical, throughput lower).
//! * Every read is POSITIONED (the byte offset rides in the
//!   `OVERLAPPED`), so there is no shared file pointer and concurrent
//!   calls race nowhere.
//! * `NO_BUFFERING` demands buffer ADDRESSES aligned to the media's
//!   logical sector: aligned caller buffers read in place; unaligned
//!   ones go through an aligned staging copy (the engine's cache
//!   layer allocates its chunks aligned, so the metadata hot path
//!   never stages).
//! * Media errors never storm: a failed bulk read degrades to one
//!   attempt per sector ([`RawHandleDevice::read_sectors_sparse`]),
//!   salvaging every readable sector and reporting the dead ones.
//!
//! The engine-side contract is [`dw_core::blockdev::BlockDevice`]; the
//! EPMNtDrv driver contract from the research remains a drop-in
//! alternative behind the same trait.

use std::sync::Arc;

use dw_core::blockdev::{AlignedBuf, BlockDevice, check_buf};
use dw_core::error::{CoreError, CoreResult};

use parking_lot::Mutex;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_NO_BUFFERING, FILE_FLAG_OVERLAPPED, FILE_FLAGS_AND_ATTRIBUTES,
    FILE_SHARE_MODE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadFile,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Ioctl::{
    DISK_GEOMETRY_EX, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, IOCTL_DISK_GET_LENGTH_INFO,
    IOCTL_STORAGE_GET_DEVICE_NUMBER, IOCTL_STORAGE_QUERY_PROPERTY,
    STORAGE_ACCESS_ALIGNMENT_DESCRIPTOR, STORAGE_DEVICE_NUMBER, STORAGE_PROPERTY_QUERY,
    StorageAccessAlignmentProperty,
};
use windows::Win32::System::Threading::CreateEventW;

use crate::win_len;

/// Open flags for raw devices: heavy sharing (we coexist with the OS)
/// plus no buffering (we run our own cache; the carve sweep must not
/// evict the user's working set). Overlapped is mandatory for
/// concurrent positioned reads.
const RAW_SHARE: FILE_SHARE_MODE = FILE_SHARE_MODE(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0);

/// Staging chunk cap for unaligned buffers (1 MiB — bounds memory for
/// arbitrarily large unaligned requests).
const STAGE_CHUNK: usize = 1024 * 1024;

/// ERROR_IO_PENDING (WIN32_ERROR 997) in its HRESULT view — the
/// normal "read queued" state windows-rs surfaces as `Err`.
const ERROR_IO_PENDING_HR: windows::core::HRESULT = windows::core::HRESULT(-2_147_023_899_i32); // 0x8007_03E5 (ERROR_IO_PENDING 997)

/// A whole physical disk (`\\.\PhysicalDriveN`) or volume (`\\.\X:`)
/// handle wrapped as a [`BlockDevice`].
pub struct RawHandleDevice {
    handle: HANDLE,
    sector_size: u32,
    total_sectors: u64,
    label: String,
    /// Whether the handle carries `FILE_FLAG_NO_BUFFERING` (buffer
    /// alignment then matters).
    no_buffering: bool,
    /// Required buffer ADDRESS alignment (logical sector, ≥ 512).
    alignment: u32,
    /// Recycled manual-reset events for overlapped waits (kernel
    /// objects are too costly to create per read).
    events: Mutex<Vec<HANDLE>>,
}

// SAFETY: the HANDLE is used only through positioned OVERLAPPED reads
// (no shared file pointer); the event pool is mutex-guarded. Win32
// handles are valid across threads.
unsafe impl Send for RawHandleDevice {}
unsafe impl Sync for RawHandleDevice {}

/// An event checked out of the pool; returns it on drop.
struct PooledEvent<'a> {
    dev: &'a RawHandleDevice,
    event: HANDLE,
}

impl Drop for PooledEvent<'_> {
    fn drop(&mut self) {
        self.dev.events.lock().push(self.event);
    }
}

impl RawHandleDevice {
    /// The raw Win32 handle (IOCTL callers: SMART, device numbers).
    #[must_use]
    pub fn as_file_handle(&self) -> *mut core::ffi::c_void {
        self.handle.0
    }

    /// Open `\\.\PhysicalDriveN` (index-based) with geometry probed.
    ///
    /// # Errors
    /// [`CoreError::OpenFailed`] when the device can't be opened;
    /// geometry failures degrade to 512-byte sectors + size from the
    /// drive layout (we never refuse to scan because geometry IOCTLs
    /// failed).
    pub fn open_physical(index: u32) -> CoreResult<Self> {
        let path = format!("\\\\.\\PhysicalDrive{index}\0");
        Self::open(&path, &format!("PhysicalDrive{index}"), true)
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
        let path = format!("\\\\.\\{up}:\0");
        Self::open(&path, &format!("Volume {up}:"), false)
    }

    /// Open any raw path (physical drive, volume; UTF-16, NUL-ended).
    fn open(path16: &str, label: &str, physical: bool) -> CoreResult<Self> {
        let wide: Vec<u16> = path16.encode_utf16().collect();
        let pcw = windows::core::PCWSTR(wide.as_ptr());
        let overlapped_flags =
            FILE_FLAGS_AND_ATTRIBUTES(FILE_FLAG_OVERLAPPED.0 | FILE_FLAG_NO_BUFFERING.0);
        // First try: overlapped + no buffering. Some filter stacks
        // (bitlocker pre-W10, odd SAN drivers) reject NO_BUFFERING on
        // volume handles — retry overlapped-only (buffered), which
        // every stack accepts. Correctness is identical either way.
        let handle = unsafe {
            CreateFileW(
                pcw,
                0x8000_0000, // GENERIC_READ
                RAW_SHARE,
                None,
                OPEN_EXISTING,
                overlapped_flags,
                None,
            )
        };
        let (handle, no_buffering) = if let Ok(h) = handle {
            (h, true)
        } else {
            let h = unsafe {
                CreateFileW(
                    pcw,
                    0x8000_0000,
                    RAW_SHARE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES(FILE_FLAG_OVERLAPPED.0),
                    None,
                )
            }
            .map_err(|e| CoreError::OpenFailed(format!("{label}: {e}")))?;
            (h, false)
        };
        let (sector_size, total_sectors) = probe_capacity(&handle, physical);
        let alignment = probe_alignment(&handle).max(sector_size).max(512);
        Ok(Self {
            handle,
            sector_size,
            total_sectors,
            label: label.to_string(),
            no_buffering,
            alignment,
            events: Mutex::new(Vec::new()),
        })
    }

    /// Check out a reset manual-reset event (creating one when the
    /// pool is empty).
    fn checkout_event(&self) -> CoreResult<PooledEvent<'_>> {
        let event = match self.events.lock().pop() {
            Some(ev) => Ok(ev),
            None => {
                // SAFETY: no name → private event; manual-reset per the
                // overlapped-read contract.
                unsafe { CreateEventW(None, true, false, windows::core::PCWSTR::null()) }
            }
        }
        .map_err(|e| CoreError::Other(format!("CreateEventW: {e}")))?;
        Ok(PooledEvent { dev: self, event })
    }

    /// One positioned overlapped read: `buf.len()` bytes at `offset`,
    /// fully satisfied or failed.
    fn read_at_offset(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        if buf.is_empty() {
            return Ok(());
        }
        let ev = self.checkout_event().map_err(std::io::Error::other)?;
        let mut ov = OVERLAPPED {
            // The union's Offset/OffsetHigh arm is the documented way
            // to carry a 64-bit file position for positioned reads.
            Anonymous: windows::Win32::System::IO::OVERLAPPED_0 {
                Anonymous: windows::Win32::System::IO::OVERLAPPED_0_0 {
                    Offset: u32::try_from(offset & 0xFFFF_FFFF).unwrap_or(u32::MAX),
                    OffsetHigh: u32::try_from(offset >> 32).unwrap_or(u32::MAX),
                },
            },
            hEvent: ev.event,
            ..Default::default()
        };
        // SAFETY: the handle is owned and opened overlapped+sharing;
        // `buf` is a valid exclusive slice of the documented length;
        // the OVERLAPPED lives until GetOverlappedResult returns.
        let queued = unsafe {
            ReadFile(
                self.handle,
                Some(buf),
                None,
                Some(std::ptr::from_mut(&mut ov)),
            )
        };
        let mut done = 0u32;
        // ERROR_IO_PENDING is the normal queued state; windows-rs
        // surfaces it as Err — both paths complete through
        // GetOverlappedResult(wait=true). WIN32_ERROR 997 → the
        // HRESULT-view comparison.
        if let Err(e) = queued
            && e.code() != ERROR_IO_PENDING_HR
        {
            return Err(std::io::Error::other(e));
        }
        // SAFETY: ov belongs to this read and hasn't been touched by
        // anything else; bWait=true blocks until completion.
        let r = unsafe {
            GetOverlappedResult(
                self.handle,
                std::ptr::from_ref(&ov),
                std::ptr::from_mut(&mut done),
                true,
            )
        };
        match r {
            Ok(()) if done as usize == buf.len() => Ok(()),
            Ok(()) => Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                format!("short read {done}/{}", buf.len()),
            )),
            Err(e) => Err(std::io::Error::other(e)),
        }
    }

    /// Strict read with NO_BUFFERING alignment handled: aligned
    /// buffers read in place; unaligned ones stage through an aligned
    /// copy in bounded chunks.
    fn read_aligned_aware(&self, offset: u64, buf: &mut [u8]) -> CoreResult<()> {
        let aligned =
            !self.no_buffering || (buf.as_ptr() as usize).is_multiple_of(self.alignment as usize);
        if aligned {
            return self
                .read_at_offset(offset, buf)
                .map_err(|e| CoreError::ReadFailed {
                    offset,
                    len: buf.len() as u64,
                    source: e,
                });
        }
        // Staging: aligned bounce copied out in ≤1 MiB pieces.
        let mut done = 0usize;
        while done < buf.len() {
            let take = (buf.len() - done).min(STAGE_CHUNK);
            let mut stage = AlignedBuf::zeroed(take, self.alignment);
            let at = offset + done as u64;
            self.read_at_offset(at, &mut stage[..take])
                .map_err(|e| CoreError::ReadFailed {
                    offset: at,
                    len: take as u64,
                    source: e,
                })?;
            buf[done..done + take].copy_from_slice(&stage[..take]);
            done += take;
        }
        Ok(())
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
        // SAFETY: handle is owned; num is the documented output size.
        let ok = unsafe {
            DeviceIoControl(
                self.handle,
                IOCTL_STORAGE_GET_DEVICE_NUMBER,
                None,
                0,
                Some(std::ptr::from_mut(&mut num).cast()),
                win_len(std::mem::size_of::<STORAGE_DEVICE_NUMBER>()),
                Some(std::ptr::from_mut(&mut returned)),
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

impl Drop for RawHandleDevice {
    fn drop(&mut self) {
        // SAFETY: we own the handle and every pooled event; drop runs
        // exactly once and no reads are in flight (borrow discipline:
        // read paths hold &self only while alive).
        unsafe {
            let _ = CloseHandle(self.handle);
            for ev in self.events.lock().drain(..) {
                let _ = CloseHandle(ev);
            }
        }
    }
}

/// Probe capacity: DISK_GEOMETRY_EX for physical drives,
/// IOCTL_DISK_GET_LENGTH_INFO for volumes (geometry fails there).
fn probe_capacity(handle: &HANDLE, physical: bool) -> (u32, u64) {
    if physical {
        let mut geo_out = vec![0u8; std::mem::size_of::<DISK_GEOMETRY_EX>() + 4096];
        // SAFETY: handle is owned; geo_out is oversized for the
        // geometry+partition tail.
        let ok = unsafe {
            DeviceIoControl(
                *handle,
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
            // DISK_GEOMETRY_EX: DISK_GEOMETRY (40 bytes; sector size
            // at offset 8) then LARGE_INTEGER DiskSize at 40. Layout
            // offsets are ABI-stable.
            let bytes: [u8; 8] = geo_out[40..48].try_into().unwrap_or([0; 8]);
            let disk_size = u64::from_le_bytes(bytes);
            let ss: [u8; 4] = geo_out[8..12].try_into().unwrap_or([0x00, 0x02, 0, 0]);
            let ss = u32::from_le_bytes(ss);
            let ss = if ss == 0 { 512 } else { ss };
            if disk_size > 0 {
                return (ss, disk_size / u64::from(ss));
            }
        }
    } else {
        let mut len: u64 = 0;
        let mut returned = 0u32;
        // SAFETY: handle is owned; len is the documented GET_LENGTH_INFO
        // output (a bare u64).
        let ok = unsafe {
            DeviceIoControl(
                *handle,
                IOCTL_DISK_GET_LENGTH_INFO,
                None,
                0,
                Some(std::ptr::from_mut(&mut len).cast()),
                win_len(std::mem::size_of::<u64>()),
                Some(std::ptr::from_mut(&mut returned)),
                None,
            )
        };
        if ok.is_ok() && len > 0 {
            return (512, len / 512);
        }
    }
    (512, 0)
}

/// Probe the media's logical/physical sector geometry (the
/// NO_BUFFERING alignment authority). Falls back to 512.
fn probe_alignment(handle: &HANDLE) -> u32 {
    let mut query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageAccessAlignmentProperty,
        QueryType: windows::Win32::System::Ioctl::PropertyStandardQuery,
        AdditionalParameters: [0; 1],
    };
    let mut desc = STORAGE_ACCESS_ALIGNMENT_DESCRIPTOR::default();
    // SAFETY: handle is owned; the query/descriptor pair is the
    // documented STORAGE_QUERY_PROPERTY exchange.
    let ok = unsafe {
        DeviceIoControl(
            *handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(std::ptr::from_mut(&mut query).cast()),
            win_len(std::mem::size_of::<STORAGE_PROPERTY_QUERY>()),
            Some(std::ptr::from_mut(&mut desc).cast()),
            win_len(std::mem::size_of::<STORAGE_ACCESS_ALIGNMENT_DESCRIPTOR>()),
            None,
            None,
        )
    };
    if ok.is_ok() && desc.BytesPerLogicalSector >= 512 {
        desc.BytesPerLogicalSector.max(512)
    } else {
        512
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
        self.read_aligned_aware(offset, buf)
    }

    /// Media-error isolation: one attempt at the requested span; on
    /// failure, ONE attempt per sector — never a retry storm — with
    /// every readable sector salvaged into `buf` and the dead ranges
    /// reported.
    fn read_sectors_sparse(&self, start_lba: u64, buf: &mut [u8]) -> CoreResult<Vec<(u64, u64)>> {
        if let Some(e) = check_buf(buf, self.sector_size) {
            return Err(e);
        }
        let ss = self.sector_size as usize;
        let offset = start_lba * ss as u64;
        if self.read_aligned_aware(offset, buf).is_ok() {
            return Ok(Vec::new());
        }
        // Degrade to per-sector salvage. Sparse reads are bulk
        // (carving windows are aligned), so the unaligned-staging
        // path below is rare; per-sector staging is still bounded.
        let mut bad: Vec<(u64, u64)> = Vec::new();
        let mut bad_start: Option<usize> = None;
        for i in 0..(buf.len() / ss) {
            let at = offset + (i * ss) as u64;
            let piece = &mut buf[i * ss..(i + 1) * ss];
            let ok = self.read_aligned_aware(at, piece).is_ok();
            match (ok, bad_start) {
                (false, None) => bad_start = Some(i * ss),
                (true, Some(s)) => {
                    bad.push((s as u64, (i * ss - s) as u64));
                    bad_start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = bad_start {
            bad.push((s as u64, (buf.len() - s) as u64));
        }
        Ok(bad)
    }

    fn preferred_read_alignment(&self) -> u32 {
        self.alignment
    }

    fn describe(&self) -> String {
        format!(
            "raw:{} ({}x{}, {})",
            self.label,
            self.total_sectors,
            self.sector_size,
            if self.no_buffering {
                "direct"
            } else {
                "buffered"
            }
        )
    }
}

/// Open a device and wrap it in the engine's read-ahead cache — the
/// standard construction for scan targets. The cache chunks are
/// allocated at the device's preferred alignment so the metadata hot
/// path reads in place (no staging copies).
///
/// # Errors
/// [`CoreError::OpenFailed`] when the device can't be opened.
pub fn open_cached_physical(index: u32) -> CoreResult<Arc<dw_core::blockdev::CachedDevice>> {
    let dev = RawHandleDevice::open_physical(index)?;
    Ok(Arc::new(dw_core::blockdev::CachedDevice::new(Arc::new(
        dev,
    ))))
}
