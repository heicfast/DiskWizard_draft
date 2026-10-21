//! PNG repair — the `CPngInfo` chunk-grammar model.
//!
//! Research: `artifacts/FileRepair/image-repair--format-grammar.md`
//! §12.2 (the fully decoded state machine):
//!
//! - **ScanCriticalChunks** is a *corruption-tolerant pattern search*,
//!   not a sequential walk: chunk positions are located by content
//!   (the chunk-name bytes), then hand-validated — "a damaged header
//!   or interleaved garbage cannot defeat analysis".
//! - **BasisRepair** (progress 20/30/50/80): scan critical chunks →
//!   hand (rebuild) the chunk stream → IHDR validation (width/height
//!   nonzero, bit-depth, color-type) → re-emit.
//! - **SampleRepair** (progress 15/25/61/85): "reconstructs a
//!   viewable PNG by synthesizing a header around the salvaged zlib
//!   IDAT stream — the extract-what's-decodable strategy".
//! - The IEND search is for "the literal end-of-PNG 12 bytes, not a
//!   parsed IEND" — and the ctor's table carries **IEND's canonical
//!   CRC 0xAE426082**.
//! - APNG is first-class: acTL/fcTL/fdAT are in the core chunk table.

use crate::{
    DamageKind, DamageReport, Format, RepairEngine, RepairOutcome, RepairTier, salvage_detail,
};

/// PNG signature: 89 50 4E 47 0D 0A 1A 0A.
const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
/// The canonical IEND chunk bytes (name + zero CRC) — the vendor's
/// table stores this 12-byte literal.
const IEND_LITERAL: [u8; 12] = [
    0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
];
/// The maximum plausible chunk length (PNG spec: 2^31-1; we clamp
/// tighter for repair sanity — a payload larger than the file is
/// garbage by construction).
const MAX_CHUNK: u32 = 0x7FFF_FFFF;

/// One chunk located in the stream.
#[derive(Debug, Clone)]
struct Chunk {
    /// Offset of the 4-byte length field.
    start: usize,
    /// End offset (exclusive) of the whole chunk.
    end: usize,
    /// The 4-byte type.
    kind: [u8; 4],
    /// The length field as stored (may disagree with the extent —
    /// damage the rebuild fixes).
    stored_len: u32,
}

/// The PNG repair engine.
pub struct PngEngine;

impl RepairEngine for PngEngine {
    fn format(&self) -> Format {
        Format::Png
    }

    fn check_magic(&self, buf: &[u8]) -> bool {
        buf.len() >= 8 && buf[..8] == SIG || buf.len() >= 4 && buf[..4] == *b"IHDR" // headerless salvage case
    }

    fn analyze(&self, buf: &[u8]) -> DamageReport {
        let mut damage = Vec::new();
        let has_sig = buf.len() >= 8 && buf[..8] == SIG;
        if !has_sig {
            damage.push(DamageKind::MissingHeader);
        }
        let chunks = scan_chunks(buf, &mut damage);
        let has_ihdr = chunks.first().is_some_and(|c| c.kind == *b"IHDR");
        let has_idat = chunks.iter().any(|c| c.kind == *b"IDAT");
        let has_iend = chunks.iter().any(|c| c.kind == *b"IEND");
        if !has_ihdr {
            damage.push(DamageKind::MissingHeader);
        }
        if !has_idat {
            damage.push(DamageKind::BrokenIndex);
        }
        if !has_iend {
            damage.push(DamageKind::Truncated);
        }
        // CRC audit on the sequential chain.
        let mut crc_bad = 0usize;
        for c in &chunks {
            if c.end <= buf.len() && crc_mismatch(buf, c) {
                crc_bad += 1;
            }
        }
        if crc_bad > 0 {
            damage.push(DamageKind::ChecksumBad);
        }

        let healthy = damage.is_empty();
        let tier = if healthy {
            None
        } else if has_ihdr && has_idat {
            Some(RepairTier::Basis)
        } else if has_idat {
            Some(RepairTier::Sample)
        } else {
            None
        };
        let detail = if healthy {
            "PNG parses clean: signature, IHDR, IDAT chain and IEND valid.".to_string()
        } else {
            let mut notes: Vec<String> = Vec::new();
            if !has_sig {
                notes.push("signature missing".to_string());
            }
            if !has_ihdr {
                notes.push("IHDR missing (sample repair will synthesize)".to_string());
            }
            if !has_idat {
                notes.push("no IDAT payload".to_string());
            }
            if !has_iend {
                notes.push("truncated (no IEND)".to_string());
            }
            if crc_bad > 0 {
                notes.push(format!("{crc_bad} chunk CRC(s) bad"));
            }
            format!("PNG damage: {}.", notes.join(", "))
        };

        DamageReport {
            format: Format::Png,
            healthy,
            tier,
            damage,
            detail,
        }
    }

