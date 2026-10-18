//! Mock-core state: the scan phase machine, license posture, sessions,
//! settings and the recovery run — the Rust twin of the TS browser
//! mock's module-level state.
use std::collections::HashSet;
use std::time::Instant;

use serde_json::json;

use super::data;

/// Licensing postures (DiskGenie licensing architecture).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LicensePosture {
    Unlicensed,
    Pro,
    #[allow(dead_code)] // The full posture set is part of the contract.
    Grace,
    #[allow(dead_code)]
    Degraded,
}

/// Scan phases (quick → deep → organizing → done; paused/stopped are
/// control states; the aggregate progress bar covers 0..38% quick,
/// 38..100% deep).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScanPhase {
    Idle,
    Preparing,
    Quick,
    Deep,
    Organizing,
    Paused,
    Done,
    Stopped,
}

impl ScanPhase {
    pub fn label(self) -> &'static str {
        match self {
            ScanPhase::Idle => "idle",
            ScanPhase::Preparing => "preparing",
            ScanPhase::Quick => "quick",
            ScanPhase::Deep => "deep",
            ScanPhase::Organizing => "organizing",
            ScanPhase::Paused => "paused",
            ScanPhase::Done => "done",
            ScanPhase::Stopped => "stopped",
        }
    }
}

/// Demo timescale (ms) — proportional to the real quick:deep ratio.
const PREPARE_MS: f64 = 1_200.0;
const QUICK_MS: f64 = 7_000.0;
const DEEP_MS: f64 = 26_000.0;
const ORGANIZE_MS: f64 = 2_400.0;
const SDR_MS: f64 = 3_600.0;

#[derive(Clone)]
pub struct RecoverRun {
    pub total_files: u64,
    pub done_files: u64,
    pub failed_files: u64,
    pub total_bytes: u64,
    pub done_bytes: u64,
    pub current_file: String,
    pub destination: String,
    pub enable_repair: bool,
    pub repaired_files: u64,
    pub finished: bool,
}

impl RecoverRun {
    fn payload(&self) -> serde_json::Value {
        json!({
            "totalFiles": self.total_files,
            "doneFiles": self.done_files,
            "failedFiles": self.failed_files,
            "totalBytes": self.total_bytes,
            "doneBytes": self.done_bytes,
            "currentFile": self.current_file,
            "destination": self.destination,
            "enableRepair": self.enable_repair,
            "repairedFiles": self.repaired_files,
            "finished": self.finished,
        })
    }
}

pub struct MockState {
    /* scan */
    pub generation: u64,
    pub phase: ScanPhase,
    pub saved_phase: ScanPhase,
    pub paused: bool,
    pub phase_started: Instant,
    pub elapsed_base: f64,
    pub percent: f64,
    pub device_id: String,
    pub device_name: String,
    pub device_raw: bool,
    pub sdr_done: bool,
    pub done_fired: bool,
    pub files_found: u64,
    pub bytes_found: u64,
    pub sectors_total: u64,
    pub results: Option<serde_json::Value>,
    /* devices */
    pub sd_connected: bool,
    pub lost_partition_discovered: bool,
    /// BitLocker volumes the user unlocked this session (FVE analog).
    pub bitlocker_unlocked: HashSet<String>,
    /* sessions */
    pub sessions: Vec<serde_json::Value>,
    /* recovery */
    pub recover: Option<RecoverRun>,
    /* licensing */
    pub license_posture: LicensePosture,
    pub license: Option<serde_json::Value>,
    /* settings */
    pub settings: serde_json::Value,
}

pub fn pro_status(lifetime: bool, simulated: bool) -> serde_json::Value {
    json!({
        "posture": "pro", "isPro": true,
        "tier": if lifetime { "lifetime" } else { "yearly" },
        "customerName": "Alex Morgan",
        "customerEmail": "alex.morgan@outlook.com",
        "licenseExpiresAt": 0,
        "graceDaysLeft": 0,
        "purchaseUrl": "https://diskwizard.app/pricing",
        "simulated": simulated,
    })
}

impl MockState {
    pub fn new() -> Self {
        Self {
            generation: 0,
            phase: ScanPhase::Idle,
            saved_phase: ScanPhase::Idle,
            paused: false,
            phase_started: Instant::now(),
            elapsed_base: 0.0,
            percent: 0.0,
            device_id: String::new(),
            device_name: String::new(),
            device_raw: false,
            sdr_done: false,
            done_fired: false,
            files_found: 0,
            bytes_found: 0,
            sectors_total: 0,
            results: None,
            sd_connected: false,
            lost_partition_discovered: false,
            bitlocker_unlocked: HashSet::new(),
            sessions: data::initial_sessions(),
            recover: None,
            license_posture: LicensePosture::Unlicensed,
            license: None,
            settings: json!({
                "resultsMode": "essential",
                "confirmStop": true,
                "enableRepairDefault": true,
                "scanOnSdInsert": false,
                "launchAtLogin": false,
            }),
        }
    }

