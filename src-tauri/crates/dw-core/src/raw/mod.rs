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
//! Memory discipline: windows are fixed-size (8 MiB), grammar walks
//! stream through bounded chunks ([`Prober`]) — no validator ever
//! materializes a whole file, so a 4 GiB `max_size` bound costs a few
//! hundred KiB, not gigabytes (the caps are LENGTH bounds, not
//! allocations).
//!
//! Analyser set (research: `artifacts/SearchDLL/
//! rawsearchdll--analyser-registry-census.md`): JPEG, PNG, GIF, BMP,
//! ZIP (central-directory walk), RAR4/5, PDF, MP4/MOV/HEIC
//! (ISO-BMFF), MKV (EBML), MP3, WAV, OGG, FLAC, OLE2 (doc/xls/ppt),
//! SQLite, PST.

mod signatures;

use std::sync::Arc;

use memchr::memmem;

use crate::blockdev::{AlignedBuf, BatchRead, BlockDevice, BlockRead};
use crate::error::CoreResult;
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

/// Window overlap: 64 KiB re-scanned per advance so signatures
/// straddling a window boundary are never missed (the longest magic
/// is 8 bytes; validators read from the device independently).
const CARVE_OVERLAP: usize = 64 * 1024;

/// Prober chunk: the fixed ceiling of any single validator scratch
/// allocation.
const PROBE_CHUNK: usize = 1024 * 1024;

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

    /// Carve `[start_byte, end_byte)` (the deep phase's sweep).
    ///
    /// The pipelined sweep (§5.1 items 3+5): the device's adaptive
    /// profile decides the window width and how many windows ride in
    /// one [`BlockDevice::read_sectors_sparse_batch`] — one at a time
    /// on unknown/HDD-class media, up to 16 in flight on flash. Media
    /// damage degrades to per-sector salvage inside the platform layer
    /// and shows up here as zeroed bad ranges whose hits are skipped —
    /// the sweep itself never stalls on a dying disk. Completion order
    /// inside a batch is irrelevant: windows are processed in offset
    /// order with the same emitted-position set as the sequential
    /// sweep, so results are byte-identical at every queue depth.
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
        let mut stats = ScanStats::default();
        let ss = u64::from(self.dev.sector_size()).max(1);
        let cap = self.dev.capacity_bytes();
        let end = end_byte.min(cap);
        let mut cursor = start_byte.min(end);
        if end == cursor {
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
        // clamped, then the window shrinks to the remaining span, then
        // the depth shrinks to the buffer budget. Ram fixtures and
        // tiny volumes end at single-window batches — identical to
        // the sequential sweep.
        let prof = self.dev.read_profile().clamped();
        let span = usize::try_from(end - cursor).unwrap_or(usize::MAX).max(1);
        let window = prof.window_bytes.min(span).max(ss as usize);
        let depth = prof
            .queue_depth
            .min((MAX_WINDOW_BUDGET / window).max(1))
            .max(1);
        let mut bufs: Vec<AlignedBuf> = (0..depth)
            .map(|_| AlignedBuf::zeroed(window, self.dev.preferred_read_alignment()))
            .collect();
        // Positions already emitted this carve (absolute): "first
        // signature wins a position" — while the SAME signature keeps
        // scanning for further hits (one file per signature PER
        // WINDOW was the old break bug).
        let mut emitted: HashSet<u64> = HashSet::new();
        'batches: while cursor < end {
            ctrl.checkpoint()?;
            // Plan this batch: consecutive windows chained by the
            // overlap advance — exactly the sequential sweep's math,
            // submitted together.
            let mut plan: Vec<(u64, usize)> = Vec::with_capacity(depth);
            let mut c = cursor;
            while plan.len() < depth && c < end {
                let want = (end - c).min(window as u64);
                // Sector-multiple read length (trait contract), ≥1 sector.
                let take = (want as usize).next_multiple_of(ss as usize).min(window);
                let take = take.max(ss as usize).min(window);
                let take = (take as u64).min(end - c) as usize;
                let take = take.max(ss as usize);
                plan.push((c, take));
                if c + take as u64 >= end {
                    break; // the final window is in this batch
                }
                let advance = (take - CARVE_OVERLAP.min(take / 2)).max(1);
                c += advance as u64;
            }
            // Pipeline the batch: `depth` windows in flight at once on
            // IOCP-class devices; the sequential default elsewhere.
            let mut batch: Vec<BatchRead<'_>> = plan
                .iter()
                .zip(bufs.iter_mut())
                .map(|(&(off, take), b)| BatchRead {
                    start_lba: off / ss,
                    buf: &mut b[..take],
                })
                .collect();
            let results = self.dev.read_sectors_sparse_batch(&mut batch)?;
            drop(batch); // release the buffer borrows before scanning
            // Process in OFFSET order (callers may not depend on the
            // completion order the device chose).
            for (i, &(off, take)) in plan.iter().enumerate() {
                ctrl.checkpoint()?;
                let is_last = off + take as u64 >= end;
                self.scan_window(
                    sink,
                    &bufs[i][..take],
                    &results[i],
                    off,
                    is_last,
                    end,
                    &finders,
                    &mut emitted,
                    &mut stats,
                );
                sink.on_progress(off + take as u64 - start_byte, end - start_byte);
            }
            let (last_off, last_take) = *plan.last().expect("plan has a window");
            if last_off + last_take as u64 >= end {
                break 'batches;
            }
            let advance = (last_take - CARVE_OVERLAP.min(last_take / 2)).max(1);
            cursor = last_off + advance as u64;
        }
        sink.on_progress(end - start_byte, end - start_byte);
        Ok(stats)
    }

    /// Scan one FILLED carve window: signature hunt over the new-hit
    /// region (everything but the overlap tail — the final window
    /// scans fully), grammar validation, entry emission. Extracted
    /// from the sequential loop so the batched pipeline reuses the
    /// exact per-window semantics.
    #[allow(clippy::too_many_arguments)] // one window-scan state pass
    fn scan_window(
        &self,
        sink: &mut dyn ScanSink,
        buf: &[u8],
        bad: &[(u64, u64)],
        window_start: u64,
        is_last: bool,
        end: u64,
        finders: &[(usize, &Signature, memmem::Finder<'static>)],
        emitted: &mut HashSet<u64>,
        stats: &mut ScanStats,
    ) {
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
                if let Some(mut h) = self.validate_hit(sig, abs, end) {
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
                        // Same-content key from the DEVICE bytes —
                        // deterministic regardless of window geometry
                        // (hashing window-relative bytes would make
                        // the key depend on where window boundaries
                        // fell). One bounded 4 KiB read per EMITTED
                        // candidate, never per raw hit; duplicate
                        // copies of one file share their true on-disk
                        // prefix and the tree marks repeats as
                        // Version — the essential-mode dedup.
                        let want = (h.length as usize).min(4096);
                        if want > 0 {
                            let mut prefix = vec![0u8; want];
                            if self.dev.read_bytes(abs, &mut prefix).is_ok() {
                                entry.content_key = Some(content_hash(&prefix, h.length));
                            }
                        }
                        entry.source = FsKind::Raw;
                        stats.entries += 1;
                        stats.bytes += entry.size;
                        emitted.insert(abs);
                        sink.on_entry(entry); // keep scanning —
                        // further hits of this signature and every
                        // other one still surface.
                    }
                }
            }
        }
    }

    /// Validate a signature hit: apply the format grammar and derive
    /// the length. `None` = the grammar rejected the magic (false
    /// positive) — several arms propagate inner `None`s.
    #[allow(clippy::unnecessary_wraps)] // mixed arms: Option propagates rejects
    fn validate_hit(&self, sig: &Signature, at: u64, end: u64) -> Option<CarvedHit> {
        let max_probe = sig.max_size.min(end.saturating_sub(at));
        let prober = Prober {
            dev: &*self.dev,
            base: at,
            limit: max_probe,
        };
        let hit = match sig.id {
            SignatureId::Jpeg => prober.size_jpeg(),
            SignatureId::Png => prober.size_tail(b"IEND", 8),
            SignatureId::Gif => prober.size_tail(&[0x3B], 1),
            SignatureId::Zip => prober.size_zip()?,
            SignatureId::Rar4 | SignatureId::Rar5 => prober.size_rar(),
            SignatureId::Pdf => prober.size_pdf(),
            SignatureId::Mp4 | SignatureId::Mov | SignatureId::Heic => prober.size_isobmff(),
            SignatureId::Mkv => prober.size_ebml(),
            SignatureId::Mp3 => prober.size_mp3(),
            SignatureId::Wav => prober.size_riff()?,
            SignatureId::Ogg => prober.size_ogg()?,
            SignatureId::Flac => prober.size_flac()?,
            SignatureId::Ole2 => prober.size_ole2(),
            SignatureId::Sqlite => prober.size_sqlite()?,
            SignatureId::Bmp => prober.size_bmp()?,
            SignatureId::Pst => CarvedHit {
                length: 512 * 1024, // PST grows in 512 KiB pages
                confidence: sig.confidence,
                category: Category::Email,
                dimensions: None,
            },
        };
        // Confidence never exceeds the signature's own bound.
        Some(CarvedHit {
            length: hit.length,
            confidence: hit.confidence.min(sig.confidence),
            category: hit.category,
            dimensions: hit.dimensions,
        })
    }
}