    fn repair_into(&self, buf: &[u8], out: &mut Vec<u8>) -> RepairOutcome {
        let mut damage = Vec::new();
        let chunks = scan_chunks(buf, &mut damage);
        let has_ihdr = chunks.first().is_some_and(|c| c.kind == *b"IHDR");
        let has_idat = chunks.iter().any(|c| c.kind == *b"IDAT");

        out.clear();
        let detail;
        let partial;
        if has_ihdr {
            // ---- BasisRepair: re-emit the chunk chain with re-stamped
            // CRCs and re-derived lengths, synthesize IEND when
            // truncated.
            out.extend_from_slice(&SIG);
            for c in &chunks {
                let end = c.end.min(buf.len());
                if c.start + 8 > end || c.start + 8 > buf.len() {
                    continue;
                }
                let payload_end = (c.start + 8 + c.stored_len as usize).min(end);
                let payload = &buf[(c.start + 8).min(buf.len())..payload_end];
                emit_chunk(out, c.kind, payload);
            }
            let has_iend_out = chunks.iter().any(|c| c.kind == *b"IEND");
            if !has_iend_out {
                out.extend_from_slice(&IEND_LITERAL);
            }
            partial = !has_iend_out;
            let mut notes = Vec::new();
            if damage.iter().any(|d| matches!(d, DamageKind::ChecksumBad)) {
                notes.push("CRCs re-stamped");
            }
            if damage
                .iter()
                .any(|d| matches!(d, DamageKind::BrokenIndex | DamageKind::Corruption))
            {
                notes.push("chunk lengths re-derived");
            }
            if !has_iend_out {
                notes.push("IEND synthesized");
            }
            detail = if notes.is_empty() {
                "Canonical re-emit: chunk chain validated, bytes preserved.".to_string()
            } else {
                format!("PNG BasisRepair: {}.", notes.join(", "))
            };
        } else if has_idat {
            // ---- SampleRepair: salvage the IDAT payloads (pattern
            // search found them), inflate what decodes, re-deflate,
            // synthesize a header around the stream.
            let mut payload = Vec::new();
            for c in &chunks {
                if c.kind == *b"IDAT" {
                    let end = (c.start + 8 + c.stored_len as usize).min(buf.len());
                    if c.start + 8 < end {
                        payload.extend_from_slice(&buf[c.start + 8..end]);
                    }
                }
            }
            let total = payload.len() as u64;
            let salvaged = inflate_prefix(&payload);
            let recovered = salvaged.len() as u64;
            let dims = width_from_anywhere(buf);
            let (w, h) = match dims {
                Some((w, h)) => (w.max(1), h.max(1)),
                None => (1, 1),
            };
            out.extend_from_slice(&SIG);
            emit_chunk(out, *b"IHDR", &build_ihdr(w, h));
            emit_chunk(out, *b"IDAT", &redeflate(&payload));
            out.extend_from_slice(&IEND_LITERAL);
            partial = recovered < total;
            detail = format!(
                "PNG SampleRepair: header synthesized around the salvaged IDAT stream — {}",
                salvage_detail("zlib", recovered, total)
            );
        } else {
            partial = true;
            detail = "PNG BeyondRepair: no IDAT payload found.".to_string();
        }

        RepairOutcome {
            repaired: has_ihdr || has_idat,
            input_bytes: buf.len() as u64,
            output_bytes: out.len() as u64,
            partial,
            detail,
        }
    }
}

/// The corruption-tolerant chunk scan — the ScanCriticalChunks
/// posture: locate chunk names by content, hand-validate the extent.
fn scan_chunks(buf: &[u8], damage: &mut Vec<DamageKind>) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    // Start after the signature when present; otherwise the pattern
    // search mode begins at 0.
    let mut i = if buf.len() >= 8 && buf[..8] == SIG {
        8
    } else {
        0
    };
    while i + 8 <= buf.len() && chunks.len() < 0x400 {
        let kind = [buf[i + 4], buf[i + 5], buf[i + 6], buf[i + 7]];
        if !valid_type(kind) {
            damage.push(DamageKind::Corruption);
            match next_chunk_start(buf, i + 1) {
                Some(p) => {
                    i = p;
                    continue;
                }
                None => break,
            }
        }
        let stored_len = u32::from_be_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);
        let end_field = (stored_len as u64) + 12;
        // Plausibility: the chunk must fit the file (a stored length
        // that overruns is either truncation or garbage).
        if end_field as usize + i > buf.len() {
            if is_critical(kind) {
                // Truncated tail chunk: keep the extent to EOF.
                chunks.push(Chunk {
                    start: i,
                    end: buf.len(),
                    kind,
                    stored_len: (buf.len() - i - 12) as u32,
                });
                damage.push(DamageKind::Truncated);
            } else {
                damage.push(DamageKind::BrokenIndex);
                match next_chunk_start(buf, i + 1) {
                    Some(p) => {
                        i = p;
                        continue;
                    }
                    None => break,
                }
            }
            break;
        }
        if stored_len > MAX_CHUNK {
            damage.push(DamageKind::BrokenIndex);
            match next_chunk_start(buf, i + 1) {
                Some(p) => {
                    i = p;
                    continue;
                }
                None => break,
            }
        }
        let end = i + 12 + stored_len as usize;
        chunks.push(Chunk {
            start: i,
            end,
            kind,
            stored_len,
        });
        i = end;
    }
    chunks
}

