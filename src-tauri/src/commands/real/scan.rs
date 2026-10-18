//! Scan commands — the real engine lifecycle: start (worker thread),
//! pause/resume/stop, progress events, results snapshots.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use dw_core::blockdev::BlockDevice;
use dw_core::fs::ScanControl;
use dw_core::scan::{Phase, ProgressObserver, ScanController, ScanReport};
use dw_core::tree::TreeNode;

use super::AppState;

/// The live scan state (interior-mutable; the worker thread updates
/// through the AppHandle's managed state).
pub struct ScanSession {
    generation: AtomicU64,
    target_id: Mutex<String>,
    target_name: Mutex<String>,
    control: Mutex<ScanControl>,
    tree: Mutex<Option<Arc<Vec<TreeNode>>>>,
    report: Mutex<Option<ScanReport>>,
    phase: Mutex<Phase>,
    started_at: Mutex<Option<Instant>>,
}

impl Default for ScanSession {
    fn default() -> Self {
        Self {
            generation: AtomicU64::new(0),
            target_id: Mutex::new(String::new()),
            target_name: Mutex::new(String::new()),
            control: Mutex::new(ScanControl::new()),
            tree: Mutex::new(None),
            report: Mutex::new(None),
            phase: Mutex::new(Phase::Preparing),
            started_at: Mutex::new(None),
        }
    }
}

impl ScanSession {
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Test/resume adoption helpers (session commands).
    pub fn set_generation(&self, gen: u64) {
        self.generation.store(gen, Ordering::SeqCst);
    }

    pub fn set_target(&self, id: &str, name: &str) {
        *self.target_id.lock() = id.to_string();
        *self.target_name.lock() = name.to_string();
    }

    pub fn tree_slot(&self) -> &Mutex<Option<Arc<Vec<TreeNode>>>> {
        &self.tree
    }

    pub fn report_slot(&self) -> &Mutex<Option<ScanReport>> {
        &self.report
    }

    pub fn phase_slot(&self) -> &Mutex<Phase> {
        &self.phase
    }

    pub fn target_id(&self) -> String {
        self.target_id.lock().clone()
    }

    fn begin(&self, device_id: &str, display_name: &str) -> u64 {
        let gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        *self.target_id.lock() = device_id.to_string();
        *self.target_name.lock() = display_name.to_string();
        *self.control.lock() = ScanControl::new();
        *self.tree.lock() = None;
        *self.report.lock() = None;
        *self.started_at.lock() = Some(Instant::now());
        gen
    }
}

/// The searching-label ticker (the live "Searching: …" strip).
fn searching_label(p: &dw_core::scan::Progress, target: &str) -> String {
    match p.phase {
        Phase::Preparing => format!("Reading {target} — boot sector, partition table…"),
        Phase::Organizing => "Organizing found files…".into(),
        Phase::Quick | Phase::Deep => format!(
            "Searching: sector {} / {}",
            p.engine_done,
            p.engine_total.max(1)
        ),
        _ => "Scanning…".into(),
    }
}

/// Progress → `scan-progress` events (the frozen payload shape).
struct EventObserver {
    app: AppHandle,
    generation: u64,
    target_id: String,
    target_name: String,
    last_emit: Instant,
    last_percent: f64,
}

impl ProgressObserver for EventObserver {
    fn on_progress(&mut self, p: &dw_core::scan::Progress) {
        // Throttle to ~4 Hz (the UI also polls snapshots; events
        // carry the fresh phase/percent).
        let big_step = (p.percent - self.last_percent).abs() >= 5.0;
        if self.last_emit.elapsed().as_millis() < 250 && !big_step && p.percent < 100.0 {
            return;
        }
        self.last_emit = Instant::now();
        self.last_percent = p.percent;
        let remaining = ((100.0 - p.percent) / 100.0 * 60.0).round() as u64;
        let payload = json!({
            "generation": self.generation,
            "targetId": self.target_id,
            "targetName": self.target_name,
            "phase": p.phase.wire(),
            "percent": p.percent,
            "stats": {
                "filesFound": p.files_found,
                "bytesFound": p.bytes_found,
                "sectorsRead": p.sectors_read,
                "sectorsTotal": p.sectors_total,
            },
            "elapsedSec": p.elapsed_sec,
            "remainingSec": remaining.max(1),
            "searchingLabel": searching_label(p, &self.target_name),
        });
        let _ = self.app.emit("scan-progress", payload);
    }

    fn on_phase(&mut self, phase: Phase) {
        let _ = self.app.emit(
            "scan-phase",
            json!({ "generation": self.generation, "phase": phase.wire() }),
        );
    }
}

