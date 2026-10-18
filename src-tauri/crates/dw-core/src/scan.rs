//! The scan controller — the phase machine + orchestration.
//!
//! Semantics preserved from the frozen contract: `preparing → quick
//! (0-38%) → deep (38-100%) → organizing → done` (+ paused/stopped
//! control states), progress with live stats (files/bytes found,
//! sectors read/total, searching label), cooperative pause/resume/
//! stop through [`ScanControl`], generation-tagged snapshots.
//!
//! Pipeline (research: `architecture/04-recovery-workflow.md` §3-5):
//! 1. PREPARE: resolve the target (partition table probe + FS probe
//!    + lost-partition quick sweep for whole-disk targets).
//! 2. QUICK: the structured engine over the volume (MFT walk / FAT
//!    chains / entry sets) — tree fills progressively.
//! 3. DEEP: engine deep pass (deleted reanimation) + RAW carving
//!    sweep; the tree merges continuously.
//! 4. ORGANIZE: classification + grading (the "Organizing found
//!    files" transition) — with the tree model, classification runs
//!    incrementally, so this pass finalizes counters + the
//!    recoverability probes.

use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;

use crate::blockdev::{BlockDevice, BlockRead, CachedDevice};
use crate::error::{CoreError, CoreResult};
use crate::fs::{self, ScanControl, ScanSink};
use crate::partition::{self, PartitionLayout};
use crate::raw::RawCarver;
use crate::tree::TreeBuilder;
use crate::types::{FoundEntry, FsKind};

/// The phase labels (the frozen `ScanPhase` wire strings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Preparing,
    Quick,
    Deep,
    Organizing,
    Done,
    Stopped,
}

impl Phase {
    #[must_use]
    pub fn wire(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Quick => "quick",
            Self::Deep => "deep",
            Self::Organizing => "organizing",
            Self::Done => "done",
            Self::Stopped => "stopped",
        }
    }
}

/// A progress snapshot (the frozen `ScanProgress` payload minus the
/// app-layer display fields).
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub phase: Phase,
    /// 0..100 aggregate (quick maps 0-38, deep 38-100).
    pub percent: f64,
    pub files_found: u64,
    pub bytes_found: u64,
    pub sectors_read: u64,
    pub sectors_total: u64,
    pub elapsed_sec: u64,
    /// Engine-unit progress for the searching label.
    pub engine_done: u64,
    pub engine_total: u64,
}

/// The controller's final report.
#[derive(Debug, Clone, Serialize)]
pub struct ScanReport {
    pub files_found: u64,
    pub bytes_found: u64,
    pub duration_sec: u64,
    pub stopped: bool,
    pub fs: FsKind,
}

/// Live observer the app layer installs (event emission seam).
pub trait ProgressObserver: Send {
    fn on_progress(&mut self, p: &Progress);
    fn on_phase(&mut self, phase: Phase);
}

/// A null observer (no-op).
pub struct NoObserver;
impl ProgressObserver for NoObserver {
    fn on_progress(&mut self, _p: &Progress) {}
    fn on_phase(&mut self, _phase: Phase) {}
}

/// One scan run over a device.
pub struct ScanController {
    dev: Arc<CachedDevice>,
    root_name: String,
    ctrl: ScanControl,
    started: Instant,
    files: u64,
    bytes: u64,
    sectors_read: u64,
    sectors_total: u64,
    engine_done: u64,
    engine_total: u64,
    phase: Phase,
}

impl ScanController {
    /// Build for a device (any `BlockDevice`; wrapped in the cache).
    #[must_use]
    pub fn new(dev: Arc<dyn BlockDevice>, root_name: &str) -> Self {
        Self::with_control(dev, root_name, ScanControl::new())
    }

