//! Scan commands — the real engine lifecycle: start (worker thread),
//! pause/resume/stop, progress events, results snapshots.
//!
//! Session-9 architecture (the owner's symptom fixes):
//! * **Delta publishing** — mid-scan tree snapshots land as APPENDED
//!   nodes (the old full-copy-per-publish was O(n²) total) with
//!   cumulative category counters (the old slice-local counters
//!   clobbered the chips on every append).
//! * **The guaranteed final adoption** — the worker folds the
//!   complete tree into the session BEFORE `scan-done` fires, and
//!   the frontend force-reloads on done: the tour race (the 1.4 s
//!   poll loop tearing down between the last null poll and the
//!   final publish → `results-snapshot-null` forever) is closed at
//!   BOTH ends.
//! * **The ETA tracker** — a windowed EWMA rate (the old
//!   `elapsed × (100−p)/p` extrapolation grew monotonically whenever
//!   progress was sub-linear: "remaining time keeps increasing").
//! * **Honest units** — quick reports records, deep reports SECTORS.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use parking_lot::Mutex;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};

use dw_core::blockdev::BlockDevice;
use dw_core::fs::ScanControl;
use dw_core::scan::{Phase, ProgressObserver, ScanController, ScanReport};
use dw_core::tree::TreeNode;

use super::AppState;

/// Fold `appended` into `tree` once it grows past this (bounds the
/// concat cost; get_scan_results slices stay O(delta)).
const FOLD_THRESHOLD: usize = 131_072;

/// The live scan state (interior-mutable; the worker thread updates
/// through the AppHandle's managed state).
pub struct ScanSession {
    generation: AtomicU64,
    target_id: Mutex<String>,
    target_name: Mutex<String>,
    control: Mutex<ScanControl>,
    /// The folded base snapshot (immutable once set).
    tree: Mutex<Option<Arc<Vec<TreeNode>>>>,
    /// Nodes appended since the last fold (live-scan deltas).
    appended: Mutex<Vec<TreeNode>>,
    /// Cumulative category counters over base+appended (the chips).
    categories: Mutex<std::collections::HashMap<String, (u64, u64)>>,
    /// Builder node count already copied out (delta bookkeeping).
    taken: AtomicUsize,
    report: Mutex<Option<ScanReport>>,
    phase: Mutex<Phase>,
    started_at: Mutex<Option<Instant>>,
    /// The worker's latest engine snapshot (the poll endpoint's data
    /// source — the UI polls when events are throttled or missed).
    progress: Mutex<Option<dw_core::scan::Progress>>,
    searching: Mutex<String>,
    /// The windowed-rate ETA (seconds; the poll endpoint echoes it).
    eta: Mutex<u64>,
}