/// PNG chunk types are ASCII letters (bit 5 of each byte set/clear
/// flags ancillary/private/reserved-safe-to-copy — the alphabet check
/// is the grammar's only hard constraint on the name).
fn valid_type(kind: [u8; 4]) -> bool {
    kind.iter().all(|&b| b.is_ascii_alphabetic())
}

fn is_critical(kind: [u8; 4]) -> bool {
    kind[0].is_ascii_uppercase()
}

/// Find the next plausible chunk start (length field + alphabetic
/// type) at/after `from` — the pattern-search half.
fn next_chunk_start(buf: &[u8], from: usize) -> Option<usize> {
    let mut p = from;
    while p + 8 <= buf.len() {
        let kind = [buf[p + 4], buf[p + 5], buf[p + 6], buf[p + 7]];
        if valid_type(kind) {
            let len = u32::from_be_bytes([buf[p], buf[p + 1], buf[p + 2], buf[p + 3]]);
            if p + 12 + len as usize <= buf.len() {
                return Some(p);
            }
        }
        p += 1;
    }
    None
}

/// CRC audit: recompute over type+data, compare with the stored tail.
fn crc_mismatch(buf: &[u8], c: &Chunk) -> bool {
    let data_end = (c.start + 8 + c.stored_len as usize).min(buf.len());
    let crc_end = (data_end + 4).min(buf.len());
    if c.start + 4 > buf.len() || data_end + 4 > buf.len() || crc_end < data_end + 4 {
        return true;
    }
    let mut h = crc32fast::Hasher::new();
    h.update(&buf[c.start + 4..data_end]);
    let got = h.finalize();
    let want = u32::from_be_bytes([
        buf[data_end],
        buf[data_end + 1],
        buf[data_end + 2],
        buf[data_end + 3],
    ]);
    got != want
}

/// Emit a chunk with a freshly computed CRC (the re-stamp).
fn emit_chunk(out: &mut Vec<u8>, kind: [u8; 4], payload: &[u8]) {
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&kind);
    out.extend_from_slice(payload);
    let mut h = crc32fast::Hasher::new();
    h.update(&kind);
    h.update(payload);
    out.extend_from_slice(&h.finalize().to_be_bytes());
}

/// Tolerant zlib inflate: keep every byte that decodes before the
/// first error (the "extract what's decodable" salvage).
fn inflate_prefix(data: &[u8]) -> Vec<u8> {
    use std::io::Read;
    let mut out = Vec::new();
    let mut dec = flate2::read::ZlibDecoder::new(data);
    // Read what we can; a broken stream yields the decoded prefix.
    let _ = dec.read_to_end(&mut out);
    out
}

/// Re-inflate then re-deflate: a canonical clean zlib stream around
/// whatever payload decodes (the sample tier's stream rebuild).
fn redeflate(data: &[u8]) -> Vec<u8> {
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;
    let raw = inflate_prefix(data);
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    let _ = enc.write_all(&raw);
    enc.finish().unwrap_or_default()
}

/// Recover width/height from any plausible IHDR payload in the buffer
/// (even a damaged-header one — the pattern search posture).
fn width_from_anywhere(buf: &[u8]) -> Option<(u32, u32)> {
    let mut p = 0;
    while let Some(pos) = memchr::memchr(b'I', &buf[p..]) {
        let abs = p + pos;
        if abs + 8 <= buf.len() && buf[abs..abs + 4] == *b"IHDR" && abs >= 4 {
            let w = u32::from_be_bytes([buf[abs + 4], buf[abs + 5], buf[abs + 6], buf[abs + 7]]);
            if abs + 12 <= buf.len() {
                let h =
                    u32::from_be_bytes([buf[abs + 8], buf[abs + 9], buf[abs + 10], buf[abs + 11]]);
                if (1..=1 << 16).contains(&w) && (1..=1 << 16).contains(&h) {
                    return Some((w, h));
                }
            }
        }
        p = abs + 1;
        if p + 4 > buf.len() {
            break;
        }
    }
    None
}

