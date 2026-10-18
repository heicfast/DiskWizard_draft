//! The licensing platform providers (Windows): MachineGuid, system-
//! volume serial, CPUID brand, DPAPI-wrapped state, hostname/OS/RAM.

use dw_license::hwid::HwIdProvider;
use dw_license::posture::LicenseState;
use dw_license::store::StateStore;
use dw_license::{from_hex, hex};

use windows::core::PCSTR;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
};
use windows::Win32::Storage::FileSystem::GetVolumeInformationW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
    REG_VALUE_TYPE, RRF_RT_REG_SZ,
};
use windows::Win32::System::SystemInformation::{GetComputerNameExW, GetSystemInfo, COMPUTER_NAME_FORMAT, SYSTEM_INFO};
use windows::Win32::System::SystemServices::{GetSystemFirmwareTable, GLOBAL_MEMORY_STATUS};

/// The Windows provider (frozen recipe inputs).
pub struct WinHwId;

impl HwIdProvider for WinHwId {
    fn machine_guid(&self) -> Result<String, String> {
        reg_read_string(
            HKEY_LOCAL_MACHINE,
            "SOFTWARE\\Microsoft\\Cryptography",
            "MachineGuid",
        )
        .ok_or_else(|| "MachineGuid unreadable".to_string())
    }

    fn system_drive_serial(&self) -> Result<u32, String> {
        let sys_dir = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        let root = format!(
            "{}\\",
            sys_dir.chars().next().unwrap_or('C').to_ascii_uppercase()
        );
        let root_w: Vec<u16> = root.encode_utf16().chain([0]).collect();
        let mut serial = 0u32;
        let ok = unsafe {
            GetVolumeInformationW(
                PCWSTR_dummy(root_w.as_ptr()),
                None,
                None,
                Some(&mut serial),
                None,
                None,
                None,
                None,
            )
        };
        if ok.as_bool() {
            Ok(serial)
        } else {
            Err("volume serial unreadable".into())
        }
    }

    fn cpuid_brand(&self) -> Result<String, String> {
        #[cfg(target_arch = "x86_64")]
        {
            use std::arch::x86_64::__cpuid;
            // Brand string: leaves 0x80000002..0x80000005.
            let mut brand = [0u8; 48];
            let mut valid = false;
            for (i, leaf) in [0x8000_0002u32, 0x8000_0003, 0x8000_0004]
                .iter()
                .enumerate()
            {
                let r = unsafe { __cpuid(*leaf) };
                if r.eax != 0 || r.ebx != 0 {
                    valid = true;
                }
                brand[i * 16..i * 16 + 4].copy_from_slice(&r.eax.to_le_bytes());
                brand[i * 16 + 4..i * 16 + 8].copy_from_slice(&r.ebx.to_le_bytes());
                brand[i * 16 + 8..i * 16 + 12].copy_from_slice(&r.ecx.to_le_bytes());
                brand[i * 16 + 12..i * 16 + 16].copy_from_slice(&r.edx.to_le_bytes());
            }
            if valid {
                let s = String::from_utf8_lossy(&brand)
                    .trim_end_matches('\0')
                    .trim()
                    .to_string();
                if !s.is_empty() {
                    return Ok(s);
                }
            }
            Err("CPUID unavailable".into())
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            Err("CPUID unavailable on this architecture".into())
        }
    }
}

/// Helper to satisfy the PCWSTR type without a windows::core import
/// dance per call-site.
fn PCWSTR_dummy(ptr: *const u16) -> windows::core::PCWSTR {
    windows::core::PCWSTR(ptr)
}

