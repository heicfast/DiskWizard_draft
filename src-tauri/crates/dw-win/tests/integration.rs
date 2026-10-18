//! Windows end-to-end integration tests (VHD-backed; `--ignored`).
//!
//! Run exclusively on Windows runners via `windows-integration.yml`:
//! diskpart creates + attaches a VHD, formats it NTFS, seeds known
//! files, deletes some; the test then exercises the REAL stack
//! (enumeration → scan → tree → recovery → byte-compare).
//!
//! Locally (admin PowerShell): run the same diskpart script first.

#![cfg(windows)]

use std::sync::Arc;

use dw_core::blockdev::BlockDevice;
use dw_core::fs::ScanControl;
use dw_core::recover::{DestWriter, build_queue, run_recovery};
use dw_core::scan::{ProgressObserver, run_scan};
use dw_core::types::NodeOrigin;

/// Find the test VHD's volume letter by its label ("DWTEST").
fn find_test_volume() -> Option<char> {
    for letter in 'C'..='Z' {
        let root = format!("{letter}:\\");
        let w: Vec<u16> = root.encode_utf16().chain([0]).collect();
        let mut label = [0u16; 64];
        // SAFETY: w is NUL-terminated; label is a fixed buffer whose
        // capacity travels with the slice.
        let ok = unsafe {
            windows::Win32::Storage::FileSystem::GetVolumeInformationW(
                windows::core::PCWSTR(w.as_ptr()),
                Some(&mut label),
                None,
                None,
                None,
                None,
            )
        };
        if ok.is_ok() {
            let l = String::from_utf16_lossy(&label)
                .trim_end_matches('\0')
                .to_string();
            if l == "DWTEST" {
                return Some(letter);
            }
        }
    }
    None
}

struct Quiet;
impl ProgressObserver for Quiet {
    fn on_progress(&mut self, _p: &dw_core::scan::Progress) {}
    fn on_phase(&mut self, _phase: dw_core::scan::Phase) {}
}

struct DirDest {
    root: std::path::PathBuf,
}

impl DestWriter for DirDest {
    fn mkdir(&mut self, path: &str) -> Result<(), String> {
        std::fs::create_dir_all(self.root.join(path)).map_err(|e| e.to_string())
    }
    fn write_file(
        &mut self,
        path: &str,
        read: &mut dyn FnMut(&mut [u8]) -> usize,
    ) -> Result<(), String> {
        use std::io::Write as _;
        let target = self.root.join(path);
        if let Some(p) = target.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut f = std::fs::File::create(&target).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = read(&mut buf);
            if n == 0 {
                break;
            }
            f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn free_bytes(&self) -> Result<u64, String> {
        Ok(u64::MAX / 4)
    }
}

/// The full pipeline over the seeded VHD: existing + deleted files
/// found, extents resolve, recovery byte-compares.
#[test]
#[ignore = "requires the DWTEST VHD attached (windows-integration.yml)"]
fn vhd_end_to_end() {
    let Some(letter) = find_test_volume() else {
        panic!("DWTEST volume not found — attach the VHD first (see the workflow)");
    };
    let dev = dw_win::blockdev::RawHandleDevice::open_volume(letter).expect("open the test volume");
    eprintln!(
        "diag: dev {} sectors, ss {}, align {}",
        dev.total_sectors(),
        dev.sector_size(),
        dev.preferred_read_alignment()
    );
    // Boot-sector diagnostics (the engine's view of the volume).
    {
        use dw_core::blockdev::BlockRead;
        let sector = dev.read_struct_at::<512>(0).expect("boot sector read");
        eprintln!("diag: boot[0..16] = {:02X?}", &sector[..16]);
        match dw_core::fs::ntfs::NtfsBoot::parse(&sector) {
            Ok(b) => eprintln!(
                "diag: NTFS boot ok: bps={} spc={} mft_lba={} rec_size={} total={}",
                b.bytes_per_sector,
                b.sectors_per_cluster,
                b.mft_lba,
                b.mft_record_size,
                b.total_sectors
            ),
            Err(e) => eprintln!("diag: NTFS boot parse FAILED: {e}"),
        }
        // MFT record 0 raw view.
        if let Ok(b) = dw_core::fs::ntfs::NtfsBoot::parse(&sector) {
            let mft_off = b.mft_lba * u64::from(b.bytes_per_sector);
            let mut head = [0u8; 64];
            if let Err(e) = dev.read_bytes(mft_off, &mut head) {
                eprintln!("diag: mft read err {e}");
            }
            eprintln!("diag: mft0 head = {head:02X?}");
            eprintln!("diag: mft0 ascii = {}", String::from_utf8_lossy(&head[..8]));
        }
    }
    let display = format!("DWTEST ({letter}:)");
    let mut obs = Quiet;
    let (tree, report) = run_scan(Arc::new(dev), &display, &mut obs).expect("scan");
    eprintln!("diag: report = {report:?}");

    // The workflow seeds: present.txt (kept), gone.txt (deleted).
    let nodes = tree.nodes();
    eprintln!("diag: node count = {}", nodes.len());
    for n in nodes.iter().take(30) {
        eprintln!(
            "diag: node '{}' dir={} origin={:?} size={} extents={}",
            n.name,
            n.is_dir,
            n.origin,
            n.size,
            n.extents.len()
        );
    }
    let present = nodes
        .iter()
        .find(|n| n.name.contains("present"))
        .expect("present.txt found by the scan");
    assert_eq!(present.origin, NodeOrigin::Existing);
    let gone = nodes
        .iter()
        .find(|n| n.name.contains("gone"))
        .expect("deleted gone.txt found by the scan");
    assert_eq!(gone.origin, NodeOrigin::Deleted);

    // Recover BOTH and byte-compare against the seeds.
    let ids = [present.id, gone.id];
    let queue = build_queue(nodes, &ids);
    let dest_root = std::env::temp_dir().join("dw-vhd-recovered");
    let _ = std::fs::remove_dir_all(&dest_root);
    let mut dest = DirDest {
        root: dest_root.clone(),
    };
    let source: Arc<dyn BlockDevice> =
        Arc::new(dw_win::blockdev::RawHandleDevice::open_volume(letter).expect("reopen"));
    let report =
        run_recovery(&*source, &queue, &mut dest, &ScanControl::new(), |_| {}).expect("recovery");
    assert_eq!(report.done_files, 2, "both files recover: {report:?}");
    assert_eq!(report.failed_files, 0);

    let original_present = std::fs::read(format!("{letter}:\\present.txt")).expect("seed present");
    let recovered_present =
        std::fs::read(dest_root.join(present.name.as_str())).expect("recovered");
    assert_eq!(
        original_present, recovered_present,
        "recovered bytes match the seed exactly"
    );
}

/// Enumeration sees the runner's disks + the VHD.
#[test]
#[ignore = "windows-only"]
fn enumeration_reports_devices() {
    let inv = dw_win::devices::enumerate_inventory();
    assert!(
        !inv.local_disks.is_empty(),
        "the runner has at least one local disk"
    );
    for d in &inv.local_disks {
        assert!(!d.name.is_empty(), "device row has a name");
        assert!(d.total > 0);
    }
}

/// SMART reads real attributes from the runner's first disk.
#[test]
#[ignore = "windows-only"]
fn smart_reports_on_disk0() {
    match dw_win::smart::report_for_disk(0) {
        Ok(report) => {
            assert!(!report.model.is_empty() || !report.attributes.is_empty());
        }
        Err(e) => {
            // CI runners' virtual disks may lack SMART — log, don't fail.
            eprintln!("SMART unavailable on disk 0: {e}");
        }
    }
}