/// Bounded random-access probe helper for grammar walks: small
/// structured reads plus chunked pattern scans. Every allocation is
/// capped at [`PROBE_CHUNK`] — the `limit` bounds a LENGTH, never a
/// buffer.
struct Prober<'a> {
    dev: &'a dyn BlockDevice,
    /// Absolute offset of the carved candidate (the hit).
    base: u64,
    /// Maximum probe span (bytes from `base`).
    limit: u64,
}

impl Prober<'_> {
    /// Read `n` bytes at `base + rel` (damage-tolerant: unreadable
    /// regions read as zeros, like the old full-buffer behavior).
    fn read(&self, rel: u64, n: usize) -> Vec<u8> {
        let n = n.min(PROBE_CHUNK);
        let avail = self.limit.saturating_sub(rel).min(n as u64) as usize;
        let mut buf = vec![0u8; avail];
        let _ = self.dev.read_bytes(self.base + rel, &mut buf);
        buf
    }

    /// Big-endian u32 at `base + rel` (0 when unreadable).
    fn u32be(&self, rel: u64) -> u32 {
        let b = self.read(rel, 4);
        if b.len() == 4 {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        } else {
            0
        }
    }

    /// Little-endian u32 at `base + rel` (0 when unreadable).
    fn u32le(&self, rel: u64) -> u32 {
        let b = self.read(rel, 4);
        if b.len() == 4 {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        } else {
            0
        }
    }

    /// Scan `[from, to)` (relative) in bounded chunks for `needle`;
    /// return the LAST match end (rel + needle.len()), tracking at
    /// most one chunk in memory at a time.
    fn find_last(&self, needle: &[u8], from: u64, to: u64) -> Option<u64> {
        let mut last = None;
        let mut pos = from;
        while pos < to {
            let take = (to - pos).min(PROBE_CHUNK as u64) as usize;
            let chunk = self.read(pos, take + needle.len() - 1);
            if chunk.len() >= needle.len() {
                let finder = memmem::Finder::new(needle);
                let mut from2 = 0usize;
                while let Some(rel) = finder.find(&chunk[from2..]) {
                    let rel = from2 + rel;
                    last = Some(pos + rel as u64 + needle.len() as u64);
                    from2 = rel + 1;
                }
            }
            pos += take as u64;
        }
        last
    }

    /// Scan `[from, to)` for `needle`; return (count, last match end).
    fn count_last(&self, needle: &[u8], from: u64, to: u64) -> (u32, Option<u64>) {
        let mut last = None;
        let mut count = 0u32;
        let mut pos = from;
        while pos < to {
            let take = (to - pos).min(PROBE_CHUNK as u64) as usize;
            let chunk = self.read(pos, take + needle.len() - 1);
            if chunk.len() >= needle.len() {
                let finder = memmem::Finder::new(needle);
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
    fn rfind_from(&self, needle: &[u8], from: u64, to: u64) -> Option<u64> {
        let mut hi = to;
        while hi > from {
            let lo = hi.saturating_sub(64 * 1024).max(from);
            let chunk = self.read(lo, (hi - lo) as usize + needle.len() - 1);
            if chunk.len() >= needle.len() {
                let finder = memmem::Finder::new(needle);
                let mut best = None;
                let mut from2 = 0usize;
                while let Some(rel) = finder.find(&chunk[from2..]) {
                    let rel = from2 + rel;
                    best = Some(lo + rel as u64);
                    from2 = rel + 1;
                }
                if let Some(rel) = best {
                    return Some(rel);
                }
            }
            hi = lo;
        }
        None
    }

    /// PNG/GIF: last tail-marker within the cap (streamed).
    fn size_tail(&self, tail: &[u8], pad: u64) -> CarvedHit {
        const CAP: u64 = 256 * 1024 * 1024;
        let span = self.limit.min(CAP);
        match self.find_last(tail, 0, span) {
            Some(end) => hit(end + pad, 95, Category::Images, None),
            None => hit(span + pad, 55, Category::Images, None),
        }
    }

    /// ZIP (research: `rawsearchdll--zip-analyser-carving.md`):
    /// index-first — find the EOCD near the archive tail, validate
    /// the central-directory offsets, size = EOCD end. Archives with
    /// no EOCD (local-header-only streams) carve as a bounded
    /// low-confidence span.
    fn size_zip(&self) -> Option<CarvedHit> {
        const CAP: u64 = 1024 * 1024 * 1024;
        let span = self.limit.min(CAP);
        // The EOCD lives within the last ~66 KiB of a zip — scan
        // backward from the end, bounded to that window.
        let search_from = span.saturating_sub(66_000);
        let eocd = self.rfind_from(b"PK\x05\x06", search_from, span)?;
        let cd_size = u64::from(self.u32le(eocd + 12));
        let cd_off = u64::from(self.u32le(eocd + 16));
        if cd_off + cd_size <= eocd {
            return Some(hit(eocd + 22, 96, Category::Archives, None));
        }
        // Structurally invalid EOCD: bounded low-confidence carve.
        Some(hit(
            span.min(64 * 1024 * 1024),
            40,
            Category::Archives,
            None,
        ))
    }

    /// RAR4/RAR5: block walk to the end-of-archive marker (each block
    /// header read on demand — never the whole archive).
    fn size_rar(&self) -> CarvedHit {
        const CAP: u64 = 2 * 1024 * 1024 * 1024;
        let span = self.limit.min(CAP);
        let mut pos = 7u64; // past the marker
        while pos + 11 <= span {
            let btype = self.read(pos + 2, 1).first().copied().unwrap_or(0);
            let fl = self.read(pos + 3, 2);
            if fl.len() < 2 {
                break;
            }
            let flags = u16::from_le_bytes([fl[0], fl[1]]);
            let sz = self.read(pos + 5, 2);
            if sz.len() < 2 {
                break;
            }
            let mut size = u64::from(u16::from_le_bytes([sz[0], sz[1]]));
            if flags & 0x8000 != 0 {
                let hi = self.read(pos + 7, 4);
                if hi.len() == 4 {
                    size += u64::from(u32::from_le_bytes([hi[0], hi[1], hi[2], hi[3]])) << 16;
                }
            }
            if size < 7 {
                break;
            }
            pos += size;
            if btype == 0x7B {
                // End-of-archive block.
                return hit(pos, 90, Category::Archives, None);
            }
        }
        hit(span, 50, Category::Archives, None)
    }

    /// PDF: %PDF → last %%EOF (streamed backward from the cap).
    fn size_pdf(&self) -> CarvedHit {
        const CAP: u64 = 2 * 1024 * 1024 * 1024;
        let span = self.limit.min(CAP);
        match self.rfind_from(b"%%EOF", 0, span) {
            Some(end) => hit(end + 5, 94, Category::Documents, None),
            None => hit(span, 45, Category::Documents, None),
        }
    }

    /// ISO-BMFF (MP4/MOV/HEIC): 32/64-bit box walk — each box header
    /// is an 8-16 byte read; jumps are arithmetic, not allocations.
    fn size_isobmff(&self) -> CarvedHit {
        const CAP: u64 = 4 * 1024 * 1024 * 1024;
        let span = self.limit.min(CAP);
        let mut pos = 0u64;
        let mut sane = false;
        while pos + 8 <= span {
            let size = u64::from(self.u32be(pos));
            let box_size = if size == 1 && pos + 16 <= span {
                let b = self.read(pos + 8, 8);
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
        hit(pos, if sane { 90 } else { 40 }, Category::Videos, None)
    }

    /// EBML (MKV/WebM): vint-sized element walk (header reads on
    /// demand).
    fn size_ebml(&self) -> CarvedHit {
        const CAP: u64 = 4 * 1024 * 1024 * 1024;
        let span = self.limit.min(CAP);
        let vint = |p: &Self, pos: u64| -> Option<(u64, usize)> {
            let first = p.read(pos, 1).first().copied()?;
            let len = first.leading_ones() as usize;
            if len == 0 || len > 8 || pos + len as u64 > span {
                return None;
            }
            let rest = p.read(pos + 1, len - 1);
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
        while pos < span {
            let Some((_, id_len)) = vint(self, pos) else {
                break;
            };
            let Some((size, size_len)) = vint(self, pos + id_len as u64) else {
                break;
            };
            let elem_len = id_len as u64 + size_len as u64 + size;
            if pos + elem_len > span {
                break;
            }
            sane = true;
            pos += elem_len;
        }
        hit(pos, if sane { 85 } else { 40 }, Category::Videos, None)
    }

    /// MP3: ID3v2 header size or a bounded frame-sync estimate.
    fn size_mp3(&self) -> CarvedHit {
        let data = self.read(0, 4096);
        if data.len() > 10 && &data[..3] == b"ID3" {
            let sz = (u64::from(data[6]) << 21)
                | (u64::from(data[7]) << 14)
                | (u64::from(data[8]) << 7)
                | u64::from(data[9]);
            return hit(sz + 10, 85, Category::Audio, None);
        }
        hit(self.limit.min(16 * 1024 * 1024), 55, Category::Audio, None)
    }

    /// RIFF/WAV: the header's own size field.
    fn size_riff(&self) -> Option<CarvedHit> {
        let data = self.read(0, 4096);
        if data.len() >= 8 {
            let riff_size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
            let total = u64::from(riff_size) + 8;
            if total >= 44 && total <= self.limit {
                return Some(hit(total, 92, Category::Audio, None));
            }
            return Some(hit(self.limit, 40, Category::Audio, None));
        }
        None
    }

    /// OGG page-walk / repeated-marker scan (≥2 markers = a stream).
    fn size_ogg(&self) -> Option<CarvedHit> {
        const CAP: u64 = 256 * 1024 * 1024;
        let span = self.limit.min(CAP);
        let (count, last) = self.count_last(b"OggS", 0, span);
        (count >= 2).then(|| hit(last.unwrap_or(span), 80, Category::Audio, None))
    }

    /// FLAC: STREAMINFO harvest + metadata-block walk.
    fn size_flac(&self) -> Option<CarvedHit> {
        let data = self.read(0, 4096);
        if data.len() > 4 && &data[..4] == b"fLaC" {
            let mut pos = 4usize;
            let mut dims = None;
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
                if last {
                    break;
                }
            }
            return Some(hit(
                self.limit.min(128 * 1024 * 1024),
                88,
                Category::Audio,
                dims,
            ));
        }
        None
    }

    /// OLE2 (doc/xls/ppt): the CFB header's FAT-sector bound.
    fn size_ole2(&self) -> CarvedHit {
        let data = self.read(0, 4096);
        if data.len() >= 0x34 {
            let sector_shift = u16::from_le_bytes([data[0x1E], data[0x1F]]);
            let num_fat = u32::from_le_bytes([data[0x2C], data[0x2D], data[0x2E], data[0x2F]]);
            let ss = 1u64 << u64::from(sector_shift).clamp(9, 12);
            let bound = (u64::from(num_fat) + 128) * ss;
            if bound > 0 && bound <= self.limit {
                return hit(bound, 85, Category::Documents, None);
            }
        }
        hit(self.limit, 40, Category::Documents, None)
    }

    /// SQLite: page_size × page_count from the header.
    fn size_sqlite(&self) -> Option<CarvedHit> {
        let data = self.read(0, 4096);
        if data.len() >= 32 {
            let page_size = u16::from_be_bytes([data[16], data[17]]);
            let page_size = if page_size == 1 {
                65_536
            } else {
                u64::from(page_size)
            };
            let pages = u32::from_be_bytes([data[28], data[29], data[30], data[31]]);
            let total = page_size * u64::from(pages.max(1));
            if total <= self.limit {
                return Some(hit(total, 90, Category::Other, None));
            }
            return Some(hit(self.limit, 55, Category::Other, None));
        }
        None
    }

    /// BMP: file-size field + DIB dimension validation.
    fn size_bmp(&self) -> Option<CarvedHit> {
        let data = self.read(0, 4096);
        if data.len() >= 26 {
            let file_size = u32::from_le_bytes([data[2], data[3], data[4], data[5]]);
            let header_size = u32::from_le_bytes([data[14], data[15], data[16], data[17]]);
            let w = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
            let h = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
            if file_size >= 26
                && u64::from(file_size) <= self.limit
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

    /// JPEG: FFD8 start → FFD9 EOI walk (SOF dimension harvest,
    /// restart-marker aware, truncated-tolerant) — streamed in 64 KiB
    /// chunks with a 4-byte straddle carry so markers spanning chunk
    /// edges parse correctly.
    fn size_jpeg(&self) -> CarvedHit {
        const CAP: u64 = 64 * 1024 * 1024;
        let span = self.limit.min(CAP);
        let mut dims = None;
        let mut chunk_base = 2u64; // past SOI
        loop {
            let take = (span - chunk_base).min(64 * 1024) as usize;
            if take == 0 {
                break;
            }
            let data = self.read(chunk_base, take + 3.min(take));
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
                    let sof = self.read(chunk_base + i as u64 + 4, 5);
                    if sof.len() >= 5 {
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
                    return hit(
                        chunk_base + i as u64 + 2,
                        if dims.is_some() { 92 } else { 88 },
                        Category::Images,
                        dims,
                    );
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
        // Truncated (no EOI within the cap): carve what we walked.
        hit(span, 55, Category::Images, dims)
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

    // ── §5.1 items 3+5: the pipelined batch sweep ─────────────────

    /// Test double: a profile-forced device whose batches complete in
    /// REVERSE order — proving the carver's results never depend on
    /// completion ordering (the IOCP contract) — and which records
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
