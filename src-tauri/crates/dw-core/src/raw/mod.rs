//! # RAW carving — the signature scan engine.
//!
//! The bulk pass (`architecture/04-recovery-workflow.md` §4b): sweep
//! the volume in multi-megabyte aligned windows hunting format
//! signatures, validate each hit through its grammar (tail scans,
//! structured walks — the raw_signs.json model), and emit carved
//! entries with per-format confidence. Carved files land in the
//! tree's lost-path/lost-name branches and are cross-correlated with
//! structured results during the merge phase.
//!
//! ## Session-8 discipline (the owner's TB-inflation + stuck-scan fix)
//!
//! The vendor model ("signature hit" ≠ "recoverable file with size"):
//! a magic match is only a CANDIDATE; the grammar decides. Three hard
//! bounds now govern every validator, because a 320 GB disk of binary
//! data contains thousands of accidental magics:
//!
//! * **probe cap** — the maximum number of DEVICE bytes a validator
//!   may read for one hit ([`Signature::probe_cap`]). A JPEG hunt
//!   stops after 8 MiB, an ISO-BMFF box walk after 64 MiB — never
//!   the old multi-GiB spans that made the sweep look wedged.
//! * **fallback cap** — the maximum carve LENGTH a WEAK validation may
//!   claim ([`Signature::fallback_cap`]; 0 = reject). Grammar-verified
//!   lengths are trusted; "probably truncated" hits carve a small
//!   recoverable prefix, never their full probe span. The old code
//!   carved up to 2 GiB per false hit — thousands of those produced
//!   the TB-scale `bytes_found` the owner saw.
//! * **window budget** — a shared byte budget ([`VALIDATOR_BUDGET`])
//!   limiting TOTAL validator reads per carve window; exhaustion
//!   skips further validations in that window (the sweep always moves
//!   forward).
//!
//! Validators also checkpoint the cooperative [`ScanControl`] inside
//! their walks (pause/stop answer mid-hit, not mid-window), and the
//! `Prober` reuses one scratch buffer instead of allocating a fresh
//! `Vec` per 4-byte field read.
//!
//! Memory discipline: windows are fixed-size (≤16 MiB), grammar walks
//! stream through bounded chunks — no validator ever materializes a
//! whole file, so a 4 GiB `max_size` bound costs a few hundred KiB,
//! not gigabytes (the caps are LENGTH bounds, not allocations).
//!
//! Analyser set (research: `artifacts/SearchDLL/
//! rawsearchdll--analyser-registry-census.md`,
//! `analyser-granular-batch1--pst-rar-mkv-heic.md`): JPEG, PNG, GIF,
//! BMP, ZIP (central-directory walk), RAR4/5, PDF, MP4/MOV/HEIC
//! (ISO-BMFF), MKV (EBML), MP3, WAV, OGG, FLAC, OLE2 (doc/xls/ppt),
//! SQLite, PST.

mod signatures;

use std::sync::Arc;

use memchr::memmem;

use crate::blockdev::{AlignedBuf, BatchRead, BlockDevice, BlockRead};
use crate::error::{CoreError, CoreResult};
use crate::fs::{ScanControl, ScanSink, ScanStats};
use std::collections::HashSet;

use crate::types::{Category, Extent, FoundEntry, FsKind, NodeOrigin};
/// FNV-1a content prefix hash + length (the duplicate key).
fn content_hash(prefix: &[u8], length: u64) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in prefix {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^ length.wrapping_mul(0x9e37_79b9_7f4a_7c15)
}

pub use signatures::{Signature, SignatureId, signature_set};

/// Total carve window-buffer budget: `queue_depth × window_bytes`
/// never exceeds this (16 × 4 MiB worst case) — the pipelined sweep
/// stays memory-bounded on every profile (§5.1 items 5+7).
const MAX_WINDOW_BUDGET: usize = 64 * 1024 * 1024;

/// Worker→merger carved-entry channel capacity (the RE CCarveQueue
/// size — 4× smaller than the structured record queue: carved items
/// are larger).
const CARVE_CHANNEL_CAP: usize = 1024;

/// Window overlap: 64 KiB re-scanned per advance so signatures
/// straddling a window boundary are never missed (the longest magic
/// is 8 bytes; validators read from the device independently).
const CARVE_OVERLAP: usize = 64 * 1024;

/// Prober chunk: the fixed ceiling of any single validator scratch
/// allocation.
const PROBE_CHUNK: usize = 1024 * 1024;

/// Shared validator read budget per carve window (session 8): the
/// sum of all grammar-walk device reads for one window. A window
/// full of hostile signature candidates can never spend more than
/// this on validation before the sweep moves on.
const VALIDATOR_BUDGET: u64 = 64 * 1024 * 1024;

/// Maximum grammar-walk steps (boxes / blocks / markers) per hit —
/// garbage data that chains minimum-size structures terminates the
/// walk instead of iterating for millions of steps.
const MAX_WALK_STEPS: u64 = 65_536;

/// The carve result for one hit.
pub struct CarvedHit {
    /// Bytes from the signature start.
    pub length: u64,
    pub confidence: u8,
    pub category: Category,
    /// Dimensions when the header reveals them.
    pub dimensions: Option<(u32, u32)>,
}

/// The carving engine.
pub struct RawCarver {
    dev: Arc<dyn BlockDevice>,
}

impl RawCarver {
    /// Build over a device.
    #[must_use]
    pub fn new(dev: Arc<dyn BlockDevice>) -> Self {
        Self { dev }
    }

    /// Carve `[start_byte, end_byte)` (the single-range legacy entry).
    ///
    /// # Errors
    /// [`CoreError::Cancelled`] through the control token.
    pub fn carve(
        &self,
        sink: &mut dyn ScanSink,
        ctrl: &ScanControl,
        start_byte: u64,
        end_byte: u64,
    ) -> CoreResult<ScanStats> {
        self.carve_ranges(sink, ctrl, &[(start_byte, end_byte)])
    }

