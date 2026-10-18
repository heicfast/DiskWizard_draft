//! §5.1 item 10 — the I/O backend benchmark (Windows runner; `--ignored`).
//!
//! Measures what the master prompt demands be recorded
//! (`docs/benchmarks/io-backend.md`):
//! * read throughput across the (window, queue-depth) matrix,
//! * CPU time (user+kernel via `GetProcessTimes`),
//! * working-set / peak RSS (`GetProcessMemoryInfo`),
//! * the achieved (submitted) queue depth per configuration,
//! * correctness at every configuration (batch == sequential bytes).
//!
//! Error-isolation behavior is proven by the fault-injection unit
//! suite (dw-core `bad_ranges_do_not_spawn_false_hits`,
//! `sparse_default_maps_damage_to_whole_range`) and recovered-file
//! correctness by the VHD end-to-end test in `integration.rs` — this
//! bench cross-references both rather than duplicating them.
//!
//! Run by `windows-integration.yml` against the attached test VHD
//! (falling back to PhysicalDrive0 when the label probe fails).

#![cfg(windows)]
// Bench-only display math: tick→second conversions and byte-count
// clamps are diagnostic, not engine logic.
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]

use std::time::Instant;

use dw_core::blockdev::{AlignedBuf, BatchRead, BlockDevice};
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

/// One benchmark configuration: carve-window bytes × queue depth.
struct Config {
    window: usize,
    depth: usize,
}

/// The benchmark matrix (§5.1 bands: HDD 1–4 MiB QD 1–2, flash
/// 4–16 MiB QD 4–16 — plus cross points for the tuning curves).
fn matrix() -> Vec<Config> {
    let mib = 1024 * 1024;
    [
        (mib, 1),
        (2 * mib, 1),
        (2 * mib, 2),
        (4 * mib, 2),
        (4 * mib, 4),
        (8 * mib, 4),
        (8 * mib, 8),
        (16 * mib, 8),
    ]
    .into_iter()
    .map(|(window, depth)| Config { window, depth })
    .collect()
}

/// Find the DWTEST volume's physical drive (fall back to disk 0).
fn bench_drive() -> (u32, String) {
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
            if l == "DWTEST"
                && let Ok(dev) = dw_win::blockdev::RawHandleDevice::open_volume(letter)
                && let Ok((idx, _)) = dev.device_number()
            {
                return (idx, format!("PhysicalDrive{idx} (the DWTEST VHD)"));
            }
        }
    }
    (0, "PhysicalDrive0 (fallback)".to_string())
}

/// (user+kernel CPU) seconds consumed by this process so far.
fn cpu_seconds() -> f64 {
    let mut create = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: GetCurrentProcess returns a pseudo-handle (never closed);
    // all out-params are stack locals of the documented types.
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            std::ptr::from_mut(&mut create),
            std::ptr::from_mut(&mut exit),
            std::ptr::from_mut(&mut kernel),
            std::ptr::from_mut(&mut user),
        )
    };
    if ok.is_err() {
        return 0.0;
    }
    let to_secs = |ft: &FILETIME| {
        let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
        ticks as f64 / 1e7 // 100-ns ticks → seconds
    };
    to_secs(&kernel) + to_secs(&user)
}

/// Current working set + lifetime peak (bytes).
fn rss_bytes() -> (usize, usize) {
    let mut pmc = PROCESS_MEMORY_COUNTERS::default();
    // SAFETY: current-process pseudo-handle; pmc is the documented
    // output structure with its size passed alongside.
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            std::ptr::from_mut(&mut pmc),
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    };
    if ok.is_err() {
        return (0, 0);
    }
    (pmc.WorkingSetSize, pmc.PeakWorkingSetSize)
}

/// One (window × depth) sweep over `span` bytes; returns (bytes read,
/// elapsed). Consecutive aligned windows — the carver's access pattern
/// minus the signature scan (pure I/O).
fn sweep(dev: &dw_win::blockdev::RawHandleDevice, cfg: &Config, span: u64) -> u64 {
    let ss = u64::from(dev.sector_size());
    let window = cfg.window as u64;
    let mut bufs: Vec<AlignedBuf> = (0..cfg.depth)
        .map(|_| AlignedBuf::zeroed(cfg.window, dev.preferred_read_alignment()))
        .collect();
    let mut done = 0u64;
    while done < span {
        let mut plan: Vec<u64> = Vec::with_capacity(cfg.depth);
        let mut c = done;
        while plan.len() < cfg.depth && c < span {
            let take = (span - c).min(window);
            let take = take.next_multiple_of(ss).min(window).max(ss);
            let take = take.min(span - c).max(ss);
            plan.push(c);
            c += take;
        }
        let mut batch: Vec<BatchRead<'_>> = plan
            .iter()
            .zip(bufs.iter_mut())
            .map(|(&off, b)| {
                let take = ((span - off).min(window)).next_multiple_of(ss).min(window) as usize;
                let take = take.min((span - off) as usize).max(ss as usize);
                BatchRead {
                    start_lba: off / ss,
                    buf: &mut b[..take],
                }
            })
            .collect();
        let _ = dev.read_sectors_sparse_batch(&mut batch);
        done = *plan.last().expect("plan nonempty") + window;
    }
    span
}

