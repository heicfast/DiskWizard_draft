//! JPEG repair — the `JPEG::CJpegInfo` marker-grammar model.
//!
//! Research: `artifacts/FileRepair/image-repair--format-grammar.md` §4
//! + §5 (the LibRaw LJPEG reader family the vendor reuses):
//!
//! - The repair constructor "seeds a minimal valid skeleton"
//!   (`FFD8` SOI … `FFD9` EOI) "onto which recovered marker segments
//!   are spliced" — our rebuild is exactly that: re-derive the marker
//!   chain, drop corrupt markers, re-sync at the next valid marker
//!   after damage, re-emit a canonical marker stream around the
//!   entropy-coded data.
//! - Truncated files get a synthesized EOI (the decodable-prefix
//!   strategy).
//! - Bounds from the vendor's parser: a **0x400 marker-count sanity
//!   bound** and **0x7FFF length clamps**.
//!
//! Marker semantics (the LJPEG state machine): SOI/EOI/RSTn/TEM are
//! standalone; SOS carries the scan header then entropy data which
//! ends at the next non-RST marker; everything else is `FF xx` + a
//! big-endian u16 length ≥ 2.

use crate::{
    DamageKind, DamageReport, Format, RepairEngine, RepairOutcome, RepairTier, salvage_detail,
};

/// The vendor's marker-count sanity bound (0x400).
const MAX_MARKERS: usize = 0x400;
/// The vendor's segment-length clamp (0x7FFF).
const MAX_LEN: usize = 0x7FFF;
/// SOI.
const SOI: u8 = 0xD8;
/// EOI.
const EOI: u8 = 0xD9;
/// Start of scan.
const SOS: u8 = 0xDA;
/// SOF0..SOF15 minus DHT(C4)/JPG(C8)/DAC(CC).
const SOF_SET: [u8; 12] = [
    0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE,
];
/// Quantization table.
const DQT: u8 = 0xDB;
/// Huffman table.
const DHT: u8 = 0xC4;
/// Standalone markers (TEM + RST0..RST7).
const STANDALONE: [u8; 9] = [0x01, 0xD0, 0xD1, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7];

/// One recovered marker segment (the intrusive-list node of the
/// vendor's inventory, as a plain range).
#[derive(Debug, Clone)]
struct Segment {
    /// Offset of the 0xFF marker byte.
    start: usize,
    /// End offset (exclusive): the byte after the segment.
    end: usize,
    kind: u8,
    /// True when this is the entropy-coded scan payload of a SOS.
    scan: bool,
}

/// The JPEG repair engine.
pub struct JpegEngine;

impl RepairEngine for JpegEngine {
    fn format(&self) -> Format {
        Format::Jpeg
    }

    fn check_magic(&self, buf: &[u8]) -> bool {
        // SOI then any marker (FFD8 FF) — or a bare FFD8 at EOF.
        buf.len() >= 3 && buf[0] == 0xFF && buf[1] == SOI && buf[2] == 0xFF
            || buf.len() == 2 && buf[0] == 0xFF && buf[1] == SOI
    }

    fn analyze(&self, buf: &[u8]) -> DamageReport {
        let mut damage = Vec::new();
        let segs = walk(buf, &mut damage);
        let found_soi = !buf.is_empty() && buf[0] == 0xFF && buf.get(1) == Some(&SOI);
        let found_eoi = segs.last().is_some_and(|s| s.kind == EOI);
        let found_sof = segs.iter().any(|s| SOF_SET.contains(&s.kind));
        let found_sos = segs.iter().any(|s| s.kind == SOS);
        let found_dht = segs.iter().any(|s| s.kind == DHT);
        let found_dqt = segs.iter().any(|s| s.kind == DQT);

        if !found_soi {
            damage.push(DamageKind::MissingHeader);
        }
        if !found_eoi {
            damage.push(DamageKind::Truncated);
        }
        if !found_sof || !found_sos || !found_dht || !found_dqt {
            damage.push(DamageKind::BadMarker);
        }

        let healthy = damage.is_empty();
        let detail = if healthy {
            "JPEG parses clean: SOI, frame, tables, scan and EOI all present.".to_string()
        } else {
            let mut notes = Vec::new();
            if !found_soi {
                notes.push("SOI missing");
            }
            if !found_sof {
                notes.push("no frame header (SOF)");
            }
            if !found_dht {
                notes.push("no Huffman tables (DHT)");
            }
            if !found_dqt {
                notes.push("no quantization tables (DQT)");
            }
            if !found_sos {
                notes.push("no scan (SOS)");
            }
            if !found_eoi {
                notes.push("truncated (no EOI)");
            }
            format!("JPEG damage: {}.", notes.join(", "))
        };

        DamageReport {
            format: Format::Jpeg,
            healthy,
            tier: if healthy {
                None
            } else {
                Some(RepairTier::Basis)
            },
            damage,
            detail,
        }
    }

