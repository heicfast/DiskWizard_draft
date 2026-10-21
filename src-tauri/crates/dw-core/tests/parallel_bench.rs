//! Parallel-pipeline performance smoke (the engine-benchmark
//! workflow's fast lane): synthetic volumes sized to exercise the
//! worker pools, asserting THROUGHPUT FLOORS so a regression that
//! re-serializes the pipeline fails CI instead of shipping.
//!
//! Run: `cargo test -p dw-core --test parallel_bench -- --ignored --nocapture`
//! (host-core dependent — the floors are set far below the measured
//! headroom on a 2-vCPU CI runner).

#![allow(clippy::unreadable_literal)] // throughput math

use std::sync::Arc;
use std::time::Instant;

use dw_core::blockdev::BlockDevice;
use dw_core::error::CoreResult;
use dw_core::fs::{ScanControl, ScanSink};
use dw_core::types::FoundEntry;

/// A minimal in-memory device (the integration-test double —
/// `testutil` is crate-internal; this is the bench's own).
struct RamDevice {
    data: Vec<u8>,
    sector_size: u32,
}

impl RamDevice {
    fn new(len_bytes: usize, sector_size: u32) -> Self {
        Self {
            data: vec![0; len_bytes],
            sector_size,
        }
    }
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
        let start = start_lba as usize * ss;
        let end = start + buf.len();
        if end > self.data.len() || !buf.len().is_multiple_of(ss) {
            return Err(dw_core::CoreError::ReadFailed {
                offset: start_lba * ss as u64,
                len: buf.len() as u64,
                source: std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "bench"),
            });
        }
        buf.copy_from_slice(&self.data[start..end]);
        Ok(())
    }
}

/// A sink that counts entries (the terminal drain).
struct Counter {
    entries: u64,
    bytes: u64,
}

impl ScanSink for Counter {
    fn on_entry(&mut self, e: FoundEntry) {
        self.entries += 1;
        self.bytes += e.size;
    }
}