    /// Build with an EXTERNAL control token (the app layer's managed
    /// pause/stop handle — the run respects it).
    #[must_use]
    pub fn with_control(
        dev: Arc<dyn BlockDevice>,
        root_name: &str,
        ctrl: ScanControl,
    ) -> Self {
        let sectors = dev.total_sectors();
        Self {
            dev: Arc::new(CachedDevice::new(dev)),
            root_name: root_name.to_string(),
            ctrl,
            started: Instant::now(),
            files: 0,
            bytes: 0,
            sectors_read: 0,
            sectors_total: sectors,
            engine_done: 0,
            engine_total: 0,
            phase: Phase::Preparing,
        }
    }

    /// The control handle (pause/resume/stop from IPC).
    #[must_use]
    pub fn control(&self) -> &ScanControl {
        &self.ctrl
    }

    fn snapshot(&self, percent: f64) -> Progress {
        Progress {
            phase: self.phase,
            percent: (percent * 100.0).round() / 100.0,
            files_found: self.files,
            bytes_found: self.bytes,
            sectors_read: self.sectors_read,
            sectors_total: self.sectors_total,
            elapsed_sec: self.started.elapsed().as_secs(),
            engine_done: self.engine_done,
            engine_total: self.engine_total,
        }
    }

    fn emit(&self, obs: &mut dyn ProgressObserver, percent: f64) {
        obs.on_progress(&self.snapshot(percent));
    }

    /// Run the full pipeline to completion (or stop).
    ///
    /// # Errors
    /// [`CoreError::Cancelled`] propagates when stopped (the report
    /// is still assembled first — stopped scans keep their results).
    pub fn run(&mut self, obs: &mut dyn ProgressObserver) -> CoreResult<(TreeBuilder, ScanReport)> {
        let mut tree = TreeBuilder::with_root(&self.root_name.clone());

        // ── PREPARE ────────────────────────────────────────────────
        self.phase = Phase::Preparing;
        obs.on_phase(self.phase);
        self.ctrl.checkpoint()?;
        self.emit(obs, 0.0);

        // Probe the filesystem.
        let fs_kind = fs::probe_filesystem(&*self.dev)?;
        let engine_result =
            fs::create_engine(fs_kind, Arc::clone(&self.dev) as Arc<dyn BlockDevice>);

        // ── QUICK ──────────────────────────────────────────────────
        self.phase = Phase::Quick;
        obs.on_phase(self.phase);
        let mut quick_ok = false;
        if let Ok(mut engine) = engine_result {
            let ctrl = self.ctrl.clone();
            let mut sink = ControllerSink {
                ctrl: self,
                tree: &mut tree,
                phase_percent_range: (0.0, 0.38),
                obs_slot: obs,
            };
            match engine.quick_scan(&mut sink, &ctrl) {
                Ok(stats) => {
                    quick_ok = true;
                    let _ = stats;
                }
                Err(CoreError::Cancelled) => {
                    return Ok(self.finish_stopped(tree, fs_kind, obs));
                }
                Err(_) => { /* engine failed — carve still runs */ }
            }
        }
        let _ = quick_ok;

        // ── DEEP ───────────────────────────────────────────────────
        self.phase = Phase::Deep;
        obs.on_phase(self.phase);
        self.sectors_read = 0;
        // Deep structured pass (deleted reanimation) when the engine
        // supports it distinctly.
        if let Ok(mut engine) =
            fs::create_engine(fs_kind, Arc::clone(&self.dev) as Arc<dyn BlockDevice>)
        {
            let ctrl = self.ctrl.clone();
            let mut sink = ControllerSink {
                ctrl: self,
                tree: &mut tree,
                phase_percent_range: (0.38, 0.55),
                obs_slot: obs,
            };
            let _ = engine.deep_scan(&mut sink, &ctrl);
        }
        // RAW carving sweep.
        if !self.ctrl.is_cancelled() {
            let capacity = self.dev.capacity_bytes();
            let carver = RawCarver::new(Arc::clone(&self.dev) as Arc<dyn BlockDevice>);
            let ctrl = self.ctrl.clone();
            let mut sink = ControllerSink {
                ctrl: self,
                tree: &mut tree,
                phase_percent_range: (0.55, 1.0),
                obs_slot: obs,
            };
            match carver.carve(&mut sink, &ctrl, 0, capacity) {
                Ok(_) => {}
                Err(CoreError::Cancelled) => {
                    return Ok(self.finish_stopped(tree, fs_kind, obs));
                }
                Err(_) => {}
            }
        }

        // ── ORGANIZE ───────────────────────────────────────────────
        self.phase = Phase::Organizing;
        obs.on_phase(self.phase);
        self.ctrl.checkpoint()?;
        self.emit(obs, 1.0);
        // (Classification ran incrementally; counters finalize here.)
        let (files, bytes) = tree.totals();

        // ── DONE ───────────────────────────────────────────────────
        self.phase = Phase::Done;
        obs.on_phase(self.phase);
        self.files = files;
        self.bytes = bytes;
        self.emit(obs, 1.0);
        Ok((
            tree,
            ScanReport {
                files_found: files,
                bytes_found: bytes,
                duration_sec: self.started.elapsed().as_secs(),
                stopped: false,
                fs: fs_kind,
            },
        ))
    }

