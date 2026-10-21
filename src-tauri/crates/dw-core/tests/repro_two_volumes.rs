//! Local reproduction: whole-disk scan with TWO NTFS volumes (one in
//! the table, one deleted) — the mft-dirs collision test.
#[test]
#[allow(clippy::too_many_lines)] // The fixture builder; one synthetic volume writer.
fn two_ntfs_volumes_whole_disk_keep_their_branches() {
    use dw_core::blockdev::BlockDevice;
    use dw_core::error::CoreResult;
    use dw_core::scan::{NoObserver, run_scan};
    use std::sync::Arc;

    struct Ram(Vec<u8>);
    impl BlockDevice for Ram {
        fn sector_size(&self) -> u32 {
            512
        }
        fn total_sectors(&self) -> u64 {
            (self.0.len() / 512) as u64
        }
        fn read_sectors(&self, lba: u64, buf: &mut [u8]) -> CoreResult<()> {
            let start = lba as usize * 512;
            let end = start + buf.len();
            if end > self.0.len() || !buf.len().is_multiple_of(512) {
                return Err(dw_core::CoreError::ReadFailed {
                    offset: lba * 512,
                    len: buf.len() as u64,
                    source: std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "x"),
                });
            }
            buf.copy_from_slice(&self.0[start..end]);
            Ok(())
        }
    }

    let ss = 512usize;
    let total_sectors = 40 * 2048usize; // 20 MiB disk
    let mut ram = Ram(vec![0u8; ss * total_sectors]);

    // MBR: one partition [2048, 2048+16M) (8 MiB), W: deleted.
    ram.0[510..512].copy_from_slice(&[0x55, 0xAA]);
    let e = 0x1BE;
    ram.0[e + 4] = 0x07;
    ram.0[e + 8..e + 12].copy_from_slice(&2048u32.to_le_bytes());
    ram.0[e + 12..e + 16].copy_from_slice(&16384u32.to_le_bytes());

    // A minimal NTFS volume writer at a given start LBA: boot + a
    // tiny MFT with root(5) + one dir(6) + one file(7).
    fn write_vol(ram: &mut Ram, start_lba: usize, label: &str) {
        let base = start_lba * 512;
        let mut boot = vec![0u8; 512];
        boot[..3].copy_from_slice(b"\xEB\x52\x90");
        boot[3..11].copy_from_slice(b"NTFS    ");
        boot[0x0B..0x0D].copy_from_slice(&512u16.to_le_bytes());
        boot[0x0D] = 1; // 1 sector/cluster
        let vol_sectors = 16384u32;
        boot[0x28..0x2C].copy_from_slice(&vol_sectors.to_le_bytes());
        boot[0x30..0x38].copy_from_slice(&16u64.to_le_bytes()); // MFT @ cluster 16 (vol-relative)
        boot[0x40] = 0x01; // 512 B records
        boot[510..512].copy_from_slice(&[0x55, 0xAA]);
        ram.0[base..base + 512].copy_from_slice(&boot);

        let mft = base + 16 * 512;
        let rec = |_no: usize, parent: u64, name: &str, flags: u16, is_dir: bool| {
            let mut r = vec![0u8; 512];
            r[..4].copy_from_slice(b"FILE");
            r[0x04..0x06].copy_from_slice(&48u16.to_le_bytes());
            r[0x06..0x08].copy_from_slice(&1u16.to_le_bytes());
            r[0x14..0x16].copy_from_slice(&48u16.to_le_bytes());
            r[0x16..0x18].copy_from_slice(&flags.to_le_bytes());
            r[0x18..0x1C].copy_from_slice(&512u32.to_le_bytes());
            r[0x30..0x32].copy_from_slice(&0x0001u16.to_le_bytes());
            r[510..512].copy_from_slice(&0x0001u16.to_le_bytes());
            let nu: Vec<u16> = name.encode_utf16().collect();
            let content = 0x18usize;
            let fn_len = content + 0x42 + nu.len() * 2;
            let mut a = vec![0u8; fn_len];
            a[0x00..0x04].copy_from_slice(&0x30u32.to_le_bytes());
            a[0x04..0x08].copy_from_slice(&(fn_len as u32).to_le_bytes());
            a[0x10..0x14].copy_from_slice(&((fn_len - content) as u32).to_le_bytes());
            a[0x14..0x16].copy_from_slice(&(content as u16).to_le_bytes());
            a[content..content + 8].copy_from_slice(&parent.to_le_bytes());
            a[content + 0x40] = nu.len() as u8;
            a[content + 0x41] = 3;
            for (i, u) in nu.iter().enumerate() {
                a[content + 0x42 + i * 2..content + 0x44 + i * 2].copy_from_slice(&u.to_le_bytes());
            }
            r[48..48 + fn_len].copy_from_slice(&a);
            if !is_dir {
                // resident $DATA
                let pos = 48 + fn_len;
                let d = [0u8; 0x48 + 2];
                let mut d2 = d;
                d2[0x00..0x04].copy_from_slice(&0x80u32.to_le_bytes());
                d2[0x04..0x08].copy_from_slice(&((0x48 + 2) as u32).to_le_bytes());
                d2[0x08] = 0;
                d2[0x10..0x14].copy_from_slice(&2u32.to_le_bytes());
                d2[0x14..0x16].copy_from_slice(&0x18u16.to_le_bytes());
                r[pos..pos + d2.len()].copy_from_slice(&d2);
            }
            r
        };
        // record 0: $MFT with data run (16 clusters @ 16)
        let mut r0 = vec![0u8; 512];
        r0[..4].copy_from_slice(b"FILE");
        r0[0x04..0x06].copy_from_slice(&48u16.to_le_bytes());
        r0[0x06..0x08].copy_from_slice(&1u16.to_le_bytes());
        r0[0x14..0x16].copy_from_slice(&48u16.to_le_bytes());
        r0[0x16..0x18].copy_from_slice(&0x0001u16.to_le_bytes());
        r0[0x18..0x1C].copy_from_slice(&512u32.to_le_bytes());
        r0[0x30..0x32].copy_from_slice(&0x0001u16.to_le_bytes());
        r0[510..512].copy_from_slice(&0x0001u16.to_le_bytes());
        let mut d = [0u8; 0x48 + 3];
        d[0x00..0x04].copy_from_slice(&0x80u32.to_le_bytes());
        d[0x04..0x08].copy_from_slice(&((0x48 + 3) as u32).to_le_bytes());
        d[0x08] = 1;
        d[0x20..0x22].copy_from_slice(&0x40u16.to_le_bytes());
        d[0x30..0x38].copy_from_slice(&(16u64 * 512).to_le_bytes());
        d[0x40] = 0x11;
        d[0x41] = 16; // 16 clusters
        d[0x42] = 16; // LCN 16
        r0[48..48 + d.len()].copy_from_slice(&d);
        r0[48 + d.len()..48 + d.len() + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        ram.0[mft..mft + 512].copy_from_slice(&r0);
        ram.0[mft + 5 * 512..mft + 6 * 512].copy_from_slice(&rec(5, 5, ".", 0x0003, true));
        let dir_name = format!("docs_{label}");
        ram.0[mft + 6 * 512..mft + 7 * 512].copy_from_slice(&rec(6, 5, &dir_name, 0x0003, true));
        let file_name = format!("f_{label}.txt");
        ram.0[mft + 7 * 512..mft + 8 * 512].copy_from_slice(&rec(7, 6, &file_name, 0x0001, false));
    }
    write_vol(&mut ram, 2048, "L"); // Partition 1
    write_vol(&mut ram, 2048 + 16384, "W"); // the deleted volume (16 MiB in)

    let dev: Arc<dyn BlockDevice> = Arc::new(ram);
    let (tree, report) = run_scan(dev, "Disk", &mut NoObserver).expect("scan");
    assert_eq!(report.files_found, 2, "BOTH volumes' files surface");
    let paths: Vec<&str> = tree
        .nodes()
        .iter()
        .filter(|n| !n.is_dir)
        .map(|n| n.path.as_str())
        .collect();
    assert!(
        paths.contains(&"Disk\\Partition 1\\docs_L\\f_L.txt"),
        "the in-table volume's file under its own branch: {paths:?}"
    );
    assert!(
        paths.contains(&"Disk\\Lost Partition 1\\docs_W\\f_W.txt"),
        "the deleted volume's file under its own branch: {paths:?}"
    );
}

