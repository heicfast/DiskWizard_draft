//! `dw-cli selftest` — the cross-platform core proof: build a real
//! FAT16 volume image (boot sector + FAT + root dir + file data),
//! scan it with the production pipeline, recover the found file to a
//! temp dir, and byte-compare. Any host, no Windows required — the
//! CI's "test the real core without UI" smoke.

use std::process::ExitCode;

use dw_core::blockdev::BlockDevice;
use dw_core::fs::ScanControl;
use dw_core::scan::{NoObserver, ScanController};

use crate::logging::Log;

/// A minimal FAT16 volume with one 100-byte file (README.TXT at
/// cluster 2) — the same geometry the dw-core scan tests use.
struct FatImage {
    data: Vec<u8>,
    sector_size: u32,
}

fn fat_image() -> FatImage {
    // Cast discipline: the fixture geometry is tiny; truncation is
    // impossible by construction (documented exception).
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let total_sectors: u64 = 8192;
    #[allow(clippy::cast_possible_truncation)]
    let mut data = vec![0u8; 512 * total_sectors as usize];

    // Boot sector.
    let s = &mut data[..512];
    s[..3].copy_from_slice(b"\xEB\x3C\x90");
    s[0x0B..0x0D].copy_from_slice(&512u16.to_le_bytes()); // bytes/sector
    s[0x0D] = 1; // sectors/cluster
    s[0x0E..0x10].copy_from_slice(&1u16.to_le_bytes()); // reserved
    s[0x10] = 1; // FAT count
    s[0x11..0x13].copy_from_slice(&16u16.to_le_bytes()); // root entries
    s[0x13..0x15].copy_from_slice(&u16::try_from(total_sectors).unwrap_or(8192).to_le_bytes());
    s[0x16..0x18].copy_from_slice(&1u16.to_le_bytes()); // FAT sectors
    s[510..512].copy_from_slice(&[0x55, 0xAA]);

    // FAT: media byte + EOC markers for clusters 0-2, file chain EOC.
    let fat = 512usize;
    data[fat..fat + 2].copy_from_slice(&0xFFF8u16.to_le_bytes());
    data[fat + 2..fat + 4].copy_from_slice(&0xFFFFu16.to_le_bytes());
    data[fat + 4..fat + 6].copy_from_slice(&0xFFFFu16.to_le_bytes());

    // Root directory entry: README.TXT @ cluster 2, 100 bytes.
    let root = 2 * 512;
    let mut e = [0u8; 32];
    e[..11].copy_from_slice(b"README  TXT");
    e[0x1A..0x1C].copy_from_slice(&2u16.to_le_bytes());
    e[0x1C..0x20].copy_from_slice(&100u32.to_le_bytes());
    data[root..root + 32].copy_from_slice(&e);

    // File content (cluster 2 → byte 3*512).
    let at = 3 * 512;
    for (i, b) in data[at..at + 100].iter_mut().enumerate() {
        *b = u8::try_from(i % 251).unwrap_or(0);
    }

    FatImage {
        data,
        sector_size: 512,
    }
}

#[allow(clippy::cast_possible_truncation)]
mod ram {
    //! A minimal in-memory BlockDevice (the CLI can't reach dw-core's
    //! test-gated testutil, so this is the production-shaped local
    //! twin — strictly for the selftest fixtures; LBA math on a
    //! 4 MiB image cannot truncate.)
    use dw_core::blockdev::BlockDevice;
    use dw_core::error::{CoreError, CoreResult};

    pub struct RamDevice {
        pub data: Vec<u8>,
        pub sector_size: u32,
    }

    impl BlockDevice for RamDevice {
        fn sector_size(&self) -> u32 {
            self.sector_size
        }
        fn total_sectors(&self) -> u64 {
            (self.data.len() / self.sector_size as usize) as u64
        }
        fn read_sectors(&self, start_lba: u64, buf: &mut [u8]) -> CoreResult<()> {
            let ss = self.sector_size as usize;
            let off = start_lba as usize * ss;
            let end = off.checked_add(buf.len()).ok_or(CoreError::ReadFailed {
                offset: start_lba * ss as u64,
                len: buf.len() as u64,
                source: std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "overflow"),
            })?;
            if end > self.data.len() {
                return Err(CoreError::ReadFailed {
                    offset: start_lba * ss as u64,
                    len: buf.len() as u64,
                    source: std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "past end"),
                });
            }
            buf.copy_from_slice(&self.data[off..end]);
            Ok(())
        }
    }
}

