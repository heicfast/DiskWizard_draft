//! BitLocker — locked detection + unlock (password / recovery key).
//!
//! Detection: a locked BitLocker volume exposes no filesystem to
//! `GetVolumeInformationW`; its boot sector carries the BitLocker
//! signature (`-FVE-FS-` at offset 3, or the FVE metadata GUID in the
//! sector). Unlock: the Win32_EncryptableVolume WMI methods via
//! PowerShell (`UnlockWithPassphrase` / numerical-password) — the
//! documented, scriptable public surface (the vendor's own
//! manage-bde fallback per the research:
//! `artifacts/PartitionRecovery/bitlockerlib--bitlocker-implementation.md`).

/// Does this volume letter look BitLocker-locked? (FVE signature in
/// sector 0 of the raw volume.)
#[must_use]
pub fn volume_is_bitlocker_locked(letter: char) -> bool {
    use dw_core::blockdev::BlockDevice;
    let Ok(dev) = crate::blockdev::RawHandleDevice::open_volume(letter) else {
        return false;
    };
    let mut sector = vec![0u8; dev.sector_size() as usize];
    if dev.read_sectors(0, &mut sector).is_err() {
        return false;
    }
    // BitLocker boot-sector signature "-FVE-FS-" at offset 3.
    if sector.len() > 11 && &sector[3..11] == b"-FVE-FS-" {
        return true;
    }
    // Windows 8+ BitLocker full-volume signature at offset 0.
    if sector.len() > 4 && &sector[0..4] == b"-FVE" {
        return true;
    }
    false
}

/// The unlock round-trip: password or 48-digit recovery key.
///
/// # Errors
/// String error with user-presentable copy on failure (wrong
/// credential, missing volume, PowerShell failure).
pub fn unlock_volume(letter: char, credential: &Credential) -> Result<(), String> {
    let up = letter.to_ascii_uppercase();
    if !up.is_ascii_alphabetic() {
        return Err(format!("Invalid drive letter {letter:?}"));
    }
    let mount = format!("{up}:");
    let script = match credential {
        Credential::Password(pw) => {
            let escaped = pw.replace('\'', "''");
            format!(
                "$v = Get-WmiObject -Namespace 'root\\cimv2\\security\\microsoftvolumeencryption' -Class Win32_EncryptableVolume -Filter \"DriveLetter='{mount}'\"; \
                 if (-not $v) {{ Write-Error 'volume not found'; exit 3 }}; \
                 $r = $v.UnlockWithPassphrase('{escaped}'); \
                 exit $r.ReturnValue"
            )
        }
        Credential::RecoveryKey(key) => {
            let escaped = key.replace('\'', "''");
            format!(
                "$v = Get-WmiObject -Namespace 'root\\cimv2\\security\\microsoftvolumeencryption' -Class Win32_EncryptableVolume -Filter \"DriveLetter='{mount}'\"; \
                 if (-not $v) {{ Write-Error 'volume not found'; exit 3 }}; \
                 $r = $v.UnlockWithNumericalPassword('{escaped}'); \
                 exit $r.ReturnValue"
            )
        }
    };
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| format!("PowerShell launch failed: {e}"))?;
    // PowerShell exit codes carry HRESULTs as unsigned values; code()
    // hands them back as i32, so compare on the u32 view.
    let code = output.status.code().unwrap_or(-1).cast_unsigned();
    match code {
        0 => Ok(()),
        // 0x8031004B = FVE_E_FAILED_AUTHENTICATION (wrong password).
        0x8031_004B => {
            Err("That password didn't unlock this drive. Check it and try again.".into())
        }
        // 0x80310033 = FVE_E_RECOVERY_KEY_REQUIRED.
        0x8031_0033 => Err("This drive needs its recovery key, not a password.".into()),
        3 => Err("BitLocker volume not found for that drive.".into()),
        other => Err(format!("BitLocker unlock failed (code {other:#x}).")),
    }
}

/// The unlock credential variants.
pub enum Credential {
    Password(String),
    RecoveryKey(String),
}

/// Is the volume now accessible (mounted with a filesystem)?
#[must_use]
pub fn volume_unlocked(letter: char) -> bool {
    let root: Vec<u16> = format!("{letter}:\\").encode_utf16().chain([0]).collect();
    let mut fs_buf = [0u16; 32];
    // SAFETY: root is NUL-terminated; fs_buf is a fixed buffer whose
    // capacity travels with the slice (the wrapper passes its len).
    let ok = unsafe {
        windows::Win32::Storage::FileSystem::GetVolumeInformationW(
            windows::core::PCWSTR(root.as_ptr()),
            None,
            None,
            None,
            None,
            Some(&mut fs_buf),
        )
    };
    // An unlocked volume reports a filesystem name (FAT/NTFS/...);
    // a locked one fails the query or returns an empty string.
    ok.is_ok() && fs_buf[0] != 0
}
