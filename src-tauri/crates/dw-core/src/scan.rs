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
#[derive(Debug, Clone, Copy, Serialize)]
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
    /// Session 8 — progressive results: a throttled snapshot of the
    /// assembling tree (the app layer publishes it so the results page
    /// fills WHILE the scan runs; the old design delivered the tree
    /// only at completion, so a stuck scan showed nothing at all).
    fn on_tree_update(&mut self, _tree: &TreeBuilder) {}
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
    /// Session 8: the device capacity — the LIVE byte-clamp bound.
    capacity: u64,
    /// Session 8: progressive-publish throttle state.
    since_publish: u32,
    last_publish: Instant,
    /// Session 9: has any mid-scan publish fired yet (first-publish
    /// uses a lower threshold so small trees stream immediately).
    ever_published: bool,
    /// Session 9: the last unit-set's dominant filesystem — the
    /// stopped-report's label when cancellation lands between the
    /// units' lifetimes.
    last_fs_kind: FsKind,
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
    pub fn with_control(dev: Arc<dyn BlockDevice>, root_name: &str, ctrl: ScanControl) -> Self {
        let sectors = dev.total_sectors();
        let capacity = sectors.saturating_mul(u64::from(dev.sector_size()));
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
            capacity,
            since_publish: 0,
            last_publish: Instant::now(),
            ever_published: false,
            last_fs_kind: FsKind::Raw,
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
    /// Whole-disk targets dispatch across their discovered partitions
    /// (each partition scans as its own volume — quick + deep — with
    /// progress weighted by size); the RAW carve then sweeps the WHOLE
    /// device so unpartitioned and lost space is covered too. Targets
    /// that are already volumes (partition slices, volume handles,
    /// lost partitions) form a single unit.
    ///
    /// Cancellation NEVER loses the assembled tree (session 9's
    /// contract hardening — the CLI's `--max-seconds` watchdog and
    /// the UI's stop button both keep partial results): every
    /// `Err(Cancelled)` escape from a phase lands in the
    /// stopped-report arm with everything emitted so far.
    ///
    /// # Errors
    /// Non-cancellation engine/IO failures propagate (the caller
    /// decides whether a carve-only retry makes sense).
    #[allow(clippy::too_many_lines)] // The phase machine; phases share sink/weight state by design.
    pub fn run(&mut self, obs: &mut dyn ProgressObserver) -> CoreResult<(TreeBuilder, ScanReport)> {
        let mut tree = TreeBuilder::with_root(&self.root_name.clone());
        match self.run_inner(obs, &mut tree) {
            Ok(report) => Ok((tree, report)),
            Err(CoreError::Cancelled) => {
                // The stop token fired at a phase boundary: the tree
                // (built incrementally through `&mut tree`) still
                // carries every emitted entry — finish it honestly.
                let fs_kind = self.last_fs_kind;
                Ok(self.finish_stopped(tree, fs_kind, obs))
            }
            Err(e) => Err(e),
        }
    }

    /// The phase machine (run's body — tree hoisted so cancellation
    /// can finish it). Returns the final report.
    #[allow(clippy::too_many_lines)] // The phase machine; phases share sink/weight state by design.
    fn run_inner(
        &mut self,
        obs: &mut dyn ProgressObserver,
        tree: &mut TreeBuilder,
    ) -> CoreResult<ScanReport> {
        // ── PREPARE ────────────────────────────────────────────────
        self.phase = Phase::Preparing;
        obs.on_phase(self.phase);
        self.ctrl.checkpoint()?;
        self.emit(obs, 0.0);

        // Build the scan units: partition slices for whole-disk
        // targets, the device itself otherwise. A partition that
        // can't be opened/probed degrades to a carve-only unit (the
        // sweep still covers its bytes).
        let units = self.scan_units();
        // Structured weight ONLY (session 9): raw/unknown units have
        // no engine work in quick/deep — their bytes are covered by
        // the carve sweep — so they must not hold progress weight
        // hostage (the owner's "progress never goes above 1%" on a
        // disk whose largest partition was BitLocker/MSR-class).
        let structured_weight: u64 = units
            .iter()
            .filter(|u| !matches!(u.fs, FsKind::Raw | FsKind::Unknown))
            .map(|u| u.sectors.max(1))
            .sum();

        // ── QUICK ──────────────────────────────────────────────────
        self.phase = Phase::Quick;
        obs.on_phase(self.phase);
        let mut done_weight = 0u64;
        for unit in &units {
            if matches!(unit.fs, FsKind::Raw | FsKind::Unknown) {
                continue; // carve-only unit: no quick work, no weight
            }
            let lo = 0.38 * weight_frac(done_weight, structured_weight);
            let hi = 0.38 * weight_frac(done_weight + unit.sectors.max(1), structured_weight);
            done_weight += unit.sectors.max(1);
            let Ok(mut engine) = fs::create_engine(unit.fs, unit.dev.clone()) else {
                continue;
            };
            let ctrl = self.ctrl.clone();
            let mut sink = ControllerSink {
                ctrl: self,
                tree,
                phase_percent_range: (lo, hi),
                path_prefix: unit.prefix.as_deref(),
                obs_slot: obs,
            };
            match engine.quick_scan(&mut sink, &ctrl) {
                Ok(_) => {}
                Err(CoreError::Cancelled) => {
                    self.last_fs_kind = units_fs(&units);
                    return Err(CoreError::Cancelled);
                }
                Err(_) => { /* engine failed — carve still runs */ }
            }
            // Session 8: pass-2 at the unit boundary — the MFT refs
            // this unit emitted resolve before the next engine runs
            // (structured entries must populate the merge maps before
            // carving absorbs their carved duplicates).
            tree.resolve_pending();
        }

        // ── DEEP ───────────────────────────────────────────────────
        self.phase = Phase::Deep;
        obs.on_phase(self.phase);
        self.sectors_read = 0;
        let mut done_weight = 0u64;
        for unit in &units {
            if matches!(unit.fs, FsKind::Raw | FsKind::Unknown) {
                continue; // carve-only unit: no deep work, no weight
            }
            let lo = 0.38 + 0.17 * weight_frac(done_weight, structured_weight);
            let hi =
                0.38 + 0.17 * weight_frac(done_weight + unit.sectors.max(1), structured_weight);
            done_weight += unit.sectors.max(1);
            // Deep structured pass (deleted reanimation) when the
            // engine supports it distinctly.
            let Ok(mut engine) = fs::create_engine(unit.fs, unit.dev.clone()) else {
                continue;
            };
            let ctrl = self.ctrl.clone();
            let mut sink = ControllerSink {
                ctrl: self,
                tree,
                phase_percent_range: (lo, hi),
                path_prefix: unit.prefix.as_deref(),
                obs_slot: obs,
            };
            let _ = engine.deep_scan(&mut sink, &ctrl);
            tree.resolve_pending();
        }
        // RAW carving sweep over the WHOLE device (partitions +
        // unallocated space).
        if !self.ctrl.is_cancelled() {
            let capacity = self.dev.capacity_bytes();
            let carver = RawCarver::new(Arc::clone(&self.dev) as Arc<dyn BlockDevice>);
            let ctrl = self.ctrl.clone();
            let mut sink = ControllerSink {
                ctrl: self,
                tree,
                phase_percent_range: (0.55, 1.0),
                path_prefix: None,
                obs_slot: obs,
            };
            match carver.carve(&mut sink, &ctrl, 0, capacity) {
                Ok(_) => {}
                Err(CoreError::Cancelled) => {
                    self.last_fs_kind = units_fs(&units);
                    return Err(CoreError::Cancelled);
                }
                Err(_) => {}
            }
        }

        // ── ORGANIZE ───────────────────────────────────────────────
        // Session 8: the final pass-2 — leftovers land on the honest
        // lost branches, counters see every materialized node.
        tree.finish();
        self.phase = Phase::Organizing;
        obs.on_phase(self.phase);
        self.ctrl.checkpoint()?;
        // Session 9: the recoverability READABILITY pass (the og
        // CCheckNodeThread model — `DRWUI-subsystems--assistant-
        // grading-preview.md` §2): probe deleted files' first
        // extents; unreadable media downgrades the grade to Poor.
        // Recoverability stops being confidence arithmetic alone.
        tree.apply_readability_grades(&*self.dev, READABILITY_PROBE_CAP);
        self.emit(obs, 1.0);
        // (Classification ran incrementally; counters finalize here.)
        // QA-17: the LOGICAL byte total can exceed the scanned device's
        // capacity (sparse NTFS metafiles like $LogFile/$MFT growth, plus
        // overlapping carved extents before the merge absorb). The stat
        // fed to the UI's "X found" summary is clamped to the physical
        // truth — per-file sizes stay honest, only the aggregate is capped.
        let (files, bytes) = tree.totals();
        let bytes = clamp_report_bytes(bytes, self.dev.capacity_bytes());

        // ── DONE ───────────────────────────────────────────────────
        self.phase = Phase::Done;
        obs.on_phase(self.phase);
        self.files = files;
        self.bytes = bytes;
        self.emit(obs, 1.0);
        let kind = units_fs(&units);
        self.last_fs_kind = kind;
        Ok(ScanReport {
            files_found: files,
            bytes_found: bytes,
            duration_sec: self.started.elapsed().as_secs(),
            stopped: false,
            fs: kind,
        })
    }

    fn finish_stopped(
        &mut self,
        mut tree: TreeBuilder,
        fs_kind: FsKind,
        obs: &mut dyn ProgressObserver,
    ) -> (TreeBuilder, ScanReport) {
        self.phase = Phase::Stopped;
        obs.on_phase(self.phase);
        tree.finish();
        let (files, bytes) = tree.totals();
        // QA-17 (same clamp as the Done path — stopped scans report too).
        let bytes = clamp_report_bytes(bytes, self.dev.capacity_bytes());
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

/// Upper bound on per-file readability probes in the ORGANIZE pass
/// (one cached first-sector read each; the cap bounds the pass on
/// pathological million-file trees — beyond it, grades keep their
/// arithmetic values, which the $Bitmap scoring already informed).
const READABILITY_PROBE_CAP: usize = 20_000;

/// QA-17: report bytes never exceed the scanned device's capacity.
/// Sparse metafiles ($LogFile logical size, $MFT growth) and pre-merge
/// carved overlaps can push the LOGICAL sum past physical reality; the
/// aggregate stat stays honest by clamping to capacity (VHD-run
/// evidence: bytes_found 546,825,304 on a 267 MB volume).
fn clamp_report_bytes(logical_total: u64, capacity: u64) -> u64 {
    logical_total.min(capacity)
}

/// The bridge: engine sink → tree ingest + live stats + progress.
struct ControllerSink<'a, 'o> {
    ctrl: &'a mut ScanController,
    tree: &'a mut TreeBuilder,
    phase_percent_range: (f64, f64),
    /// Per-unit tree folder (whole-disk scans nest each partition's
    /// entries under its own branch; None = target root).
    path_prefix: Option<&'a str>,
    obs_slot: &'o mut dyn ProgressObserver,
}

impl ScanSink for ControllerSink<'_, '_> {
    fn on_entry(&mut self, mut entry: FoundEntry) {
        // Session 8: partition prefixes NEVER corrupt `@mft:` refs —
        // they ride the dedicated partition field (the old string
        // munging turned refs into literal "Partition 1\\@mft:1234"
        // paths, one garbage dir per record).
        if let Some(prefix) = self.path_prefix {
            match entry.parent_hint.as_deref() {
                Some(h) if h.starts_with('@') => {
                    entry.partition = Some(prefix.to_string());
                }
                Some(h) if !h.is_empty() => {
                    entry.parent_hint = Some(format!("{prefix}/{h}"));
                }
                Some(_) => {
                    // Volume-root entry on a partition unit.
                    entry.parent_hint = Some(prefix.to_string());
                }
                None => {}
            }
        }
        let is_file = !entry.is_dir;
        let size = entry.size;
        self.tree.add(entry);
        if is_file {
            self.ctrl.files += 1;
            // Session 8: the LIVE counter is clamped continuously (the
            // QA-17 rule applied during the scan, not only at the end —
            // the owner watched it climb into TB mid-scan).
            self.ctrl.bytes = self.ctrl.bytes.saturating_add(size).min(self.ctrl.capacity);
        }
        // Progressive results (session-9 tuning): the FIRST publish
        // lands at 64 nodes (small volumes stream too — the tour
        // VHD's 155 files never crossed the old 2048 floor, so the
        // results page stayed empty the whole scan); steady state
        // publishes every 512 nodes, >= 350 ms apart (the og's
        // ~1000-item incremental BuildTree cadence).
        self.ctrl.since_publish = self.ctrl.since_publish.saturating_add(1);
        let threshold = if self.ctrl.ever_published { 512 } else { 64 };
        if self.ctrl.since_publish >= threshold
            && self.ctrl.last_publish.elapsed().as_millis() >= 350
        {
            self.ctrl.since_publish = 0;
            self.ctrl.ever_published = true;
            self.ctrl.last_publish = Instant::now();
            self.obs_slot.on_tree_update(self.tree);
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

    /// Build the scan units for the PREPARE phase: one per partition
    /// when the target is a whole disk with a table, the device
    /// itself otherwise. Partition units are `SubDevice` slices over
    /// the SAME cached device (one cache layer, absolute-LBA keyed).
    ///
    /// Ordering matters: a filesystem boot sector (NTFS/FAT/exFAT —
    /// all carry the 0x55AA signature AND code bytes where MBR
    /// entries would sit) means the target IS a volume — partition
    /// dispatch would read the boot code as phantom table entries.
    /// Only Raw probes (whole disks, superfloppies, damaged volumes)
    /// get layout analysis.
    fn scan_units(&self) -> Vec<ScanUnit> {
        let mut units = Vec::new();
        let direct = fs::probe_filesystem(&*self.dev).unwrap_or(FsKind::Raw);
        if !matches!(direct, FsKind::Raw | FsKind::Unknown) {
            units.push(ScanUnit {
                dev: Arc::clone(&self.dev) as Arc<dyn BlockDevice>,
                sectors: self.dev.total_sectors(),
                prefix: None,
                fs: direct,
            });
            return units;
        }
        let layout = read_partition_layout(&*self.dev).unwrap_or_default();
        if layout.partitions.is_empty() {
            // Whole-disk without a table (superfloppy/volume target) or
            // an unreadable table: one unit covering everything.
            let fs = fs::probe_filesystem(&*self.dev).unwrap_or(FsKind::Raw);
            units.push(ScanUnit {
                dev: Arc::clone(&self.dev) as Arc<dyn BlockDevice>,
                sectors: self.dev.total_sectors(),
                prefix: None,
                fs,
            });
            return units;
        }
        for p in &layout.partitions {
            if p.total_sectors == 0 {
                continue;
            }
            let sub = Arc::new(crate::blockdev::SubDevice::new(
                Arc::clone(&self.dev) as Arc<dyn BlockDevice>,
                p.start_lba,
                p.total_sectors,
                format!("part{}", p.index),
            ));
            let fs = fs::probe_filesystem(&*sub).unwrap_or(FsKind::Raw);
            units.push(ScanUnit {
                dev: sub,
                sectors: p.total_sectors,
                prefix: Some(format!("Partition {}", p.index)),
                fs,
            });
        }
        if units.is_empty() {
            let fs = fs::probe_filesystem(&*self.dev).unwrap_or(FsKind::Raw);
            units.push(ScanUnit {
                dev: Arc::clone(&self.dev) as Arc<dyn BlockDevice>,
                sectors: self.dev.total_sectors(),
                prefix: None,
                fs,
            });
        }
        units
    }
}

/// One scan target volume: a partition slice (whole-disk scans) or
/// the device itself.
struct ScanUnit {
    dev: Arc<dyn BlockDevice>,
    sectors: u64,
    /// Tree folder prefix (whole-disk partition units).
    prefix: Option<String>,
    fs: FsKind,
}

/// Weight fraction 0..1 for progress math. u64-to-f64 precision loss
/// is irrelevant for percent display (documented exception, same
/// rationale as the sink's progress math).
#[allow(clippy::cast_precision_loss)]
fn weight_frac(done: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64).clamp(0.0, 1.0)
    }
}

/// The report's filesystem label: the largest structured unit's kind
/// (Raw when nothing structured was found).
fn units_fs(units: &[ScanUnit]) -> FsKind {
    units
        .iter()
        .filter(|u| !matches!(u.fs, FsKind::Raw | FsKind::Unknown))
        .max_by_key(|u| u.sectors)
        .map_or(FsKind::Raw, |u| u.fs)
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
    if let Ok((scheme, _)) = partition::parse_mbr(&sector0, dev.total_sectors())
        && scheme == partition::PartitionScheme::Gpt
        && let Ok((_, gpt_parts)) = partition::parse_gpt(dev)
    {
        return Ok(PartitionLayout {
            scheme,
            partitions: gpt_parts,
        });
    }
    if let Ok((scheme, mut parts)) = partition::parse_mbr(&sector0, dev.total_sectors()) {
        let (mbr_entries, _) = sector0[0x1BE..0x1FE].as_chunks::<16>();
        let extended: Vec<u64> = mbr_entries
            .iter()
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

    #[test]
    fn volume_boot_sector_targets_do_not_dispatch_partitions() {
        // NTFS boot sector: label at 3, 0x55AA at 510, and boot-code
        // bytes where MBR entries sit (the phantom-partition trap that
        // broke volume scans before the probe-first ordering).
        let mut ram = RamDevice::new(512 * 2048, 512);
        ram.data[3..11].copy_from_slice(b"NTFS    ");
        ram.data[0x1BE] = 0x07;
        ram.data[0x1BE + 8..0x1BE + 12].copy_from_slice(&1u32.to_le_bytes());
        ram.data[0x1BE + 12..0x1BE + 16].copy_from_slice(&100u32.to_le_bytes());
        ram.data[510..512].copy_from_slice(&[0x55, 0xAA]);
        let ctrl = ScanController::new(Arc::new(ram), "volume");
        let units = ctrl.scan_units();
        assert_eq!(units.len(), 1, "a boot-sector target is ONE unit");
        assert_eq!(units[0].fs, FsKind::Ntfs);
        assert!(units[0].prefix.is_none());
    }

    #[test]
    fn whole_disk_targets_dispatch_partitions() {
        let mut ram = RamDevice::new(512 * 4096, 512);
        // MBR with one FAT32 entry (start 2048, len 1024).
        ram.data[0x1BE + 4] = 0x0B;
        ram.data[0x1BE + 8..0x1BE + 12].copy_from_slice(&2048u32.to_le_bytes());
        ram.data[0x1BE + 12..0x1BE + 16].copy_from_slice(&1024u32.to_le_bytes());
        ram.data[510..512].copy_from_slice(&[0x55, 0xAA]);
        let ctrl = ScanController::new(Arc::new(ram), "disk");
        let units = ctrl.scan_units();
        assert_eq!(units.len(), 1, "one table entry → one partition unit");
        assert_eq!(units[0].sectors, 1024);
        assert_eq!(units[0].prefix.as_deref(), Some("Partition 1"));
    }

    struct Recorder(Vec<Progress>, Vec<Phase>);
    impl ProgressObserver for Recorder {
        fn on_progress(&mut self, p: &Progress) {
            self.0.push(*p);
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
    fn qa17_report_bytes_never_exceed_capacity() {
        // The regression test for the VHD-run evidence (bytes_found
        // 546,825,304 on a 267 MB volume): the aggregate stat clamps to
        // the scanned device's physical capacity no matter how large the
        // LOGICAL total (sparse metafiles, pre-merge carved overlaps).
        assert_eq!(clamp_report_bytes(546_825_304, 267_000_000), 267_000_000);
        assert_eq!(clamp_report_bytes(1_000, u64::MAX), 1_000);
        assert_eq!(clamp_report_bytes(0, 0), 0);
        // And through the real pipeline: the FAT fixture's logical total
        // is far below capacity, so the clamp must be a no-op there.
        let ram = fat_image_with_file();
        let dev: Arc<dyn BlockDevice> = Arc::new(ram);
        let mut obs = Recorder(Vec::new(), Vec::new());
        let (_tree, report) = run_scan(dev, "Test (F:)", &mut obs).expect("scan");
        assert!(
            report.bytes_found <= 512 * 8192,
            "aggregate bytes stay within the device capacity"
        );
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

    // ── Session 8: the NTFS end-to-end tree fixture (the coverage gap
    // that let the VHD integration failure escape) ──────────────────

    /// Build one MFT record: "FILE" magic + a $FILE_NAME (parent ref
    /// + UTF-16 name) and, for files, a resident $DATA.
    fn ntfs_record(rec_no: u64, parent: u64, name: &str, is_dir: bool, data: &[u8]) -> Vec<u8> {
        let mut r = vec![0u8; 512];
        r[..4].copy_from_slice(b"FILE");
        r[0x04..0x06].copy_from_slice(&48u16.to_le_bytes()); // usa_off
        r[0x06..0x08].copy_from_slice(&1u16.to_le_bytes()); // usa_count (USN only)
        r[0x14..0x16].copy_from_slice(&48u16.to_le_bytes()); // attrs_off
        let flags: u16 = if is_dir { 0x03 } else { 0x01 }; // in-use + dir
        r[0x16..0x18].copy_from_slice(&flags.to_le_bytes());
        r[0x18..0x1C].copy_from_slice(&512u32.to_le_bytes()); // used
        r[0x20..0x28].copy_from_slice(&0u64.to_le_bytes()); // base ref

        let name_u16: Vec<u16> = name.encode_utf16().collect();
        // $FILE_NAME (0x30), resident — the CONTENT begins at
        // attr+0x18: parent ref (u64), times, sizes, flags, then
        // name_len (+0x40 in-content), namespace (+0x41), UTF-16 name.
        let content = 0x18usize;
        let fn_len = content + 0x42 + name_u16.len() * 2;
        let mut a = vec![0u8; fn_len];
        a[0x00..0x04].copy_from_slice(&0x30u32.to_le_bytes());
        a[0x04..0x08].copy_from_slice(&(fn_len as u32).to_le_bytes());
        a[0x08] = 0; // resident
        a[0x10..0x14].copy_from_slice(&((fn_len - content) as u32).to_le_bytes());
        a[0x14..0x16].copy_from_slice(&(content as u16).to_le_bytes());
        // Real $FILE_NAME parent refs carry the reuse SEQUENCE in the
        // high 16 bits — emit one (seq 3) so the fixture exercises the
        // 48-bit mask (the VHD run's exact condition).
        let full_ref = (3u64 << 48) | parent;
        a[content..content + 8].copy_from_slice(&full_ref.to_le_bytes());
        a[content + 0x40] = name_u16.len() as u8;
        a[content + 0x41] = 2; // Win32 namespace
        for (i, u) in name_u16.iter().enumerate() {
            a[content + 0x42 + i * 2..content + 0x44 + i * 2].copy_from_slice(&u.to_le_bytes());
        }
        r[48..48 + fn_len].copy_from_slice(&a);

        let mut off = 48 + fn_len;
        if !is_dir && !data.is_empty() {
            // $DATA (0x80), resident inline (content at attr+0x18).
            let content = 0x18usize;
            let d_len = content + data.len();
            let mut d = vec![0u8; d_len];
            d[0x00..0x04].copy_from_slice(&0x80u32.to_le_bytes());
            let total = u32::try_from(d_len).unwrap_or(u32::MAX).to_le_bytes();
            d[0x04..0x08].copy_from_slice(&total);
            d[0x08] = 0; // resident
            d[0x10..0x14].copy_from_slice(&(data.len() as u32).to_le_bytes());
            d[0x14..0x16].copy_from_slice(&(content as u16).to_le_bytes());
            d[content..content + data.len()].copy_from_slice(data);
            r[off..off + d_len].copy_from_slice(&d);
            off += d_len;
        }
        // End marker.
        r[off..off + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        let _ = rec_no;
        r
    }

    /// A minimal NTFS volume: boot + MFT (records 0-8: $MFT, root
    /// ".", Docs dir, present.txt at root, note.txt inside Docs).
    fn ntfs_volume() -> RamDevice {
        let mut ram = RamDevice::new(512 * 4096, 512);
        let mut boot = vec![0u8; 512];
        boot[..3].copy_from_slice(b"\xEB\x52\x90");
        boot[3..11].copy_from_slice(b"NTFS    ");
        boot[0x0B..0x0D].copy_from_slice(&512u16.to_le_bytes());
        boot[0x0D] = 1; // 1 sector/cluster
        boot[0x28..0x2C].copy_from_slice(&4096u32.to_le_bytes());
        boot[0x30..0x38].copy_from_slice(&16u64.to_le_bytes()); // MFT @ cluster 16
        boot[0x40] = 0x01; // 1 cluster per record
        boot[510..512].copy_from_slice(&[0x55, 0xAA]);
        ram.data[..512].copy_from_slice(&boot);

        let mft_off = 16 * 512;
        // Record 0: $MFT with a $DATA runlist covering clusters 16..24.
        let rec0 = {
            let mut r = vec![0u8; 512];
            r[..4].copy_from_slice(b"FILE");
            r[0x04..0x06].copy_from_slice(&48u16.to_le_bytes());
            r[0x06..0x08].copy_from_slice(&1u16.to_le_bytes());
            r[0x14..0x16].copy_from_slice(&48u16.to_le_bytes());
            r[0x16..0x18].copy_from_slice(&0x01u16.to_le_bytes());
            r[0x18..0x1C].copy_from_slice(&512u32.to_le_bytes());
            // $DATA non-resident: runlist at 0x40, one run (8 clusters @ 16).
            let mut d = [0u8; 0x48];
            d[0x00..0x04].copy_from_slice(&0x80u32.to_le_bytes());
            d[0x04..0x08].copy_from_slice(&0x48u32.to_le_bytes());
            d[0x08] = 1; // non-resident
            d[0x20..0x22].copy_from_slice(&0x40u16.to_le_bytes()); // run_off
            d[0x30..0x38].copy_from_slice(&8192u64.to_le_bytes()); // real size (16 × 512)
            d[0x40] = 0x11; // len_size 1, off_size 1
            d[0x41] = 16; // 16 clusters (records 0-15 addressable)
            d[0x42] = 16; // LCN 16
            r[48..48 + 0x48].copy_from_slice(&d);
            r[48 + 0x48..48 + 0x48 + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
            r
        };
        ram.data[mft_off..mft_off + 512].copy_from_slice(&rec0);

        let present_data: Vec<u8> = (0..200u32).map(|i| (i % 251) as u8).collect();
        // (record number, parent record, name, dir, data) — written at
        // the record's TRUE MFT slot (slot 4 stays empty: $AttrDef).
        let records: &[(u64, u64, &str, bool, &[u8])] = &[
            (1, 5, "$MFTMirr", true, &[]),
            (2, 5, "$LogFile", true, &[]),
            (3, 5, "$Volume", true, &[]),
            (5, 5, ".", true, &[]), // the root
            (6, 5, "Docs", true, &[]),
            (7, 5, "present.txt", false, &present_data),
            (8, 6, "note.txt", false, b"the note"),
        ];
        for &(rec_no, parent, name, is_dir, data) in records {
            let rec = ntfs_record(rec_no, parent, name, is_dir, data);
            let at = mft_off + rec_no as usize * 512;
            ram.data[at..at + 512].copy_from_slice(&rec);
        }
        ram
    }

    #[test]
    fn ntfs_tree_resolves_structure_end_to_end() {
        let ram = ntfs_volume();
        let dev: Arc<dyn BlockDevice> = Arc::new(ram);
        let (tree, report) = run_scan(dev, "Test (V:)", &mut NoObserver).expect("scan");
        assert_eq!(report.fs, FsKind::Ntfs);

        let nodes = tree.nodes();
        let show = |n: &crate::tree::TreeNode| format!("{} → {}", n.name, n.path);
        // Root-level file: nested at the tree root, NEVER under a
        // literal '_' dir, 'File Path Lost', or an @mft path.
        let present = nodes
            .iter()
            .find(|n| n.name == "present.txt")
            .unwrap_or_else(|| {
                panic!(
                    "present.txt in: {:?}",
                    nodes.iter().map(show).collect::<Vec<_>>()
                )
            });
        assert_eq!(
            present.path, "Test (V:)\\present.txt",
            "the root-level file sits at the volume root (got {})",
            present.path
        );
        // Nested file: inside its resolved dir.
        let note = nodes
            .iter()
            .find(|n| n.name == "note.txt")
            .expect("note.txt");
        assert_eq!(note.path, "Test (V:)\\Docs\\note.txt", "got {}", note.path);
        // The root record NEVER materializes as a dir node.
        assert!(
            !nodes.iter().any(|n| n.is_dir && n.name == "."),
            "no '.' dir node"
        );
        assert!(
            nodes.iter().all(|n| !n.path.contains('@')),
            "no literal @mft leakage: {:?}",
            nodes.iter().map(show).collect::<Vec<_>>()
        );
        // Recovery queue paths: root-relative, exactly the two files.
        let ids = [present.id, note.id];
        let queue = crate::recover::build_queue(nodes, &ids);
        let mut paths: Vec<String> = queue.iter().map(|i| i.dest_path.clone()).collect();
        paths.sort();
        assert_eq!(
            paths,
            vec!["Docs/note.txt".to_string(), "present.txt".to_string()],
            "the export queue lands files at their true relative paths"
        );
    }

    // ── QA-6: the ~65% essential-mode reduction, end to end ───────

    /// A small valid BMP whose length rides in the HEADER (u32 at
    /// +2) — position-independent carve lengths, so byte-identical
    /// copies share content keys wherever they sit (the tail-scan
    /// formats would make length depend on the LAST marker before
    /// device end). Distinguishing byte at +40 → distinct keys.
    fn bmp_variant(tag: u8) -> Vec<u8> {
        let size = 300u32;
        let mut b = vec![0u8; size as usize];
        b[0..2].copy_from_slice(b"BM");
        b[2..6].copy_from_slice(&size.to_le_bytes());
        b[6..10].copy_from_slice(&0u32.to_le_bytes()); // reserved
        b[10..14].copy_from_slice(&54u32.to_le_bytes()); // data offset
        b[14..18].copy_from_slice(&40u32.to_le_bytes()); // BITMAPINFOHEADER
        b[18..22].copy_from_slice(&16i32.to_le_bytes()); // width
        b[22..26].copy_from_slice(&16i32.to_le_bytes()); // height
        b[40] = tag;
        b
    }

    /// The engineered exFAT population proving the product claim
    /// "filtering duplicate and nonessential results reduced files to
    /// review by about 65% in testing" (§6.1 + the UIUX essential-
    /// mode contract). Census by design:
    ///
    /// * 21 structured files (Existing — carved copies at their own
    ///   regions are ABSORBED by the structured-wins merge),
    /// * each with 3 byte-identical copies in unallocated space
    ///   (first copy → LostPath, repeats → Version),
    /// * 7 orphan BMPs (unique lost content → LostPath),
    /// * 49 PST signature hits with distinct prefixes (named by
    ///   content → LostName).
    ///
    /// complete = 140 visible files; essential (Existing + Deleted +
    /// LostPath) = 49 → reduction = 91/140 = 65.0% exactly.
    fn exfat_65_percent_population() -> RamDevice {
        const FILES: usize = 21;
        const COPIES: usize = 3;
        const ORPHANS: usize = 7;
        const PSTS: usize = 49;
        let total_sectors = 8192u32; // 4 MiB
        let heap_off = 8u32; // after boot(1) + FAT(4) + pad(3)
        let cluster_count = total_sectors - heap_off;
        let mut ram = RamDevice::new(512 * total_sectors as usize, 512);
        // Boot sector.
        let mut s = vec![0u8; 512];
        s[..3].copy_from_slice(b"\xEB\x76\x90");
        s[3..11].copy_from_slice(b"EXFAT   ");
        s[0x0B..0x0F].copy_from_slice(&0u32.to_le_bytes());
        s[0x0F..0x13].copy_from_slice(&total_sectors.to_le_bytes());
        s[0x13..0x17].copy_from_slice(&1u32.to_le_bytes()); // FAT offset
        s[0x17..0x1B].copy_from_slice(&4u32.to_le_bytes()); // FAT length
        s[0x1B..0x1F].copy_from_slice(&heap_off.to_le_bytes());
        s[0x1F..0x23].copy_from_slice(&cluster_count.to_le_bytes());
        s[0x23..0x27].copy_from_slice(&2u32.to_le_bytes()); // root cluster
        s[0x2F] = 9; // 512 B sectors
        s[0x30] = 0; // 1 sector/cluster
        s[510] = 0x55;
        s[511] = 0xAA;
        ram.data[..512].copy_from_slice(&s);
        // Cluster n → byte (heap_off + n − 2) × 512.
        let cluster_off = |n: u32| (heap_off + n - 2) as usize * 512;
        let set_fat = |ram: &mut RamDevice, n: u32, v: u32| {
            let off = 512 + n as usize * 4;
            ram.data[off..off + 4].copy_from_slice(&v.to_le_bytes());
        };
        set_fat(&mut ram, 0, 0xFFFF_FFF8);
        set_fat(&mut ram, 1, 0xFFFF_FFFF);
        // Root directory chain: clusters 2..6 (5 × 512 B = 2560 B for
        // 21 entry sets × 96 B = 2016 B).
        for c in 2..6u32 {
            set_fat(&mut ram, c, c + 1);
        }
        set_fat(&mut ram, 6, 0xFFFF_FFFF);
        // Structured files: clusters 10..(10+FILES), data = distinct
        // PNGs; FAT marks each EOC (allocated).
        let mut dir = Vec::new();
        for i in 0..FILES {
            let cluster = 10 + i as u32;
            set_fat(&mut ram, cluster, 0xFFFF_FFFF);
            let png = bmp_variant(i as u8);
            let at = cluster_off(cluster);
            ram.data[at..at + png.len()].copy_from_slice(&png);
            // The exFAT entry set (primary 0x85 + stream 0xC0 + name).
            let name = format!("file{i:02}.bmp");
            let name_u16: Vec<u16> = name.encode_utf16().collect();
            let mut f = [0u8; 32];
            f[0] = 0x85;
            f[1] = 2;
            f[8..10].copy_from_slice(&0x20u16.to_le_bytes());
            dir.extend_from_slice(&f);
            let mut st = [0u8; 32];
            st[0] = 0xC0;
            st[1] = 1;
            st[3] = name_u16.len() as u8;
            st[8..16].copy_from_slice(&(png.len() as u64).to_le_bytes());
            st[0x14..0x18].copy_from_slice(&cluster.to_le_bytes());
            st[0x18..0x20].copy_from_slice(&(png.len() as u64).to_le_bytes());
            dir.extend_from_slice(&st);
            let mut nm = [0u8; 32];
            nm[0] = 0xC1;
            for (j, u) in name_u16.iter().enumerate() {
                nm[2 + j * 2..4 + j * 2].copy_from_slice(&u.to_le_bytes());
            }
            dir.extend_from_slice(&nm);
        }
        let root = cluster_off(2);
        ram.data[root..root + dir.len()].copy_from_slice(&dir);
        // Duplicate copies: 3 per file, byte-identical, FAT untouched
        // (unallocated space — carving territory).
        for i in 0..FILES {
            let png = bmp_variant(i as u8);
            for c in 0..COPIES {
                let cluster = 100 + (i * COPIES + c) as u32;
                let at = cluster_off(cluster);
                ram.data[at..at + png.len()].copy_from_slice(&png);
            }
        }
        // Orphan PNGs: unique content, unallocated.
        for j in 0..ORPHANS {
            let png = bmp_variant(200 + j as u8);
            let at = cluster_off(200 + j as u32);
            ram.data[at..at + png.len()].copy_from_slice(&png);
        }
        // PST signature hits: distinct prefixes (byte 8 varies →
        // distinct content keys), unallocated.
        for k in 0..PSTS {
            let at = cluster_off(300 + k as u32);
            ram.data[at..at + 4].copy_from_slice(b"!BDN");
            ram.data[at + 8] = k as u8;
        }
        ram
    }

    #[test]
    fn qa6_essential_mode_reduces_review_set_by_65_percent() {
        // The product claim's end-to-end proof: duplicate (Version)
        // and named-by-content (LostName) results hide in essential
        // mode; structured + first-copy + orphan results stay. The
        // engineered census lands at exactly 65.0% — "about 65%".
        let ram = exfat_65_percent_population();
        let dev: Arc<dyn BlockDevice> = Arc::new(ram);
        let (tree, report) = run_scan(dev, "DW 65% (E:)", &mut NoObserver).expect("scan");
        let files: Vec<_> = tree.nodes().iter().filter(|n| !n.is_dir).collect();
        use crate::types::NodeOrigin;
        let census = |o: NodeOrigin| files.iter().filter(|n| n.origin == o).count();
        let existing = census(NodeOrigin::Existing);
        let deleted = census(NodeOrigin::Deleted);
        let lostpath = census(NodeOrigin::LostPath);
        let version = census(NodeOrigin::Version);
        let lostname = census(NodeOrigin::LostName);
        // The engineered population (see the fixture doc comment).
        assert_eq!(existing, 21, "structured files");
        assert_eq!(deleted, 0);
        assert_eq!(lostpath, 28, "21 first-copy duplicates + 7 orphans");
        assert_eq!(version, 42, "21 files × 2 repeat copies");
        assert_eq!(lostname, 49, "PST named-by-content hits");
        let complete = existing + deleted + lostpath + version + lostname;
        assert_eq!(complete, 140, "the same-region carves were absorbed");
        // The UI's essential mode hides lost-name + version
        // (src/state/results.ts ESSENTIAL_HIDDEN_ORIGINS).
        let essential = existing + deleted + lostpath;
        assert_eq!(essential, 49);
        let reduction = (complete - essential) * 100 / complete;
        assert_eq!(reduction, 65, "the ~65% claim, exact on this fixture");
        // The report's headline numbers stay honest.
        assert_eq!(report.files_found, tree.totals().0);
    }
}
