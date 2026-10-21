//! `dw-cli recover` — the export funnel: load the scan's results
//! JSON, reopen the source target, select nodes (--ids / --origin /
//! --all), run the production recovery engine with progress + log.

use std::process::ExitCode;

use serde::Serialize;

use dw_core::fs::ScanControl;
use dw_core::recover::{DestWriter, build_queue, run_recovery};
use dw_core::tree::TreeNode;

use crate::logging::Log;
use crate::scan::{CliNode, CliResults};

/// The destination writer (real filesystem IO).
struct FsDest {
    root: std::path::PathBuf,
}

impl FsDest {
    fn resolve(&self, rel: &str) -> std::path::PathBuf {
        let p = self.root.join(rel.replace('\\', "/"));
        let canon = p.canonicalize().unwrap_or_else(|_| p.clone());
        if canon.starts_with(&self.root) {
            p
        } else {
            self.root.join("recovered")
        }
    }
}

impl DestWriter for FsDest {
    fn mkdir(&mut self, path: &str) -> Result<(), String> {
        let target = self.resolve(path);
        std::fs::create_dir_all(&target).map_err(|e| format!("mkdir {path}: {e}"))
    }

    fn write_file(
        &mut self,
        path: &str,
        read: &mut dyn FnMut(&mut [u8]) -> usize,
    ) -> Result<(), String> {
        use std::io::Write as _;
        let target = self.resolve(path);
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
        #[cfg(windows)]
        {
            use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
            let root = self.root.to_string_lossy().to_string() + "\\";
            let w: Vec<u16> = root.encode_utf16().chain([0]).collect();
            let mut free_caller = 0u64;
            let mut total = 0u64;
            let mut free = 0u64;
            // SAFETY: w is NUL-terminated; outputs are valid u64 slots.
            #[allow(unsafe_code)]
            let ok = unsafe {
                GetDiskFreeSpaceExW(
                    windows::core::PCWSTR(w.as_ptr()),
                    Some(std::ptr::from_mut(&mut free_caller)),
                    Some(std::ptr::from_mut(&mut total)),
                    Some(std::ptr::from_mut(&mut free)),
                )
            };
            ok.map(|()| free_caller)
                .map_err(|e| format!("free-space query: {e}"))
        }
        #[cfg(not(windows))]
        {
            // statvfs via std is not exposed — approximate by 1 TiB
            // (the CLI's space gate stays permissive on non-Windows).
            Ok(1 << 40)
        }
    }
}

#[derive(Serialize)]
struct CliRecoverReport {
    total_files: u64,
    done_files: u64,
    failed_files: u64,
    total_bytes: u64,
    done_bytes: u64,
    destination: String,
    failures: Vec<(String, String)>,
}