    /// Begin a scan for `device` (a JSON with id/name/health).
    pub fn start(&mut self, device: &serde_json::Value) {
        self.generation += 1;
        self.phase = ScanPhase::Preparing;
        self.saved_phase = ScanPhase::Preparing;
        self.paused = false;
        self.phase_started = Instant::now();
        self.elapsed_base = 0.0;
        self.percent = 0.0;
        self.device_id = device["id"].as_str().unwrap_or_default().to_string();
        self.device_name = device["name"].as_str().unwrap_or("Scan").to_string();
        self.device_raw = device["health"].as_str() == Some("raw");
        self.sdr_done = false;
        self.done_fired = false;
        self.files_found = 0;
        self.bytes_found = 0;
        self.sectors_total = data::sectors_total(&self.device_id);
        self.results = None;
    }

    /// One ticker step. Returns an optional (event, payload) to emit.
    pub fn tick(&mut self) -> Option<(&'static str, serde_json::Value)> {
        // Recovery funnel runs on the same ticker.
        if let Some(r) = self.recover.as_mut() {
            if !r.finished {
                let step = (r.total_files / 90).max(1);
                r.done_files = (r.done_files + step).min(r.total_files);
                r.done_bytes = (r.total_bytes * r.done_files)
                    .checked_div(r.total_files)
                    .unwrap_or(0);
                r.current_file = if r.done_files >= r.total_files {
                    "Finalizing…".to_string()
                } else {
                    data::current_file(r.done_files)
                };
                if r.enable_repair && r.done_files % 23 == 0 {
                    r.repaired_files += 1;
                }
                r.finished = r.done_files >= r.total_files;
                if r.finished {
                    r.failed_files = if r.total_files > 900 {
                        r.total_files * 3 / 1000
                    } else {
                        0
                    };
                    let payload = r.payload();
                    self.recover = None;
                    return Some(("recover-done", payload));
                }
                let payload = r.payload();
                return Some(("recover-progress", payload));
            }
        }

        if matches!(
            self.phase,
            ScanPhase::Idle | ScanPhase::Done | ScanPhase::Stopped
        ) {
            return None;
        }
        if self.paused {
            return None;
        }

        let in_phase = self.phase_started.elapsed().as_millis() as f64;
        match self.phase {
            ScanPhase::Preparing => {
                if in_phase >= PREPARE_MS {
                    self.phase = ScanPhase::Quick;
                    self.phase_started = Instant::now();
                    self.elapsed_base = 0.0;
                }
            }
            ScanPhase::Quick => {
                let phase_ms = if self.device_raw {
                    SDR_MS + QUICK_MS
                } else {
                    QUICK_MS
                };
                let frac = (in_phase / phase_ms).min(1.0);
                self.percent = (frac * 38.0).min(38.0);
                self.sdr_done = self.device_raw && in_phase >= SDR_MS;
                apply_quick_stats(self);
                if frac >= 1.0 {
                    self.phase = ScanPhase::Deep;
                    self.phase_started = Instant::now();
                    self.elapsed_base = phase_ms / 1000.0;
                    self.results = Some(super::tree::generate(
                        &self.device_id,
                        self.generation,
                        0.38,
                    ));
                }
            }
            ScanPhase::Deep => {
                let frac = (in_phase / DEEP_MS).min(1.0);
                self.percent = 38.0 + frac * 62.0;
                let (files, bytes) = data::totals(&self.device_id);
                self.files_found = (files as f64 * (0.41 + 0.59 * frac)).round() as u64;
                self.bytes_found = (bytes as f64 * (0.36 + 0.64 * frac)).round() as u64;
                let target = ((0.38 + frac * 0.62) * 40.0).round() / 40.0;
                let current = self.current_fraction();
                if target > current + f64::EPSILON {
                    self.results = Some(super::tree::generate(
                        &self.device_id,
                        self.generation,
                        target,
                    ));
                }
                if !self.lost_partition_discovered && frac > 0.45 {
                    self.lost_partition_discovered = true;
                }
                if frac >= 1.0 {
                    self.phase = ScanPhase::Organizing;
                    self.phase_started = Instant::now();
                    self.percent = 100.0;
                }
            }
            ScanPhase::Organizing if in_phase >= ORGANIZE_MS => {
                self.phase = ScanPhase::Done;
                self.results = Some(super::tree::generate(&self.device_id, self.generation, 1.0));
                self.save_session();
                let payload = json!({
                    "generation": self.generation,
                    "targetName": self.device_name,
                    "stats": { "filesFound": self.files_found, "bytesFound": self.bytes_found },
                    "durationSec": (self.elapsed_base + in_phase / 1000.0).round() as u64,
                    // QA-8 parity: sim devices are mounted-volume analogs
                    // (the live-scan notice renders in sim tours too).
                    "sourceVolumeLive": true,
                });
                return Some(("scan-done", payload));
            }
            _ => {}
        }
        Some(("scan-progress", self.progress_payload()))
    }