    fn repair_into(&self, buf: &[u8], out: &mut Vec<u8>) -> RepairOutcome {
        let mut damage = Vec::new();
        let segs = walk(buf, &mut damage);
        let found_soi = !buf.is_empty() && buf[0] == 0xFF && buf.get(1) == Some(&SOI);
        let found_eoi = segs.last().is_some_and(|s| s.kind == EOI);
        let found_sos = segs.iter().any(|s| s.kind == SOS);

        // The skeleton: SOI … recovered segments … EOI (the vendor's
        // ctor template onto which segments are spliced).
        out.clear();
        out.extend_from_slice(&[0xFF, SOI]);
        let mut scan_bytes = 0usize;
        for seg in &segs {
            if seg.kind == SOI || seg.kind == EOI {
                continue; // re-emitted by the skeleton
            }
            let end = seg.end.min(buf.len());
            if seg.start < end {
                out.extend_from_slice(&buf[seg.start..end]);
                if seg.scan {
                    scan_bytes += end - seg.start;
                }
            }
        }
        out.extend_from_slice(&[0xFF, EOI]);

        let total_scan = segs
            .iter()
            .filter(|s| s.scan)
            .map(|s| s.end.saturating_sub(s.start))
            .sum::<usize>();
        // The decodable-skeleton contract: without a frame header
        // (SOF), the scan (SOS), Huffman (DHT) and quantization
        // (DQT) tables, the marker stream cannot decode — no
        // container-level repair exists (the salvage would lie).
        let found_sof = segs.iter().any(|s| SOF_SET.contains(&s.kind));
        let found_dqt = segs.iter().any(|s| s.kind == DQT);
        let found_dht = segs.iter().any(|s| s.kind == DHT);
        let decodable = found_sof && found_sos && found_dht && found_dqt;
        let repaired = decodable && (scan_bytes > 0 || !segs.is_empty());
        let detail = if !decodable {
            let missing = [
                (!found_sof).then_some("frame header (SOF)"),
                (!found_dqt).then_some("quantization tables (DQT)"),
                (!found_dht).then_some("Huffman tables (DHT)"),
                (!found_sos).then_some("scan (SOS)"),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ");
            format!("JPEG BeyondRepair: {missing} destroyed — no container-level repair exists.")
        } else if found_soi && found_eoi && damage.is_empty() {
            "Canonical re-emit: marker chain validated, bytes preserved.".to_string()
        } else {
            let mut notes = Vec::new();
            if !found_soi {
                notes.push("SOI synthesized");
            }
            if !found_eoi {
                notes.push("EOI synthesized (decodable prefix)");
            }
            if damage
                .iter()
                .any(|d| matches!(d, DamageKind::Corruption | DamageKind::BadMarker))
            {
                notes.push("corrupt markers dropped, resynced");
            }
            let pct_note = if !found_eoi && total_scan > 0 {
                format!(
                    " ({})",
                    salvage_detail("scan", scan_bytes as u64, total_scan as u64)
                )
            } else {
                String::new()
            };
            format!("JPEG repair: {}.{pct_note}", notes.join(", "))
        };

        RepairOutcome {
            repaired,
            input_bytes: buf.len() as u64,
            output_bytes: out.len() as u64,
            partial: !found_eoi,
            detail,
        }
    }
}

/// The marker walk — corruption-tolerant, bounded, resyncing.
///
/// Reports [`DamageKind::Corruption`] for every resync it performs.
fn walk(buf: &[u8], damage: &mut Vec<DamageKind>) -> Vec<Segment> {
    let mut segs = Vec::new();
    if buf.len() < 2 {
        damage.push(DamageKind::Truncated);
        return segs;
    }
    let mut i = 0usize;
    let mut had_soi = buf[0] == 0xFF && buf[1] == SOI;
    if !had_soi {
        damage.push(DamageKind::MissingHeader);
        // Search a bounded window for the real SOI.
        if let Some(p) = find_marker(buf, 2, SOI) {
            i = p;
            had_soi = true;
        } else {
            // No SOI at all: walk from the first FF-marker we can
            // find (the skeleton re-emits SOI regardless).
            i = buf.iter().position(|&b| b == 0xFF).unwrap_or(buf.len());
        }
        if i >= buf.len() {
            damage.push(DamageKind::Corruption);
            return segs;
        }
    }
    if had_soi {
        i += 2; // past SOI
    }
    while i < buf.len() && segs.len() < MAX_MARKERS {
        if buf[i] != 0xFF {
            damage.push(DamageKind::Corruption);
            match next_ff(buf, i) {
                Some(p) => {
                    i = p;
                    continue;
                }
                None => break,
            }
        }
        if i + 1 >= buf.len() {
            // A trailing 0xFF with no marker byte — truncation.
            damage.push(DamageKind::Truncated);
            break;
        }
        let kind = buf[i + 1];
        if kind == 0xFF || kind == 0x00 {
            // Not a real marker (stuffing or garbage) — resync.
            damage.push(DamageKind::Corruption);
            match next_ff(buf, i + 2) {
                Some(p) => {
                    i = p;
                    continue;
                }
                None => break,
            }
        }
        if kind == EOI {
            segs.push(Segment {
                start: i,
                end: i + 2,
                kind,
                scan: false,
            });
            // Anything after EOI is trailing garbage — ignored
            // (embedded-JPEG carriers append payloads; the vendor's
            // CScanJepgInfo handles those separately).
            break;
        }
        if STANDALONE.contains(&kind) {
            segs.push(Segment {
                start: i,
                end: i + 2,
                kind,
                scan: false,
            });
            i += 2;
            continue;
        }
        if i + 4 > buf.len() {
            // Truncated inside a marker header.
            damage.push(DamageKind::Truncated);
            break;
        }
        let len = u16::from_be_bytes([buf[i + 2], buf[i + 3]]) as usize;
        if !(2..=MAX_LEN).contains(&len) || i + 2 + len > buf.len() {
            // Bad length: clamp when it only slightly overruns
            // (truncation), resync when it is garbage.
            if len >= 2 && i + 2 + len > buf.len() && len <= MAX_LEN {
                segs.push(Segment {
                    start: i,
                    end: buf.len(),
                    kind,
                    scan: false,
                });
                damage.push(DamageKind::Truncated);
                break;
            }
            damage.push(DamageKind::BadMarker);
            match next_ff(buf, i + 4) {
                Some(p) => {
                    i = p;
                    continue;
                }
                None => break,
            }
        }
        let end = i + 2 + len;
        if kind == SOS {
            // The scan: entropy data runs until the next marker that
            // is not an RST (the LJPEG scan-termination rule).
            let scan_end = scan_terminator(buf, end);
            segs.push(Segment {
                start: i,
                end: scan_end,
                kind,
                scan: true,
            });
            if scan_end >= buf.len() {
                damage.push(DamageKind::Truncated);
                break;
            }
            i = scan_end;
            continue;
        }
        segs.push(Segment {
            start: i,
            end,
            kind,
            scan: false,
        });
        i = end;
    }
    segs
}

/// Find the next 0xFF that begins a plausible marker at/after `from`.
fn next_ff(buf: &[u8], from: usize) -> Option<usize> {
    let mut p = from;
    while p + 1 < buf.len() {
        if buf[p] == 0xFF && buf[p + 1] != 0x00 && buf[p + 1] != 0xFF {
            return Some(p);
        }
        p += 1;
    }
    None
}

/// Locate a specific standalone marker (`SOI`/`EOI`-class) at/after
/// `from` — the bounded content search.
fn find_marker(buf: &[u8], from: usize, kind: u8) -> Option<usize> {
    let mut p = from;
    while p + 1 < buf.len() {
        if buf[p] == 0xFF && buf[p + 1] == kind {
            return Some(p);
        }
        p += 1;
    }
    None
}

/// The entropy scan terminator: first 0xFF not followed by 0x00
/// (stuffing) and not an RST marker.
fn scan_terminator(buf: &[u8], from: usize) -> usize {
    let mut p = from;
    while p + 1 < buf.len() {
        if buf[p] == 0xFF {
            let next = buf[p + 1];
            if next == 0x00 {
                p += 2;
                continue;
            }
            if (0xD0..=0xD7).contains(&next) || next == 0xFF {
                p += 1;
                continue;
            }
            return p;
        }
        p += 1;
    }
    buf.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid JPEG: SOI, DQT, SOF0, DHT, SOS, entropy, EOI.
    fn build_jpeg(entropy: &[u8]) -> Vec<u8> {
        let mut v = vec![0xFF, SOI];
        // DQT: len 67 (2 + 64+1 payload)
        v.extend_from_slice(&[0xFF, DQT, 0x00, 67]);
        v.extend_from_slice(&[0u8; 65]);
        // SOF0: len 11 (2 + 9: precision, h, w, ncomp, 3×1)
        v.extend_from_slice(&[0xFF, 0xC0, 0x00, 11, 8, 0, 1, 0, 1, 1, 1, 0x11, 0]);
        // DHT: len 22
        v.extend_from_slice(&[0xFF, DHT, 0x00, 22]);
        v.extend_from_slice(&[0u8; 20]);
        // SOS: len 8 + 1 component
        v.extend_from_slice(&[0xFF, SOS, 0x00, 8, 1, 1, 0x11, 0, 63, 0]);
        v.extend_from_slice(entropy);
        // Stuffed FF00 pair inside entropy must survive.
        v.extend_from_slice(&[0xFF, EOI]);
        v
    }

    #[test]
    fn healthy_file_passes_through() {
        let jpeg = build_jpeg(&[0x12, 0x34, 0x56, 0xFF, 0x00, 0x78]);
        let e = JpegEngine;
        assert!(e.check_magic(&jpeg));
        let r = e.analyze(&jpeg);
        assert!(r.healthy, "detail: {}", r.detail);
        let mut out = Vec::new();
        let o = e.repair_into(&jpeg, &mut out);
        assert!(o.repaired);
        assert!(!o.partial);
        assert_eq!(out, jpeg, "canonical re-emit is byte-exact");
    }

    #[test]
    fn truncated_gets_synthesized_eoi() {
        let jpeg = build_jpeg(&[0xAA; 200]);
        let cut = &jpeg[..jpeg.len() - 2];
        let e = JpegEngine;
        let r = e.analyze(cut);
        assert!(!r.healthy);
        assert!(r.damage.contains(&DamageKind::Truncated));
        let mut out = Vec::new();
        let o = e.repair_into(cut, &mut out);
        assert!(o.repaired);
        assert!(o.partial);
        assert_eq!(&out[out.len() - 2..], &[0xFF, EOI]);
        // The entropy prefix is preserved byte-exact.
        assert!(
            out.windows(6)
                .any(|w| w == [0xAA, 0xAA, 0xAA, 0xAA, 0xFF, EOI])
        );
    }

    #[test]
    fn corrupt_marker_is_dropped_and_resynced() {
        let jpeg = build_jpeg(&[0xAB; 300]);
        // Insert a corrupt APP0 segment between SOI and DQT: its
        // length is garbage — the walk must drop it and resync
        // WITHOUT losing the decodable skeleton (SOF/DQT/DHT/SOS).
        let mut damaged = vec![0xFF, SOI, 0xFF, 0xE0, 0xFF, 0xFF, 0x11, 0x22];
        damaged.extend_from_slice(&jpeg[2..]);
        let e = JpegEngine;
        let mut out = Vec::new();
        let o = e.repair_into(&damaged, &mut out);
        assert!(o.repaired, "{}", o.detail);
        assert!(out.starts_with(&[0xFF, SOI]));
        assert!(out.ends_with(&[0xFF, EOI]));
        // The SOF must still be present (resync recovered it).
        assert!(out.windows(2).any(|w| w == [0xFF, 0xC0]));
        let r = e.analyze(&damaged);
        assert!(r.damage.contains(&DamageKind::BadMarker));
    }

    #[test]
    fn destroyed_dqt_is_honestly_beyond_repair() {
        let mut jpeg = build_jpeg(&[0xAB; 300]);
        // Corrupt the DQT segment's length to garbage — DQT is
        // dropped and without quantization tables no container-level
        // repair exists (the honesty contract).
        jpeg[4] = 0xFF;
        jpeg[5] = 0xFF;
        let e = JpegEngine;
        let mut out = Vec::new();
        let o = e.repair_into(&jpeg, &mut out);
        assert!(!o.repaired);
        assert!(o.detail.contains("BeyondRepair"));
        assert!(o.detail.contains("quantization tables"));
    }

    #[test]
    fn missing_soi_is_resynthesized() {
        let jpeg = build_jpeg(&[0x11; 50]);
        let cut = &jpeg[2..];
        let e = JpegEngine;
        let r = e.analyze(cut);
        assert!(r.damage.contains(&DamageKind::MissingHeader));
        let mut out = Vec::new();
        let o = e.repair_into(cut, &mut out);
        assert!(o.repaired);
        assert!(out.starts_with(&[0xFF, SOI]));
    }

    #[test]
    fn marker_count_is_bounded() {
        // A pathological stream of TEM markers — the walk must bound
        // AND the verdict must be honest (no SOF/tables/scan = no
        // decodable skeleton, whatever the marker count).
        let mut v = vec![0xFF, SOI];
        for _ in 0..20_000 {
            v.extend_from_slice(&[0xFF, 0x01]);
        }
        let e = JpegEngine;
        let mut out = Vec::new();
        let o = e.repair_into(&v, &mut out);
        assert!(!o.repaired);
        assert!(o.detail.contains("BeyondRepair"));
        // 0x400 bound + skeleton.
        assert!(out.len() < 0x400 * 2 + 16);
    }
}
