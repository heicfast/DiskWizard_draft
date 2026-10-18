//! SDR / boot-repair write path — the WRITE-ENABLED volume handle.
//!
//! The scan path is read-only by architecture (§5.1: never write to
//! the source). Repair is a separate, user-confirmed action: the
//! platform layer opens the volume with GENERIC_WRITE only here,
//! for the planned sectors only, after the dw-core repair engine
//! snapshotted every sector it overwrites (the CopySectors/Undo
//! transaction model from `artifacts/PartitionRecovery/
//! rawfixer-fs-repair.md`).

use dw_core::error::{CoreError, CoreResult};

/// Open a volume for repair writes (letter form: `F:`).
///
/// # Errors
/// [`CoreError::OpenFailed`] when the volume can't be opened for
/// write (locked by another process, no permission — SDR typically
/// needs elevation; the app layer surfaces that).
pub fn open_volume_for_repair(letter: char, sector_size: u32) -> CoreResult<RepairHandle> {
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let up = letter.to_ascii_uppercase();
    if !up.is_ascii_alphabetic() {
        return Err(CoreError::OpenFailed(format!(
            "bad volume letter {letter:?}"
        )));
    }
    let path = format!("\\\\.\\{up}:\0");
    let wide: Vec<u16> = path.encode_utf16().collect();
    // GENERIC_READ | GENERIC_WRITE (raw constants — the blockdev
    // pattern), shared access; synchronous + buffered (repair
    // writes are small and ordered; FlushFileBuffers on drop
    // commits them).
    let handle = unsafe {
        CreateFileW(
            windows::core::PCWSTR(wide.as_ptr()),
            0x8000_0000 | 0x4000_0000,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .map_err(|e| CoreError::OpenFailed(format!("repair open {up}: {e}")))?;
    let sector_size = sector_size.clamp(512, 4096).next_power_of_two();
    Ok(RepairHandle {
        handle,
        sector_size,
    })
}

/// A write-enabled volume handle (dropped → flush + close).
pub struct RepairHandle {
    handle: windows::Win32::Foundation::HANDLE,
    sector_size: u32,
}

impl RepairHandle {
    /// Write one sector-aligned block at `lba` (the repair engine's
    /// writer callback shape). Positioned via OVERLAPPED (no shared
    /// file pointer — the blockdev contract).
    ///
    /// # Errors
    /// [`std::io::Error`] on OS failure or a short write.
    pub fn write_sector(&self, lba: u64, data: &[u8]) -> std::io::Result<()> {
        use windows::Win32::Storage::FileSystem::WriteFile;
        use windows::Win32::System::IO::OVERLAPPED;

        let off: u64 = lba
            .checked_mul(u64::from(self.sector_size))
            .ok_or_else(|| std::io::Error::other("lba overflow"))?;
        // The union's Offset/OffsetHigh arm (the blockdev pattern —
        // windows-rs 0.62 hides them inside the anonymous union).
        let mut ov = OVERLAPPED {
            Anonymous: windows::Win32::System::IO::OVERLAPPED_0 {
                Anonymous: windows::Win32::System::IO::OVERLAPPED_0_0 {
                    Offset: u32::try_from(off & 0xFFFF_FFFF).unwrap_or(u32::MAX),
                    OffsetHigh: u32::try_from(off >> 32).unwrap_or(u32::MAX),
                },
            },
            ..Default::default()
        };
        let mut written = 0u32;
        unsafe {
            WriteFile(
                self.handle,
                Some(data),
                Some(std::ptr::from_mut(&mut written)),
                Some(std::ptr::from_mut(&mut ov)), // synchronous → inline
            )
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        }
        if written as usize != data.len() {
            return Err(std::io::Error::other("short repair write"));
        }
        Ok(())
    }
}

impl Drop for RepairHandle {
    fn drop(&mut self) {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::Storage::FileSystem::FlushFileBuffers;
        let _ = unsafe { FlushFileBuffers(self.handle) };
        let _ = unsafe { CloseHandle(self.handle) };
    }
}