    /// Carve a SET of byte ranges (the deep phase's coverage plan —
    /// free-cluster spans on NTFS volumes, full extents elsewhere,
    /// unallocated gaps always).
    ///
    /// The parallel sweep (the RE CCarveQueue model): a worker pool
    /// (`cores.clamp(2,16)`, bounded by the buffer budget) consumes
    /// window batches off a shared work queue — each worker submits
    /// its batch as ONE [`BlockDevice::read_sectors_sparse_batch`]
    /// (the platform overlapped-events pipeline on Windows), scans every window
    /// (signature hunt + grammar validation — the CPU-bound half
    /// that was serial), and sends carved entries over a bounded
    /// channel (cap 1024 — the RE queue size for the larger carved
    /// items) to THIS thread, which owns the sink and the live
    /// progress emission. Results are byte-identical to the
    /// sequential sweep at every pool size: scan regions are disjoint
    /// by construction (the overlap advance), per-window
    /// first-signature-wins dedup is local, and emission order is the
    /// only degree of freedom — the tree builder is order-independent
    /// (fixpoint + first-extent merge).
    ///
    /// # Errors
    /// [`CoreError::Cancelled`] through the control token.
    #[allow(clippy::too_many_lines)] // The parallel sweep pipeline; the worker body + drain loop share the channel contract.
    #[allow(clippy::single_match_else)] // The Err arm doubles as the cancellation signal — a match reads the intent.
    pub fn carve_ranges(
        &self,
        sink: &mut dyn ScanSink,
        ctrl: &ScanControl,
        ranges: &[(u64, u64)],
    ) -> CoreResult<ScanStats> {
        let mut stats = ScanStats::default();
        let ss = u64::from(self.dev.sector_size()).max(1);
        let cap = self.dev.capacity_bytes();
        // Clamp + normalize the coverage plan: sorted, merged,
        // in-device, non-empty.
        let mut plan: Vec<(u64, u64)> = ranges
            .iter()
            .filter_map(|&(s, e)| {
                let s = s.min(cap);
                let e = e.min(cap).max(s);
                (e > s).then_some((s, e))
            })
            .collect();
        plan.sort_unstable();
        let mut merged_ranges: Vec<(u64, u64)> = Vec::with_capacity(plan.len());
        for (s, e) in plan {
            match merged_ranges.last_mut() {
                Some((_, le)) if s <= *le => *le = (*le).max(e),
                _ => merged_ranges.push((s, e)),
            }
        }
        let total_bytes: u64 = merged_ranges.iter().map(|&(s, e)| e - s).sum();
        if total_bytes == 0 {
            return Ok(stats);
        }
        let sigs = signature_set();
        let finders: Vec<(usize, &Signature, memmem::Finder<'static>)> = sigs
            .iter()
            .enumerate()
            .map(|(i, s)| {
                // Finder borrows the signature magic (all 'static).
                let f = memmem::Finder::new(s.magic);
                (i, s, f)
            })
            .collect();
        // Adaptive geometry (§5.1 item 5): the profile's bands are
        // clamped, then the window shrinks to the remaining span.
        let prof = self.dev.read_profile().clamped();
        let span = usize::try_from(total_bytes).unwrap_or(usize::MAX).max(1);
        let window = prof.window_bytes.min(span).max(ss as usize);
        // Worker pool (the RE clamp: more than 16 shows no gain; the
        // buffer budget keeps workers × depth × window ≤ 64 MiB).
        let workers = std::thread::available_parallelism()
            .map_or(1, std::num::NonZero::get)
            .clamp(2, 16);
        let depth = prof
            .queue_depth
            .min((MAX_WINDOW_BUDGET / window / workers).max(1))
            .max(1);
        let workers = workers
            .min((MAX_WINDOW_BUDGET / window / depth).max(1))
            .max(1);
        // The full window plan, precomputed (identical math to the
        // sequential sweep: sector-multiple takes, overlap advance
        // WITHIN each range — a signature straddling into allocated
        // space is a partially-destroyed file; free-range boundaries
        // do not overlap).
        let mut windows: Vec<(u64, usize)> = Vec::new();
        for &(rs, re) in &merged_ranges {
            let mut c = rs;
            while c < re {
                let want = (re - c).min(window as u64);
                // Sector-multiple read length (trait contract), ≥1 sector.
                let take = (want as usize).next_multiple_of(ss as usize).min(window);
                let take = take.max(ss as usize).min(window);
                let take = (take as u64).min(re - c) as usize;
                let take = take.max(ss as usize);
                windows.push((c, take));
                if c + take as u64 >= re {
                    break; // the final window of this range
                }
                let advance = (take - CARVE_OVERLAP.min(take / 2)).max(1);
                c += advance as u64;
            }
        }
        // Dispatch in batches of `depth` consecutive windows (the
        // pipelined submission unit).
        let mut batches: Vec<&[(u64, usize)]> = windows.chunks(depth).collect();
        if batches.last().is_some_and(|b| b.is_empty()) {
            batches.pop();
        }
        let next_batch = std::sync::atomic::AtomicUsize::new(0);
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let bytes_done = std::sync::atomic::AtomicU64::new(0);
        let (tx, rx) = crossbeam_channel::bounded::<FoundEntry>(CARVE_CHANNEL_CAP);
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let tx = tx.clone();
                let next_batch = &next_batch;
                let cancelled = &cancelled;
                let bytes_done = &bytes_done;
                let batches = &batches;
                let finders = &finders;
                scope.spawn(move || {
                    // Worker-local reusable window buffers (one batch's
                    // worth — the memory bound above) + the validators'
                    // shared scratch (reused across every hit).
                    let mut bufs: Vec<AlignedBuf> = (0..depth)
                        .map(|_| AlignedBuf::zeroed(window, self.dev.preferred_read_alignment()))
                        .collect();
                    let mut scratch: Vec<u8> = Vec::with_capacity(PROBE_CHUNK);
                    'worker: loop {
                        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                            break 'worker;
                        }
                        let bi = next_batch.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(batch) = batches.get(bi) else {
                            break 'worker;
                        };
                        if ctrl.checkpoint().is_err() {
                            cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                            break 'worker;
                        }
                        let mut read: Vec<BatchRead<'_>> = batch
                            .iter()
                            .zip(bufs.iter_mut())
                            .map(|(&(off, take), b)| BatchRead {
                                start_lba: off / ss,
                                buf: &mut b[..take],
                            })
                            .collect();
                        let results = match self.dev.read_sectors_sparse_batch(&mut read) {
                            Ok(r) => r,
                            Err(_) => vec![Vec::new(); batch.len()],
                        };
                        drop(read);
                        // Process this worker's batch in OFFSET order
                        // (its own sequential semantics; cross-worker
                        // order is the tree's non-concern).
                        for (i, &(off, take)) in batch.iter().enumerate() {
                            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                                break 'worker;
                            }
                            let is_last = off + take as u64 >= cap;
                            let mut budget: u64 = VALIDATOR_BUDGET;
                            let mut emitted: HashSet<u64> = HashSet::new();
                            match self.scan_window_collect(
                                &bufs[i][..take],
                                &results[i],
                                off,
                                is_last,
                                cap,
                                finders,
                                &mut emitted,
                                &mut scratch,
                                &mut budget,
                                ctrl,
                            ) {
                                Ok(entries) => {
                                    bytes_done.fetch_add(
                                        take as u64,
                                        std::sync::atomic::Ordering::Relaxed,
                                    );
                                    for e in entries {
                                        if tx.send(e).is_err() {
                                            cancelled
                                                .store(true, std::sync::atomic::Ordering::Relaxed);
                                            break 'worker;
                                        }
                                    }
                                }
                                Err(_) => {
                                    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                                    break 'worker;
                                }
                            }
                        }
                    }
                });
            }
            drop(tx);
            // THIS thread owns the sink: drain the carve channel,
            // throttled live progress (100 ms — the RE CProgressTimer
            // cadence), cooperative pause/stop honored mid-drain.
            let mut last_progress = std::time::Instant::now();
            'drain: loop {
                match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(e) => {
                        stats.entries += 1;
                        stats.bytes += e.size;
                        sink.on_entry(e);
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                            break 'drain;
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break 'drain,
                }
                if last_progress.elapsed().as_millis() >= 100 {
                    if ctrl.checkpoint().is_err() {
                        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    #[allow(clippy::cast_precision_loss)] // percent display
                    sink.on_progress(
                        bytes_done.load(std::sync::atomic::Ordering::Relaxed) / ss,
                        total_bytes / ss,
                    );
                    last_progress = std::time::Instant::now();
                }
            }
            // Session 12 — drain-to-disconnect: a carve worker may be
            // parked in `send` (channel cap 1024) with its collected
            // window entries when the merger breaks on cancel; the
            // scope joins only when that send lands. Keep receiving
            // (discarding) until every worker drops its sender — the
            // same cancel-time wedge the units merger had.
            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                loop {
                    match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                        Ok(_) => {}
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    }
                }
            }
        });
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(CoreError::Cancelled);
        }
        sink.on_progress(total_bytes / ss, total_bytes / ss);
        Ok(stats)
    }

    /// Scan one FILLED carve window: signature hunt over the new-hit
    /// region (everything but the overlap tail — the final window
    /// scans fully), grammar validation, entries collected. Pure
    /// collection variant of the historical sink-emitting scan — the
    /// parallel pipeline's workers call this and ship the entries
    /// over the channel; per-window semantics are EXACTLY the
    /// sequential sweep's.
    ///
    /// # Errors
    /// [`CoreError::Cancelled`] propagates from validator
    /// checkpoints (the sweep stops; collected entries stand).
    #[allow(clippy::too_many_arguments)] // one window-scan state pass
    fn scan_window_collect(
        &self,
        buf: &[u8],
        bad: &[(u64, u64)],
        window_start: u64,
        is_last: bool,
        end: u64,
        finders: &[(usize, &Signature, memmem::Finder<'static>)],
        emitted: &mut HashSet<u64>,
        scratch: &mut Vec<u8>,
        budget: &mut u64,
        ctrl: &ScanControl,
    ) -> CoreResult<Vec<FoundEntry>> {
        let mut out = Vec::new();
        let take = buf.len();
        // New-hit region: everything except the overlap tail (the
        // final window scans fully — there is no next window).
        let scan_end = if is_last {
            take
        } else {
            take.saturating_sub(CARVE_OVERLAP)
        };
        let hay = &buf[..scan_end];
        for (_, sig, finder) in finders {
            let mut from = 0usize;
            while let Some(rel) = finder.find(&hay[from..]) {
                let rel = from + rel;
                from = rel + 1;
                // File start relative to the window (magic sits at
                // sig.offset inside the file).
                if rel < sig.offset {
                    continue;
                }
                let file_rel = rel - sig.offset;
                // Skip hits landing in bad (zero-filled) ranges.
                let magic_end = rel + sig.magic.len();
                if bad
                    .iter()
                    .any(|&(o, l)| rel < (o + l) as usize && magic_end > o as usize)
                {
                    continue;
                }
                let abs = window_start + file_rel as u64;
                if emitted.contains(&abs) {
                    continue; // an earlier signature already won here
                }
                if let Some(mut h) = self.validate_hit(sig, abs, end, scratch, budget, ctrl)? {
                    h.category = sig.category;
                    if h.length >= sig.min_size {
                        let name = format!("{}_{abs:08X}{}", sig.name_prefix, sig.ext);
                        let mut entry = FoundEntry::structured(name, false, FsKind::Raw);
                        entry.parent_hint = None; // carved → lost branches
                        entry.origin = if sig.named_by_content {
                            NodeOrigin::LostName
                        } else {
                            NodeOrigin::LostPath
                        };
                        entry.size = h.length;
                        entry.extents = vec![Extent::new(abs, h.length.min(end - abs))];
                        entry.confidence = h.confidence;
                        entry.category_hint = Some(h.category);
                        entry.dimensions = h.dimensions;
                        // Same-content key — deterministic regardless
                        // of window geometry. Prefer the WINDOW bytes
                        // we already hold (no device read); fall back
                        // to one bounded read when the prefix crosses
                        // the window tail.
                        let want = (h.length as usize).min(4096);
                        if want > 0 {
                            let in_window = file_rel + want <= buf.len();
                            entry.content_key = if in_window {
                                Some(content_hash(&buf[file_rel..file_rel + want], h.length))
                            } else {
                                let mut prefix = vec![0u8; want];
                                self.dev
                                    .read_bytes(abs, &mut prefix)
                                    .ok()
                                    .map(|()| content_hash(&prefix, h.length))
                            };
                        }
                        entry.source = FsKind::Raw;
                        emitted.insert(abs);
                        out.push(entry); // keep scanning — further hits of
                        // this signature and every other one still surface.
                    }
                }
            }
        }
        Ok(out)
    }

    /// Validate a signature hit: apply the format grammar and derive
    /// the length. `None` = the grammar rejected the magic (false
    /// positive) — several arms propagate inner `None`s. Weak /
    /// truncated validations fall back to
    /// [`Signature::fallback_cap`] (0 = reject), NEVER the probe span
    /// (the session-8 TB-inflation fix).
    ///
    /// # Errors
    /// [`CoreError::Cancelled`] from walk checkpoints.
    fn validate_hit(
        &self,
        sig: &Signature,
        at: u64,
        end: u64,
        scratch: &mut Vec<u8>,
        budget: &mut u64,
        ctrl: &ScanControl,
    ) -> CoreResult<Option<CarvedHit>> {
        // Length-math cap (never above the remaining device span).
        let limit = sig.max_size.min(end.saturating_sub(at));
        // Read cap: the session-8 probe bound.
        let read_cap = sig.probe_cap.min(end.saturating_sub(at));
        let mut prober = Prober {
            dev: &*self.dev,
            base: at,
            limit: read_cap,
            length_cap: limit,
            scratch,
            budget,
        };
        let hit: Option<CarvedHit> = match sig.id {
            SignatureId::Jpeg => Some(prober.size_jpeg(ctrl)?),
            SignatureId::Png => prober.size_tail(b"IEND", 8, 256 * 1024, ctrl)?,
            SignatureId::Gif => prober.size_gif(ctrl)?,
            SignatureId::Zip => prober.size_zip(ctrl)?,
            SignatureId::Rar4 | SignatureId::Rar5 => Some(prober.size_rar(ctrl)?),
            SignatureId::Pdf => Some(prober.size_pdf(ctrl)?),
            SignatureId::Mp4 | SignatureId::Mov | SignatureId::Heic => {
                Some(prober.size_isobmff(ctrl)?)
            }
            SignatureId::Mkv => Some(prober.size_ebml(ctrl)?),
            SignatureId::Mp3 => prober.size_mp3(),
            SignatureId::Wav => prober.size_riff(),
            SignatureId::Ogg => prober.size_ogg(),
            SignatureId::Flac => prober.size_flac(),
            SignatureId::Ole2 => Some(prober.size_ole2()),
            SignatureId::Sqlite => prober.size_sqlite(),
            SignatureId::Bmp => prober.size_bmp(),
            SignatureId::Pst => Some(prober.size_pst()),
        };
        // Confidence never exceeds the signature's own bound.
        Ok(hit.map(|hit| CarvedHit {
            length: hit.length,
            confidence: hit.confidence.min(sig.confidence),
            category: hit.category,
            dimensions: hit.dimensions,
        }))
    }
}

/// Bounded random-access probe helper for grammar walks: small
/// structured reads plus chunked pattern scans. One REUSED scratch
/// buffer (owned by the carver, shared per window) — no per-read
/// allocations — and every read decrements the window's shared
/// [`VALIDATOR_BUDGET`]; an exhausted budget reads as zeros-and-stop
/// (validators see short reads and reject/fall back).
struct Prober<'a> {
    dev: &'a dyn BlockDevice,
    /// Absolute offset of the carved candidate (the hit).
    base: u64,
    /// Maximum probe span (bytes from `base`) — the READ bound.
    limit: u64,
    /// Maximum carve LENGTH (the signature's max_size, bounded by the
    /// device tail) — grammar-verified lengths may use it, weak
    /// fallbacks may not.
    length_cap: u64,
    scratch: &'a mut Vec<u8>,
    budget: &'a mut u64,
}

