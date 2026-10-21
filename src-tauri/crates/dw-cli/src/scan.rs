//! `dw-cli scan` — the full production pipeline (prepare → quick →
//! deep → carve → organize) with live progress, a complete log, the
//! results JSON (nodes + extents, recovery-ready) and the auto-saved
//! .rsf session.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use dw_core::blockdev::{BlockDevice, BlockRead};
use dw_core::fs::ScanControl;
use dw_core::scan::{Phase, ProgressObserver, ScanController};
use dw_core::tree::TreeNode;

use crate::logging::Log;
use crate::target;

/// Grace period between the cooperative cancel and the hard exit
/// (seconds). The pipeline unwinds in well under 10 s when healthy
/// (engine checkpoints fire per window/entry; IO waits are bounded);
/// 90 s tolerates a slow final readability probe or a deep validator
/// checkpoint gap without ever approaching a CI job timeout.
const HARD_STOP_GRACE_S: u64 = 90;

/// The hard-stop exit code (distinct from every cargo/IO failure so
/// CI evidence can attribute the kill to the watchdog alone).
const HARD_STOP_EXIT_CODE: i32 = 75;

/// The CLI results envelope — everything `recover` (and CI analysis)
/// needs: the report, the target spec for re-opening, and the FULL
/// node set including recovery extents (the UI wire omits them; the
/// CLI is the engine-side truth).
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliResults {
    pub target: String,
    pub target_name: String,
    pub live_volume: bool,
    pub stopped: bool,
    pub files_found: u64,
    pub bytes_found: u64,
    pub duration_sec: u64,
    pub fs: String,
    pub session_file: Option<String>,
    pub nodes: Vec<CliNode>,
    /// Peak process working set during the scan (bytes; 0 = sampler
    /// unavailable on this platform).
    pub peak_working_set: u64,
    /// Peak private (committed) bytes.
    pub peak_private_bytes: u64,
    /// Per-phase wall seconds (wire names).
    pub phase_timings: Vec<(String, f64)>,
    /// Aggregate read throughput: scanned capacity / wall seconds.
    pub throughput_mbps: f64,
    /// Session 12 — the carve coverage plan's bytes (0 = never reached
    /// the carve phase).
    pub carve_plan_bytes: u64,
    /// The scanned capacity (the plan ratio's denominator).
    pub carve_plan_capacity: u64,
    /// Free-cluster ranges the NTFS $Bitmap contributed.
    pub carve_free_ranges: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliNode {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub name: String,
    pub is_dir: bool,
    pub category: String,
    pub size: u64,
    pub modified: i64,
    pub origin: String,
    pub recoverability: String,
    pub path: String,
    pub extents: Vec<dw_core::types::Extent>,
    pub confidence: u8,
}

fn nodes_to_cli(nodes: &[TreeNode]) -> Vec<CliNode> {
    nodes
        .iter()
        .map(|n| CliNode {
            id: n.id,
            parent_id: n.parent_id,
            name: n.name.clone(),
            is_dir: n.is_dir,
            category: n.category.wire().to_string(),
            size: n.size,
            modified: n.modified,
            origin: n.origin.wire().to_string(),
            recoverability: n.recoverability.wire().to_string(),
            path: n.path.clone(),
            extents: n.extents.clone(),
            confidence: n.confidence,
        })
        .collect()
}

/// The CLI progress observer: throttled stderr lines + verbose log +
/// per-phase wall timing (the speed evidence the workflows record).
struct CliObserver<'a> {
    log: &'a Log,
    last_line: Instant,
    last_percent: f64,
    phase_marks: Vec<(String, Instant)>,
}

impl CliObserver<'_> {
    fn phase_report(&self) -> Vec<(String, f64)> {
        let mut out = Vec::new();
        for w in self.phase_marks.windows(2) {
            let secs = w[1]
                .1
                .duration_since(w[0].1)
                .as_secs_f64()
                .round_to_tens_ms();
            out.push((w[0].0.clone(), secs));
        }
        if let Some((last, at)) = self.phase_marks.last() {
            out.push((last.clone(), at.elapsed().as_secs_f64().round_to_tens_ms()));
        }
        out
    }
}

/// Round to 10 ms (readable logs; sub-10 ms precision is noise).
trait RoundTens {
    fn round_to_tens_ms(self) -> f64;
}

impl RoundTens for f64 {
    fn round_to_tens_ms(self) -> f64 {
        (self * 100.0).round() / 100.0
    }
}