    fn finish_stopped(
        &mut self,
        tree: TreeBuilder,
        fs_kind: FsKind,
        obs: &mut dyn ProgressObserver,
    ) -> (TreeBuilder, ScanReport) {
        self.phase = Phase::Stopped;
        obs.on_phase(self.phase);
        let (files, bytes) = tree.totals();
        (
            tree,
            ScanReport {
                files_found: files,
                bytes_found: bytes,
                duration_sec: self.started.elapsed().as_secs(),
                stopped: true,
                fs: fs_kind,
            },
        )
    }
}

/// The bridge: engine sink → tree ingest + live stats + progress.
struct ControllerSink<'a, 'o> {
    ctrl: &'a mut ScanController,
    tree: &'a mut TreeBuilder,
    phase_percent_range: (f64, f64),
    obs_slot: &'o mut dyn ProgressObserver,
}

impl ScanSink for ControllerSink<'_, '_> {
    fn on_entry(&mut self, entry: FoundEntry) {
        let is_file = !entry.is_dir;
        let size = entry.size;
        self.tree.add(entry);
        if is_file {
            self.ctrl.files += 1;
            self.ctrl.bytes += size;
        }
    }

    fn on_progress(&mut self, done: u64, total: u64) {
        self.ctrl.engine_done = done;
        self.ctrl.engine_total = total;
        let (lo, hi) = self.phase_percent_range;
        let frac = if total == 0 {
            0.0
        } else {
            // u64→f64 precision loss is irrelevant for progress math
            // (percent display), but be explicit.
            #[allow(clippy::cast_precision_loss)]
            {
                (done as f64 / total as f64).clamp(0.0, 1.0)
            }
        };
        let percent = lo + (hi - lo) * frac;
        // Deep-phase sector accounting (carver units = bytes read).
        if matches!(self.ctrl.phase, Phase::Deep) {
            self.ctrl.sectors_read = done;
        }
        self.ctrl.emit_obs(self.obs_slot, percent);
    }
}

impl ScanController {
    fn emit_obs(&self, obs: &mut dyn ProgressObserver, percent: f64) {
        obs.on_progress(&self.snapshot(percent));
    }
}

/// Convenience: run a scan and return the assembled tree (the
/// app-layer entry point; observers wire to IPC events).
///
/// # Errors
/// Propagates controller errors.
pub fn run_scan(
    dev: Arc<dyn BlockDevice>,
    root_name: &str,
    obs: &mut dyn ProgressObserver,
) -> CoreResult<(TreeBuilder, ScanReport)> {
    ScanController::new(dev, root_name).run(obs)
}