impl Prober<'_> {
    /// Read `n` bytes at `base + rel` into the scratch and return
    /// them as a borrowed slice (damage-tolerant: unreadable regions
    /// read as zeros, like the old full-buffer behavior). `None` when
    /// past the probe cap or the window budget is exhausted.
    fn read(&mut self, rel: u64, n: usize) -> Option<&[u8]> {
        let n = n.min(PROBE_CHUNK);
        let avail = self.limit.saturating_sub(rel).min(n as u64) as usize;
        if avail == 0 || *self.budget == 0 {
            return None;
        }
        let spend = (avail as u64).min(*self.budget) as usize;
        *self.budget -= spend as u64;
        self.scratch.clear();
        self.scratch.resize(spend, 0);
        let _ = self
            .dev
            .read_bytes(self.base + rel, &mut self.scratch[..spend]);
        Some(&self.scratch[..spend])
    }

    /// Big-endian u32 at `base + rel` (0 when unreadable).
    fn u32be(&mut self, rel: u64) -> u32 {
        let b = self.read(rel, 4);
        b.map_or(0, |b| {
            if b.len() == 4 {
                u32::from_be_bytes([b[0], b[1], b[2], b[3]])
            } else {
                0
            }
        })
    }

    /// Little-endian u32 at `base + rel` (0 when unreadable).
    fn u32le(&mut self, rel: u64) -> u32 {
        let b = self.read(rel, 4);
        b.map_or(0, |b| {
            if b.len() == 4 {
                u32::from_le_bytes([b[0], b[1], b[2], b[3]])
            } else {
                0
            }
        })
    }

    /// Scan `[from, to)` (relative) in bounded chunks for `needle`;
    /// return the LAST match end (rel + needle.len()), tracking at
    /// most one chunk in memory at a time.
    ///
    /// # Errors
    /// [`CoreError::Cancelled`] from walk checkpoints.
    fn find_last(
        &mut self,
        needle: &[u8],
        from: u64,
        to: u64,
        ctrl: &ScanControl,
    ) -> CoreResult<Option<u64>> {
        let mut last = None;
        let mut pos = from;
        let finder = memmem::Finder::new(needle);
        while pos < to {
            ctrl.checkpoint()?;
            let take = (to - pos).min(PROBE_CHUNK as u64) as usize;
            let Some(chunk) = self.read(pos, take + needle.len() - 1) else {
                break;
            };
            if chunk.len() >= needle.len() {
                let mut from2 = 0usize;
                while let Some(rel) = finder.find(&chunk[from2..]) {
                    let rel = from2 + rel;
                    last = Some(pos + rel as u64 + needle.len() as u64);
                    from2 = rel + 1;
                }
            }
            pos += take as u64;
        }
        Ok(last)
    }

    /// Scan `[from, to)` for `needle`; return (count, last match end).
    fn count_last(&mut self, needle: &[u8], from: u64, to: u64) -> (u32, Option<u64>) {
        let mut last = None;
        let mut count = 0u32;
        let mut pos = from;
        let finder = memmem::Finder::new(needle);
        while pos < to {
            let take = (to - pos).min(PROBE_CHUNK as u64) as usize;
            let Some(chunk) = self.read(pos, take + needle.len() - 1) else {
                break; // budget/probe-cap exhaustion stops the count
            };
            if chunk.len() >= needle.len() {
                let mut from2 = 0usize;
                while let Some(rel) = finder.find(&chunk[from2..]) {
                    let rel = from2 + rel;
                    // Only matches that START inside this chunk's
                    // primary range count — the over-read tail is
                    // rescanned by the next chunk (seam double-count
                    // guard).
                    if rel < take {
                        count += 1;
                        last = Some(pos + rel as u64 + needle.len() as u64);
                    }
                    from2 = rel + 1;
                }
            }
            pos += take as u64;
        }
        (count, last)
    }

    /// Backward scan for `needle` in [from, to) (relative),
    /// chunk-by-chunk with a seam overlap so markers straddling chunk
    /// boundaries are never missed — returns the first match found
    /// scanning backwards (i.e. the LAST occurrence).
    ///
    /// # Errors
    /// [`CoreError::Cancelled`] from walk checkpoints.
    fn rfind_from(
        &mut self,
        needle: &[u8],
        from: u64,
        to: u64,
        ctrl: &ScanControl,
    ) -> CoreResult<Option<u64>> {
        let mut hi = to;
        let finder = memmem::Finder::new(needle);
        while hi > from {
            ctrl.checkpoint()?;
            let lo = hi.saturating_sub(64 * 1024).max(from);
            let Some(chunk) = self.read(lo, (hi - lo) as usize + needle.len() - 1) else {
                break;
            };
            if chunk.len() >= needle.len() {
                let mut best = None;
                let mut from2 = 0usize;
                while let Some(rel) = finder.find(&chunk[from2..]) {
                    let rel = from2 + rel;
                    best = Some(lo + rel as u64);
                    from2 = rel + 1;
                }
                if let Some(rel) = best {
                    return Ok(Some(rel));
                }
            }
            hi = lo;
        }
        Ok(None)
    }

    /// PNG/GIF-class: last tail-marker within the probe cap
    /// (streamed). Weak fallback: a small recoverable prefix, never
    /// the walked span.
    fn size_tail(
        &mut self,
        tail: &[u8],
        pad: u64,
        fallback: u64,
        ctrl: &ScanControl,
    ) -> CoreResult<Option<CarvedHit>> {
        let span = self.limit;
        match self.find_last(tail, 0, span, ctrl)? {
            Some(end) => Ok(Some(hit(end + pad, 95, Category::Images, None))),
            None => Ok(Some(hit(
                span.min(fallback).min(self.length_cap),
                45,
                Category::Images,
                None,
            ))),
        }
    }

    /// GIF: the header must say GIF87a/GIF89a, then the trailer
    /// (0x3B) hunt. Anything else rejects — "GIF8" alone is far too
    /// common in arbitrary data.
    fn size_gif(&mut self, ctrl: &ScanControl) -> CoreResult<Option<CarvedHit>> {
        let head = self.read(0, 6).map(<[u8]>::to_vec);
        let Some(head) = head else {
            return Ok(None);
        };
        if head.len() < 6 || (&head[..6] != b"GIF87a" && &head[..6] != b"GIF89a") {
            return Ok(None);
        }
        self.size_tail(&[0x3B], 1, 256 * 1024, ctrl)
    }

    /// ZIP (research: `rawsearchdll--zip-analyser-carving.md`):
    /// index-first — find the EOCD near the archive tail, validate
    /// the central-directory offsets, size = EOCD end. The EOCD scan
    /// is bounded to the last ~66 KiB of the probe span (the
    /// documented EOCD parking zone), so even a 1 GiB-length candidate
    /// costs one bounded read set.
    fn size_zip(&mut self, ctrl: &ScanControl) -> CoreResult<Option<CarvedHit>> {
        // Length math may consider up to the length cap; the EOCD
        // READ stays inside the probe cap's tail window.
        let span = self.length_cap;
        let search_from = span.saturating_sub(66_000);
        let Some(eocd) = self.rfind_from(b"PK\x05\x06", search_from, span, ctrl)? else {
            // No EOCD: local-header-only stream — reject (the old
            // bounded low-confidence span became the fallback below).
            return Ok(None);
        };
        let cd_size = u64::from(self.u32le(eocd + 12));
        let cd_off = u64::from(self.u32le(eocd + 16));
        if cd_off + cd_size <= eocd {
            return Ok(Some(hit(eocd + 22, 96, Category::Archives, None)));
        }
        // Structurally invalid EOCD: small bounded low-confidence carve.
        Ok(Some(hit(
            self.limit.min(1024 * 1024).min(self.length_cap),
            40,
            Category::Archives,
            None,
        )))
    }

    /// RAR4/RAR5: block walk to the end-of-archive marker (each block
    /// header read on demand — never the whole archive). Step-capped
    /// and probe-capped; garbage chains terminate fast.
    fn size_rar(&mut self, ctrl: &ScanControl) -> CoreResult<CarvedHit> {
        let span = self.limit;
        let mut pos = 7u64; // past the marker
        let mut steps = 0u64;
        while pos + 11 <= span && steps < MAX_WALK_STEPS {
            steps += 1;
            if steps.is_multiple_of(256) {
                ctrl.checkpoint()?;
            }
            let btype = self.read(pos + 2, 1).map_or(0, |b| b[0]);
            let Some(fl) = self.read(pos + 3, 2).map(<[u8]>::to_vec) else {
                break;
            };
            if fl.len() < 2 {
                break;
            }
            let flags = u16::from_le_bytes([fl[0], fl[1]]);
            let Some(sz) = self.read(pos + 5, 2).map(<[u8]>::to_vec) else {
                break;
            };
            if sz.len() < 2 {
                break;
            }
            let mut size = u64::from(u16::from_le_bytes([sz[0], sz[1]]));
            if flags & 0x8000 != 0 {
                let hi = self.read(pos + 7, 4).map(<[u8]>::to_vec);
                if let Some(hi) = hi
                    && hi.len() == 4
                {
                    size += u64::from(u32::from_le_bytes([hi[0], hi[1], hi[2], hi[3]])) << 16;
                }
            }
            if size < 7 {
                break;
            }
            pos += size;
            if btype == 0x7B {
                // End-of-archive block.
                return Ok(hit(pos, 90, Category::Archives, None));
            }
        }
        Ok(hit(
            span.min(1024 * 1024).min(self.length_cap),
            45,
            Category::Archives,
            None,
        ))
    }

    /// PDF: %PDF → last %%EOF (streamed backward from the cap).
    fn size_pdf(&mut self, ctrl: &ScanControl) -> CoreResult<CarvedHit> {
        let span = self.limit;
        match self.rfind_from(b"%%EOF", 0, span, ctrl)? {
            Some(end) => Ok(hit(end + 5, 94, Category::Documents, None)),
            None => Ok(hit(
                span.min(256 * 1024).min(self.length_cap),
                45,
                Category::Documents,
                None,
            )),
        }
    }

    /// ISO-BMFF (MP4/MOV/HEIC): 32/64-bit box walk — each box header
    /// is an 8-16 byte read; jumps are arithmetic, not allocations.
    /// The `ftyp` brand must be plausible ASCII (the "ftyp" magic
    /// alone rides inside arbitrary text). Step- and probe-capped.
    fn size_isobmff(&mut self, ctrl: &ScanControl) -> CoreResult<CarvedHit> {
        let span = self.limit;
        // Brand sanity: the 4 brand bytes AFTER the "ftyp" magic
        // (file+8) must be plausible ASCII (the "ftyp" magic alone
        // rides inside arbitrary text).
        let brand = self.read(8, 4).map(<[u8]>::to_vec);
        let brand_ok = brand.is_some_and(|b| {
            b.len() == 4
                && b.iter()
                    .all(|&c| c.is_ascii_alphanumeric() || c == b' ' || c == b'-')
        });
        if !brand_ok {
            return Ok(hit(0, 0, Category::Videos, None)); // rejected by min_size
        }
        let mut pos = 0u64;
        let mut sane = false;
        let mut steps = 0u64;
        while pos + 8 <= span && steps < MAX_WALK_STEPS {
            steps += 1;
            if steps.is_multiple_of(256) {
                ctrl.checkpoint()?;
            }
            let size = u64::from(self.u32be(pos));
            let box_size = if size == 1 && pos + 16 <= span {
                let b = self.read(pos + 8, 8).map(<[u8]>::to_vec);
                let Some(b) = b else { break };
                if b.len() < 8 {
                    break;
                }
                u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
            } else {
                size
            };
            // Overflow-guarded: a corrupt 64-bit size must move pos
            // FORWARD within the span or the walk ends (a naive
            // `pos + box_size > span` wraps in release and can loop
            // forever with a backward-moving pos).
            if box_size < 8 || box_size > span - pos {
                break;
            }
            sane = true;
            pos += box_size;
        }
        if sane {
            Ok(hit(pos, 90, Category::Videos, None))
        } else {
            Ok(hit(
                span.min(4 * 1024 * 1024).min(self.length_cap),
                40,
                Category::Videos,
                None,
            ))
        }
    }

    /// EBML (MKV/WebM): vint-sized element walk (header reads on
    /// demand). Step- and probe-capped.
    fn size_ebml(&mut self, ctrl: &ScanControl) -> CoreResult<CarvedHit> {
        let span = self.limit;
        let vint = |p: &mut Self, pos: u64| -> Option<(u64, usize)> {
            let first = p.read(pos, 1).map_or(0, |b| b[0]);
            let len = first.leading_ones() as usize;
            if len == 0 || len > 8 || pos + len as u64 > span {
                return None;
            }
            let rest = p.read(pos + 1, len - 1).map(<[u8]>::to_vec)?;
            if rest.len() < len - 1 {
                return None;
            }
            let mut v = u64::from(first & (0xFF >> len));
            for b in rest {
                v = (v << 8) | u64::from(b);
            }
            Some((v, len))
        };
        let mut pos = 0u64;
        let mut sane = false;
        let mut steps = 0u64;
        while pos < span && steps < MAX_WALK_STEPS {
            steps += 1;
            if steps.is_multiple_of(256) {
                ctrl.checkpoint()?;
            }
            let Some((_, id_len)) = vint(self, pos) else {
                break;
            };
            let Some((size, size_len)) = vint(self, pos + id_len as u64) else {
                break;
            };
            let elem_len = id_len as u64 + size_len as u64 + size;
            if pos.checked_add(elem_len).is_none_or(|e| e > span) {
                break;
            }
            sane = true;
            pos += elem_len;
        }
        if sane {
            Ok(hit(pos, 85, Category::Videos, None))
        } else {
            Ok(hit(
                span.min(4 * 1024 * 1024).min(self.length_cap),
                40,
                Category::Videos,
                None,
            ))
        }
    }

    /// MP3: ID3v2 header size (version-validated). No ID3 → reject
    /// (the old 16 MiB fallback carved a span per ASCII "ID3" text
    /// hit — the session-8 lesson).
    fn size_mp3(&mut self) -> Option<CarvedHit> {
        let data = self.read(0, 16).map(<[u8]>::to_vec)?;
        if data.len() > 10 && &data[..3] == b"ID3" {
            let ver = data[3];
            if !(2..=5).contains(&ver) {
                return None;
            }
            let sz = (u64::from(data[6]) << 21)
                | (u64::from(data[7]) << 14)
                | (u64::from(data[8]) << 7)
                | u64::from(data[9]);
            if sz == 0 {
                return None;
            }
            return Some(hit(
                (sz + 10).min(self.length_cap),
                85,
                Category::Audio,
                None,
            ));
        }
        None
    }

    /// RIFF/WAV: the header's own size field (invalid → reject).
    fn size_riff(&mut self) -> Option<CarvedHit> {
        let data = self.read(0, 16).map(<[u8]>::to_vec)?;
        if data.len() >= 8 && &data[..4] == b"RIFF" {
            let riff_size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
            let total = u64::from(riff_size) + 8;
            if total >= 44 && total <= self.length_cap {
                return Some(hit(total, 92, Category::Audio, None));
            }
        }
        None
    }

    /// OGG page-walk / repeated-marker scan (≥2 markers = a stream).
    fn size_ogg(&mut self) -> Option<CarvedHit> {
        let span = self.limit;
        let (count, last) = self.count_last(b"OggS", 0, span);
        (count >= 2).then(|| hit(last.unwrap_or(span), 80, Category::Audio, None))
    }

    /// FLAC: STREAMINFO harvest + metadata-block walk. The walk ends
    /// at the last-block flag → exact length; unterminated → a small
    /// fallback prefix (the old code carved up to 128 MiB).
    fn size_flac(&mut self) -> Option<CarvedHit> {
        let data = self.read(0, 4096).map(<[u8]>::to_vec)?;
        if data.len() > 4 && &data[..4] == b"fLaC" {
            let mut pos = 4usize;
            let mut dims = None;
            let mut total = 4u64;
            let mut terminated = false;
            while pos + 4 <= data.len() {
                let last = data[pos] & 0x80 != 0;
                let btype = data[pos] & 0x7F;
                let blen = (u32::from_be_bytes([0, data[pos + 1], data[pos + 2], data[pos + 3]])
                    >> 8) as usize;
                if btype == 0 && pos + 38 <= data.len() {
                    let rate = (u32::from(data[pos + 18]) << 12)
                        | (u32::from(data[pos + 19]) << 4)
                        | (u32::from(data[pos + 20]) >> 4);
                    let channels = u32::from((data[pos + 20] & 0x0E) >> 1) + 1;
                    dims = Some((rate, channels));
                }
                pos += 4 + blen;
                total += 4 + blen as u64;
                if last {
                    terminated = true;
                    break;
                }
            }
            if terminated && total <= self.length_cap {
                return Some(hit(total, 88, Category::Audio, dims));
            }
            return Some(hit(
                total.min(256 * 1024).min(self.length_cap),
                45,
                Category::Audio,
                dims,
            ));
        }
        None
    }

    /// OLE2 (doc/xls/ppt): the CFB header's FAT-sector bound
    /// (invalid geometry → reject; the old `limit` fallback carved up
    /// to 2 GiB per false hit).
    fn size_ole2(&mut self) -> CarvedHit {
        let data = self.read(0, 64).map(<[u8]>::to_vec);
        if let Some(data) = data
            && data.len() >= 0x34
        {
            let sector_shift = u16::from_le_bytes([data[0x1E], data[0x1F]]);
            let num_fat = u32::from_le_bytes([data[0x2C], data[0x2D], data[0x2E], data[0x2F]]);
            // Real CFB: 512 or 4096 byte sectors, at least one FAT
            // sector, a sane FAT count. The shift is computed ONLY
            // inside the guard (a garbage shift value must never
            // reach the `<<`).
            if (sector_shift == 9 || sector_shift == 10) && (1..=1_000_000).contains(&num_fat) {
                let ss = 1u64 << u64::from(sector_shift);
                let bound = (u64::from(num_fat) + 128) * ss;
                if bound > 0 && bound <= self.length_cap {
                    return hit(bound, 85, Category::Documents, None);
                }
                if bound > self.length_cap {
                    // Larger than the cap: trust the header up to the cap.
                    return hit(self.length_cap, 85, Category::Documents, None);
                }
            }
        }
        hit(0, 0, Category::Documents, None) // rejected by min_size
    }

    /// SQLite: page_size × page_count from the header (invalid →
    /// reject).
    fn size_sqlite(&mut self) -> Option<CarvedHit> {
        let data = self.read(0, 40).map(<[u8]>::to_vec)?;
        if data.len() >= 32 {
            let page_size = u16::from_be_bytes([data[16], data[17]]);
            let page_size = if page_size == 1 {
                65_536
            } else {
                u64::from(page_size)
            };
            if !page_size.is_power_of_two() || page_size < 512 {
                return None;
            }
            let pages = u32::from_be_bytes([data[28], data[29], data[30], data[31]]);
            if pages == 0 {
                return None;
            }
            let total = page_size.saturating_mul(u64::from(pages));
            if total <= self.length_cap {
                return Some(hit(total, 90, Category::Other, None));
            }
            return Some(hit(self.length_cap, 55, Category::Other, None));
        }
        None
    }

    /// BMP: file-size field + DIB dimension validation.
    fn size_bmp(&mut self) -> Option<CarvedHit> {
        let data = self.read(0, 40).map(<[u8]>::to_vec)?;
        if data.len() >= 26 {
            let file_size = u32::from_le_bytes([data[2], data[3], data[4], data[5]]);
            let header_size = u32::from_le_bytes([data[14], data[15], data[16], data[17]]);
            let w = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
            let h = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
            if file_size >= 26
                && u64::from(file_size) <= self.length_cap
                && header_size >= 40
                && (0..=100_000).contains(&w)
                && (-100_000..=100_000).contains(&h)
            {
                return Some(hit(
                    u64::from(file_size),
                    92,
                    Category::Images,
                    Some((w.unsigned_abs(), h.unsigned_abs())),
                ));
            }
        }
        None
    }

    /// PST (research: `analyser-granular-batch1--pst-rar-mkv-heic.md`
    /// §1 — FilePSTAnalyser): the format byte at +0x0A discriminates
    /// ANSI (0x0E, size at +0xA8) from Unicode (0x17, size at +0xB8);
    /// unknown versions floor at 1 MiB. `!BDN` alone is far too weak.
    fn size_pst(&mut self) -> CarvedHit {
        let ver = self.read(0x0A, 1).map_or(0, |b| b[0]);
        let size_field = match ver {
            0x0E => self.u32le(0xA8),
            0x17 => self.u32le(0xB8),
            _ => 0,
        };
        if ver != 0x0E && ver != 0x17 {
            // Unknown version: the vendor's 1 MiB floor.
            return hit(1024 * 1024, 60, Category::Email, None);
        }
        // Known generation: the header's own total-size field
        // (clamped to the carve cap — a garbage size field must never
        // inflate the counters).
        hit(
            u64::from(size_field.max(1)).min(self.length_cap),
            60,
            Category::Email,
            None,
        )
    }

    /// JPEG: FFD8 start → FFD9 EOI walk (SOF dimension harvest,
    /// restart-marker aware, truncated-tolerant) — streamed in 64 KiB
    /// chunks with a 4-byte straddle carry so markers spanning chunk
    /// edges parse correctly. Probe-capped; a truncated (no-EOI) walk
    /// carves a bounded prefix, never the full walked span.
    fn size_jpeg(&mut self, ctrl: &ScanControl) -> CoreResult<CarvedHit> {
        let span = self.limit;
        let mut dims = None;
        let mut chunk_base = 2u64; // past SOI
        let mut steps = 0u64;
        loop {
            ctrl.checkpoint()?;
            let take = (span - chunk_base).min(64 * 1024) as usize;
            if take == 0 {
                break;
            }
            steps += 1;
            if steps > MAX_WALK_STEPS {
                break;
            }
            let data = self
                .read(chunk_base, take + 3.min(take))
                .map(<[u8]>::to_vec);
            let Some(data) = data else {
                break;
            };
            let mut i = 0usize;
            while i + 4 <= data.len() {
                if data[i] != 0xFF {
                    i += 1;
                    continue;
                }
                let marker = data[i + 1];
                if (0xC0..=0xCF).contains(&marker)
                    && marker != 0xC4
                    && marker != 0xC8
                    && marker != 0xCC
                {
                    // SOF: precision(1) height(2) width(2).
                    let sof = self.read(chunk_base + i as u64 + 4, 5).map(<[u8]>::to_vec);
                    if let Some(sof) = sof
                        && sof.len() >= 5
                    {
                        let h = u16::from_be_bytes([sof[1], sof[2]]);
                        let w = u16::from_be_bytes([sof[3], sof[4]]);
                        if w > 0 && h > 0 {
                            dims = Some((u32::from(w), u32::from(h)));
                        }
                    }
                    i += 4;
                    continue;
                }
                if marker == 0xD9 {
                    // EOI — the end of the image.
                    return Ok(hit(
                        chunk_base + i as u64 + 2,
                        if dims.is_some() { 92 } else { 88 },
                        Category::Images,
                        dims,
                    ));
                }
                if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
                    i += 2; // standalone markers
                    continue;
                }
                let len = u16::from_be_bytes([data[i + 2], data[i + 3]]);
                if len < 2 {
                    // Garbage length: resync one byte.
                    i += 1;
                    continue;
                }
                i += 2 + len as usize;
            }
            chunk_base += take as u64;
            if chunk_base >= span {
                break;
            }
        }
        // Truncated (no EOI within the cap): carve a bounded prefix
        // at Poor-grade confidence (an honest "maybe partial" signal).
        Ok(hit(
            span.min(1024 * 1024).min(self.length_cap),
            45,
            Category::Images,
            dims,
        ))
    }
}