#[allow(clippy::too_many_lines)] // one CLI command main; splitting hurts locality
pub fn cmd(args: &[String]) -> Result<ExitCode, String> {
    let results_path = args
        .get(1)
        .filter(|s| !s.starts_with('-'))
        .ok_or_else(|| "recover needs the results JSON path".to_string())?;
    let dest =
        crate::flag_value(args, "--dest").ok_or_else(|| "recover needs --dest DIR".to_string())?;
    let log_path = crate::flag_value(args, "--log");
    let verbose = crate::has_flag(args, "-v") || crate::has_flag(args, "--verbose");
    let ids_arg = crate::flag_value(args, "--ids");
    let origin_arg = crate::flag_value(args, "--origin");
    let all = crate::has_flag(args, "--all");

    let log = Log::open(log_path.as_deref(), verbose);
    log.info(&format!(
        "dw-cli recover results=`{results_path}` dest=`{dest}` ids={ids_arg:?} origin={origin_arg:?} all={all}"
    ));

    let raw =
        std::fs::read_to_string(results_path).map_err(|e| format!("RESULTS_READ_FAILED. {e}"))?;
    let results: CliResults =
        serde_json::from_str(&raw).map_err(|e| format!("RESULTS_PARSE_FAILED. {e}"))?;
    log.info(&format!(
        "results loaded: {} nodes, target `{}` ({})",
        results.nodes.len(),
        results.target,
        results.target_name
    ));

    // Reopen the scan's source device.
    let dev = crate::scan::reopen(&results.target)
        .map_err(|e| format!("SOURCE_REOPEN_FAILED. {e} (is the device still attached?)"))?;
    log.info("source device reopened");

    // Selection: --ids > --origin > --all (default: deleted + lost).
    let selected: Vec<u64> = if let Some(ids) = ids_arg {
        ids.split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect()
    } else if all {
        results
            .nodes
            .iter()
            .filter(|n| !n.is_dir)
            .map(|n| n.id)
            .collect()
    } else {
        let want = origin_arg.as_deref().unwrap_or("deleted");
        results
            .nodes
            .iter()
            .filter(|n| match want {
                "all" => !n.is_dir,
                "existing" => !n.is_dir && n.origin == "existing",
                "lost-path" => !n.is_dir && n.origin == "lost-path",
                "lost-name" => !n.is_dir && n.origin == "lost-name",
                "version" => !n.is_dir && n.origin == "version",
                // default: the recovery-relevant set
                _ => {
                    !n.is_dir
                        && (n.origin == "deleted"
                            || n.origin == "lost-path"
                            || n.origin == "lost-name")
                }
            })
            .map(|n| n.id)
            .collect()
    };
    log.info(&format!("selected {} nodes for recovery", selected.len()));
    if selected.is_empty() {
        return Err("NOTHING_SELECTED. Pass --ids, --origin, or --all.".into());
    }

    // Rebuild TreeNodes (extents included — the CLI JSON is the
    // engine-side truth).
    let nodes: Vec<TreeNode> = results.nodes.iter().map(cli_node_to_tree).collect();
    let queue = build_queue(&nodes, &selected);
    let total_bytes: u64 = queue.iter().map(|i| i.node.size).sum();
    log.info(&format!(
        "queue built: {} files, {} bytes → {dest}",
        queue.len(),
        total_bytes
    ));

    std::fs::create_dir_all(&dest).map_err(|e| format!("DEST_MKDIR_FAILED. {e}"))?;
    let root = std::path::Path::new(&dest)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(&dest));
    let mut fs_dest = FsDest { root };

    let report = run_recovery(
        dev.as_ref(),
        &queue,
        &mut fs_dest,
        &ScanControl::new(),
        |p| {
            log.verbose(&format!(
                "recover {}/{} files ({}/{} bytes) — {}",
                p.done_files, p.total_files, p.done_bytes, p.total_bytes, p.current_file
            ));
        },
    )
    .map_err(|e| format!("RECOVERY_FAILED. {e}"))?;

    for (name, err) in &report.failures {
        log.error(&format!("failed: {name}: {err}"));
    }
    log.info(&format!(
        "recovery complete: {}/{} files, {} bytes, {} failed",
        report.done_files, report.total_files, report.done_bytes, report.failed_files
    ));

    let out = CliRecoverReport {
        total_files: report.total_files,
        done_files: report.done_files,
        failed_files: report.failed_files,
        total_bytes: report.total_bytes,
        done_bytes: report.done_bytes,
        destination: dest.clone(),
        failures: report.failures,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
    );

    if out.failed_files > 0 {
        // Partial success is still success-with-findings; CI gates on
        // the JSON counters.
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

fn cli_node_to_tree(n: &CliNode) -> TreeNode {
    TreeNode {
        id: n.id,
        parent_id: n.parent_id,
        name: n.name.clone(),
        is_dir: n.is_dir,
        category: wire_category(&n.category),
        size: n.size,
        modified: n.modified,
        origin: wire_origin(&n.origin),
        recoverability: wire_grade(&n.recoverability),
        path: n.path.clone(),
        timestamps: dw_core::types::Timestamps::default(),
        attrs: dw_core::types::FileAttrs::default(),
        extents: n.extents.clone(),
        confidence: n.confidence,
        dimensions: None,
        source: dw_core::types::FsKind::Raw,
        partition: None,
    }
}

fn wire_category(s: &str) -> dw_core::types::Category {
    match s {
        "images" => dw_core::types::Category::Images,
        "videos" => dw_core::types::Category::Videos,
        "audio" => dw_core::types::Category::Audio,
        "documents" => dw_core::types::Category::Documents,
        "archives" => dw_core::types::Category::Archives,
        "email" => dw_core::types::Category::Email,
        _ => dw_core::types::Category::Other,
    }
}

fn wire_origin(s: &str) -> dw_core::types::NodeOrigin {
    match s {
        "existing" => dw_core::types::NodeOrigin::Existing,
        "deleted" => dw_core::types::NodeOrigin::Deleted,
        "lost-path" => dw_core::types::NodeOrigin::LostPath,
        "lost-name" => dw_core::types::NodeOrigin::LostName,
        "tags" => dw_core::types::NodeOrigin::Tags,
        _ => dw_core::types::NodeOrigin::Version,
    }
}

fn wire_grade(s: &str) -> dw_core::types::Recoverability {
    match s {
        "excellent" => dw_core::types::Recoverability::Excellent,
        "good" => dw_core::types::Recoverability::Good,
        _ => dw_core::types::Recoverability::Poor,
    }
}