    fn current_fraction(&self) -> f64 {
        self.results
            .as_ref()
            .and_then(|r| r["categories"].as_array())
            .map(|cats| {
                let head: u64 = cats.iter().filter_map(|c| c["count"].as_u64()).sum();
                let total = data::totals(&self.device_id).0.max(1);
                (head as f64 / total as f64).min(1.0)
            })
            .unwrap_or(0.0)
    }

    fn save_session(&mut self) {
        if self.done_fired {
            return;
        }
        self.done_fired = true;
        let now = time_now_secs();
        let dt = civil_now();
        let p = |n: u64| format!("{n:02}");
        let safe = self
            .device_name
            .chars()
            .map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c })
            .collect::<String>();
        // Naming parity with the TS mock: auto_<Volume>_<MM>_<DD>_<HH>_<MM>_<SS>.rsf
        let file = format!(
            "auto_{}_{}_{:02}_{:02}_{:02}_{:02}.rsf",
            safe,
            p(dt.1),
            dt.2,
            dt.3,
            dt.4,
            dt.5
        );
        let session = json!({
            "id": format!("rs-{now}"),
            "targetName": self.device_name,
            "status": "completed",
            "filesFound": self.files_found,
            "bytesFound": self.bytes_found,
            "startedAt": now.saturating_sub(40),
            "sessionFile": file,
        });
        self.sessions
            .retain(|s| s["targetName"] != json!(self.device_name));
        self.sessions.insert(0, session);
        self.sessions.truncate(6);
    }

    pub fn progress_payload(&self) -> serde_json::Value {
        json!({
            "generation": self.generation,
            "targetId": self.device_id,
            "targetName": self.device_name,
            "phase": self.phase.label(),
            "percent": (self.percent * 100.0).round() / 100.0,
            "stats": {
                "filesFound": self.files_found,
                "bytesFound": self.bytes_found,
                "sectorsRead": (self.sectors_total as f64 * self.percent / 100.0) as u64,
                "sectorsTotal": self.sectors_total,
            },
            "elapsedSec": (self.elapsed_base + self.phase_started.elapsed().as_secs_f64()).floor() as u64,
            "remainingSec": (((100.0 - self.percent) / 100.0
                * if self.phase == ScanPhase::Quick { QUICK_MS } else { DEEP_MS } / 1000.0).round() as u64).max(1),
            "searchingLabel": self.searching_label(),
        })
    }

    fn searching_label(&self) -> String {
        match self.phase {
            ScanPhase::Organizing => "Organizing found files…".into(),
            ScanPhase::Preparing => format!(
                "Reading {} — boot sector, partition table…",
                self.device_name
            ),
            _ if self.device_raw && !self.sdr_done => {
                "SD Card Rescue — restoring access to the card…".into()
            }
            _ => {
                let read = (self.sectors_total as f64 * self.percent / 100.0) as u64;
                let folder = data::FOLDER_TOUR
                    [((self.files_found / 900) as usize) % data::FOLDER_TOUR.len()];
                format!(
                    "Searching: {folder} · sector {read} / {}",
                    self.sectors_total
                )
            }
        }
    }

    pub fn sessions_json(&self) -> serde_json::Value {
        json!(self.sessions)
    }

    pub fn license_json(&self) -> serde_json::Value {
        self.license.clone().unwrap_or_else(|| {
            json!({
                "posture": "unlicensed", "isPro": false, "tier": "",
                "customerName": "", "customerEmail": "",
                "licenseExpiresAt": 0, "graceDaysLeft": 0,
                "purchaseUrl": "https://diskwizard.app/pricing",
                "simulated": false,
            })
        })
    }
}

fn apply_quick_stats(s: &mut MockState) {
    let (files, bytes) = data::totals(&s.device_id);
    let frac = if s.percent >= 38.0 {
        1.0
    } else {
        s.percent / 38.0
    };
    s.files_found = (files as f64 * 0.41 * frac).round() as u64;
    s.bytes_found = (bytes as f64 * 0.36 * frac).round() as u64;
}

fn time_now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// (year, month, day, hour, min, sec) in UTC — session file naming.
fn civil_now() -> (u64, u64, u64, u64, u64, u64) {
    let secs = time_now_secs();
    let days = secs / 86400;
    let (y, m, d) = civil_from_days(days as i64);
    let rem = secs % 86400;
    (y as u64, m, d, rem / 3600, (rem % 3600) / 60, rem % 60)
}

/// Days-since-epoch → civil date (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u64, u64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}
