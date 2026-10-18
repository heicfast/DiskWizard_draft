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
pub fn open_volume_for_repair(letter: char) -> CoreResult<RepairHandle> {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::SystemServices::GENERIC_READ;
    use windows::Win32::Storage::FileSystem::CreateFileW;

    let up = letter.to_ascii_uppercase();
    if !up.is_ascii_alphabetic() {
        return Err(CoreError::OpenFailed(format!(
            "bad volume letter {letter:?}"
        )));
    }
    let path = format!("\\\\.\\{up}:\0");
    let wide: Vec<u16> = path.encode_utf16().collect();
    // GENERIC_READ | GENERIC_WRITE, shared read (repair refuses
    // exclusive-locked volumes), synchronous (repair writes are
    // small + ordered; buffered is CORRECT here — write-through
    // ordering via FlushFileBuffers before close).
    let generic_all = GENERIC_READ.0 | 0x4000_0000; // GENERIC_WRITE
    let handle = unsafe {
        CreateFileW(
            windows::core::PCWSTR(wide.as_ptr()),
            generic_all,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .map_err(|e| CoreError::OpenFailed(format!("repair open {up}: {e}")))?;
    Ok(RepairHandle { handle, _letter: up })
}

/// A write-enabled volume handle (dropped → flush + close).
pub struct RepairHandle {
    handle: windows::Win32::Foundation::HANDLE,
    _letter: char,
}

impl RepairHandle {
    /// Write one sector-aligned block at `lba` (the repair engine's
    /// writer callback shape).
    ///
    /// # Errors
    /// [`CoreError::ReadFailed`] (write leg) on OS failure.
    pub fn write_sector(&self, lba: u64, data: &[u8]) -> std::io::Result<()> {
        use windows::Win32::Storage::FileSystem::{SetFilePointerEx, WriteFile};
        use windows::Win32::Storage::FileSystem::FILE_BEGIN;

        let off: u64 = lba
            .checked_mul(512)
            .ok_or_else(|| std::io::Error::other("lba overflow"))?;
        let large = windows::Win32::Foundation::LARGE_INTEGER {
            QuadPart: off as i64,
        };
        unsafe {
            SetFilePointerEx(self.handle, large, None, FILE_BEGIN)
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            let mut written = 0u32;
            WriteFile(
                self.handle,
                Some(data),
                Some(&mut written),
                None, // synchronous handle
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
        use windows::Win32::Storage::FileSystem::FlushFileBuffers;
        let _ = unsafe { FlushFileBuffers(self.handle) };
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.handle) };
    }
}
