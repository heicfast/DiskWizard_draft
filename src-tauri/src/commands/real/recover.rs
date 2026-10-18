//! Recovery commands — the export funnel, destinations, preview.

use std::sync::Arc;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use dw_core::blockdev::BlockDevice;
use dw_core::recover::{build_queue, run_recovery, DestWriter};
use dw_core::tree::TreeNode;

use super::scan::resolve_target;
use super::AppState;

/// The filesystem destination writer (real IO; the core's seam impl).
struct FsDest {
    root: std::path::PathBuf,
}

impl FsDest {
    fn resolve(&self, rel: &str) -> std::path::PathBuf {
        // Defense-in-depth traversal guard (the queue already
        // sanitizes segments; this rejects any escape outright).
        let p = self.root.join(rel);
        let canon = p.canonicalize().unwrap_or_else(|_| p.clone());
        if !canon.starts_with(&self.root) {
            self.root.join("recovered")
        } else {
            p
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
            let ok = unsafe {
                GetDiskFreeSpaceExW(
                    windows::core::PCWSTR(w.as_ptr()),
                    Some(&mut free_caller),
                    Some(&mut total),
                    Some(&mut free),
                )
            };
            ok.as_bool()
                .then_some(free)
                .ok_or_else(|| "free-space query failed".to_string())
        }
        #[cfg(not(windows))]
        Ok(u64::MAX / 4)
    }
}

#[tauri::command]
pub fn list_recover_destinations(source_device_id: Option<String>) -> Value {
    #[cfg(windows)]
    {
        let inv = dw_win::devices::enumerate_inventory();
        let source_disk = source_device_id.as_deref().and_then(|id| {
            dw_win::volume::parse_device_id(id).map(|t| match t {
                dw_win::volume::DeviceTarget::WholeDisk(n)
                | dw_win::volume::DeviceTarget::Partition { disk: n, .. }
                | dw_win::volume::DeviceTarget::LostPartition { disk: n, .. } => Some(n),
                dw_win::volume::DeviceTarget::Volume(l) => dw_win::blockdev::RawHandleDevice::open_volume(l)
                    .ok()
                    .and_then(|d| d.device_number().ok().map(|(n, _)| n)),
            }).flatten()
        });
        let mut rows: Vec<Value> = Vec::new();
        for d in inv.local_disks.iter().chain(inv.external_devices.iter()) {
            if d.letter.is_none() || d.total == 0 {
                continue;
            }
            let is_source = source_disk.is_some_and(|src| src == d.disk_index);
            rows.push(json!({
                "id": format!("dest-{}", d.id),
                "name": d.name,
                "kind": "local",
                "freeBytes": d.total.saturating_sub(d.used),
                "totalBytes": d.total,
                "isSource": is_source,
            }));
        }
        json!(rows)
    }
    #[cfg(not(windows))]
    {
        let _ = source_device_id;
        json!([])
    }
}

/// The recovery worker state (one run at a time; progress events).
#[tauri::command]
pub fn start_recovery(
    app: AppHandle,
    s: State<'_, AppState>,
    generation: Option<u64>,
    node_ids: Option<Vec<u64>>,
    destination_name: Option<String>,
    enable_repair: Option<bool>,
) -> Result<Value, String> {
    super::license::require_licensed(&s)?;
    let generation = generation.unwrap_or_default();
    if s.scan.generation() != generation || generation == 0 {
        return Err("NO_SCAN_RESULTS. Start a scan before recovering.".into());
    }
    let nodes = s
        .scan
        .tree
        .lock()
        .clone()
        .ok_or("NO_SCAN_RESULTS. The scan has no results yet.")?;
    let ids = node_ids.unwrap_or_default();
    if ids.is_empty() {
        return Err("BAD_REQUEST. Select files to recover.".into());
    }
    let queue = build_queue(&nodes, &ids);
    if queue.is_empty() {
        return Err("NOTHING_SELECTED. The selection contains no recoverable files.".into());
    }
    // Destination: resolve by name → path (the picker's rows carry
    // the volume label; the actual folder defaults to a timestamped
    // Recovered folder at the volume root).
    let (dest_root, dest_display) = resolve_destination(destination_name.as_deref())?;
    let (dev, _name) = resolve_target(&s.scan.target_id())?;
    let enable = enable_repair.unwrap_or(true);
    let app = app.clone();
    std::thread::Builder::new()
        .name("dw-recover")
        .spawn(move || {
            let state: State<'_, AppState> = app.state();
            let mut dest = FsDest { root: dest_root.clone() };
            let ctrl = state.scan.control.lock().clone();
            let total_files = queue.len() as u64;
            let total_bytes: u64 = queue.iter().map(|i| i.node.size).sum();
            let app2 = app.clone();
            let result = run_recovery(&*dev, &queue, &mut dest, &ctrl, move |p| {
                let _ = &app2;
                let payload = json!({
                    "generation": generation,
                    "totalFiles": p.total_files,
                    "doneFiles": p.done_files,
                    "failedFiles": p.failed_files,
                    "totalBytes": p.total_bytes,
                    "doneBytes": p.done_bytes,
                    "currentFile": p.current_file,
                    "destination": dest_display,
                    "enableRepair": enable,
                    "repairedFiles": 0,
                    "finished": false,
                });
                let _ = app2.emit("recover-progress", payload);
            });
            match result {
                Ok(report) => {
                    let _ = app.emit(
                        "recover-done",
                        json!({
                            "generation": generation,
                            "totalFiles": report.total_files,
                            "doneFiles": report.done_files,
                            "failedFiles": report.failed_files,
                            "totalBytes": report.total_bytes,
                            "doneBytes": report.done_bytes,
                            "currentFile": "Complete",
                            "destination": dest_display,
                            "enableRepair": enable,
                            "repairedFiles": 0,
                            "finished": true,
                        }),
                    );
                }
                Err(e) => {
                    let _ = app.emit(
                        "recover-error",
                        json!({ "generation": generation, "error": e.to_string() }),
                    );
                }
            }
        })
        .map_err(|e| format!("THREAD_SPAWN_FAILED. {e}"))?;
    Ok(json!({ "ok": true }))
}