/// Resolve a device id → (block device, display name).
fn resolve_target(device_id: &str) -> Result<(Arc<dyn BlockDevice>, String), String> {
    #[cfg(windows)]
    {
        use dw_win::volume::{parse_device_id, DeviceTarget};
        let target = parse_device_id(device_id)
            .ok_or_else(|| format!("UNKNOWN_DEVICE. {device_id}"))?;
        match target {
            DeviceTarget::WholeDisk(n) => {
                let dev = dw_win::blockdev::RawHandleDevice::open_physical(n)
                    .map_err(|e| format!("DEVICE_OPEN_FAILED. {e}"))?;
                Ok((Arc::new(dev), format!("Disk {n}")))
            }
            DeviceTarget::Partition { disk: n, part_no: _ } => {
                // The engine reads the whole disk and resolves the
                // partition through the table.
                let dev = dw_win::blockdev::RawHandleDevice::open_physical(n)
                    .map_err(|e| format!("DEVICE_OPEN_FAILED. {e}"))?;
                Ok((Arc::new(dev), format!("Disk {n}")))
            }
            DeviceTarget::LostPartition { disk: n, start_lba } => {
                let dev = dw_win::blockdev::RawHandleDevice::open_physical(n)
                    .map_err(|e| format!("DEVICE_OPEN_FAILED. {e}"))?;
                let sub = Arc::new(dw_core::blockdev::SubDevice::new(
                    Arc::new(dev) as Arc<dyn BlockDevice>,
                    start_lba,
                    u64::MAX / 512, // clamped to the parent by SubDevice
                    format!("lost@{start_lba}"),
                ));
                Ok((sub, format!("Lost partition at sector {start_lba}")))
            }
            DeviceTarget::Volume(letter) => {
                let dev = dw_win::blockdev::RawHandleDevice::open_volume(letter)
                    .map_err(|e| format!("DEVICE_OPEN_FAILED. {e}"))?;
                Ok((Arc::new(dev), volume_display_name(letter)))
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = device_id;
        Err("NOT_AVAILABLE. Scanning requires Windows.".into())
    }
}

#[cfg(windows)]
fn volume_display_name(letter: char) -> String {
    let root = format!("{letter}:\\");
    let w: Vec<u16> = root.encode_utf16().chain([0]).collect();
    let mut label = [0u16; 64];
    let ok = unsafe {
        windows::Win32::Storage::FileSystem::GetVolumeInformationW(
            windows::core::PCWSTR(w.as_ptr()),
            Some(label.as_mut_ptr()),
            Some(label.len() as u32),
            None,
            None,
            None,
            None,
            None,
        )
    };
    if ok.as_bool() {
        let l = String::from_utf16_lossy(&label).trim_end_matches('\0').to_string();
        if !l.is_empty() {
            return format!("{l} ({letter}:)");
        }
    }
    format!("Drive {letter}:")
}

/// The shared start path.
fn start_scan_common(
    app: &AppHandle,
    s: &AppState,
    device_id: &str,
    display_name: String,
    dev: Arc<dyn BlockDevice>,
) -> Result<Value, String> {
    super::license::require_licensed(s)?;
    let generation = s.scan.begin(device_id, &display_name);
    // Keep the worker's control in the managed state.
    let control = s.scan.control.lock().clone();
    let app = app.clone();
    std::thread::Builder::new()
        .name(format!("dw-scan-{generation}"))
        .spawn(move || {
            let mut obs = EventObserver {
                app: app.clone(),
                generation,
                target_id: device_id.to_string(),
                target_name: display_name.clone(),
                last_emit: Instant::now(),
                last_percent: 0.0,
            };
            let state: State<'_, AppState> = app.state();
            let mut controller =
                ScanController::with_control(dev, &display_name, control.clone());
            let result = controller.run(&mut obs);
            match result {
                Ok((tree, report)) => {
                    let (files, bytes) = tree.totals();
                    *state.scan.tree.lock() = Some(Arc::new(tree.nodes().to_vec()));
                    *state.scan.phase.lock() = Phase::Done;
                    *state.scan.report.lock() = Some(report.clone());
                    // Auto-save the .rsf session (the mock contract's
                    // auto_<Volume>_<ts>.rsf naming).
                    let now = unix_now();
                    let session_file =
                        dw_core::session::auto_save_name(&display_name, now);
                    let path = dw_core::session::sessions_dir().join(&session_file);
                    let _ = dw_core::session::save(
                        &path,
                        &dw_core::session::SessionHeader {
                            files_found: files,
                            bytes_found: bytes,
                            fs: report.fs.wire().to_string(),
                            duration_sec: report.duration_sec,
                            stopped: report.stopped,
                        },
                        &dw_core::session::SessionDevice {
                            name: display_name.clone(),
                            total_bytes: 0,
                            saved_at_unix: now,
                        },
                        tree.nodes(),
                    );
                    let _ = app.emit(
                        "scan-done",
                        json!({
                            "generation": generation,
                            "targetName": display_name,
                            "stats": { "filesFound": files, "bytesFound": bytes },
                            "durationSec": report.duration_sec,
                            "stopped": report.stopped,
                        }),
                    );
                }
                Err(e) => {
                    *state.scan.phase.lock() = Phase::Stopped;
                    let _ = app.emit(
                        "scan-error",
                        json!({ "generation": generation, "error": e.to_string() }),
                    );
                }
            }
        })
        .map_err(|e| format!("THREAD_SPAWN_FAILED. {e}"))?;
    Ok(json!({ "generation": generation }))
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

#[tauri::command]
pub fn start_scan(
    app: AppHandle,
    s: State<'_, AppState>,
    device_id: String,
) -> Result<Value, String> {
    let (dev, name) = resolve_target(&device_id)?;
    start_scan_common(&app, &s, &device_id, name, dev)
}

#[tauri::command]
pub fn start_folder_scan(
    app: AppHandle,
    s: State<'_, AppState>,
    label: String,
) -> Result<Value, String> {
    super::license::require_licensed(&s)?;
    #[cfg(windows)]
    {
        let letter = label
            .chars()
            .next()
            .filter(|c| c.is_ascii_alphabetic())
            .ok_or("BAD_REQUEST. The folder path needs a drive letter.")?;
        let dev = dw_win::blockdev::RawHandleDevice::open_volume(letter)
            .map_err(|e| format!("DEVICE_OPEN_FAILED. {e}"))?;
        start_scan_common(&app, &s, &label, label.clone(), Arc::new(dev))
    }
    #[cfg(not(windows))]
    {
        let _ = (app, label);
        Err("NOT_AVAILABLE. Folder scans require Windows.".into())
    }
}

#[tauri::command]
pub fn start_disk_image_scan(
    app: AppHandle,
    s: State<'_, AppState>,
    image_path: String,
) -> Result<Value, String> {
    super::license::require_licensed(&s)?;
    let name = std::path::Path::new(&image_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Disk image")
        .to_string();
    let dev = dw_core::blockdev::ImageFile::open(
        std::path::Path::new(&image_path),
        dw_core::blockdev::SectorSizeMode::Probe,
    )
    .map_err(|e| format!("IMAGE_OPEN_FAILED. {e}"))?;
    start_scan_common(&app, &s, &image_path, name, Arc::new(dev))
}

#[tauri::command]
pub fn pause_scan(s: State<'_, AppState>) -> Value {
    s.scan.control.lock().set_paused(true);
    json!({ "ok": true })
}

#[tauri::command]
pub fn resume_scan(s: State<'_, AppState>) -> Value {
    s.scan.control.lock().set_paused(false);
    json!({ "ok": true })
}

#[tauri::command]
pub fn stop_scan(s: State<'_, AppState>) -> Value {
    s.scan.control.lock().cancel();
    json!({ "ok": true })
}

#[tauri::command]
pub fn get_scan_progress(s: State<'_, AppState>) -> Value {
    let generation = s.scan.generation();
    if generation == 0 {
        return Value::Null;
    }
    let phase = *s.scan.phase.lock();
    let report = s.scan.report.lock().clone();
    let (files, bytes) = report
        .as_ref()
        .map(|r| (r.files_found, r.bytes_found))
        .unwrap_or((0, 0));
    let elapsed = s
        .scan
        .started_at
        .lock()
        .map(|t| t.elapsed().as_secs())
        .unwrap_or_default();
    json!({
        "generation": generation,
        "targetId": *s.scan.target_id.lock(),
        "targetName": *s.scan.target_name.lock(),
        "phase": phase.wire(),
        "percent": if phase == Phase::Done { 100.0 } else { 0.0 },
        "stats": {
            "filesFound": files,
            "bytesFound": bytes,
            "sectorsRead": 0,
            "sectorsTotal": 0,
        },
        "elapsedSec": elapsed,
        "remainingSec": 1,
        "searchingLabel": "Scanning…",
    })
}

#[tauri::command]
pub fn get_scan_results(s: State<'_, AppState>, generation: u64) -> Value {
    if s.scan.generation() != generation {
        return Value::Null;
    }
    let tree = s.scan.tree.lock().clone();
    let Some(nodes) = tree else {
        return Value::Null();
    };
    let mut categories: std::collections::HashMap<String, (u64, u64)> =
        std::collections::HashMap::new();
    let now = unix_now();
    let mut wire_nodes = Vec::with_capacity(nodes.len());
    for n in nodes.iter() {
        if !n.is_dir {
            let c = categories
                .entry(n.category.wire().to_string())
                .or_insert((0, 0));
            c.0 += 1;
            c.1 += n.size;
        }
        wire_nodes.push(json!({
            "id": n.id,
            "parentId": n.parent_id,
            "name": n.name,
            "isDir": n.is_dir,
            "category": n.category.wire(),
            "size": n.size,
            "modified": n.modified,
            "origin": n.origin.wire(),
            "recoverability": n.recoverability.wire(),
            "path": n.path,
            "width": n.dimensions.map(|d| d.0),
            "height": n.dimensions.map(|d| d.1),
            "deletedAgo": n.timestamps
                .deleted
                .map(|t| (now - t) / 86_400),
            "ext": n.name.rsplit('.').next().unwrap_or("").to_string(),
        }));
    }
    let report = s.scan.report.lock().clone();
    json!({
        "generation": generation,
        "targetName": *s.scan.target_name.lock(),
        "categories": categories.iter().map(|(k, (count, size))| json!({
            "category": k, "count": count, "size": size,
        })).collect::<Vec<_>>(),
        "nodes": wire_nodes,
        "durationSec": report.map(|r| r.duration_sec).unwrap_or_default(),
    })
}