impl Default for ScanSession {
    fn default() -> Self {
        Self {
            generation: AtomicU64::new(0),
            target_id: Mutex::new(String::new()),
            target_name: Mutex::new(String::new()),
            control: Mutex::new(ScanControl::new()),
            tree: Mutex::new(None),
            appended: Mutex::new(Vec::new()),
            categories: Mutex::new(std::collections::HashMap::new()),
            taken: AtomicUsize::new(0),
            report: Mutex::new(None),
            phase: Mutex::new(Phase::Preparing),
            started_at: Mutex::new(None),
            progress: Mutex::new(None),
            searching: Mutex::new("Scanning…".into()),
            eta: Mutex::new(0),
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

    /// The COMPLETE results view (base + appended) — the recovery
    /// queue and preview paths read everything.
    pub fn snapshot_nodes(&self) -> Option<Arc<Vec<TreeNode>>> {
        let base = self.tree.lock().clone();
        let appended = self.appended.lock();
        match (base, appended.is_empty()) {
            (None, _) => None,
            (Some(b), true) => Some(b),
            (Some(b), false) => {
                let mut all = Vec::with_capacity(b.len() + appended.len());
                all.extend_from_slice(&b);
                all.extend_from_slice(&appended);
                Some(Arc::new(all))
            }
        }
    }

    /// Total materialized nodes (base + appended).
    fn node_count(&self) -> usize {
        let base = self
            .tree
            .lock()
            .as_ref()
            .map_or(0, |b: &std::sync::Arc<Vec<TreeNode>>| b.len());
        base + self.appended.lock().len()
    }

    /// Mid-scan progressive publish: copy the builder's APPENDED
    /// nodes (everything past `taken`), update the cumulative
    /// category counters, fold when large. The old per-publish full
    /// copy was O(n) per 2048 nodes → O(n²/2048) total — a 1M-file
    /// tree spent minutes just copying.
    ///
    /// Lock order (the session-wide contract): `tree` → `appended` →
    /// `categories`, ALL held across the fold so a concurrent poller
    /// can never observe the new base with the un-cleared delta
    /// (duplicate nodes) or the old base with a cleared one (lost
    /// nodes).
    fn publish_delta(&self, nodes: &[TreeNode]) {
        let taken = self.taken.load(Ordering::Acquire);
        if nodes.len() <= taken {
            return; // no growth (pure re-emits)
        }
        let mut base = self.tree.lock();
        let mut appended = self.appended.lock();
        let mut cats = self.categories.lock();
        for n in &nodes[taken..] {
            if !n.is_dir {
                let c = cats.entry(n.category.wire().to_string()).or_insert((0, 0));
                c.0 += 1;
                c.1 += n.size;
            }
        }
        appended.extend_from_slice(&nodes[taken..]);
        self.taken.store(nodes.len(), Ordering::Release);
        if appended.len() >= FOLD_THRESHOLD && base.is_some() {
            let mut all = Vec::with_capacity(
                base.as_ref()
                    .map_or(0, |b: &std::sync::Arc<Vec<TreeNode>>| b.len())
                    + appended.len(),
            );
            if let Some(b) = base.as_deref() {
                all.extend_from_slice(b);
            }
            all.extend_from_slice(&appended);
            *base = Some(Arc::new(all));
            appended.clear();
        }
    }

    /// The final adoption: the COMPLETE tree replaces the session
    /// state (fresh dir sizes/paths after the finish() fixpoint —
    /// delta copies could be stale on rolled-up fields), categories
    /// recompute from the authoritative counters. Called BEFORE
    /// `scan-done` emits so the done-reload can never race it.
    fn adopt_final(
        &self,
        nodes: Arc<Vec<TreeNode>>,
        counts: Vec<dw_core::tree::CategoryCountWire>,
    ) {
        *self.tree.lock() = Some(nodes);
        self.appended.lock().clear();
        self.taken.store(usize::MAX, Ordering::Release);
        let mut cats = self.categories.lock();
        cats.clear();
        for c in counts {
            cats.insert(c.category.wire().to_string(), (c.count, c.size));
        }
    }

    /// Session/resume adoption (the .rsf load path). Counters are
    /// computed from the parameter BEFORE any lock; the assignment
    /// holds tree → appended → categories together (the canonical
    /// order — a concurrent poller sees the adoption atomically).
    pub fn adopt_loaded(&self, nodes: Arc<Vec<TreeNode>>) {
        let mut new_cats = std::collections::HashMap::new();
        for n in nodes.iter() {
            if !n.is_dir {
                let c = new_cats
                    .entry(n.category.wire().to_string())
                    .or_insert((0, 0));
                c.0 += 1;
                c.1 += n.size;
            }
        }
        let mut tree = self.tree.lock();
        let mut appended = self.appended.lock();
        let mut cats = self.categories.lock();
        *tree = Some(nodes);
        appended.clear();
        self.taken.store(usize::MAX, Ordering::Release);
        *cats = new_cats;
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
        self.appended.lock().clear();
        self.categories.lock().clear();
        self.taken.store(0, Ordering::Release);
        *self.report.lock() = None;
        *self.started_at.lock() = Some(Instant::now());
        next
    }
}

/// The searching-label ticker (the live "Searching: …" strip). Quick
/// phase counts records ("Files Found" — the og's status string);
/// deep phase reports SECTORS (the og's "Reading sector: X/Y").
fn searching_label(p: &dw_core::scan::Progress, target: &str) -> String {
    match p.phase {
        Phase::Preparing => format!("Reading {target} — boot sector, partition table…"),
        Phase::Organizing => "Organizing found files…".into(),
        Phase::Quick => format!("Quick scanning — {} files found", p.files_found),
        Phase::Deep => format!(
            "Searching: sector {} / {}",
            p.engine_done,
            p.engine_total.max(1)
        ),
        _ => "Scanning…".into(),
    }
}

/// Windowed ETA (session 9): an EWMA over measured percent-per-second
/// samples — the honest replacement for the old
/// `elapsed × (100 − p) / p` extrapolation that grew monotonically
/// while progress was sub-linear (the owner's "remaining time keeps
/// increasing"). Needs ≥1 s of measured movement before it commits;
/// clamps to a day so a slow start can't claim weeks.
#[derive(Default)]
struct EtaTracker {
    last: Option<(Instant, f64)>,
    rate: Option<f64>,
}

impl EtaTracker {
    fn update(&mut self, percent: f64) -> u64 {
        if percent >= 100.0 {
            return 0;
        }
        let now = Instant::now();
        if let Some((t, p)) = self.last {
            let dt = now.duration_since(t).as_secs_f64();
            if dt >= 1.0 {
                if percent > p {
                    let inst = (percent - p) / dt;
                    self.rate = Some(self.rate.map_or(inst, |r| r * 0.7 + inst * 0.3));
                }
                // percent <= p: no update (a stall keeps the last
                // measured rate — honest, and never the old runaway).
                self.last = Some((now, percent));
            }
        } else {
            self.last = Some((now, percent));
        }
        match self.rate {
            Some(r) if r > 1e-6 => (((100.0 - percent) / r).ceil() as u64).clamp(1, 86_400),
            _ => 60,
        }
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
    eta: EtaTracker,
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
        let eta = self.eta.update(p.percent);
        let label = searching_label(p, &self.target_name);
        // Refresh the poll slots (single managed state write).
        if let Some(state) = self.app.try_state::<AppState>() {
            *state.scan.progress.lock() = Some(*p);
            *state.scan.searching.lock() = label.clone();
            *state.scan.eta.lock() = eta;
        }
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
            "remainingSec": eta.max(1),
            "searchingLabel": label,
        });
        let _ = self.app.emit("scan-progress", payload);
    }