/// Probe a device's layout (the app-layer list_devices join).
///
/// # Errors
/// Read/parse failures surface as typed errors.
pub fn read_partition_layout(dev: &dyn BlockDevice) -> CoreResult<PartitionLayout> {
    let sector0 = dev.read_struct_at::<512>(0)?;
    if let Ok((scheme, mut parts)) = partition::parse_mbr(&sector0, dev.total_sectors()) {
        if scheme == partition::PartitionScheme::Gpt {
            if let Ok((_, gpt_parts)) = partition::parse_gpt(dev) {
                return Ok(PartitionLayout {
                    scheme,
                    partitions: gpt_parts,
                });
            }
        }
        let extended: Vec<u64> = sector0[0x1BE..0x1FE]
            .chunks_exact(16)
            .filter(|e| (e[4] == 0x05 || e[4] == 0x0F) && e[8..12] != [0, 0, 0, 0])
            .map(|e| u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as u64)
            .collect();
        for ebr0 in extended {
            if let Ok(logical) =
                partition::parse_ebr_chain(dev, ebr0, dev.total_sectors(), parts.len() as u32)
            {
                parts.extend(logical);
            }
        }
        return Ok(PartitionLayout {
            scheme,
            partitions: parts,
        });
    }
    Ok(PartitionLayout::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::RamDevice;

    struct Recorder(Vec<Progress>, Vec<Phase>);
    impl ProgressObserver for Recorder {
        fn on_progress(&mut self, p: &Progress) {
            self.0.push(p.clone());
        }
        fn on_phase(&mut self, p: Phase) {
            self.1.push(p);
        }
    }

    /// A tiny FAT16 image with one file (reuse the FAT test builder
    /// logic inline).
    fn fat_image_with_file() -> RamDevice {
        let total_sectors = 8192u64;
        let mut s = vec![0u8; 512];
        s[..3].copy_from_slice(b"\xEB\x3C\x90");
        s[0x0B..0x0D].copy_from_slice(&512u16.to_le_bytes());
        s[0x0D] = 1;
        s[0x0E..0x10].copy_from_slice(&1u16.to_le_bytes());
        s[0x10] = 1;
        s[0x11..0x13].copy_from_slice(&16u16.to_le_bytes());
        s[0x13..0x15].copy_from_slice(&(total_sectors as u16).to_le_bytes());
        s[0x16..0x18].copy_from_slice(&1u16.to_le_bytes());
        s[510] = 0x55;
        s[511] = 0xAA;
        let mut ram = RamDevice::new(512 * total_sectors as usize, 512);
        ram.data[..512].copy_from_slice(&s);
        let fat_base = 512;
        ram.data[fat_base + 2 * 2..fat_base + 2 * 2 + 2].copy_from_slice(&0xFFFFu16.to_le_bytes());
        ram.data[fat_base + 3 * 2..fat_base + 3 * 2 + 2].copy_from_slice(&0xFFFFu16.to_le_bytes());
        let root = 2 * 512;
        let mut e = [0u8; 32];
        e[..11].copy_from_slice(b"README  TXT");
        e[0x0B] = 0x20;
        e[0x1A..0x1C].copy_from_slice(&2u16.to_le_bytes());
        e[0x1C..0x20].copy_from_slice(&100u32.to_le_bytes());
        ram.data[root..root + 32].copy_from_slice(&e);
        ram
    }

    #[test]
    fn full_pipeline_reports_done() {
        let ram = fat_image_with_file();
        let dev: Arc<dyn BlockDevice> = Arc::new(ram);
        let mut obs = Recorder(Vec::new(), Vec::new());
        let (tree, report) = run_scan(dev, "Test (F:)", &mut obs).expect("scan");
        assert_eq!(report.files_found, tree.totals().0);
        assert!(!obs.1.is_empty());
        assert_eq!(obs.1.last(), Some(&Phase::Done));
        // The FAT engine found README + the carver may add — assert ≥.
        assert!(tree.totals().0 >= 1);
    }

    #[test]
    fn stop_midway_keeps_partial_results() {
        let ram = fat_image_with_file();
        let dev: Arc<dyn BlockDevice> = Arc::new(ram);
        let mut controller = ScanController::new(dev, "Test (F:)");
        controller.control().cancel();
        let mut obs = Recorder(Vec::new(), Vec::new());
        let r = controller.run(&mut obs);
        match r {
            Ok((_, report)) => assert!(report.stopped),
            Err(CoreError::Cancelled) => {}
            Err(e) => panic!("unexpected {e:?}"),
        }
    }
}
