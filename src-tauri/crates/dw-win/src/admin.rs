//! Elevation check — the app manifest requests requireAdministrator;
//! this confirms the context (boot gating + logging).

/// Is the process running elevated (administrator)?
#[must_use]
pub fn is_elevated() -> bool {
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken, GetTokenInformation};
    use windows::Win32::Security::{TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows::Win32::Foundation::{CloseHandle, HANDLE};

    let mut token = HANDLE::default();
    let ok = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
    if ok.is_err() {
        return false;
    }
    let mut elevation = TOKEN_ELEVATION::default();
    let mut ret = 0u32;
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            Some(&mut ret),
        )
    };
    unsafe { CloseHandle(token) };
    ok.is_ok() && elevation.TokenIsElevated != 0
}
