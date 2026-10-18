//! Elevation check — the app manifest requests requireAdministrator;
//! this confirms the context (boot gating + logging).

use crate::win_len;
/// Is the process running elevated (administrator)?
#[must_use]
pub fn is_elevated() -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token = HANDLE::default();
    // SAFETY: GetCurrentProcess returns a pseudo-handle (no ownership);
    // token is written only on success and closed below.
    let ok = unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY,
            std::ptr::from_mut(&mut token),
        )
    };
    if ok.is_err() {
        return false;
    }
    let mut elevation = TOKEN_ELEVATION::default();
    let mut ret = 0u32;
    // SAFETY: token is an owned handle from OpenProcessToken; the
    // elevation buffer is the documented TOKEN_ELEVATION size.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some(std::ptr::from_mut(&mut elevation).cast()),
            win_len(std::mem::size_of::<TOKEN_ELEVATION>()),
            &raw mut ret,
        )
    };
    // SAFETY: closing our own token handle; a close failure carries no
    // recoverable action (the elevation answer is already decided).
    let _ = unsafe { CloseHandle(token) };
    ok.is_ok() && elevation.TokenIsElevated != 0
}
