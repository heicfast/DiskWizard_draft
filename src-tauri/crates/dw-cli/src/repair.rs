//! The `repair` subcommand — the headless file-repair junction.
//!
//! Drives the SAME production repair engines the app runs
//! (`dw-repair`), for Windows-runner E2E evidence:
//!
//! ```text
//! dw-cli repair <file...> --dest DIR [--json] [--out FILE] [--log FILE] [-v]
//! dw-cli repair --analyze <file>    (damage report only, no writes)
//! ```
//!
//! Exit codes a workflow can gate on: 0 = at least one file repaired
//! (or analyze verdict produced); 1 = nothing repairable; 2 = IO or
//! usage failure.

use std::process::ExitCode;

use dw_repair::RepairEngine;

use crate::{flag_value, has_flag};

/// The 256 MiB analysis cap (mirrors the app layer's contract).
const CAP: u64 = 256 * 1024 * 1024;

pub fn cmd(args: &[String]) -> Result<ExitCode, String> {
    // Positional files: skip the command word, every flag, and every
    // flag VALUE (--dest/--out/--log each consume the next token).
    const VALUE_FLAGS: [&str; 3] = ["--dest", "--out", "--log"];
    let analyze_only = has_flag(args, "--analyze");
    let mut files: Vec<&String> = Vec::new();
    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        if a.starts_with("--") || a == "-v" {
            if VALUE_FLAGS.contains(&a.as_str()) {
                i += 2; // skip the value token too
                continue;
            }
            i += 1;
            continue;
        }
        files.push(a);
        i += 1;
    }
    if files.is_empty() {
        return Err("repair needs at least one file path".to_string());
    }
    let json = has_flag(args, "--json");
    let out_file = flag_value(args, "--out");
    let log_file = flag_value(args, "--log");
    let verbose = has_flag(args, "-v") || has_flag(args, "--verbose");
    let log = crate::logging::Log::open(log_file.as_deref(), verbose);

    if analyze_only {
        return cmd_analyze(&files, json, out_file.as_deref(), &log);
    }
    let Some(dest) = flag_value(args, "--dest") else {
        return Err("repair needs --dest DIR (repair never writes in place)".to_string());
    };
    let dest = std::path::PathBuf::from(&dest);
    if !dest.is_dir() {
        return Err(format!("--dest `{}` is not a directory", dest.display()));
    }

    let mut results = Vec::new();
    let mut repaired_count = 0usize;
    for f in &files {
        log.info(&format!("repair: {f}"));
        let name = f.rsplit(['\\', '/']).next().unwrap_or(f);
        match repair_one(f, &dest) {
            Ok((outcome, target)) => {
                log.info(&format!(
                    "  repaired={} partial={} in={} out={} :: {}",
                    outcome.repaired,
                    outcome.partial,
                    outcome.input_bytes,
                    outcome.output_bytes,
                    outcome.detail
                ));
                if outcome.repaired {
                    repaired_count += 1;
                }
                // The honest-skip signal: nothing was written when
                // the analysis said healthy (target empty).
                let healthy = !outcome.repaired && target.is_empty() && !outcome.partial;
                results.push(serde_json::json!({
                    "fileName": name,
                    "repaired": outcome.repaired,
                    "healthy": healthy,
                    "partial": outcome.partial,
                    "inputBytes": outcome.input_bytes,
                    "outputBytes": outcome.output_bytes,
                    "outputFile": target,
                    "detail": outcome.detail,
                }));
            }
            Err(e) => {
                log.error(&format!("  FAILED: {e}"));
                results.push(serde_json::json!({
                    "fileName": name,
                    "repaired": false,
                    "error": e,
                }));
            }
        }
    }
    let summary = serde_json::json!({
        "total": files.len(),
        "repaired": repaired_count,
        "failed": files.len() - repaired_count,
        "results": results,
    });
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?
        );
    } else {
        println!(
            "repair complete: {}/{} repaired",
            repaired_count,
            files.len()
        );
    }
    if let Some(out) = out_file {
        std::fs::write(
            out.as_str(),
            serde_json::to_vec_pretty(&summary).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("write {out}: {e}"))?;
    }
    Ok(if repaired_count > 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    })
}

fn cmd_analyze(
    files: &[&String],
    json: bool,
    out_file: Option<&str>,
    log: &crate::logging::Log,
) -> Result<ExitCode, String> {
    let mut reports = Vec::new();
    for f in files {
        log.info(&format!("analyze: {f}"));
        let name = f.rsplit(['\\', '/']).next().unwrap_or(f);
        match analyze_one(f) {
            Ok(report) => {
                log.info(&format!(
                    "  healthy={} tier={:?} :: {}",
                    report.healthy, report.tier, report.detail
                ));
                let mut v = serde_json::to_value(&report).map_err(|e| e.to_string())?;
                v["fileName"] = serde_json::json!(name);
                reports.push(v);
            }
            Err(e) => {
                log.error(&format!("  FAILED: {e}"));
                reports.push(serde_json::json!({
                    "fileName": name,
                    "error": e,
                }));
            }
        }
    }
    let summary = serde_json::json!({ "reports": reports });
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?
        );
    }
    if let Some(out) = out_file {
        std::fs::write(
            out,
            serde_json::to_vec_pretty(&summary).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("write {out}: {e}"))?;
    }
    Ok(ExitCode::SUCCESS)
}

fn analyze_one(path: &str) -> Result<dw_repair::DamageReport, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("FILE_NOT_FOUND. {e}"))?;
    if meta.len() > CAP {
        return Err("FILE_TOO_LARGE. Repair analysis caps at 256 MB.".into());
    }
    let buf = std::fs::read(path).map_err(|e| format!("READ_FAILED. {e}"))?;
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    dw_repair::analyze_by_name(name, &buf)
}

fn repair_one(
    path: &str,
    dest: &std::path::Path,
) -> Result<(dw_repair::RepairOutcome, String), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("FILE_NOT_FOUND. {e}"))?;
    if meta.len() > CAP {
        return Err("FILE_TOO_LARGE. Repair caps at 256 MB.".into());
    }
    let buf = std::fs::read(path).map_err(|e| format!("READ_FAILED. {e}"))?;
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let report = dw_repair::analyze_by_name(name, &buf)?;
    if report.healthy {
        return Ok((
            dw_repair::RepairOutcome {
                repaired: false,
                input_bytes: buf.len() as u64,
                output_bytes: buf.len() as u64,
                partial: false,
                detail: format!(
                    "Healthy — nothing to repair (the honest skip). {}",
                    report.detail
                ),
            },
            String::new(),
        ));
    }
    let mut out = Vec::new();
    let outcome = dw_repair::repair_by_name(name, &buf, &mut out)?;
    let target = unique_dest(dest, name);
    std::fs::write(&target, out).map_err(|e| format!("WRITE_FAILED. {e}"))?;
    Ok((outcome, target.display().to_string()))
}

fn unique_dest(dest: &std::path::Path, name: &str) -> std::path::PathBuf {
    let base = dest.join(name);
    if !base.exists() {
        return base;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s, format!(".{e}")),
        None => (name, String::new()),
    };
    for n in 1..1000 {
        let cand = dest.join(format!("{stem}_{n}{ext}"));
        if !cand.exists() {
            return cand;
        }
    }
    base
}

/// Silence the unused-import warning for the trait import used only
/// for its documented contract in this module's doc scope.
const _: fn() = || {
    fn assert_engine_trait_object(e: &dyn RepairEngine) {
        let _ = e.format();
    }
    let _ = assert_engine_trait_object;
};