fn resolve_destination(name: Option<&str>) -> Result<(std::path::PathBuf, String), String> {
    #[cfg(windows)]
    {
        // Default: the first non-source writable volume, under a
        // timestamped Recovered folder. Named destinations resolve by
        // the inventory's display name.
        let inv = dw_win::devices::enumerate_inventory();
        let mut chosen: Option<(String, String)> = None; // (letter, display)
        for d in inv.local_disks.iter().chain(inv.external_devices.iter()) {
            let Some(letter) = &d.letter else { continue };
            let display = d.name.clone();
            if let Some(want) = name {
                if display == want || display.starts_with(&format!("{want} (")) {
                    chosen = Some((letter.clone(), display));
                    break;
                }
            }
        }
        if chosen.is_none() && name.is_none() {
            for d in inv.local_disks.iter().chain(inv.external_devices.iter()) {
                if let Some(letter) = &d.letter {
                    chosen = Some((letter.clone(), d.name.clone()));
                    break;
                }
            }
        }
        let Some((letter, display)) = chosen else {
            return Err("NO_DESTINATION. No writable volume found.".into());
        };
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let root = std::path::PathBuf::from(format!("{letter}:\\DiskWizard Recovered {stamp}"));
        std::fs::create_dir_all(&root).map_err(|e| format!("mkdir destination: {e}"))?;
        Ok((root, display))
    }
    #[cfg(not(windows))]
    {
        let _ = name;
        let dir = std::env::temp_dir().join("dw-recovered");
        std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
        Ok((dir, "Recovered".into()))
    }
}

/// Preview a file's leading bytes (the preview panel's real data
/// source; images decode client-side from the base64).
#[tauri::command]
pub fn preview_file(
    s: State<'_, AppState>,
    generation: u64,
    node_id: u64,
    max_bytes: Option<u64>,
) -> Result<Value, String> {
    let cap = max_bytes.unwrap_or(2 * 1024 * 1024).min(16 * 1024 * 1024);
    if s.scan.generation() != generation {
        return Err("STALE_GENERATION. The results changed; re-open the preview.".into());
    }
    let nodes = s
        .scan
        .tree_slot()
        .lock()
        .clone()
        .ok_or("NO_SCAN_RESULTS. Nothing to preview.")?;
    let Some(node) = nodes.iter().find(|n: &&TreeNode| n.id == node_id) else {
        return Err("UNKNOWN_NODE. That file is no longer in the results.".into());
    };
    if node.is_dir {
        return Err("IS_DIRECTORY. Directories have no preview bytes.".into());
    }
    let (dev, _name) = resolve_target(&s.scan.target_id())?;
    let take = node.size.min(cap) as usize;
    let mut buf = vec![0u8; take];
    // Stream through the node's extents.
    let mut filled = 0usize;
    let mut remaining = take as u64;
    for ext in &node.extents {
        if remaining == 0 {
            break;
        }
        let want = (ext.len as usize).min(remaining as usize);
        if want == 0 {
            continue;
        }
        if dev.read_bytes(ext.offset, &mut buf[filled..filled + want]).is_err() {
            buf[filled..filled + want].fill(0);
        }
        filled += want;
        remaining -= want as u64;
    }
    let b64 = b64_encode(&buf);
    let mime = mime_for(&node.name);
    Ok(json!({
        "mime": mime,
        "size": node.size,
        "data": b64,
    }))
}

/// Standard base64 (the preview payload codec — no external dep).
fn b64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 0x3F] as char);
        out.push(T[(n >> 12) as usize & 0x3F] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 0x3F] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 0x3F] as char } else { '=' });
    }
    out
}

fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "heic" | "heif" => "image/heic",
        "pdf" => "application/pdf",
        "txt" | "md" | "csv" | "log" | "json" | "xml" | "html" => "text/plain",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}
