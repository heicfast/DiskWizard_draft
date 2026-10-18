//! Scan commands — the real engine lifecycle: start (worker thread),
//! pause/resume/stop, progress events, results snapshots.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use parking_lot::Mutex;
use serde_json::{Value, json};
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
    /// The worker's latest engine snapshot (the poll endpoint's data
    /// source — the UI polls when events are throttled or missed).
    progress: Mutex<Option<dw_core::scan::Progress>>,
    searching: Mutex<String>,
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
            progress: Mutex::new(None),
            searching: Mutex::new("Scanning…".into()),
        }
    }
}

impl ScanSession {
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Test/resume adoption helpers (session commands).
    pub fn set_generation(&self, value: u64) {
        self.generation.store(value, Ordering::SeqCst);
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
        let next = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        *self.target_id.lock() = device_id.to_string();
        *self.target_name.lock() = display_name.to_string();
        *self.control.lock() = ScanControl::new();
        *self.tree.lock() = None;
        *self.report.lock() = None;
        *self.started_at.lock() = Some(Instant::now());
        next
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
        // Refresh the poll slots (single managed state write).
        if let Some(state) = self.app.try_state::<AppState>() {
            *state.scan.progress.lock() = Some(*p);
            *state.scan.searching.lock() = searching_label(p, &self.target_name);
        }
        let rate = if p.percent > 0.0 {
            (p.elapsed_sec as f64 * (100.0 - p.percent) / p.percent).ceil() as u64
        } else {
            60
        };
        let remaining = rate.max(1);
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
pub(super) fn resolve_target(device_id: &str) -> Result<(Arc<dyn BlockDevice>, String), String> {
    #[cfg(windows)]
    {
        use dw_win::volume::{DeviceTarget, parse_device_id};
        let target =
            parse_device_id(device_id).ok_or_else(|| format!("UNKNOWN_DEVICE. {device_id}"))?;
        match target {
            DeviceTarget::WholeDisk(n) => {
                let dev = dw_win::blockdev::RawHandleDevice::open_physical(n)
                    .map_err(|e| format!("DEVICE_OPEN_FAILED. {e}"))?;
                Ok((Arc::new(dev), format!("Disk {n}")))
            }
            DeviceTarget::Partition { disk: n, part_no } => {
                // The engine must scan the VOLUME (LBA 0 = the
                // partition's boot sector), never the whole-disk MBR.
                // Slice the selected partition out of the parsed table.
                let dev = dw_win::blockdev::RawHandleDevice::open_physical(n)
                    .map_err(|e| format!("DEVICE_OPEN_FAILED. {e}"))?;
                let parent: Arc<dyn BlockDevice> = Arc::new(dev);
                let (start, len) = partition_extent(&parent, part_no)
                    .ok_or_else(|| format!("PARTITION_NOT_FOUND. disk {n} partition {part_no}"))?;
                let sub = Arc::new(dw_core::blockdev::SubDevice::new(
                    parent,
                    start,
                    len,
                    format!("disk{n}-part{part_no}"),
                ));
                Ok((sub, format!("Disk {n} Partition {part_no}")))
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

/// Locate partition `part_no` on an open whole-disk device →
/// `(start_lba, len_sectors)`.
///
/// Matching order: the parsed table's index (GPT entry order and
/// dense MBR tables number exactly like Windows' PartitionNumber),
/// then an MBR slot fallback — Windows numbers MBR primaries by SLOT
/// position (slot i → i+1), which sparse tables shift relative to the
/// count-based parse.
#[cfg(windows)]
fn partition_extent(disk: &Arc<dyn BlockDevice>, part_no: u32) -> Option<(u64, u64)> {
    use dw_core::blockdev::BlockRead;
    if let Ok(layout) = dw_win::devices::read_layout(disk.as_ref()) {
        if let Some(p) = layout.partitions.iter().find(|p| p.index == part_no) {
            return Some((p.start_lba, p.total_sectors));
        }
    }
    // MBR slot fallback: slot i (0-based) ⇒ Windows PartitionNumber
    // i+1 for primaries.
    let sector0 = disk.read_struct_at::<512>(0).ok()?;
    if u16::from_le_bytes([sector0[510], sector0[511]]) != 0xAA55 {
        return None;
    }
    let slot = (part_no.clamp(1, 4) - 1) as usize;
    let entry = &sector0[0x1BE + slot * 16..0x1BE + slot * 16 + 16];
    let ptype = entry[4];
    if ptype == 0x00 || ptype == 0x05 || ptype == 0x0F || ptype == 0xEE {
        return None;
    }
    let start = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as u64;
    let len = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as u64;
    (len > 0).then_some((start, len))
}

#[cfg(windows)]
fn volume_display_name(letter: char) -> String {
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
    // QA-8 (§5.1 item 8 disclosure): a mounted-volume target means a
    // LIVE scan — the UI must disclose consistency limits (no VSS
    // snapshot). Computed here (the worker only owns the device
    // handle, not the target id's mount semantics).
    let source_volume_live = source_volume_live(device_id);
    // Hoist the borrowed id into an owned String — the worker thread
    // requires 'static captures (device_id: &str cannot escape).
    let device_id = device_id.to_string();
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
            let mut controller = ScanController::with_control(dev, &display_name, control.clone());
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
                    let session_file = dw_core::session::auto_save_name(&display_name, now);
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
                            "sourceVolumeLive": source_volume_live,
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

/// QA-8: true when the scan target is a MOUNTED volume (drive-letter
/// handle) — the scan ran live on a writable filesystem, so the results
/// header must disclose the consistency limits (files changed during
/// the scan may not be captured; §5.1 item 8's "or clearly disclose"
/// arm). Partition-slice and whole-disk targets are raw reads of
/// unmounted regions — not live.
fn source_volume_live(device_id: &str) -> bool {
    #[cfg(windows)]
    {
        matches!(
            dw_win::volume::parse_device_id(device_id),
            Ok(dw_win::volume::DeviceTarget::Volume(_))
        )
    }
    #[cfg(not(windows))]
    {
        let _ = device_id;
        false
    }
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
    let progress = s.scan.progress.lock().clone();
    let report = s.scan.report.lock().clone();
    // Prefer the worker's live snapshot; fall back to the final
    // report for done/stopped scans whose events already flushed.
    let (percent, stats, elapsed, searching) = match (&progress, &report) {
        (Some(p), _) => (
            p.percent,
            (
                p.files_found,
                p.bytes_found,
                p.sectors_read,
                p.sectors_total,
            ),
            p.elapsed_sec,
            s.scan.searching.lock().clone(),
        ),
        (None, Some(r)) => (
            if phase == Phase::Done { 100.0 } else { 0.0 },
            (r.files_found, r.bytes_found, 0, 0),
            r.duration_sec,
            "Scanning…".to_string(),
        ),
        (None, None) => (
            if phase == Phase::Done { 100.0 } else { 0.0 },
            (0, 0, 0, 0),
            s.scan
                .started_at
                .lock()
                .map(|t| t.elapsed().as_secs())
                .unwrap_or_default(),
            "Scanning…".to_string(),
        ),
    };
    let remaining = if percent > 0.0 && percent < 100.0 {
        ((elapsed as f64 * (100.0 - percent) / percent).ceil() as u64).max(1)
    } else if percent >= 100.0 {
        0
    } else {
        1
    };
    let (files, bytes, sectors_read, sectors_total) = stats;
    json!({
        "generation": generation,
        "targetId": *s.scan.target_id.lock(),
        "targetName": *s.scan.target_name.lock(),
        "phase": phase.wire(),
        "percent": percent,
        "stats": {
            "filesFound": files,
            "bytesFound": bytes,
            "sectorsRead": sectors_read,
            "sectorsTotal": sectors_total,
        },
        "elapsedSec": elapsed,
        "remainingSec": remaining,
        "searchingLabel": searching,
    })
}

#[tauri::command]
pub fn get_scan_results(s: State<'_, AppState>, generation: u64) -> Value {
    if s.scan.generation() != generation {
        return Value::Null;
    }
    let tree = s.scan.tree.lock().clone();
    let Some(nodes) = tree else {
        return Value::Null;
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
        // The scan target's device id — the recovery funnel passes this
        // to `list_recover_destinations` so source rows are flagged, and
        // `start_recovery` re-derives the same exclusion server-side.
        "deviceId": s.scan.target_id(),
        "categories": categories.iter().map(|(k, (count, size))| json!({
            "category": k, "count": count, "size": size,
        })).collect::<Vec<_>>(),
        "nodes": wire_nodes,
        "durationSec": report.map(|r| r.duration_sec).unwrap_or_default(),
    })
}