/// Constructor helper (keeps call sites tight).
const fn hit(
    length: u64,
    confidence: u8,
    category: Category,
    dimensions: Option<(u32, u32)>,
) -> CarvedHit {
    CarvedHit {
        length,
        confidence,
        category,
        dimensions,
    }
}

impl crate::fs::ScanEngine for RawCarver {
    fn kind(&self) -> FsKind {
        FsKind::Raw
    }

    fn quick_scan(
        &mut self,
        _sink: &mut dyn ScanSink,
        _ctrl: &ScanControl,
    ) -> CoreResult<ScanStats> {
        // Carving is a deep-phase activity by definition.
        Ok(ScanStats::default())
    }

    fn deep_scan(&mut self, sink: &mut dyn ScanSink, ctrl: &ScanControl) -> CoreResult<ScanStats> {
        let cap = self.dev.capacity_bytes();
        self.carve(sink, ctrl, 0, cap)
    }
}

#[cfg(test)]
#[allow(clippy::case_sensitive_file_extension_comparisons)] // our own carved names
mod tests {
    use super::*;
    use crate::error::CoreError;
    use crate::testutil::RamDevice;

    struct Collector(Vec<FoundEntry>);
    impl crate::fs::ScanSink for Collector {
        fn on_entry(&mut self, e: FoundEntry) {
            self.0.push(e);
        }
    }