/// Build a synthetic NTFS volume with `records` MFT records of real
/// shape: $MFT record 0 with the data run, the root at 5, then
/// alternating in-use/deleted file records (each with $FILE_NAME +
/// a non-resident $DATA run) — the walk's actual parse path.
fn synthetic_ntfs(records: u64) -> RamDevice {
    assert!(records >= 16);
    // 512 B records = 512 B clusters: the MFT spans  clusters.
    let total_sectors = 16 + records + 64;
    let mut ram = RamDevice::new(512 * total_sectors as usize, 512);
    let mut boot = vec![0u8; 512];
    boot[..3].copy_from_slice(b"\xEB\x52\x90");
    boot[3..11].copy_from_slice(b"NTFS    ");
    boot[0x0B..0x0D].copy_from_slice(&512u16.to_le_bytes());
    boot[0x0D] = 1; // 1 sector/cluster
    boot[0x28..0x2C].copy_from_slice(&(total_sectors as u32).to_le_bytes());
    boot[0x30..0x38].copy_from_slice(&16u64.to_le_bytes()); // MFT @ cluster 16
    boot[0x40] = 0x01; // 512 B records
    boot[510..512].copy_from_slice(&[0x55, 0xAA]);
    ram.data[..512].copy_from_slice(&boot);

    let mft_off = 16 * 512;
    // Record 0: $MFT with the covering data run.
    let mft_clusters = records; // 512 B record per 512 B cluster
    {
        let mut r = vec![0u8; 512];
        r[..4].copy_from_slice(b"FILE");
        r[0x04..0x06].copy_from_slice(&48u16.to_le_bytes());
        r[0x06..0x08].copy_from_slice(&1u16.to_le_bytes());
        r[0x14..0x16].copy_from_slice(&48u16.to_le_bytes());
        r[0x16..0x18].copy_from_slice(&0x0001u16.to_le_bytes()); // in-use
        r[0x18..0x1C].copy_from_slice(&512u32.to_le_bytes());
        let mut d = [0u8; 0x48 + 6];
        d[0x00..0x04].copy_from_slice(&0x80u32.to_le_bytes());
        d[0x04..0x08].copy_from_slice(&((0x48 + 6) as u32).to_le_bytes());
        d[0x08] = 1; // non-resident
        d[0x20..0x22].copy_from_slice(&0x40u16.to_le_bytes());
        d[0x30..0x38].copy_from_slice(&(mft_clusters * 512).to_le_bytes());
        d[0x40] = 0x33; // len_size 3, off_size 3
        let len3 = mft_clusters.to_le_bytes(); // 3-byte run length
        d[0x41] = len3[0];
        d[0x42] = len3[1];
        d[0x43] = len3[2];
        d[0x44] = 16; // LCN 16, 3-byte offset
        r[48..48 + d.len()].copy_from_slice(&d);
        r[48 + d.len()..48 + d.len() + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        ram.data[mft_off..mft_off + 512].copy_from_slice(&r);
    }
    // The record builder (resident $FILE_NAME + non-resident $DATA).
    let build_record = |name: &str, in_use: bool, lcn: u64| {
        let mut r = vec![0u8; 512];
        r[..4].copy_from_slice(b"FILE");
        r[0x04..0x06].copy_from_slice(&48u16.to_le_bytes());
        r[0x06..0x08].copy_from_slice(&1u16.to_le_bytes());
        r[0x14..0x16].copy_from_slice(&48u16.to_le_bytes());
        let flags = u16::from(in_use); // the in-use bit
        r[0x16..0x18].copy_from_slice(&flags.to_le_bytes());
        r[0x18..0x1C].copy_from_slice(&512u32.to_le_bytes());
        r[0x30..0x32].copy_from_slice(&0x0001u16.to_le_bytes());
        r[510..512].copy_from_slice(&0x0001u16.to_le_bytes());
        let name_u16: Vec<u16> = name.encode_utf16().collect();
        // $FILE_NAME.
        let content = 0x18usize;
        let fn_len = content + 0x42 + name_u16.len() * 2;
        let mut a = vec![0u8; fn_len];
        a[0x00..0x04].copy_from_slice(&0x30u32.to_le_bytes());
        a[0x04..0x08].copy_from_slice(&(fn_len as u32).to_le_bytes());
        a[0x10..0x14].copy_from_slice(&((fn_len - content) as u32).to_le_bytes()); // content len
        a[0x14..0x16].copy_from_slice(&(content as u16).to_le_bytes());
        a[content..content + 8].copy_from_slice(&5u64.to_le_bytes()); // parent = root
        a[content + 0x40] = name_u16.len() as u8;
        a[content + 0x41] = 3; // Win32&DOS
        for (i, u) in name_u16.iter().enumerate() {
            a[content + 0x42 + i * 2..content + 0x44 + i * 2].copy_from_slice(&u.to_le_bytes());
        }
        r[48..48 + fn_len].copy_from_slice(&a);
        // $DATA (non-resident): a 2-cluster run at lcn (1-byte
        // runlist fields keep the encoding trivial).
        let pos = 48 + fn_len;
        let run: [u8; 3] = [0x11, 0x02, lcn as u8];
        let dl = 0x48 + run.len();
        let mut d = [0u8; 0x4B];
        d[0x00..0x04].copy_from_slice(&0x80u32.to_le_bytes());
        d[0x04..0x08].copy_from_slice(&(dl as u32).to_le_bytes());
        d[0x08] = 1; // non-resident
        d[0x20..0x22].copy_from_slice(&0x48u16.to_le_bytes());
        d[0x30..0x38].copy_from_slice(&1024u64.to_le_bytes());
        d[0x38..0x40].copy_from_slice(&1024u64.to_le_bytes());
        d[0x48..0x48 + run.len()].copy_from_slice(&run);
        r[pos..pos + dl].copy_from_slice(&d);
        r
    };
    // Root at 5, then records: in-use files at even slots, deleted at
    // odd slots (the walk sees both paths).
    ram.data[mft_off + 5 * 512..mft_off + 6 * 512].copy_from_slice(&build_record(".", true, 0));
    for rec in 6..records {
        let in_use = rec % 2 == 0;
        let name = format!("bench{rec:06}.dat");
        let lcn = 16 + (rec % 60) + 4;
        let r = build_record(&name, in_use, lcn);
        let at = mft_off + rec as usize * 512;
        ram.data[at..at + 512].copy_from_slice(&r);
    }
    ram
}

/// The MFT walk throughput floor: 200 000 records must parse (read +
/// fixup + attributes + emission) faster than 60 s — the measured
/// headroom on a 2-vCPU CI runner is several times that; a pipeline
/// regression (re-serialized reads, a stalled drain) falls through.
#[test]
#[ignore = "perf: run in the engine-benchmark workflow"]
fn parallel_mft_walk_throughput_floor() {
    const RECORDS: u64 = 200_000;
    let ram = synthetic_ntfs(RECORDS);
    let dev: Arc<dyn BlockDevice> = Arc::new(ram);
    let mut engine = dw_core::fs::create_engine(dw_core::types::FsKind::Ntfs, dev).expect("engine");
    let mut sink = Counter {
        entries: 0,
        bytes: 0,
    };
    let t0 = Instant::now();
    let stats = engine
        .quick_scan(&mut sink, &ScanControl::new())
        .expect("walk");
    let secs = t0.elapsed().as_secs_f64();
    #[allow(clippy::cast_precision_loss)] // throughput display
    let rps = RECORDS as f64 / secs;
    println!(
        "parallel MFT walk: {RECORDS} records in {secs:.2}s = {rps:.0} rec/s, {} entries, {} bytes",
        stats.entries, stats.bytes
    );
    assert!(stats.entries > RECORDS / 3, "the walk emitted entries");
    assert!(
        secs < 60.0,
        "throughput floor blown: {rps:.0} rec/s ({secs:.1}s for {RECORDS} records)"
    );
}

/// The parallel carve floor: 256 MiB of signature-dense synthetic
/// data carves faster than 90 s (memchr + the worker pool; the
/// floor sits far under the measured headroom).
#[test]
#[ignore = "perf: run in the engine-benchmark workflow"]
fn parallel_carve_throughput_floor() {
    const SPAN: u64 = 256 * 1024 * 1024;
    const CLUSTER: usize = 4096;
    let sectors = (SPAN / 512) as usize;
    let mut ram = RamDevice::new(512 * sectors, 512);
    // Seed: a JPEG every 4 KiB (signature-dense — the worst case for
    // the validators).
    let jpeg_prefix: Vec<u8> = [
        &[0xFFu8, 0xD8, 0xFF, 0xE0, 0, 0x10][..],
        b"JFIF",
        &[0u8, 1, 1, 0, 0, 1, 0, 1, 0, 0][..],
        &[
            0xFF, 0xC0, 0, 0x11, 8, 0x02, 0x80, 0x01, 0xE0, 3, 0x01, 0x11, 0, 2, 0x11, 0x01, 0x03,
            0x11, 0x01,
        ][..],
    ]
    .concat();
    let jpeg_suffix = [0xFFu8, 0xD9];
    for off in (0..ram.data.len()).step_by(CLUSTER) {
        let end = (off + CLUSTER).min(ram.data.len());
        let body = end - off;
        let pre = jpeg_prefix.len().min(body);
        ram.data[off..off + pre].copy_from_slice(&jpeg_prefix[..pre]);
        if body >= 2 {
            ram.data[end - 2..end].copy_from_slice(&jpeg_suffix);
        }
    }
    let dev: Arc<dyn BlockDevice> = Arc::new(ram);
    let carver = dw_core::raw::RawCarver::new(dev);
    let mut sink = Counter {
        entries: 0,
        bytes: 0,
    };
    let t0 = Instant::now();
    let stats = carver
        .carve(&mut sink, &ScanControl::new(), 0, SPAN)
        .expect("carve");
    let secs = t0.elapsed().as_secs_f64();
    #[allow(clippy::cast_precision_loss)] // throughput display
    let mbps = SPAN as f64 / secs / (1024.0 * 1024.0);
    println!(
        "parallel carve: {mbps:.0} MiB/s over 256 MiB, {} entries, {} bytes",
        stats.entries, stats.bytes
    );
    assert!(stats.entries > 10_000, "the signature density carved");
    assert!(
        secs < 90.0,
        "carve throughput floor blown: {mbps:.1} MiB/s ({secs:.1}s)"
    );
}
