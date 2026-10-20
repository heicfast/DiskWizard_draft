//! # dw-win — the DiskWizard Windows platform layer
//!
//! Everything Windows-specific behind the `dw-core` seams:
//! raw-IO [`BlockDevice`] implementations (`\\.\PhysicalDriveN`,
//! volume handles), physical-disk + volume enumeration (the
//! `list_devices` inventory), SMART multi-transport health reports,
//! BitLocker status/unlock, and the licensing hardware-identity
//! providers (MachineGuid / volume serial / CPUID / DPAPI).
//!
//! This crate only compiles on Windows (`cfg(windows)` gate in the
//! app); every module cites its research artifact lineage.

#![cfg(windows)]

/// Win32 `u32` length for a buffer/size value (saturating).
///
/// Every `size_of`/`len()` handed to a Win32 API's `u32` parameter
/// routes through here: the saturate arm is unreachable for the struct
/// and few-KiB buffers we pass (bounded far below 4 GiB), and a
/// saturated length is safe if ever hit — the call simply reports a
/// shorter usable buffer instead of truncating unpredictably.
#[must_use]
pub(crate) fn win_len(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

pub mod admin;
pub mod bitlocker;
pub mod blockdev;
pub mod devices;
pub mod license_plat;
pub mod repair;
pub mod smart;
pub mod volume;

/// Raise the CALLING thread's scheduling priority to
/// `THREAD_PRIORITY_HIGHEST` — the compute-worker posture the og app
/// gives its scan/classify/check threads (uniform
/// `QThread::TimeCriticalPriority`, research:
/// `DRWUI-exe--thread-architecture.md`). HIGHEST, not
/// TIME_CRITICAL: the WebView render thread must keep painting while
/// the scan burns cores. Returns success (false = best-effort skip —
/// priority is an optimization, never a gate).
pub fn raise_thread_priority_highest() -> bool {
    use windows::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
    };
    // SAFETY: GetCurrentThread returns a pseudo-handle valid only on
    // this thread (exactly where we call); SetThreadPriority takes it
    // by value with no ownership transfer.
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST) }.is_ok()
}
