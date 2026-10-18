//! EBR (Extended Boot Record) chain walking — logical partitions
//! inside MBR extended containers.
//!
//! Layout (classic): the container entry points at the first EBR;
//! each EBR holds one real entry + a link entry to the next EBR.
//! Both child LBAs are EBR-RELATIVE. The walk is cycle-guarded
//! (linked-list corruption is routine on damaged disks — the
//! FAT-cycle-guard discipline from the research applies here too).

use crate::error::{CoreError, CoreResult};

use super::{PartitionInfo, has_dos_signature, mbr_type_to_fs};

/// Walk the EBR chain starting at `ebr0_lba`.
///
/// Returns logical partitions with ABSOLUTE start LBAs. Errors
/// mid-chain degrade into partial results + a salvage note (the walk
/// never fails the whole device).
pub fn parse_ebr_chain(
    dev: &dyn crate::blockdev::BlockDevice,
    ebr0_lba: u64,
    device_sectors: u64,
    start_index: u32,
) -> CoreResult<Vec<PartitionInfo>> {
    let mut out = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut current = ebr0_lba;
    let mut index = start_index;
    let ss = dev.sector_size() as usize;
    let mut sector = vec![0u8; ss];
    loop {
        if current == 0 || !visited.insert(current) {
            break; // cycle or terminal
        }
        if current >= device_sectors {
            break;
        }
        if dev.read_sectors(current, &mut sector).is_err() {
            break; // unreadable EBR — keep what we have
        }
        if !has_dos_signature(&sector) {
            break;
        }
        let read_u32 = |off: usize| {
            u32::from_le_bytes([
                sector[off],
                sector[off + 1],
                sector[off + 2],
                sector[off + 3],
            ]) as u64
        };
        let first_type = sector[0x1BE + 4];
        let first_start = read_u32(0x1BE + 8);
        let first_len = read_u32(0x1BE + 12);
        if first_type != 0 && first_len > 0 {
            let abs = current + first_start;
            if abs < device_sectors {
                index += 1;
                out.push(PartitionInfo {
                    index,
                    start_lba: abs,
                    total_sectors: first_len.min(device_sectors - abs),
                    fs: mbr_type_to_fs(first_type),
                    in_table: true,
                    type_tag: format!("ebr-0x{first_type:02X}"),
                });
            }
        }
        let link_type = sector[0x1CE + 4];
        let link_start = read_u32(0x1CE + 8);
        if link_type == 0 || link_start == 0 {
            break;
        }
        current = ebr0_lba + link_start;
    }
    if out.is_empty() && ebr0_lba != 0 {
        // An extended container with no readable children: surface as a
        // salvage note, not a hard error.
        return Err(CoreError::SalvageSkipped {
            structure: "EBR chain",
            at: ebr0_lba,
            detail: "no readable logical entries".into(),
        });
    }
    Ok(out)
}

/// Test-only helper: build a 512-byte EBR sector.
#[cfg(test)]
pub(crate) fn build_ebr(first: Option<(u8, u32, u32)>, link: Option<u32>, base: &mut [u8]) {
    base[..512].fill(0);
    if let Some((t, s, l)) = first {
        base[0x1BE] = 0x00;
        base[0x1BE + 4] = t;
        base[0x1BE + 8..0x1BE + 12].copy_from_slice(&s.to_le_bytes());
        base[0x1BE + 12..0x1BE + 16].copy_from_slice(&l.to_le_bytes());
    }
    if let Some(next) = link {
        base[0x1CE + 4] = 0x05;
        base[0x1CE + 8..0x1CE + 12].copy_from_slice(&next.to_le_bytes());
    }
    base[510] = 0x55;
    base[511] = 0xAA;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::RamDevice;
    use crate::types::FsKind;

    #[test]
    fn walks_a_two_link_chain() {
        let mut ram = RamDevice::new(512 * 10_000, 512);
        // EBR0 at 1000: logical (type 0x07) at +63 len 1000; link to +2000
        let mut ebr0 = vec![0u8; 512];
        build_ebr(Some((0x07, 63, 1000)), Some(2000), &mut ebr0);
        ram.data[1000 * 512..1001 * 512].copy_from_slice(&ebr0);
        // EBR1 at 3000 (1000+2000): logical at +63 len 800; no link
        let mut ebr1 = vec![0u8; 512];
        build_ebr(Some((0x0B, 63, 800)), None, &mut ebr1);
        ram.data[3000 * 512..3001 * 512].copy_from_slice(&ebr1);

        let parts = parse_ebr_chain(&ram, 1000, 10_000, 4).expect("chain");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].start_lba, 1063);
        assert_eq!(parts[0].total_sectors, 1000);
        assert_eq!(parts[1].start_lba, 3063);
        assert_eq!(parts[1].fs, FsKind::Fat32);
        assert_eq!(parts[0].index, 5);
        assert_eq!(parts[1].index, 6);
    }

    #[test]
    fn cycle_guard_terminates() {
        let mut ram = RamDevice::new(512 * 1000, 512);
        // EBR at 10 links to itself (relative 0 → ebr0 + 0).
        let mut ebr = vec![0u8; 512];
        build_ebr(Some((0x83, 63, 100)), Some(0), &mut ebr);
        ram.data[10 * 512..11 * 512].copy_from_slice(&ebr);
        let parts = parse_ebr_chain(&ram, 10, 1000, 0).expect("chain");
        assert_eq!(parts.len(), 1);
    }

    #[test]
    fn unreadable_ebr_yields_partial() {
        let mut ram = RamDevice::new(512 * 10_000, 512);
        let mut ebr0 = vec![0u8; 512];
        build_ebr(Some((0x07, 63, 100)), Some(2000), &mut ebr0);
        ram.data[100 * 512..101 * 512].copy_from_slice(&ebr0); // wrong spot
        // No valid EBR at 1000: chain yields nothing → salvage error.
        let r = parse_ebr_chain(&ram, 1000, 10_000, 0);
        assert!(r.is_err());
    }
}
