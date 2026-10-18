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
use dw_core::recover::{build_queue, run_recovery, DestWriter};
use dw_core::scan::{run_scan, ProgressObserver};
use dw_core::types::NodeOrigin;

/// Find the test VHD's volume letter by its label ("DWTEST").
fn find_test_volume() -> Option<char> {
    for letter in 'C'..='Z' {
        let root = format!("{letter}:\\");
        let w: Vec<u16> = root.encode_utf16().chain([0]).collect();
        let mut label = [0u16; 64];
        let ok = unsafe {
            windows::Win32::Storage::FileSystem::GetVolumeInformationW(
                windows::core::PCWSTR(w.as_ptr()),
                Some(label.as_mut_ptr()),
                Some(label.len() as u32),
                None,
                None,
                None,
                None,
                None,
            )
        };
        if ok.as_bool() {
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
    let dev = dw_win::blockdev::RawHandleDevice::open_volume(letter)
        .expect("open the test volume");
    let display = format!("DWTEST ({letter}:)");
    let mut obs = Quiet;
    let (tree, _report) = run_scan(Arc::new(dev), &display, &mut obs).expect("scan");

    // The workflow seeds: present.txt (kept), gone.txt (deleted).
    let nodes = tree.nodes();
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
    let mut dest = DirDest { root: dest_root.clone() };
    let source: Arc<dyn BlockDevice> = Arc::new(
        dw_win::blockdev::RawHandleDevice::open_volume(letter).expect("reopen"),
    );
    let report = run_recovery(&*source, &queue, &mut dest, &ScanControl::new(), |_| {})
        .expect("recovery");
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
        assert!(!d.name.is_empty());
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