/// Synthesize an IHDR payload for the sample tier: 8-bit RGBA, no
/// interlace — the most decodable baseline.
fn build_ihdr(w: u32, h: u32) -> [u8; 13] {
    let mut ih = [0u8; 13];
    ih[0..4].copy_from_slice(&w.to_be_bytes());
    ih[4..8].copy_from_slice(&h.to_be_bytes());
    ih[8] = 8; // bit depth
    ih[9] = 6; // color type RGBA
    ih[10] = 0; // compression
    ih[11] = 0; // filter
    ih[12] = 0; // interlace
    ih
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal 1x1 RGBA PNG, built honestly.
    fn build_png() -> Vec<u8> {
        // 1x1 RGBA red pixel, filter byte 0.
        let raw = [0u8, 255, 0, 0, 255];
        use flate2::{Compression, write::ZlibEncoder};
        use std::io::Write;
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&raw).unwrap();
        let idat = enc.finish().unwrap();
        let mut v = SIG.to_vec();
        emit_chunk(&mut v, *b"IHDR", &build_ihdr(1, 1));
        emit_chunk(&mut v, *b"IDAT", &idat);
        v.extend_from_slice(&IEND_LITERAL);
        v
    }

    #[test]
    fn healthy_png_passes_through() {
        let png = build_png();
        let e = PngEngine;
        let r = e.analyze(&png);
        assert!(r.healthy, "detail: {}", r.detail);
        let mut out = Vec::new();
        let o = e.repair_into(&png, &mut out);
        assert!(o.repaired && !o.partial);
        assert_eq!(out, png, "canonical re-emit is byte-exact");
    }

    #[test]
    fn bad_crc_is_restamped() {
        let mut png = build_png();
        // Corrupt the last CRC byte of the IDAT chunk (the 4 bytes
        // immediately before the IEND literal begins).
        let iend_at = png.len() - 12;
        png[iend_at - 1] ^= 0xFF;
        let e = PngEngine;
        let r = e.analyze(&png);
        assert!(r.damage.contains(&DamageKind::ChecksumBad));
        let mut out = Vec::new();
        let o = e.repair_into(&png, &mut out);
        assert!(o.repaired);
        // The output must parse with all CRCs valid now.
        let r2 = e.analyze(&out);
        assert!(
            !r2.damage.contains(&DamageKind::ChecksumBad),
            "re-stamped: {}",
            r2.detail
        );
    }

    #[test]
    fn truncated_png_gets_iend() {
        let png = build_png();
        let cut = &png[..png.len() - 12];
        let e = PngEngine;
        let mut out = Vec::new();
        let o = e.repair_into(cut, &mut out);
        assert!(o.repaired && o.partial);
        assert!(out.ends_with(&IEND_LITERAL));
        assert!(out.windows(4).any(|w| w == *b"IDAT"));
    }

    #[test]
    fn missing_header_sample_repairs() {
        let png = build_png();
        // Strip signature + IHDR: raw IDAT chain + IEND.
        let cut = &png[8 + 25..]; // 8 sig + 25 IHDR chunk (12+13)
        let e = PngEngine;
        let r = e.analyze(cut);
        assert_eq!(r.tier, Some(RepairTier::Sample));
        let mut out = Vec::new();
        let o = e.repair_into(cut, &mut out);
        assert!(o.repaired);
        // Output: signature + IHDR + IDAT + IEND, decodable chain.
        assert!(out.starts_with(&SIG));
        assert!(out.ends_with(&IEND_LITERAL));
        let r2 = e.analyze(&out);
        assert!(r2.healthy, "sample output parses clean: {}", r2.detail);
    }

    #[test]
    fn garbage_between_chunks_is_resynced() {
        let png = build_png();
        // Inject garbage between IHDR and IDAT.
        let at = 8 + 25;
        let garbage = [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01, 0x02];
        let mut damaged = Vec::new();
        damaged.extend_from_slice(&png[..at]);
        damaged.extend_from_slice(&garbage);
        damaged.extend_from_slice(&png[at..]);
        let e = PngEngine;
        let mut out = Vec::new();
        let o = e.repair_into(&damaged, &mut out);
        assert!(o.repaired);
        let r = e.analyze(&out);
        assert!(r.healthy, "resync produced a clean chain: {}", r.detail);
    }
}
