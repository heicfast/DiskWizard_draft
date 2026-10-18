//! Hardware-identity seam — the platform fingerprint providers.
//!
//! The FROZEN composite algorithm (doc §3.5, unchanged so v1
//! activations re-match): `SHA-256(MachineGuid ‖ system-drive volume
//! serial ‖ CPUID)` on Windows. The concrete providers live in
//! `dw-win` (registry + volume APIs + CPUID); this seam keeps the
//! licensing crate platform-independent and unit-testable.

/// The platform providers injected by the app layer.
pub trait HwIdProvider: Send + Sync {
    /// `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid`.
    /// # Errors
    /// String when unreadable.
    fn machine_guid(&self) -> Result<String, String>;

    /// The system-drive volume serial (DWORD, as on the wire little-
    /// endian — hashed as 4 raw bytes by the caller).
    /// # Errors
    /// String when unreadable.
    fn system_drive_serial(&self) -> Result<u32, String>;

    /// The CPUID brand string (leaf 0x80000002..05).
    /// # Errors
    /// String when unavailable.
    fn cpuid_brand(&self) -> Result<String, String>;
}

/// Compute the composite hardware fingerprint from provider inputs
/// (pure — tests pin the recipe; the algorithm is FROZEN).
pub fn hardware_id_from<P: HwIdProvider>(p: &P) -> Result<String, String> {
    let machine_guid = p.machine_guid()?;
    let serial = p.system_drive_serial()?;
    let cpuid = p.cpuid_brand()?;
    let mut hasher = sha2::Sha256::new();
    use sha2::Digest;
    hasher.update(machine_guid.as_bytes());
    hasher.update(serial.to_le_bytes());
    hasher.update(cpuid.as_bytes());
    Ok(crate::hex(&hasher.finalize()))
}

/// Hash one component identity: `sha256(label || value)`.
#[must_use]
pub fn component_hash(label: &str, value: Option<String>) -> Option<String> {
    let v = value?;
    let mut hasher = sha2::Sha256::new();
    use sha2::Digest;
    hasher.update(label.as_bytes());
    hasher.update(v.as_bytes());
    Some(crate::hex(&hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedProvider;

    impl HwIdProvider for FixedProvider {
        fn machine_guid(&self) -> Result<String, String> {
            Ok("12345678-1234-1234-1234-123456789012".into())
        }
        fn system_drive_serial(&self) -> Result<u32, String> {
            Ok(0xDEAD_BEEF)
        }
        fn cpuid_brand(&self) -> Result<String, String> {
            Ok("Test CPU @ 3.00GHz".into())
        }
    }

    #[test]
    fn composite_recipe_is_pinned() {
        use sha2::Digest;
        let id = hardware_id_from(&FixedProvider).unwrap();
        let mut h = sha2::Sha256::new();
        h.update(b"12345678-1234-1234-1234-123456789012");
        h.update(0xDEAD_BEEF_u32.to_le_bytes());
        h.update(b"Test CPU @ 3.00GHz");
        assert_eq!(id, crate::hex(&h.finalize()));
        assert_eq!(id.len(), 64);
    }

    #[test]
    fn component_recipe_is_pinned() {
        use sha2::Digest;
        let c = component_hash("machine:", Some("guid-x".into())).unwrap();
        let mut h = sha2::Sha256::new();
        h.update(b"machine:guid-x");
        assert_eq!(c, crate::hex(&h.finalize()));
        assert!(component_hash("machine:", None).is_none());
    }
}
