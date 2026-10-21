//! WebP repair — the RIFF container rebuild.
//!
//! Research: `artifacts/FileRepair/image-repair--state-machines.md`
//! §3 (CWebpWork): "WebP repair is container-level: the RIFF frame
//! is re-derived, chunk sizes recomputed" — the same RIFF grammar
//! the scan-side RIFFAnalyser carves. Validation is structural (the
//! VP8/VP8L bitstream magics); the vendor goes further and decodes
//! via the statically-linked libwebp, which our repair layer leaves
//! to the preview pipeline (the decoder is the same arbiter).

use crate::{DamageKind, DamageReport, Format, RepairEngine, RepairOutcome, RepairTier};

/// RIFF header: "RIFF" + size + "WEBP".
const RIFF: &[u8; 4] = b"RIFF";
const WEBP: &[u8; 4] = b"WEBP";

/// The WebP repair engine.
pub struct WebpEngine;

impl RepairEngine for WebpEngine {
    fn format(&self) -> Format {
        Format::Webp
    }

    fn check_magic(&self, buf: &[u8]) -> bool {
        buf.len() >= 12 && &buf[..4] == RIFF && &buf[8..12] == WEBP
            || buf.len() >= 12 && &buf[..4] == RIFF && starts_with_known_chunk(&buf[12..])
    }

    fn analyze(&self, buf: &[u8]) -> DamageReport {
        let mut damage = Vec::new();
        let chunks = walk(buf, &mut damage);
        let riff_size_ok = buf.len() >= 8 && {
            let declared = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
            declared as u64 + 8 == buf.len() as u64
        };
        if !riff_size_ok {
            damage.push(DamageKind::BrokenIndex);
        }
        if chunks.is_empty() {
            damage.push(DamageKind::Corruption);
        }
        let healthy = damage.is_empty();
        let detail = if healthy {
            format!(
                "WebP parses clean: {} chunks, RIFF size consistent.",
                chunks.len()
            )
        } else {
            let mut notes = Vec::new();
            if !riff_size_ok {
                notes.push("RIFF size inconsistent".to_string());
            }
            if chunks.is_empty() {
                notes.push("no image chunk".to_string());
            }
            if damage.contains(&DamageKind::Corruption) && !chunks.is_empty() {
                notes.retain(|n| n != "no image chunk");
                notes.push("corrupt chunks dropped".to_string());
            }
            if damage.contains(&DamageKind::Truncated) {
                notes.push("truncated tail".to_string());
            }
            format!("WebP damage: {}.", notes.join(", "))
        };
        DamageReport {
            format: Format::Webp,
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
        let chunks = walk(buf, &mut damage);
        out.clear();
        if chunks.is_empty() {
            return RepairOutcome {
                repaired: false,
                input_bytes: buf.len() as u64,
                output_bytes: 0,
                partial: true,
                detail: "WebP BeyondRepair: no image chunk survives.".to_string(),
            };
        }
        // Re-emit: fresh RIFF frame with the true size, chunk
        // records (fourcc + size + payload) byte-exact.
        out.extend_from_slice(RIFF);
        out.extend_from_slice(&0u32.to_le_bytes()); // size placeholder
        out.extend_from_slice(WEBP);
        for c in &chunks {
            let end = c.end.min(buf.len());
            if c.start + 8 <= end {
                out.extend_from_slice(&buf[c.start..end]);
            }
        }
        let riff_size = (out.len() - 8) as u32;
        out[4..8].copy_from_slice(&riff_size.to_le_bytes());
        let mut notes = Vec::new();
        if damage.contains(&DamageKind::BrokenIndex) {
            notes.push("RIFF size recomputed".to_string());
        }
        if damage.contains(&DamageKind::Corruption) {
            notes.push("corrupt chunks dropped".to_string());
        }
        RepairOutcome {
            repaired: true,
            input_bytes: buf.len() as u64,
            output_bytes: out.len() as u64,
            partial: damage.contains(&DamageKind::Truncated),
            detail: if notes.is_empty() {
                "WebP canonical re-emit: RIFF frame validated, bytes preserved.".to_string()
            } else {
                format!("WebP repair: {}.", notes.join(", "))
            },
        }
    }
}

#[derive(Debug)]
struct Chunk {
    start: usize,
    end: usize,
}

/// The RIFF chunk walk (4CC + u32 LE size; odd sizes pad).
fn walk(buf: &[u8], damage: &mut Vec<DamageKind>) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut i = 12usize.min(buf.len());
    while i + 8 <= buf.len() && chunks.len() < 0x1000 {
        let size = u32::from_le_bytes([buf[i + 4], buf[i + 5], buf[i + 6], buf[i + 7]]) as usize;
        let padded = size + (size & 1);
        if i + 8 + padded > buf.len() {
            // Truncated final chunk: keep the extent.
            chunks.push(Chunk {
                start: i,
                end: buf.len(),
            });
            damage.push(DamageKind::Truncated);
            break;
        }
        chunks.push(Chunk {
            start: i,
            end: i + 8 + padded,
        });
        i += 8 + padded;
    }
    if chunks.is_empty() && buf.len() > 12 {
        damage.push(DamageKind::Corruption);
    }
    chunks
}

fn starts_with_known_chunk(buf: &[u8]) -> bool {
    buf.len() >= 4
        && (&buf[..4] == b"VP8 "
            || &buf[..4] == b"VP8L"
            || &buf[..4] == b"VP8X"
            || &buf[..4] == b"ANIM")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lossless WebP: RIFF + WEBP + VP8L chunk (minimal payload).
    fn build_webp() -> Vec<u8> {
        let payload = [0x2F, 0x00, 0x00, 0x10, 0x08, 0x06, 0x00, 0x00, 0x05, 0x00];
        let mut v = Vec::new();
        v.extend_from_slice(RIFF);
        v.extend_from_slice(&((4 + 8 + payload.len()) as u32).to_le_bytes());
        v.extend_from_slice(WEBP);
        v.extend_from_slice(b"VP8L");
        v.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        v.extend_from_slice(&payload);
        v
    }

    #[test]
    fn healthy_webp_passes_through() {
        let w = build_webp();
        let e = WebpEngine;
        let r = e.analyze(&w);
        assert!(r.healthy, "detail: {}", r.detail);
        let mut out = Vec::new();
        let o = e.repair_into(&w, &mut out);
        assert!(o.repaired);
        assert_eq!(out, w);
    }

    #[test]
    fn lying_riff_size_is_recomputed() {
        let mut w = build_webp();
        w[4..8].copy_from_slice(&0xFFFFu32.to_le_bytes());
        let e = WebpEngine;
        let r = e.analyze(&w);
        assert!(r.damage.contains(&DamageKind::BrokenIndex));
        let mut out = Vec::new();
        let o = e.repair_into(&w, &mut out);
        assert!(o.repaired);
        let declared = u32::from_le_bytes([out[4], out[5], out[6], out[7]]);
        assert_eq!(declared as u64 + 8, out.len() as u64);
        let r2 = e.analyze(&out);
        assert!(r2.healthy, "recomputed: {}", r2.detail);
    }

    #[test]
    fn truncated_chunk_is_kept_honestly() {
        let w = build_webp();
        let cut = &w[..w.len() - 3];
        let e = WebpEngine;
        let mut out = Vec::new();
        let o = e.repair_into(cut, &mut out);
        assert!(o.repaired);
        assert!(o.partial);
        // RIFF size still consistent in the output.
        let declared = u32::from_le_bytes([out[4], out[5], out[6], out[7]]);
        assert_eq!(declared as u64 + 8, out.len() as u64);
    }
}