    fn on_phase(&mut self, phase: Phase) {
        let _ = self.app.emit(
            "scan-phase",
            json!({ "generation": self.generation, "phase": phase.wire() }),
        );
    }

    /// Progressive results: the session stores the builder's APPENDED
    /// nodes (delta publishing — see [`ScanSession::publish_delta`]);
    /// the UI's `get_scan_results` polls accumulate them.
    fn on_tree_update(&mut self, tree: &dw_core::tree::TreeBuilder) {
        if let Some(state) = self.app.try_state::<AppState>() {
            state.scan.publish_delta(tree.nodes());
        }
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
            // The og's compute-worker posture (priority-7 threads,
            // `DRWUI-exe--thread-architecture.md`): HIGHEST, not
            // TIME_CRITICAL — the WebView render thread must keep
            // painting while we burn cores.
            #[cfg(windows)]
            dw_win::raise_thread_priority_highest();
            let mut obs = EventObserver {
                app: app.clone(),
                generation,
                target_id: device_id.to_string(),
                target_name: display_name.clone(),
                last_emit: Instant::now(),
                last_percent: 0.0,
                eta: EtaTracker::default(),
            };
            let state: State<'_, AppState> = app.state();
            let mut controller = ScanController::with_control(dev, &display_name, control.clone());
            let result = controller.run(&mut obs);
            match result {
                Ok((tree, report)) => {
                    let (files, bytes) = tree.totals();
                    let counts = tree.category_counts();
                    // THE GUARANTEED FINAL ADOPTION — before scan-done,
                    // before the session save: the frontend's poll
                    // loop dies at done, and its done-handler reload
                    // (since=0) must find the complete tree waiting.
                    state
                        .scan
                        .adopt_final(Arc::new(tree.nodes().to_vec()), counts);
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

fn unix_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
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
            Some(dw_win::volume::DeviceTarget::Volume(_))
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
    let (percent, stats, elapsed, searching, eta) = match (&progress, &report) {
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
            *s.scan.eta.lock(),
        ),
        (None, Some(r)) => (
            if phase == Phase::Done { 100.0 } else { 0.0 },
            (r.files_found, r.bytes_found, 0, 0),
            r.duration_sec,
            "Scanning…".to_string(),
            if phase == Phase::Done { 0 } else { 60 },
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
            60,
        ),
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
        "remainingSec": eta,
        "searchingLabel": searching,
    })
}

/// One wire node (the frozen FileNode shape; timestamps in unix
/// MILLISECONDS — session 9's precision contract — plus the ADDITIVE
/// `modifiedRaw`: the full 100-ns FILETIME ticks for NTFS sources,
/// the RE precision contract preserved end-to-end; absent on
/// filesystems without sub-ms timestamps).
fn wire_node(n: &TreeNode, now_ms: i64) -> Value {
    json!({
        "id": n.id,
        "parentId": n.parent_id,
        "name": n.name,
        "isDir": n.is_dir,
        "category": n.category.wire(),
        "size": n.size,
        "modified": n.modified,
        "modifiedRaw": n.timestamps.modified_raw,
        "origin": n.origin.wire(),
        "recoverability": n.recoverability.wire(),
        "path": n.path,
        "width": n.dimensions.map(|d| d.0),
        "height": n.dimensions.map(|d| d.1),
        "deletedAgo": n.timestamps
            .deleted
            .map(|t| (now_ms - t) / 86_400_000),
        "ext": n.name.rsplit('.').next().unwrap_or("").to_string(),
        "partition": n.partition,
    })
}

#[tauri::command]
pub fn get_scan_results(s: State<'_, AppState>, generation: u64, since: Option<u64>) -> Value {
    if s.scan.generation() != generation {
        return Value::Null;
    }
    let node_count = s.scan.node_count();
    if node_count == 0 {
        return Value::Null;
    }
    // The delta: entries appended after `since` (clamped — a stale
    // `since` beyond the current tree reads as empty, never panics).
    let since = since.unwrap_or(0).min(node_count as u64) as usize;
    let now_ms = unix_now_ms();
    let mut wire_nodes = Vec::with_capacity(node_count - since);
    {
        // tree + appended held TOGETHER (the canonical order): the
        // fold inside publish_delta mutates both under the same
        // pair, so this view is atomic — never the new base with an
        // un-cleared delta (duplicates), never the old base with a
        // cleared one (lost nodes).
        let base = s.scan.tree.lock();
        let appended = s.scan.appended.lock();
        let base_len = base
            .as_ref()
            .map_or(0, |b: &std::sync::Arc<Vec<TreeNode>>| b.len());
        if since < base_len {
            if let Some(b) = base.as_deref() {
                for n in &b[since..] {
                    wire_nodes.push(wire_node(n, now_ms));
                }
            }
            for n in appended.iter() {
                wire_nodes.push(wire_node(n, now_ms));
            }
        } else {
            let off = since - base_len;
            for n in appended.iter().skip(off) {
                wire_nodes.push(wire_node(n, now_ms));
            }
        }
    }
    // Cumulative categories (the chips were clobbered by slice-local
    // counts on every append — the old mid-scan bug).
    let categories: Vec<Value> = s
        .scan
        .categories
        .lock()
        .iter()
        .map(|(k, (count, size))| {
            json!({
                "category": k, "count": count, "size": size,
            })
        })
        .collect();
    let report = s.scan.report.lock().clone();
    // The Reconstruction tab's partition roll-ups: group the tree's
    // file nodes by their partition label (whole-disk scans carry
    // them; single-volume scans fold into one "Primary Partition").
    let partitions = partition_summaries(&s.scan);
    json!({
        "generation": generation,
        "targetName": *s.scan.target_name.lock(),
        // The scan target's device id — the recovery funnel passes this
        // to `list_recover_destinations` so source rows are flagged, and
        // `start_recovery` re-derives the same exclusion server-side.
        "deviceId": s.scan.target_id(),
        // The delta protocol: `since` echoes the request boundary,
        // `nodeCount` carries the full-tree size (the frontend
        // accumulates and knows when it has everything).
        "since": since,
        "nodeCount": node_count,
        "categories": categories,
        "nodes": wire_nodes,
        "partitions": partitions,
        "durationSec": report.map(|r| r.duration_sec).unwrap_or_default(),
    })
}

/// Per-partition file/byte roll-ups from the current tree (base +
/// appended, read under both locks). Nodes without a partition label
/// (single-volume scans) fold into one primary row.
fn partition_summaries(scan: &ScanSession) -> Vec<Value> {
    let mut map: std::collections::BTreeMap<String, (u64, u64)> = std::collections::BTreeMap::new();
    let mut unassigned = (0u64, 0u64);
    {
        let base = scan.tree.lock();
        let appended = scan.appended.lock();
        let mut visit = |nodes: &[TreeNode]| {
            for n in nodes.iter() {
                if n.is_dir {
                    continue;
                }
                match &n.partition {
                    Some(p) => {
                        let e = map.entry(p.clone()).or_insert((0, 0));
                        e.0 += 1;
                        e.1 += n.size;
                    }
                    None => {
                        unassigned.0 += 1;
                        unassigned.1 += n.size;
                    }
                }
            }
        };
        if let Some(b) = base.as_ref() {
            visit(b);
        }
        visit(&appended);
    }
    let mut out: Vec<Value> = Vec::with_capacity(map.len() + 1);
    for (label, (count, bytes)) in map {
        out.push(json!({
            "id": label, "label": label, "fileSystem": "",
            "sizeBytes": 0, "isLost": false,
            "fileCount": count, "bytesFound": bytes,
        }));
    }
    if out.is_empty() && unassigned.0 > 0 {
        out.push(json!({
            "id": "p0", "label": "Primary Partition", "fileSystem": "",
            "sizeBytes": 0, "isLost": false,
            "fileCount": unassigned.0, "bytesFound": unassigned.1,
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eta_tracker_never_runs_away_on_stalls() {
        let mut eta = EtaTracker::default();
        // First sample: no rate yet → the honest 60 s default.
        assert_eq!(eta.update(1.0), 60);
        // Measured movement: 1%/s for 10 s → ~99 s remaining at 1%.
        std::thread::sleep(std::time::Duration::from_millis(1050));
        let e = eta.update(2.0);
        assert!(e > 60 && e <= 120, "eta={e}");
        // A long stall (percent frozen): the estimate must not grow
        // unboundedly — the old formula would multiply elapsed×99.
        std::thread::sleep(std::time::Duration::from_millis(1050));
        let e2 = eta.update(2.0);
        assert!(e2 <= 86_400, "clamped to a day, got {e2}");
        // Completion zeroes.
        assert_eq!(eta.update(100.0), 0);
    }

    #[test]
    fn eta_tracker_clamps_to_a_day() {
        let mut eta = EtaTracker::default();
        std::thread::sleep(std::time::Duration::from_millis(1050));
        // A crawl: 0.001 %/s → the raw math says 10^8 s; the clamp
        // answers one day.
        let e = eta.update(1.001);
        assert!(e <= 86_400, "clamped, got {e}");
    }
}
