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

/// The CLI progress observer: throttled stderr lines + verbose log.
struct CliObserver<'a> {
    log: &'a Log,
    last_line: Instant,
    last_percent: f64,
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
    }
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
    log.info(&format!(
        "target resolved: {} ({} bytes, {}-byte sectors, live_volume={})",
        t.name,
        dev.capacity_bytes(),
        dev.sector_size(),
        t.live_volume
    ));

    // The CI guard: a watchdog cancels the control token after N
    // seconds (partial results are KEPT — same semantics as Stop).
    let control = ScanControl::new();
    if let Some(secs) = max_seconds {
        let ctrl = control.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(secs));
            ctrl.cancel();
        });
    }

    let started = Instant::now();
    let mut controller = ScanController::with_control(dev, &t.name, control.clone());
    let mut observer = CliObserver {
        log: &log,
        last_line: Instant::now(),
        last_percent: 0.0,
    };
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
        "SUMMARY files={} bytes={} duration={}s nodes={} stopped={}",
        results.files_found,
        results.bytes_found,
        results.duration_sec,
        results.nodes.len(),
        results.stopped
    ));
    Ok(())
}

/// Re-exported for recover: the Arc<dyn BlockDevice> the scan used is
/// reopened from the stored spec.
pub fn reopen(spec: &str) -> Result<Arc<dyn BlockDevice>, String> {
    Ok(target::resolve(spec)?.dev)
}