/// One REG_SZ read from HKLM.
fn reg_read_string(hkey: HKEY, subkey: &str, value: &str) -> Option<String> {
    let subkey_w: Vec<u16> = subkey.encode_utf16().chain([0]).collect();
    let value_w: Vec<u16> = value.encode_utf16().chain([0]).collect();
    let mut key = HKEY::default();
    let status = unsafe {
        RegOpenKeyExW(hkey, PCWSTR_dummy(subkey_w.as_ptr()), None, KEY_READ, &mut key)
    };
    if status.is_err() {
        return None;
    }
    let mut ty = REG_VALUE_TYPE(0);
    let mut len = 0u32;
    let ok = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR_dummy(value_w.as_ptr()),
            None,
            Some(&mut ty),
            None,
            Some(&mut len),
        )
    };
    if ok.is_err() || len == 0 || len > 4096 {
        unsafe { RegCloseKey(key) };
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    let ok = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR_dummy(value_w.as_ptr()),
            None,
            Some(&mut ty),
            Some(buf.as_mut_ptr()),
            Some(&mut len),
        )
    };
    unsafe { RegCloseKey(key) };
    if ok.is_err() {
        return None;
    }
    let utf16: Vec<u16> = buf[..len as usize]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Some(String::from_utf16_lossy(&utf16).trim_end_matches('\0').to_string())
}

/// DPAPI-wrapped persistence (the at-rest layer, doc §5.4).
pub struct DpapiStore {
    path: std::path::PathBuf,
}

impl DpapiStore {
    #[must_use]
    pub fn new(path: std::path::PathBuf) -> Self {
        Self { path }
    }
}

impl StateStore for DpapiStore {
    fn save(&self, state: &LicenseState) -> Result<(), String> {
        let json = serde_json::to_vec(state).map_err(|e| format!("serialize: {e}"))?;
        let blob = dpapi_protect(&json)?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("mkdir: {e}"))?;
        }
        std::fs::write(&self.path, blob).map_err(|e| format!("write: {e}"))
    }

    fn load(&self) -> Result<LicenseState, String> {
        match std::fs::read(&self.path) {
            Ok(blob) => {
                let json = dpapi_unprotect(&blob)?;
                serde_json::from_slice(&json)
                    .map_err(|e| format!("corrupt license state: {e}"))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(LicenseState::default()),
            Err(e) => Err(format!("read: {e}")),
        }
    }

    fn clear(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("remove: {e}")),
        }
    }
}

/// DPAPI-encrypt bytes (LocalMachine scope — survives the elevation
/// context differences between the installer and the app run).
fn dpapi_protect(plain: &[u8]) -> Result<Vec<u8>, String> {
    let mut in_blob = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr().cast_mut(),
    };
    let mut out = CRYPT_INTEGER_BLOB::default();
    let r = unsafe { CryptProtectData(&mut in_blob, None, None, None, None, 0, &mut out) };
    if r.is_err() {
        return Err(format!("CryptProtectData: {}", r.0));
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe { windows::Win32::System::Memory::LocalFree(Some(windows::Win32::Foundation::HLOCAL(out.pbData))) };
    Ok(bytes)
}

/// DPAPI-decrypt bytes.
fn dpapi_unprotect(cipher: &[u8]) -> Result<Vec<u8>, String> {
    let mut in_blob = CRYPT_INTEGER_BLOB {
        cbData: cipher.len() as u32,
        pbData: cipher.as_ptr().cast_mut(),
    };
    let mut out = CRYPT_INTEGER_BLOB::default();
    let r = unsafe { CryptUnprotectData(&mut in_blob, None, None, None, None, 0, &mut out) };
    if r.is_err() {
        return Err(format!("CryptUnprotectData: {}", r.0));
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe { windows::Win32::System::Memory::LocalFree(Some(windows::Win32::Foundation::HLOCAL(out.pbData))) };
    Ok(bytes)
}

/// Hostname (display + audit).
#[must_use]
pub fn hostname() -> String {
    let mut buf = [0u16; 256];
    let mut len = buf.len() as u32;
    let ok = unsafe {
        GetComputerNameExW(
            COMPUTER_NAME_FORMAT(1), // ComputerNameDnsHostname
            buf.as_mut_ptr(),
            &mut len,
        )
    };
    if ok.as_bool() {
        String::from_utf16_lossy(&buf[..len as usize])
    } else {
        std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into())
    }
}