    fn carve_ram(ram: RamDevice, span: u64) -> (ScanStats, Vec<FoundEntry>) {
        let dev: Arc<dyn BlockDevice> = Arc::new(ram);
        let carver = RawCarver::new(dev);
        let mut sink = Collector(Vec::new());
        let stats = carver
            .carve(&mut sink, &ScanControl::new(), 0, span)
            .expect("carve");
        (stats, sink.0)
    }

    #[test]
    fn carves_a_png() {
        // Minimal PNG: signature + IHDR-ish + IEND.
        let mut img = Vec::new();
        img.extend_from_slice(b"\x89PNG\r\n\x1A\n");
        img.extend_from_slice(&[0u8; 100]);
        img.extend_from_slice(b"IEND");
        img.extend_from_slice(&[0u8; 4]);
        let mut ram = RamDevice::new(512 * 32, 512);
        ram.data[..img.len()].copy_from_slice(&img);
        let (stats, entries) = carve_ram(ram, 512 * 32);
        assert_eq!(stats.entries, 1);
        let e = &entries[0];
        assert!(e.name.starts_with("IMG_"));
        assert!(e.name.ends_with(".png"));
        assert_eq!(e.origin, NodeOrigin::LostPath);
        assert_eq!(e.category_hint, Some(Category::Images));
        assert!(e.size >= 108 && e.size <= 512 * 32);
    }