/// Diagnostic: same fixture, but inspect scan_units + each engine alone.
#[test]
#[ignore = "diagnostic: the per-unit engine probe used during the session-11 fix"]
#[allow(clippy::too_many_lines)] // The diagnostic fixture mirror.
fn repro_two_volumes_diag() {
    use dw_core::blockdev::SubDevice;
    use dw_core::blockdev::{BlockDevice, CachedDevice};
    use dw_core::fs::{ScanControl, ScanSink, create_engine, probe_filesystem};
    use dw_core::types::FoundEntry;
    use std::sync::Arc;

    struct Ram(Vec<u8>);
    impl BlockDevice for Ram {
        fn sector_size(&self) -> u32 {
            512
        }
        fn total_sectors(&self) -> u64 {
            (self.0.len() / 512) as u64
        }
        fn read_sectors(&self, lba: u64, buf: &mut [u8]) -> dw_core::CoreResult<()> {
            let start = lba as usize * 512;
            let end = start + buf.len();
            if end > self.0.len() || !buf.len().is_multiple_of(512) {
                return Err(dw_core::CoreError::ReadFailed {
                    offset: lba * 512,
                    len: buf.len() as u64,
                    source: std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "x"),
                });
            }
            buf.copy_from_slice(&self.0[start..end]);
            Ok(())
        }
    }
    let mut ram = Ram(vec![0u8; 512 * 40 * 2048]);
    ram.0[510..512].copy_from_slice(&[0x55, 0xAA]);
    let e = 0x1BE;
    ram.0[e + 4] = 0x07;
    ram.0[e + 8..e + 12].copy_from_slice(&2048u32.to_le_bytes());
    ram.0[e + 12..e + 16].copy_from_slice(&16384u32.to_le_bytes());

    fn write_vol(ram: &mut Ram, start_lba: usize, label: &str) {
        let base = start_lba * 512;
        let mut boot = vec![0u8; 512];
        boot[..3].copy_from_slice(b"\xEB\x52\x90");
        boot[3..11].copy_from_slice(b"NTFS    ");
        boot[0x0B..0x0D].copy_from_slice(&512u16.to_le_bytes());
        boot[0x0D] = 1;
        boot[0x28..0x2C].copy_from_slice(&16384u32.to_le_bytes());
        boot[0x30..0x38].copy_from_slice(&16u64.to_le_bytes());
        boot[0x40] = 0x01;
        boot[510..512].copy_from_slice(&[0x55, 0xAA]);
        ram.0[base..base + 512].copy_from_slice(&boot);
        let mft = base + 16 * 512;
        let rec = |parent: u64, name: &str, flags: u16| {
            let mut r = vec![0u8; 512];
            r[..4].copy_from_slice(b"FILE");
            r[0x04..0x06].copy_from_slice(&48u16.to_le_bytes());
            r[0x06..0x08].copy_from_slice(&1u16.to_le_bytes());
            r[0x14..0x16].copy_from_slice(&48u16.to_le_bytes());
            r[0x16..0x18].copy_from_slice(&flags.to_le_bytes());
            r[0x18..0x1C].copy_from_slice(&512u32.to_le_bytes());
            r[0x30..0x32].copy_from_slice(&0x0001u16.to_le_bytes());
            r[510..512].copy_from_slice(&0x0001u16.to_le_bytes());
            let nu: Vec<u16> = name.encode_utf16().collect();
            let content = 0x18usize;
            let fn_len = content + 0x42 + nu.len() * 2;
            let mut a = vec![0u8; fn_len];
            a[0x00..0x04].copy_from_slice(&0x30u32.to_le_bytes());
            a[0x04..0x08].copy_from_slice(&(fn_len as u32).to_le_bytes());
            a[0x10..0x14].copy_from_slice(&((fn_len - content) as u32).to_le_bytes());
            a[0x14..0x16].copy_from_slice(&(content as u16).to_le_bytes());
            a[content..content + 8].copy_from_slice(&parent.to_le_bytes());
            a[content + 0x40] = nu.len() as u8;
            a[content + 0x41] = 3;
            for (i, u) in nu.iter().enumerate() {
                a[content + 0x42 + i * 2..content + 0x44 + i * 2].copy_from_slice(&u.to_le_bytes());
            }
            r[48..48 + fn_len].copy_from_slice(&a);
            let pos = 48 + fn_len;
            let mut d = [0u8; 0x4A];
            d[0x00..0x04].copy_from_slice(&0x80u32.to_le_bytes());
            d[0x04..0x08].copy_from_slice(&0x4A_u32.to_le_bytes());
            d[0x10..0x14].copy_from_slice(&2u32.to_le_bytes());
            d[0x14..0x16].copy_from_slice(&0x18u16.to_le_bytes());
            r[pos..pos + d.len()].copy_from_slice(&d);
            r
        };
        let mut r0 = vec![0u8; 512];
        r0[..4].copy_from_slice(b"FILE");
        r0[0x04..0x06].copy_from_slice(&48u16.to_le_bytes());
        r0[0x06..0x08].copy_from_slice(&1u16.to_le_bytes());
        r0[0x14..0x16].copy_from_slice(&48u16.to_le_bytes());
        r0[0x16..0x18].copy_from_slice(&0x0001u16.to_le_bytes());
        r0[0x18..0x1C].copy_from_slice(&512u32.to_le_bytes());
        r0[0x30..0x32].copy_from_slice(&0x0001u16.to_le_bytes());
        r0[510..512].copy_from_slice(&0x0001u16.to_le_bytes());
        let mut d = [0u8; 0x48 + 3];
        d[0x00..0x04].copy_from_slice(&0x80u32.to_le_bytes());
        d[0x04..0x08].copy_from_slice(&((0x48 + 3) as u32).to_le_bytes());
        d[0x08] = 1;
        d[0x20..0x22].copy_from_slice(&0x40u16.to_le_bytes());
        d[0x30..0x38].copy_from_slice(&(16u64 * 512).to_le_bytes());
        d[0x40] = 0x11;
        d[0x41] = 16;
        d[0x42] = 16;
        r0[48..48 + d.len()].copy_from_slice(&d);
        r0[48 + d.len()..48 + d.len() + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        ram.0[mft..mft + 512].copy_from_slice(&r0);
        ram.0[mft + 5 * 512..mft + 6 * 512].copy_from_slice(&rec(5, ".", 0x0003));
        let dir = format!("docs_{label}");
        ram.0[mft + 6 * 512..mft + 7 * 512].copy_from_slice(&rec(5, &dir, 0x0003));
        let file = format!("f_{label}.txt");
        ram.0[mft + 7 * 512..mft + 8 * 512].copy_from_slice(&rec(6, &file, 0x0001));
    }
    write_vol(&mut ram, 2048, "L");
    write_vol(&mut ram, 2048 + 16384, "W");

    let cached = Arc::new(CachedDevice::new(Arc::new(ram)));
    let dev: Arc<dyn BlockDevice> = Arc::clone(&cached) as Arc<dyn BlockDevice>;
    // The partition units exactly as scan_units builds them.
    let sub_l = Arc::new(SubDevice::new(
        Arc::clone(&dev) as Arc<dyn BlockDevice>,
        2048,
        16384,
        "part1".to_string(),
    ));
    let sub_w = Arc::new(SubDevice::new(
        Arc::clone(&dev) as Arc<dyn BlockDevice>,
        2048 + 16384,
        16384,
        "lost0".to_string(),
    ));
    let dl: Arc<dyn BlockDevice> = Arc::clone(&sub_l) as Arc<dyn BlockDevice>;
    let dw: Arc<dyn BlockDevice> = Arc::clone(&sub_w) as Arc<dyn BlockDevice>;
    for (name, d) in [("L-in-table", &dl), ("W-lost", &dw)] {
        let fs = probe_filesystem(d.as_ref()).unwrap();
        let mut engine = match create_engine(fs, Arc::clone(d)) {
            Ok(e) => e,
            Err(e) => {
                println!("{name}: create_engine FAILED: {e:?}");
                continue;
            }
        };
        struct C {
            v: Vec<String>,
        }
        impl ScanSink for C {
            fn on_entry(&mut self, e: FoundEntry) {
                self.v.push(format!("{} (rec {:?})", e.name, e.record_id));
            }
        }
        let mut sink = C { v: Vec::new() };
        match engine.quick_scan(&mut sink, &ScanControl::new()) {
            Ok(st) => println!("{name}: fs={:?} entries={} → {:?}", fs, st.entries, sink.v),
            Err(e) => println!("{name}: quick_scan FAILED: {e:?}"),
        }
    }
    println!("--- full pipeline (run_scan) as in the other test ---");
}