impl ProgressObserver for CliObserver<'_> {
    fn on_progress(&mut self, p: &dw_core::scan::Progress) {
        // ~2 Hz on stderr; every line lands in the log file.
        let big = (p.percent - self.last_percent).abs() >= 10.0;
        if self.last_line.elapsed() < Duration::from_millis(500) && !big {
            return;
        }
        self.last_line = Instant::now();
        self.last_percent = p.percent;
        self.log.verbose(&format!(
            "phase {:<9} {:5.1}%  files {:>7}  bytes {:>13}  sectors {}/{}  elapsed {}s",
            p.phase.wire(),
            p.percent,
            p.files_found,
            p.bytes_found,
            p.sectors_read,
            p.sectors_total,
            p.elapsed_sec
        ));
    }

    fn on_phase(&mut self, phase: Phase) {
        self.log.info(&format!("PHASE → {}", phase.wire()));
        self.phase_marks
            .push((phase.wire().to_string(), Instant::now()));
    }
}

/// Peak-memory sampler (Windows: GetProcessMemoryInfo every 250 ms;
/// other platforms: zeros). Runs until the handle drops — the scan
/// command reads the peaks at completion.
struct MemorySampler {
    peaks: std::sync::Arc<std::sync::Mutex<(u64, u64)>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl MemorySampler {
    fn start() -> Self {
        let peaks = std::sync::Arc::new(std::sync::Mutex::new((0u64, 0u64)));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (p2, s2) = (std::sync::Arc::clone(&peaks), std::sync::Arc::clone(&stop));
        let join = std::thread::Builder::new()
            .name("dw-cli-memsampler".into())
            .spawn(move || {
                #[cfg(windows)]
                loop {
                    if s2.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    if let Some((ws, privb)) = sample_self() {
                        let mut g = p2.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                        if ws > g.0 {
                            g.0 = ws;
                        }
                        if privb > g.1 {
                            g.1 = privb;
                        }
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
                #[cfg(not(windows))]
                {
                    let _ = (&p2, &s2);
                }
            })
            .ok();
        Self { peaks, stop, join }
    }

    fn peaks(&self) -> (u64, u64) {
        *self
            .peaks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Drop for MemorySampler {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// One GetProcessMemoryInfo sample: (working set, private bytes).
#[cfg(windows)]
fn sample_self() -> Option<(u64, u64)> {
    use windows::Win32::System::ProcessStatus::GetProcessMemoryInfo;
    use windows::Win32::System::Threading::GetCurrentProcess;
    let mut pmc = windows::Win32::System::ProcessStatus::PROCESS_MEMORY_COUNTERS::default();
    // SAFETY: GetCurrentProcess returns a pseudo-handle (no
    // ownership); pmc is the documented output buffer.
    #[allow(unsafe_code)] // audited Win32 call (the crate policy)
    #[allow(clippy::cast_possible_truncation)] // struct size ≤ 4 GiB by construction
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            std::ptr::from_mut(&mut pmc),
            std::mem::size_of::<windows::Win32::System::ProcessStatus::PROCESS_MEMORY_COUNTERS>()
                as u32,
        )
    };
    if ok.is_err() {
        return None;
    }
    Some((pmc.WorkingSetSize as u64, pmc.PagefileUsage as u64))
}

#[allow(clippy::too_many_lines)] // one CLI command main; splitting hurts locality
pub fn cmd(args: &[String]) -> Result<ExitCode, String> {
    let spec = args
        .get(1)
        .filter(|s| !s.starts_with('-'))
        .ok_or_else(|| "scan needs a target (device id or image path)".to_string())?
        .clone();
    let out = crate::flag_value(args, "--out");
    let log_path = crate::flag_value(args, "--log");
    let verbose = crate::has_flag(args, "-v") || crate::has_flag(args, "--verbose");
    let max_seconds: Option<u64> =
        crate::flag_value(args, "--max-seconds").and_then(|v| v.parse().ok());

    let log = Log::open(log_path.as_deref(), verbose);
    log.info(&format!(
        "dw-cli scan target=`{spec}` out={out:?} verbose={verbose} max_seconds={max_seconds:?}"
    ));

    let t = target::resolve(&spec)?;
    let dev = t.dev.clone();
    let capacity_for_perf = dev.capacity_bytes();
    let total_sectors = dev.total_sectors();
    let sector_size = dev.sector_size();
    let describe = dev.describe();
    log.info(&format!(
        "target resolved: {} ({} bytes, {}-byte sectors, live_volume={})",
        t.name,
        dev.capacity_bytes(),
        dev.sector_size(),
        t.live_volume
    ));

    // The CI guard: a TWO-STAGE watchdog (session 12). Stage one
    // cancels the control token after N seconds — partial results are
    // KEPT (same semantics as Stop; the pipeline unwinds
    // cooperatively: engine checkpoints, channel drain-to-disconnect,
    // bounded IO waits). Stage two is the belt-and-suspenders net: if
    // the process is STILL alive GRACE seconds later, some path wedged
    // past every cooperative checkpoint — exit hard with a loud
    // diagnostic instead of zombie-ing until the CI job timeout kills
    // us (the observed 38-minute hang). Exit code 75 distinguishes
    // "watchdog hard-stop" from every other failure in CI evidence.
    let control = ScanControl::new();
    if let Some(secs) = max_seconds {
        let ctrl = control.clone();
        std::thread::Builder::new()
            .name("dw-cli-watchdog".into())
            .spawn(move || {
                std::thread::sleep(Duration::from_secs(secs));
                ctrl.cancel();
                eprintln!(
                    "WATCHDOG: max-seconds {secs} reached — cooperative cancel issued; \
                     hard exit in {HARD_STOP_GRACE_S}s if the pipeline does not unwind"
                );
                std::thread::sleep(Duration::from_secs(HARD_STOP_GRACE_S));
                eprintln!(
                    "WATCHDOG: pipeline failed to unwind within {HARD_STOP_GRACE_S}s of \
                     cancellation — hard exit (a stuck scan path; report the scan log)."
                );
                std::process::exit(HARD_STOP_EXIT_CODE);
            })
            .ok();
    }

    let started = Instant::now();
    let mut controller = ScanController::with_control(dev, &t.name, control.clone());
    let mut observer = CliObserver {
        log: &log,
        last_line: Instant::now(),
        last_percent: 0.0,
        phase_marks: Vec::new(),
    };
    // Environment header — every speed number carries its context
    // (the owner's "all settings and environment recorded").
    log.info(&format!(
        "env: cores={} capacity={} sectors={}x{}B describe=`{}` live={}",
        std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
        capacity_for_perf,
        total_sectors,
        sector_size,
        describe,
        t.live_volume
    ));
    #[cfg(windows)]
    log.info(&format!(
        "env-ram: {}",
        total_physical_bytes().map_or_else(|| "n/a".into(), |b| b.to_string())
    ));
    let mem = MemorySampler::start();
    log.info("scan starting (prepare → quick → deep → carve → organize)");
    let result = controller.run(&mut observer);
    let stopped = matches!(&result, Ok((_, r)) if r.stopped)
        || matches!(&result, Err(dw_core::error::CoreError::Cancelled));

    let (tree, report) = match result {
        Ok((tree, report)) => (tree, report),
        Err(dw_core::error::CoreError::Cancelled) => {
            log.error("scan cancelled (watchdog or token) — partial results kept");
            return finish_partial(&t, &log, started, out.as_deref());
        }
        Err(e) => {
            log.error(&format!("scan failed: {e}"));
            return Err(format!("SCAN_FAILED. {e}"));
        }
    };
    let _ = &report;
    log.info(&format!(
        "scan finished: {} files, {} bytes, {} s, fs={}",
        report.files_found,
        report.bytes_found,
        report.duration_sec,
        report.fs.wire()
    ));
    // Session 12 — the carve coverage evidence: did the $Bitmap
    // guidance engage, and how much of the device did the sweep
    // actually have to read? (A plan ≈ capacity with 0 free ranges on
    // NTFS = the bitmap was unavailable and the fallback swept
    // everything — correct but slower; the CI record now shows which.)
    if report.carve_plan_capacity > 0 {
        #[allow(clippy::cast_precision_loss)] // percent display
        let pct = (report.carve_plan_bytes as f64 / report.carve_plan_capacity as f64) * 100.0;
        let guided = if report.carve_free_ranges > 0 {
            format!("bitmap-guided, {} free ranges", report.carve_free_ranges)
        } else {
            "full-extent (no bitmap guidance)".to_string()
        };
        log.info(&format!(
            "carve-plan: {pct:.1}% of capacity ({} of {} bytes, {})",
            report.carve_plan_bytes, report.carve_plan_capacity, guided
        ));
    }

    // Auto-save the .rsf session (the app contract: resume-ready).
    // Fixture-grade times: the wrap window is centuries out.
    #[allow(clippy::cast_possible_wrap)]
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default();
    let session_file = dw_core::session::auto_save_name(&t.name, now);
    let session_path = dw_core::session::sessions_dir().join(&session_file);
    let session_saved = dw_core::session::save(
        &session_path,
        &dw_core::session::SessionHeader {
            files_found: report.files_found,
            bytes_found: report.bytes_found,
            fs: report.fs.wire().to_string(),
            duration_sec: report.duration_sec,
            stopped: report.stopped,
        },
        &dw_core::session::SessionDevice {
            name: t.name.clone(),
            total_bytes: 0,
            saved_at_unix: now,
        },
        tree.nodes(),
    )
    .is_ok();
    log.info(&format!(
        "session {} → {}",
        if session_saved {
            "saved"
        } else {
            "SAVE-FAILED"
        },
        session_path.display()
    ));

    let (peak_ws, peak_priv) = mem.peaks();
    drop(mem);
    let phase_timings = observer.phase_report();
    for (name, secs) in &phase_timings {
        log.info(&format!("phase-time {name}={secs:.2}s"));
    }
    // MiB-per-second for the evidence logs (documented precision
    // exception: throughput display, not accounting). Session 12:
    // sub-second scans floored `duration_sec` to 0 and reported 0.0
    // MiB/s — the benchmark's 0.9 s / 2 GB scan failed its own gate.
    // The wall clock is the CLI's own precise Instant; the report's
    // integer-second field stays (the wire contract).
    let wall_secs = started.elapsed().as_secs_f64().max(0.001);
    #[allow(clippy::cast_precision_loss)]
    let throughput_mbps = (capacity_for_perf / 1024 / 1024) as f64 / wall_secs;
    log.info(&format!(
        "perf: wall={wall_secs:.2}s throughput={throughput_mbps:.1} MiB/s peak_ws={peak_ws} peak_private={peak_priv}"
    ));
    let results = CliResults {
        target: t.spec.clone(),
        target_name: t.name.clone(),
        live_volume: t.live_volume,
        stopped,
        files_found: report.files_found,
        bytes_found: report.bytes_found,
        duration_sec: report.duration_sec,
        fs: report.fs.wire().to_string(),
        session_file: session_saved.then(|| session_path.display().to_string()),
        nodes: nodes_to_cli(tree.nodes()),
        peak_working_set: peak_ws,
        peak_private_bytes: peak_priv,
        phase_timings,
        throughput_mbps,
        carve_plan_bytes: report.carve_plan_bytes,
        carve_plan_capacity: report.carve_plan_capacity,
        carve_free_ranges: report.carve_free_ranges,
    };
    write_results(&results, out.as_deref(), &log)?;
    Ok(ExitCode::SUCCESS)
}

/// A cancelled/timeout scan still yields its partial tree — the
/// controller's report assembled it before the Cancelled propagated.
fn finish_partial(
    t: &target::Target,
    log: &Log,
    started: Instant,
    out: Option<&str>,
) -> Result<ExitCode, String> {
    // The controller consumed the tree; without a second run there is
    // nothing durable to report — say so honestly.
    log.error(&format!(
        "scan aborted after {}s without a results snapshot (run without --max-seconds or raise it)",
        started.elapsed().as_secs()
    ));
    let _ = (t, out);
    Err(
        "SCAN_CANCELLED. No results snapshot (the watchdog fired before any phase completed)."
            .into(),
    )
}

fn write_results(results: &CliResults, out: Option<&str>, log: &Log) -> Result<(), String> {
    let json = serde_json::to_string_pretty(results).map_err(|e| e.to_string())?;
    match out {
        Some(path) => {
            std::fs::write(path, json).map_err(|e| format!("OUT_WRITE_FAILED. {e}"))?;
            log.info(&format!(
                "results written: {path} ({} nodes)",
                results.nodes.len()
            ));
        }
        None => {
            println!("{json}");
        }
    }
    // The one-line CI summary on stderr (parseable).
    log.info(&format!(
        "SUMMARY files={} bytes={} duration={}s nodes={} stopped={} mbps={:.1} peak_ws={}",
        results.files_found,
        results.bytes_found,
        results.duration_sec,
        results.nodes.len(),
        results.stopped,
        results.throughput_mbps,
        results.peak_working_set
    ));
    Ok(())
}

/// Re-exported for recover: the Arc<dyn BlockDevice> the scan used is
/// reopened from the stored spec.
pub fn reopen(spec: &str) -> Result<Arc<dyn BlockDevice>, String> {
    Ok(target::resolve(spec)?.dev)
}

/// Total physical RAM (bytes) for the environment header.
#[cfg(windows)]
fn total_physical_bytes() -> Option<u64> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    #[allow(clippy::cast_possible_truncation)] // struct size ≤ 4 GiB by construction
    let mut ms = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: ms is the documented output struct with dwLength set.
    #[allow(unsafe_code)] // audited Win32 call (the crate policy)
    unsafe { GlobalMemoryStatusEx(std::ptr::from_mut(&mut ms)) }
        .is_ok()
        .then_some(ms.ullTotalPhys)
}