/// OS version string (display).
#[must_use]
pub fn os_version() -> String {
    let key = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion";
    let product = reg_read_string(HKEY_LOCAL_MACHINE, key, "ProductName")
        .unwrap_or_else(|| "Windows".into());
    let display = reg_read_string(HKEY_LOCAL_MACHINE, key, "DisplayVersion")
        .or_else(|| reg_read_string(HKEY_LOCAL_MACHINE, key, "ReleaseId"))
        .unwrap_or_default();
    format!("{product} {display}").trim().to_string()
}

/// Total physical memory MB.
#[must_use]
pub fn ram_mb() -> Option<u64> {
    let mut status = GLOBAL_MEMORY_STATUS {
        dwLength: std::mem::size_of::<GLOBAL_MEMORY_STATUS>() as u32,
        ..Default::default()
    };
    unsafe { windows::Win32::System::SystemServices::GlobalMemoryStatusEx(&mut status) }
        .ok()
        .ok()?;
    Some(status.ullTotalPhys / (1024 * 1024))
}

/// Logical CPU count.
#[must_use]
pub fn cpu_cores() -> Option<u32> {
    let mut info = SYSTEM_INFO::default();
    unsafe { GetSystemInfo(&mut info) };
    u32::try_from(info.dwNumberOfProcessors).ok()
}

/// Machine model (SMBIOS via GetSystemFirmwareTable "RSMB").
#[must_use]
pub fn machine_model() -> Option<String> {
    let size = unsafe { GetSystemFirmwareTable(0x5253_4D42, 0, None, 0) };
    if size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    let got = unsafe {
        GetSystemFirmwareTable(0x5253_4D42, 0, Some(buf.as_mut_ptr()), buf.len() as u32)
    };
    if got == 0 {
        return None;
    }
    // Parse minimal SMBIOS: header (8 bytes: u8 major, minor, u16 len,
    // u32 addr), then structures: header (u8 type, u8 len, u16 handle)
    // + strings. Type 1 = System Information: manufacturer (str 1),
    // product (str 2).
    let data = &buf[..got as usize];
    if data.len() < 8 {
        return None;
    }
    let struct_len = u16::from_le_bytes([data[6], data[7]]) as usize;
    if data.len() < 8 + struct_len {
        return None;
    }
    let mut pos = 8usize;
    let mut strings: Vec<String>;
    while pos + 4 <= data.len() {
        let stype = data[pos];
        let slen = data[pos + 1] as usize;
        if slen < 4 || pos + slen > data.len() {
            break;
        }
        if stype == 1 {
            // String area follows the formatted part.
            let area = &data[pos + slen..data.len().min(pos + slen + 256)];
            strings = parse_smbios_strings(area);
            if strings.len() >= 2 {
                let model = format!("{} {}", strings[0], strings[1]);
                return Some(model.trim().to_string());
            }
        }
        // Skip the formatted part + string area (double-NUL).
        let mut p = pos + slen;
        let mut nuls = 0;
        while p < data.len() && nuls < 2 {
            if data[p] == 0 {
                nuls += 1;
            } else {
                nuls = 0;
            }
            p += 1;
        }
        pos = p;
    }
    None
}

fn parse_smbios_strings(area: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    let mut nuls = 0;
    for &b in area {
        if b == 0 {
            nuls += 1;
            if nuls == 2 {
                break;
            }
            out.push(String::from_utf8_lossy(&cur).trim().to_string());
            cur.clear();
        } else {
            nuls = 0;
            cur.push(b);
        }
    }
    out
}

/// Time helpers for the FILETIME conversions the license scheduler
/// logs use (unix seconds ⇆ FILETIME).
#[must_use]
pub fn filetime_to_unix(ft: &FILETIME) -> i64 {
    let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
    ticks as i64 / 10_000_000 - 11_644_473_600
}

/// Unused-import silencers (kept types referenced by future callers).
#[allow(dead_code)]
fn _links() {
    let _ = hex(vec![0u8]);
    let _ = from_hex("00");
}