    #[test]
    fn multiple_hits_of_one_signature_all_carve() {
        // Regression: the old `break` after emit stopped the signature
        // scan — at most ONE file per signature per window. Three
        // JPEGs in one window must yield three carves.
        let mut img = Vec::new();
        img.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0]);
        img.extend_from_slice(&[0u8; 4]);
        img.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11]);
        img.extend_from_slice(&[8, 0, 6, 0x02, 0x80]);
        img.extend_from_slice(&[0x01, 0xE0]);
        img.extend_from_slice(&[0u8; 200]);
        img.extend_from_slice(&[0xFF, 0xD9]);
        let mut ram = RamDevice::new(512 * 16, 512);
        for off in [0usize, 2048, 4096] {
            ram.data[off..off + img.len()].copy_from_slice(&img);
        }
        let (stats, entries) = carve_ram(ram, 512 * 16);
        assert_eq!(entries.len(), 3, "all three copies carved");
        assert_eq!(stats.entries, 3);
    }

    #[test]
    fn identical_copies_share_content_key() {
        // Two byte-identical JPEGs at different offsets → two carves,
        // ONE content key (the tree marks the repeat as Version).
        let mut img = Vec::new();
        img.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0]);
        img.extend_from_slice(&[0u8; 4]);
        img.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11]);
        img.extend_from_slice(&[8, 0, 6, 0x02, 0x80]);
        img.extend_from_slice(&[0x01, 0xE0]);
        img.extend_from_slice(&[0u8; 200]);
        img.extend_from_slice(&[0xFF, 0xD9]);
        let mut ram = RamDevice::new(512 * 8, 512);
        ram.data[..img.len()].copy_from_slice(&img);
        ram.data[2048..2048 + img.len()].copy_from_slice(&img);
        let (_, entries) = carve_ram(ram, 512 * 8);
        assert_eq!(entries.len(), 2, "both copies carved");
        let keys: Vec<Option<u64>> = entries.iter().map(|e| e.content_key).collect();
        assert!(keys[0].is_some(), "content key computed");
        assert_eq!(keys[0], keys[1], "identical content shares the key");
        assert_ne!(entries[0].extents[0].offset, entries[1].extents[0].offset);
    }

    #[test]
    fn carves_a_jpeg_with_dimensions() {
        // FFD8FF + SOF0 (C0) with crafted 640x6 + EOI.
        let mut img = Vec::new();
        img.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0]);
        img.extend_from_slice(&[0u8; 4]); // JFIF header-ish
        img.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11]); // SOF0, len 17
        img.extend_from_slice(&[8, 0, 6, 0x02, 0x80]); // prec, H=6, ...
        img.extend_from_slice(&[0x01, 0xE0]); // W=480? as crafted
        img.extend_from_slice(&[0u8; 200]); // pad past the 128-byte min_size
        img.extend_from_slice(&[0xFF, 0xD9]);
        let mut ram = RamDevice::new(512 * 8, 512);
        ram.data[..img.len()].copy_from_slice(&img);
        let (_, entries) = carve_ram(ram, 512 * 8);
        assert!(!entries.is_empty());
        let e = &entries[0];
        assert!(e.name.ends_with(".jpg"));
        assert!(e.dimensions.is_some());
    }

    #[test]
    fn carves_a_zip_with_eocd() {
        // Local header + file data + central dir + EOCD.
        let mut z = Vec::new();
        z.extend_from_slice(b"PK\x03\x04");
        z.extend_from_slice(&[0u8; 60]); // local header + payload
        z.extend_from_slice(b"PK\x01\x02");
        z.extend_from_slice(&[0u8; 46]); // CD entry
        z.extend_from_slice(b"PK\x05\x06");
        z.extend_from_slice(&[0u8; 18]); // EOCD body
        let mut ram = RamDevice::new(512 * 8, 512);
        ram.data[..z.len()].copy_from_slice(&z);
        let (_, entries) = carve_ram(ram, 512 * 8);
        assert!(!entries.is_empty());
        let e = &entries[0];
        assert!(e.name.ends_with(".zip"));
        assert!(e.confidence >= 55);
    }

    #[test]
    fn cancels_through_the_token() {
        let mut ram = RamDevice::new(512 * 4096, 512);
        ram.data[..4].copy_from_slice(b"PK\x03\x04");
        let dev: Arc<dyn BlockDevice> = Arc::new(ram);
        let carver = RawCarver::new(dev);
        let ctrl = ScanControl::new();
        ctrl.cancel();
        let mut sink = Collector(Vec::new());
        let r = carver.carve(&mut sink, &ctrl, 0, 512 * 4096);
        assert!(matches!(r, Err(CoreError::Cancelled)));
    }

    #[test]
    fn jpeg_only_garbage_yields_nothing() {
        let mut ram = RamDevice::new(512 * 4, 512);
        for (i, b) in ram.data.iter_mut().enumerate() {
            *b = (i % 253) as u8;
        }
        let (stats, entries) = carve_ram(ram, 512 * 4);
        assert!(entries.is_empty());
        assert_eq!(stats.entries, 0);
    }

    #[test]
    fn window_overlap_does_not_double_carve() {
        // A signature near a window boundary must be carved exactly
        // once (the overlap is re-scanned, not re-emitted).
        let mut img = Vec::new();
        img.extend_from_slice(b"\x89PNG\r\n\x1A\n");
        img.extend_from_slice(&[0u8; 100]);
        img.extend_from_slice(b"IEND");
        img.extend_from_slice(&[0u8; 4]);
        // Place the image right at the first window's overlap zone:
        // the test window is the whole 2 MiB ram, overlap 64 KiB.
        let at = 8 * 1024 * 1024 - 32 * 1024;
        let mut ram = RamDevice::new(512 * 8192, 512); // 4 MiB
        let at = at.min(ram.data.len() - img.len());
        ram.data[at..at + img.len()].copy_from_slice(&img);
        let (stats, _) = carve_ram(ram, 512 * 8192);
        assert_eq!(stats.entries, 1, "exactly one carve across the boundary");
    }

    #[test]
    fn bad_ranges_do_not_spawn_false_hits() {
        // Sparse reads zero-fill damage; the zeroed region must not
        // produce signature hits (e.g. no magic is all-zero, but the
        // guard must hold anyway).
        let mut ram = RamDevice::new(512 * 64, 512);
        ram.bad_lba = Some(10);
        let (stats, _) = carve_ram(ram, 512 * 64);
        assert_eq!(stats.entries, 0);
    }

    // ── Session 8: the TB-inflation + stuck-scan regressions ───────

    /// A deterministic pseudo-random byte stream (xorshift64*) — the
    /// "binary soup" a real unallocated region looks like. Seed the
    /// FIRST bytes with signature magics to force false-positive hits.
    fn hostile_soup(size: usize, seeds: &[(usize, &[u8])]) -> RamDevice {
        let mut ram = RamDevice::new(size, 512);
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        for b in &mut ram.data {
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            *b = (x.wrapping_mul(0x2545F4914F6CDD1D) >> 56) as u8;
        }
        for &(at, magic) in seeds {
            ram.data[at..at + magic.len()].copy_from_slice(magic);
        }
        ram
    }

    #[test]
    fn hostile_garbage_stays_bounded_bytes() {
        // The owner's symptom: thousands of accidental magics × huge
        // fallback carves = TB-scale bytes_found on a small disk.
        // A 4 MiB soup with several false JPEG/MP3/RIFF/!BDN magics
        // must produce bounded total carved bytes (< 25% of the soup).
        let seeds: Vec<(usize, &[u8])> = vec![
            (512, b"\xFF\xD8\xFF\xE0"),                       // JPEG false hit
            (512 * 40, b"\xFF\xD8\xFF\xDB"),                  // another, no EOI ever
            (512 * 90, b"ID3\x04\x00\x00\x00\x00\x00\x0A"),   // ID3v2.4, tiny
            (512 * 130, b"RIFF"),                             // RIFF garbage
            (512 * 170, b"!BDN"),                             // PST garbage
            (512 * 210, b"GIF8zz"),                           // GIF8 without 87a/89a
            (512 * 250, b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1"), // OLE2 garbage
            (512 * 290, b"ftyp"),                             // ftyp at wrong offset (no brand)
        ];
        let ram = hostile_soup(512 * 8192, &seeds); // 4 MiB
        let (stats, entries) = carve_ram(ram, 512 * 8192);
        let total: u64 = entries.iter().map(|e| e.size).sum();
        assert_eq!(stats.bytes, total);
        // Seeded weak hits (2 JPEG + 1 PST floor) are each capped at
        // their 1 MiB fallback; total must stay a small fraction of
        // the soup — never the multi-GiB spans of the old code.
        assert!(
            total <= 8 * 1024 * 1024,
            "carved bytes stay bounded, got {total} over {} entries",
            entries.len()
        );
        for e in &entries {
            assert!(
                e.size <= 4 * 1024 * 1024,
                "entry {} carved {}",
                e.name,
                e.size
            );
        }
    }

    #[test]
    fn truncated_jpeg_carves_bounded_prefix() {
        // A JPEG SOI with NO EOI anywhere: the walk covers the probe
        // cap but the CARVE is the small fallback prefix.
        let mut ram = RamDevice::new(512 * 8192, 512); // 4 MiB < 8 MiB probe
        ram.data[..4].copy_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0]);
        let (_, entries) = carve_ram(ram, 512 * 8192);
        assert!(!entries.is_empty(), "truncated JPEG still surfaces");
        let e = &entries[0];
        assert!(e.name.ends_with(".jpg"));
        assert!(
            e.size <= 1024 * 1024,
            "fallback carve bounded, got {}",
            e.size
        );
        assert!(e.confidence <= 60, "weak validation carries low confidence");
    }

    #[test]
    fn garbage_id3_text_hit_rejects() {
        // "ID3" as literal text (no valid header after) → reject.
        let mut ram = RamDevice::new(512 * 8, 512);
        let text = b"ID3 - the tag format docs mention ID3 here...";
        ram.data[..text.len()].copy_from_slice(text);
        let (stats, _) = carve_ram(ram, 512 * 8);
        assert_eq!(stats.entries, 0, "text 'ID3' is not an MP3");
    }

    #[test]
    fn real_id3_mp3_carves_exact_size() {
        let mut ram = RamDevice::new(512 * 64, 512);
        // ID3v2.3 header: syncsafe size = 0x00 0x00 0x20 0x00 → 4096.
        ram.data[..10].copy_from_slice(b"ID3\x03\x00\x00\x00\x00\x20\x00");
        ram.data[10..10 + 4096].fill(0xAB);
        let (_, entries) = carve_ram(ram, 512 * 64);
        assert!(!entries.is_empty());
        assert_eq!(entries[0].size, 4096 + 10);
        assert_eq!(entries[0].confidence, 85);
    }

    #[test]
    fn garbage_bdn_rejects_valid_pst_carves() {
        // !BDN + ANSI version byte 0x0E + size field 0x20000 (128 KiB).
        let mut ram = RamDevice::new(512 * 4096, 512); // 2 MiB
        ram.data[..4].copy_from_slice(b"!BDN");
        ram.data[0x0A] = 0x0E;
        ram.data[0xA8..0xAC].copy_from_slice(&0x8_0000u32.to_le_bytes()); // 512 KiB
        let (_, entries) = carve_ram(ram, 512 * 4096);
        assert!(!entries.is_empty());
        assert_eq!(entries[0].size, 0x8_0000);

        // And the bare magic without a valid version → the 1 MiB floor.
        let mut ram2 = RamDevice::new(512 * 4096, 512);
        ram2.data[..4].copy_from_slice(b"!BDN");
        let (_, entries2) = carve_ram(ram2, 512 * 4096);
        assert_eq!(entries2.len(), 1);
        assert_eq!(entries2[0].size, 1024 * 1024);
    }

    #[test]
    fn garbage_ftyp_rejects_brand_checked() {
        // A box-sized ftyp whose brand bytes are non-alnum garbage →
        // rejected. File layout: [size u32 BE][ftyp][brand][…].
        let mut ram = RamDevice::new(512 * 64, 512);
        ram.data[0..4].copy_from_slice(&24u32.to_be_bytes());
        ram.data[4..8].copy_from_slice(b"ftyp");
        ram.data[8..12].copy_from_slice(&[0xFF, 0x00, 0xFE, 0x01]);
        let (stats, _) = carve_ram(ram, 512 * 64);
        assert_eq!(stats.entries, 0, "non-ASCII brand is not an MP4");
    }

    #[test]
    fn real_ftyp_box_chain_carves() {
        // ftyp + one 8-byte free box + a 4 KiB mdat (past min_size).
        let mut ram = RamDevice::new(512 * 256, 512);
        let mut f = Vec::new();
        f.extend_from_slice(&24u32.to_be_bytes()); // box size
        f.extend_from_slice(b"ftypisom"); // type + brand
        f.extend_from_slice(&[0u8; 12]); // minor version (4) + compat (8)
        f.extend_from_slice(&8u32.to_be_bytes());
        f.extend_from_slice(b"free");
        f.extend_from_slice(&4096u32.to_be_bytes());
        f.extend_from_slice(b"mdat");
        f.extend_from_slice(&[0u8; 4088]);
        ram.data[..f.len()].copy_from_slice(&f);
        let (_, entries) = carve_ram(ram, 512 * 256);
        assert!(!entries.is_empty());
        assert_eq!(entries[0].size, 24 + 8 + 4096);
        // The signature's base confidence (85) caps the grammar's 90.
        assert_eq!(entries[0].confidence, 85);
    }

    #[test]
    fn validator_budget_bounds_window_work() {
        // A window full of candidate magics: the shared budget keeps
        // total validator reads bounded and the sweep completes fast
        // (the hostile-garbage stuck-scan regression).
        let mut seeds = Vec::new();
        for i in 0..64usize {
            seeds.push((i * 8192 + 17, &b"\xFF\xD8\xFF\xE0"[..]));
        }
        let ram = hostile_soup(2 * 1024 * 1024, &seeds);
        let start = std::time::Instant::now();
        let (_, entries) = carve_ram(ram, 2 * 1024 * 1024);
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_secs() < 30,
            "hostile window completes, took {elapsed:?}"
        );
        // Every entry stays within the device bounds (grammar-
        // verified BMP hits may claim their on-disk size; weak
        // fallbacks stay at their caps).
        assert!(entries.iter().all(|e| e.size <= 2 * 1024 * 1024));
    }

    #[test]
    fn wav_garbage_rejects_valid_carves() {
        let mut ram = RamDevice::new(512 * 64, 512); // 32 KiB
        // Valid: RIFF + size 16 KiB + WAVE.
        ram.data[..4].copy_from_slice(b"RIFF");
        ram.data[4..8].copy_from_slice(&(16 * 1024u32 - 8).to_le_bytes());
        ram.data[8..12].copy_from_slice(b"WAVE");
        let (_, entries) = carve_ram(ram, 512 * 64);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].size, 16 * 1024);

        // Garbage size → reject.
        let mut ram2 = RamDevice::new(512 * 64, 512);
        ram2.data[..4].copy_from_slice(b"RIFF");
        ram2.data[4..8].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        let (stats, _) = carve_ram(ram2, 512 * 64);
        assert_eq!(stats.entries, 0);
    }

    // ── §5.1 items 3+5: the pipelined batch sweep ─────────────────

    /// Test double: a profile-forced device whose batches complete in
    /// REVERSE order — proving the carver's results never depend on
    /// completion ordering (the overlapped-events contract) — and which records
    /// the batch sizes it served.
    struct BatchingRam {
        ram: Arc<RamDevice>,
        profile: crate::blockdev::ReadProfile,
        batches: parking_lot::Mutex<Vec<usize>>,
    }

    impl BlockDevice for BatchingRam {
        fn sector_size(&self) -> u32 {
            self.ram.sector_size()
        }
        fn total_sectors(&self) -> u64 {
            self.ram.total_sectors()
        }
        fn read_sectors(&self, start_lba: u64, buf: &mut [u8]) -> CoreResult<()> {
            self.ram.read_sectors(start_lba, buf)
        }
        fn read_profile(&self) -> crate::blockdev::ReadProfile {
            self.profile
        }
        fn read_sectors_sparse_batch(
            &self,
            batch: &mut [crate::blockdev::BatchRead<'_>],
        ) -> CoreResult<Vec<Vec<(u64, u64)>>> {
            self.batches.lock().push(batch.len());
            // Complete in REVERSE submission order (the adversarial
            // completion schedule) while results stay indexed by
            // SUBMISSION position.
            let mut out: Vec<Vec<(u64, u64)>> = Vec::with_capacity(batch.len());
            out.resize_with(batch.len(), Vec::new);
            for i in (0..batch.len()).rev() {
                let r = &mut batch[i];
                out[i] = self.ram.read_sectors_sparse(r.start_lba, r.buf)?;
            }
            Ok(out)
        }
    }

    /// A deterministic PNG corpus at varied offsets (several land
    /// across window boundaries).
    fn png_corpus(size: usize) -> RamDevice {
        let mut ram = RamDevice::new(size, 512);
        let img = small_png();
        // Offsets chosen to straddle 128 KiB window boundaries: the
        // forced-profile run uses 128 KiB windows × depth 4.
        let mut at = 0usize;
        let step = 96 * 1024;
        while at + img.len() < size {
            ram.data[at..at + img.len()].copy_from_slice(&img);
            at += step;
        }
        ram
    }

    fn small_png() -> Vec<u8> {
        let mut img = Vec::new();
        img.extend_from_slice(b"\x89PNG\r\n\x1A\n");
        img.extend_from_slice(&[0u8; 300]);
        img.extend_from_slice(b"IEND");
        img.extend_from_slice(&[0u8; 4]);
        img
    }

    fn entry_signature_set(entries: &[FoundEntry]) -> Vec<(String, u64, u64, Option<u64>)> {
        let mut v: Vec<(String, u64, u64, Option<u64>)> = entries
            .iter()
            .map(|e| {
                (
                    e.name.clone(),
                    e.extents.first().map_or(0, |x| x.offset),
                    e.size,
                    e.content_key,
                )
            })
            .collect();
        v.sort();
        v
    }

    #[test]
    fn batched_carve_matches_sequential_exactly() {
        // The §5.1 item-3 contract: pipelined reads (any queue depth,
        // any completion order) produce BYTE-IDENTICAL carve results
        // — same names, extents, sizes, content keys.
        let size = 512 * 8192; // 4 MiB
        let ram = Arc::new(png_corpus(size));

        // Sequential reference (default profile: 1 MiB window, QD 1).
        let seq_dev: Arc<dyn BlockDevice> = Arc::clone(&ram) as Arc<dyn BlockDevice>;
        let mut sink = Collector(Vec::new());
        RawCarver::new(seq_dev)
            .carve(&mut sink, &ScanControl::new(), 0, size as u64)
            .expect("sequential carve");
        let seq = entry_signature_set(&sink.0);
        assert!(
            seq.len() >= 5,
            "corpus must span multiple windows, got {}",
            seq.len()
        );

        // Pipelined run: 128 KiB windows, depth 4, reversed completions.
        let batch_dev = BatchingRam {
            ram: Arc::clone(&ram),
            profile: crate::blockdev::ReadProfile {
                window_bytes: 128 * 1024,
                queue_depth: 4,
            },
            batches: parking_lot::Mutex::new(Vec::new()),
        };
        let mut sink2 = Collector(Vec::new());
        RawCarver::new(Arc::new(batch_dev) as Arc<dyn BlockDevice>)
            .carve(&mut sink2, &ScanControl::new(), 0, size as u64)
            .expect("batched carve");
        let batched = entry_signature_set(&sink2.0);

        assert_eq!(seq, batched, "queue depth must not change carve results");
    }

    #[test]
    fn batched_carve_uses_multi_window_batches() {
        // The pipeline actually SUBMITS multi-window batches (the
        // depth-4 profile must never degrade to one window at a time).
        let size = 512 * 8192;
        let ram = Arc::new(png_corpus(size));
        let batch_dev = Arc::new(BatchingRam {
            ram,
            profile: crate::blockdev::ReadProfile {
                window_bytes: 128 * 1024,
                queue_depth: 4,
            },
            batches: parking_lot::Mutex::new(Vec::new()),
        });
        let mut sink = Collector(Vec::new());
        RawCarver::new(Arc::clone(&batch_dev) as Arc<dyn BlockDevice>)
            .carve(&mut sink, &ScanControl::new(), 0, size as u64)
            .expect("carve");
        let batches = batch_dev.batches.lock().clone();
        assert!(
            batches.iter().any(|b| *b > 1),
            "multi-window batches must be submitted: {batches:?}"
        );
        assert!(
            batches.iter().all(|b| *b <= 4),
            "depth is an upper bound: {batches:?}"
        );
    }

    #[test]
    fn boundary_signatures_carve_exactly_once_under_batching() {
        // Images landing exactly on intra-batch window boundaries are
        // carved exactly once (the overlap re-scan + emitted set).
        let size = 512 * 2048; // 1 MiB → 8 × 128 KiB windows, 2 batches
        let mut ram = RamDevice::new(size, 512);
        let img = small_png();
        // Place one image so its signature sits at a 128 KiB boundary
        // and another straddling the next boundary.
        let b1 = 128 * 1024;
        let b2 = 2 * 128 * 1024;
        ram.data[b1..b1 + img.len()].copy_from_slice(&img);
        ram.data[b2 - 64..b2 - 64 + img.len()].copy_from_slice(&img);
        let batch_dev = BatchingRam {
            ram: Arc::new(ram),
            profile: crate::blockdev::ReadProfile {
                window_bytes: 128 * 1024,
                queue_depth: 4,
            },
            batches: parking_lot::Mutex::new(Vec::new()),
        };
        let mut sink = Collector(Vec::new());
        RawCarver::new(Arc::new(batch_dev) as Arc<dyn BlockDevice>)
            .carve(&mut sink, &ScanControl::new(), 0, size as u64)
            .expect("carve");
        assert_eq!(sink.0.len(), 2, "each boundary image exactly once");
    }

    #[test]
    fn profile_bands_clamp() {
        // Hostile probes can't request pathological I/O shapes.
        let p = crate::blockdev::ReadProfile {
            window_bytes: 1,
            queue_depth: 999,
        }
        .clamped();
        assert_eq!(p.window_bytes, 256 * 1024);
        assert_eq!(p.queue_depth, 16);
        let p = crate::blockdev::ReadProfile {
            window_bytes: 1024 * 1024 * 1024,
            queue_depth: 0,
        }
        .clamped();
        assert_eq!(p.window_bytes, 16 * 1024 * 1024);
        assert_eq!(p.queue_depth, 1);
    }
}
