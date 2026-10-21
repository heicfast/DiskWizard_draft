//! File-repair commands — the DRWUI repair-wizard surface.
//!
//! Research: `architecture/06-repair-chain.md` §2 (entry points) +
//! phase-2 §5 (the contracts):
//!
//! - `repair_analyze_file` — the `CFileRepairPreviewCtl` /
//!   `CHKRepairFileSign` role: read the file, dispatch by signature,
//!   return the exact DamageReport (the preview-failed → repair
//!   offer flow).
//! - `repair_files` — the `RepairingWnd` batch ("Repairing (1/2): …"):
//!   per-file progress events, repaired output written ONLY under
//!   the user-selected destination (never in place — phase-2 §5
//!   safety), healthy files skipped with an honest verdict.
//! - Every repair function gates on a valid license (repair is a
//!   paid surface; SMART stays the free one).
//!
//! The engines live in the `dw-repair` crate (in-process — phase-2
//! §5: no per-tool exe spawning, unlike the vendor's out-of-proc
//! EUFileRepair hosts).

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, State};

use super::AppState;

/// The analysis cap: repair grammars are whole-file scans (PDF object
/// walks, OOXML inventories); 256 MiB bounds the hostile case.
const ANALYZE_CAP: u64 = 256 * 1024 * 1024;

/// Analyze one file's damage (the preview-failed → repair offer).
///
/// # Errors
/// String error (user-facing code prefix) on IO or dispatch failure.
#[tauri::command]
pub async fn repair_analyze_file(s: State<'_, AppState>, path: String) -> Result<Value, String> {
    super::license::require_licensed(&s)?;
    run_analyze(&path)
}

fn run_analyze(path: &str) -> Result<Value, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("FILE_NOT_FOUND. {e}"))?;
    if meta.len() > ANALYZE_CAP {
        return Err("FILE_TOO_LARGE. Repair analysis caps at 256 MB.".into());
    }
    let buf = std::fs::read(path).map_err(|e| format!("READ_FAILED. {e}"))?;
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    // The by-name dispatch: signature first, extension fallback (a
    // destroyed header is exactly the damage class being asked about).
    let report = dw_repair::analyze_by_name(name, &buf).map_err(|e| e.to_string())?;
    let mut v = serde_json::to_value(&report).map_err(|e| e.to_string())?;
    if let Some(fmt) = dw_repair::format_for_name(name) {
        v["format"] = json!(fmt);
        v["family"] = json!(fmt.family());
    }
    v["fileName"] = json!(name);
    v["fileSize"] = json!(meta.len());
    Ok(v)
}

/// Batch-repair files into a destination folder (never in place).
///
/// # Errors
/// String error on argument/validation failure; per-file failures
/// ride the results array (batch resilience — the vendor's
/// "failure removes the item and continues").
#[tauri::command]
pub async fn repair_files(
    app: AppHandle,
    s: State<'_, AppState>,
    paths: Vec<String>,
    destination: String,
) -> Result<Value, String> {
    super::license::require_licensed(&s)?;
    if paths.is_empty() {
        return Err("BAD_REQUEST. Select files to repair.".into());
    }
    if paths.len() > 1000 {
        return Err("BAD_REQUEST. Repair batch caps at 1000 files.".into());
    }
    let dest = std::path::PathBuf::from(&destination);
    if !dest.is_dir() {
        return Err("BAD_DESTINATION. The repair destination folder does not exist.".into());
    }
    let app = app.clone();
    let total = paths.len();
    std::thread::Builder::new()
        .name("dw-repair".to_string())
        .spawn(move || {
            let mut results = Vec::new();
            let mut done = 0usize;
            let mut repaired_count = 0usize;
            for p in &paths {
                let name = p.rsplit(['\\', '/']).next().unwrap_or(p);
                let _ = app.emit(
                    "repair-progress",
                    json!({
                        "total": total,
                        "done": done,
                        "currentFile": name,
                        "stage": "repairing",
                    }),
                );
                let entry = match run_repair_one(p, &dest) {
                    Ok(v) => {
                        if v["repaired"].as_bool() == Some(true) {
                            repaired_count += 1;
                        }
                        v
                    }
                    Err(e) => json!({
                        "fileName": name,
                        "repaired": false,
                        "error": e,
                    }),
                };
                results.push(entry);
                done += 1;
            }
            let _ = app.emit(
                "repair-done",
                json!({
                    "total": total,
                    "repaired": repaired_count,
                    "failed": total - repaired_count,
                    "destination": destination,
                }),
            );
        })
        .map_err(|e| format!("THREAD_SPAWN_FAILED. {e}"))?;
    Ok(json!({ "ok": true }))
}

/// Repair one file into the destination (collision-numbered).
fn run_repair_one(path: &str, dest: &std::path::Path) -> Result<Value, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("FILE_NOT_FOUND. {e}"))?;
    if meta.len() > ANALYZE_CAP {
        return Err("FILE_TOO_LARGE. Repair caps at 256 MB.".into());
    }
    let buf = std::fs::read(path).map_err(|e| format!("READ_FAILED. {e}"))?;
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let report = dw_repair::analyze_by_name(name, &buf).map_err(|e| e.to_string())?;
    if report.healthy {
        // The honest skip: nothing to do (CHKRepairFileSign's pass).
        return Ok(json!({
            "fileName": name,
            "repaired": false,
            "healthy": true,
            "detail": report.detail,
        }));
    }
    let mut out = Vec::new();
    let outcome = dw_repair::repair_by_name(name, &buf, &mut out).map_err(|e| e.to_string())?;
    let target = unique_dest(dest, name);
    std::fs::write(&target, &out).map_err(|e| format!("WRITE_FAILED. {e}"))?;
    Ok(json!({
        "fileName": name,
        "repaired": outcome.repaired,
        "healthy": false,
        "partial": outcome.partial,
        "outputFile": target.display().to_string(),
        "inputBytes": outcome.input_bytes,
        "outputBytes": outcome.output_bytes,
        "detail": outcome.detail,
        "damage": serde_json::to_value(&report.damage).unwrap_or_default(),
    }))
}

/// The collision policy (numbered suffix, the recovery funnel's).
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

/// Repair-during-recovery (phase-2 §5): after the export lands, walk
/// the recovered copies and repair the damaged repairable formats in
/// place (these are OUR destination copies — the source device is
/// never touched). Returns (repaired, scanned).
pub fn repair_recovered_tree(root: &std::path::Path) -> (usize, usize) {
    let mut repaired = 0usize;
    let mut scanned = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if dw_repair::format_for_name(name).is_none() {
                continue;
            }
            let Ok(meta) = std::fs::metadata(&p) else {
                continue;
            };
            if meta.len() == 0 || meta.len() > ANALYZE_CAP {
                continue;
            }
            let Ok(buf) = std::fs::read(&p) else {
                continue;
            };
            scanned += 1;
            let Ok(report) = dw_repair::analyze_by_name(name, &buf) else {
                continue;
            };
            if report.healthy {
                continue;
            }
            let mut out = Vec::new();
            if let Ok(outcome) = dw_repair::repair_by_name(name, &buf, &mut out)
                && outcome.repaired
                && std::fs::write(&p, &out).is_ok()
            {
                repaired += 1;
            }
        }
    }
    (repaired, scanned)
}