pub fn cmd(args: &[String]) -> Result<ExitCode, String> {
    let verbose = crate::has_flag(args, "-v") || crate::has_flag(args, "--verbose");
    let log = Log::open(None, verbose);
    log.info("selftest: building the FAT16 fixture volume");

    let img = fat_image();
    let expected: Vec<u8> = img.data[3 * 512..3 * 512 + 100].to_vec();
    let expected_len = expected.len();
    let dev: std::sync::Arc<dyn BlockDevice> = std::sync::Arc::new(ram::RamDevice {
        data: img.data,
        sector_size: img.sector_size,
    });

    log.info("selftest: scanning (full production pipeline)");
    let mut controller = ScanController::new(dev.clone(), "SELFTEST (F:)");
    let (tree, report) = controller
        .run(&mut NoObserver)
        .map_err(|e| format!("SELFTEST_SCAN_FAILED. {e}"))?;
    log.info(&format!(
        "selftest: scan found {} files / {} bytes (fs {})",
        report.files_found,
        report.bytes_found,
        report.fs.wire()
    ));

    let readme = tree
        .nodes()
        .iter()
        .find(|n| n.name == "README.TXT")
        .ok_or("SELFTEST_ASSERT_FAILED. README.TXT not found by the engine")?;
    log.info(&format!(
        "selftest: README.TXT found ({} bytes, origin {}, path `{}`)",
        readme.size,
        readme.origin.wire(),
        readme.path
    ));

    // Recover it through the production export engine.
    let tmp = std::env::temp_dir().join(format!("dw-cli-selftest-{}", std::process::id()));
    let recover_root = tmp.clone();
    std::fs::create_dir_all(&tmp).map_err(|e| format!("SELFTEST_MKDIR_FAILED. {e}"))?;
    let queue = dw_core::recover::build_queue(tree.nodes(), &[readme.id]);
    let mut dest = FsDest { root: recover_root };
    let recovery = dw_core::recover::run_recovery(
        dev.as_ref(),
        &queue,
        &mut dest,
        &ScanControl::new(),
        |_| {},
    )
    .map_err(|e| format!("SELFTEST_RECOVER_FAILED. {e}"))?;
    log.info(&format!(
        "selftest: recovered {}/{} files",
        recovery.done_files, recovery.total_files
    ));

    // Byte-compare.
    let recovered = tmp.join("README.TXT");
    let got = std::fs::read(&recovered).map_err(|e| {
        format!(
            "SELFTEST_VERIFY_FAILED. cannot read {}: {e}",
            recovered.display()
        )
    })?;
    let _ = std::fs::remove_dir_all(&tmp);
    if got == expected {
        log.info("selftest: BYTE-COMPARE PASS — scan → recover → verify chain healthy");
        println!(
            "{}",
            serde_json::json!({
                "selftest": "pass",
                "filesFound": report.files_found,
                "bytesFound": report.bytes_found,
                "recovered": recovery.done_files,
                "byteCompare": "identical",
            })
        );
        Ok(ExitCode::SUCCESS)
    } else {
        log.error(&format!(
            "BYTE-COMPARE MISMATCH: {} vs {expected_len} bytes",
            got.len()
        ));
        Err("SELFTEST_BYTE_MISMATCH. The recovered file differs from the source.".into())
    }
}

/// The destination writer (real filesystem IO, traversal-guarded).
struct FsDest {
    root: std::path::PathBuf,
}

impl dw_core::recover::DestWriter for FsDest {
    fn mkdir(&mut self, path: &str) -> Result<(), String> {
        let target = self.root.join(path.replace('\\', "/"));
        std::fs::create_dir_all(&target).map_err(|e| format!("mkdir {path}: {e}"))
    }

    fn write_file(
        &mut self,
        path: &str,
        read: &mut dyn FnMut(&mut [u8]) -> usize,
    ) -> Result<(), String> {
        use std::io::Write as _;
        let target = self.root.join(path.replace('\\', "/"));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
        }
        let mut f = std::fs::File::create(&target).map_err(|e| format!("create {path}: {e}"))?;
        let mut buf = vec![0u8; 512 * 1024];
        loop {
            let n = read(&mut buf);
            if n == 0 {
                break;
            }
            f.write_all(&buf[..n])
                .map_err(|e| format!("write {path}: {e}"))?;
        }
        Ok(())
    }

    fn free_bytes(&self) -> Result<u64, String> {
        Ok(1 << 40) // permissive on every host
    }
}