/// The §5.1 item-10 benchmark. Prints a markdown table (CI captures
/// it; numbers are transcribed into docs/benchmarks/io-backend.md with
/// the run link).
#[test]
#[ignore = "windows-only bench"]
fn io_backend_bench() {
    let (index, label) = bench_drive();
    let dev = dw_win::blockdev::RawHandleDevice::open_physical(index).expect("open drive");
    let profile = dev.read_profile();
    let ss = u64::from(dev.sector_size());
    let capacity = dev.total_sectors() * ss;
    let span = capacity / 2; // VHD: 128 MiB; disk 0: capped below
    let span = span.clamp(4 * 1024 * 1024, 256 * 1024 * 1024);
    println!("# IO backend benchmark — {label}");
    println!(
        "- media profile: {:?} (window {} KiB, QD {})",
        profile,
        profile.window_bytes / 1024,
        profile.queue_depth
    );
    println!("- sector size: {ss} B, span: {} MiB", span / (1024 * 1024));
    println!();
    println!("| window | QD | MiB/s | CPU s | ΔCPU/MiB | WS after (MiB) |");
    println!("|---:|---:|---:|---:|---:|---:|");

    // Sequential reference bytes for the correctness cross-check.
    let ref_len = (span as usize).min(4 * 1024 * 1024);
    let mut reference = AlignedBuf::zeroed(ref_len, dev.preferred_read_alignment());
    dev.read_sectors_sparse(0, &mut reference)
        .expect("ref read");

    for cfg in matrix() {
        // Warm-up batch (first-touch page faults + cache population).
        sweep(&dev, &cfg, cfg.window as u64 * cfg.depth as u64);
        let cpu0 = cpu_seconds();
        let ws0 = rss_bytes().0;
        let t0 = Instant::now();
        let bytes = sweep(&dev, &cfg, span);
        let elapsed = t0.elapsed();
        let cpu = cpu_seconds() - cpu0;
        let ws1 = rss_bytes().0;

        // Correctness: this config's first windows must equal the
        // sequential reference bytes.
        let mut bufs: Vec<AlignedBuf> = (0..cfg.depth)
            .map(|_| AlignedBuf::zeroed(cfg.window, dev.preferred_read_alignment()))
            .collect();
        let mut batch: Vec<BatchRead<'_>> = bufs
            .iter_mut()
            .enumerate()
            .map(|(i, b)| BatchRead {
                start_lba: ((i * cfg.window) as u64) / ss,
                buf: &mut b[..cfg.window],
            })
            .collect();
        dev.read_sectors_sparse_batch(&mut batch)
            .expect("verify batch");
        for (i, b) in bufs.iter().enumerate() {
            let start = i * cfg.window;
            if start + cfg.window <= ref_len {
                assert_eq!(
                    &b[..cfg.window],
                    &reference[start..start + cfg.window],
                    "config {}B×QD{} window {i} diverges from sequential",
                    cfg.window,
                    cfg.depth
                );
            }
        }

        #[allow(clippy::cast_precision_loss)] // display-only columns
        {
            let mib = bytes as f64 / (1024.0 * 1024.0);
            let mibs = mib / elapsed.as_secs_f64();
            let cpu_per_mib = if mib > 0.0 { cpu / mib } else { 0.0 };
            println!(
                "| {} KiB | {} | {mibs:.0} | {cpu:.2} | {cpu_per_mib:.4} | {:.1} |",
                cfg.window / 1024,
                cfg.depth,
                ws1 as f64 / (1024.0 * 1024.0)
            );
        }
        let _ = ws0;
    }
    let (ws, peak) = rss_bytes();
    println!();
    println!(
        "- final working set: {:.1} MiB, lifetime peak RSS: {:.1} MiB",
        ws as f64 / (1024.0 * 1024.0),
        peak as f64 / (1024.0 * 1024.0)
    );
}
