//! GIF repair — the extent re-chain model.
//!
//! Research: `artifacts/FileRepair/image-repair--gif-bmp-svg-tiff-walks.md`
//! §2: GIF repair is "not a linear LZW walk: it rebuilds the block
//! stream from sorted extent maps" — corrupted LZW data is not
//! re-coded; salvaged block extents are re-chained into a valid
//! stream. Our v1 implements the same philosophy as a
//! block-boundary walk with resync: image descriptors (0x2C),
//! extensions (0x21) and the trailer (0x3B) are located by content,
//! their sub-block chains validated, and the valid blocks re-emitted
//! between a rebuilt header and a synthesized trailer.

use crate::{DamageKind, DamageReport, Format, RepairEngine, RepairOutcome, RepairTier};

/// Fixed header size: signature(6) + logical screen descriptor(7).
const HEAD_FIXED: usize = 13;
/// Image descriptor introducer.
const IMG: u8 = 0x2C;
/// Extension introducer.
const EXT: u8 = 0x21;
/// Trailer.
const TRAILER: u8 = 0x3B;

/// The GIF repair engine.
pub struct GifEngine;

impl RepairEngine for GifEngine {
    fn format(&self) -> Format {
        Format::Gif
    }

    fn check_magic(&self, buf: &[u8]) -> bool {
        buf.len() >= 6 && (&buf[..4] == b"GIF8") && (&buf[4..6] == b"9a" || &buf[4..6] == b"7a")
    }

    fn analyze(&self, buf: &[u8]) -> DamageReport {
        let mut damage = Vec::new();
        let blocks = walk(buf, &mut damage);
        if buf.len() < HEAD_FIXED {
            damage.push(DamageKind::Truncated);
        }
        if blocks.iter().all(|b| !b.is_image) {
            damage.push(DamageKind::Corruption);
        }
        if !blocks.iter().any(|b| b.trailer) {
            damage.push(DamageKind::Truncated);
        }
        let healthy = damage.is_empty();
        let detail = if healthy {
            format!(
                "GIF parses clean: {} blocks, trailer present.",
                blocks.len()
            )
        } else {
            let mut notes = Vec::new();
            if buf.len() < HEAD_FIXED {
                notes.push("header truncated".to_string());
            }
            if blocks.iter().all(|b| !b.is_image) {
                notes.push("no image descriptor".to_string());
            }
            if !blocks.iter().any(|b| b.trailer) {
                notes.push("no trailer (truncated)".to_string());
            }
            if damage.contains(&DamageKind::Corruption) && blocks.iter().any(|b| b.is_image) {
                notes.retain(|n| n != "no image descriptor");
                notes.push("garbage resynced".to_string());
            }
            format!("GIF damage: {}.", notes.join(", "))
        };
        DamageReport {
            format: Format::Gif,
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
        let blocks = walk(buf, &mut damage);
        out.clear();
        if buf.len() < HEAD_FIXED {
            // Beyond the header there is nothing to rebuild around.
            return RepairOutcome {
                repaired: false,
                input_bytes: buf.len() as u64,
                output_bytes: 0,
                partial: true,
                detail: "GIF BeyondRepair: the header is gone.".to_string(),
            };
        }
        // Header + logical screen descriptor + GCT, byte-exact.
        out.extend_from_slice(&buf[..head_end(buf)]);
        let mut notes = Vec::new();
        for b in &blocks {
            let end = b.end.min(buf.len());
            if b.start < end {
                out.extend_from_slice(&buf[b.start..end]);
            }
        }
        let had_trailer = blocks.iter().any(|b| b.trailer);
        if !had_trailer {
            out.push(TRAILER);
            notes.push("trailer synthesized".to_string());
        }
        if damage.contains(&DamageKind::Corruption) {
            notes.push("corrupt blocks dropped, extents re-chained".to_string());
        }
        RepairOutcome {
            // The honesty contract (the fuzz corpus's class): repaired
            // means OUTPUT EXISTS — a blocks list of zero-extent
            // records writes nothing and must report unrepaired.
            repaired: !out.is_empty(),
            input_bytes: buf.len() as u64,
            output_bytes: out.len() as u64,
            partial: !had_trailer,
            detail: if notes.is_empty() {
                "GIF canonical re-emit: block extents validated, bytes preserved.".to_string()
            } else {
                format!("GIF repair: {}.", notes.join(", "))
            },
        }
    }
}

#[derive(Debug)]
struct Block {
    start: usize,
    end: usize,
    is_image: bool,
    trailer: bool,
}

/// The block walk: image descriptors, extensions, and sub-block
/// chains with bounds-checked resync.
/// The true data start: fixed header + the global color table when
/// the packed byte's GCT flag is set (2^(code+1) entries × 3 bytes).
fn head_end(buf: &[u8]) -> usize {
    if buf.len() < HEAD_FIXED {
        return buf.len();
    }
    let packed = buf[10];
    let gct = if packed & 0x80 != 0 {
        3usize << ((packed & 0x07) + 1)
    } else {
        0
    };
    (HEAD_FIXED + gct).min(buf.len())
}

fn walk(buf: &[u8], damage: &mut Vec<DamageKind>) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut i = head_end(buf);
    while i < buf.len() && blocks.len() < 0x4000 {
        match buf[i] {
            TRAILER => {
                blocks.push(Block {
                    start: i,
                    end: i + 1,
                    is_image: false,
                    trailer: true,
                });
                break;
            }
            IMG => {
                // Image descriptor: introducer + 8 rect bytes + packed
                // (10), then the LZW min-code byte, then the sub-block
                // chain.
                if i + 11 > buf.len() {
                    damage.push(DamageKind::Truncated);
                    break;
                }
                let data_start = i + 11;
                if let Some(end) = subblock_end(buf, data_start) {
                    blocks.push(Block {
                        start: i,
                        end,
                        is_image: true,
                        trailer: false,
                    });
                    i = end;
                } else {
                    damage.push(DamageKind::Truncated);
                    blocks.push(Block {
                        start: i,
                        end: buf.len(),
                        is_image: true,
                        trailer: false,
                    });
                    break;
                }
            }
            EXT => {
                // Extension: introducer + label + sub-block chain.
                if i + 2 > buf.len() {
                    damage.push(DamageKind::Truncated);
                    break;
                }
                if let Some(end) = subblock_end(buf, i + 2) {
                    blocks.push(Block {
                        start: i,
                        end,
                        is_image: false,
                        trailer: false,
                    });
                    i = end;
                } else {
                    damage.push(DamageKind::Truncated);
                    blocks.push(Block {
                        start: i,
                        end: buf.len(),
                        is_image: false,
                        trailer: false,
                    });
                    break;
                }
            }
            other => {
                let _ = other;
                damage.push(DamageKind::Corruption);
                // Resync: next introducer byte that parses.
                match (i + 1..buf.len())
                    .find(|&p| buf[p] == IMG || buf[p] == EXT || buf[p] == TRAILER)
                {
                    Some(p) => {
                        i = p;
                    }
                    None => break,
                }
            }
        }
    }
    blocks
}

/// Walk a sub-block chain (len byte + data, 0 terminates). Returns
/// None when the chain runs off the end (truncation).
fn subblock_end(buf: &[u8], mut p: usize) -> Option<usize> {
    while p < buf.len() {
        let len = buf[p] as usize;
        if len == 0 {
            return Some(p + 1);
        }
        if p + 1 + len > buf.len() {
            return None;
        }
        p += 1 + len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2x2 one-frame GIF87a, global color table, 1 image.
    fn build_gif() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"GIF89a");
        // Logical screen: 2x2, GCT flag 0x80, size 2 (2 entries), bg 0, aspect 0.
        v.extend_from_slice(&[2, 0, 2, 0, 0x80, 0, 0]);
        // GCT: 2 entries × 3 bytes.
        v.extend_from_slice(&[255, 0, 0, 0, 0, 255]);
        // Image descriptor: 0x2C + left(2) + top(2) + w(2) + h(2) + packed.
        v.extend_from_slice(&[IMG, 0, 0, 0, 0, 2, 0, 2, 0, 0]);
        v.extend_from_slice(&[2]); // LZW min code size
        v.extend_from_slice(&[2, 0x84, 0x51]); // sub-block
        v.extend_from_slice(&[0]); // terminator
        v.push(TRAILER);
        v
    }

    #[test]
    fn healthy_gif_passes_through() {
        let gif = build_gif();
        let e = GifEngine;
        let r = e.analyze(&gif);
        assert!(r.healthy, "detail: {}", r.detail);
        let mut out = Vec::new();
        let o = e.repair_into(&gif, &mut out);
        assert!(o.repaired && !o.partial);
        assert_eq!(out, gif);
    }

    #[test]
    fn truncated_gif_gets_trailer() {
        let gif = build_gif();
        let cut = &gif[..gif.len() - 1];
        let e = GifEngine;
        let mut out = Vec::new();
        let o = e.repair_into(cut, &mut out);
        assert!(o.repaired && o.partial);
        assert_eq!(*out.last().unwrap(), TRAILER);
    }

    #[test]
    fn garbage_is_resynced() {
        let mut gif = build_gif();
        let trailer_at = gif.len() - 1;
        gif.splice(trailer_at..trailer_at, [0xDE, 0xAD, 0xBE]);
        let e = GifEngine;
        let r = e.analyze(&gif);
        assert!(r.damage.contains(&DamageKind::Corruption));
        let mut out = Vec::new();
        let o = e.repair_into(&gif, &mut out);
        assert!(o.repaired);
        assert_eq!(*out.last().unwrap(), TRAILER);
        let r2 = e.analyze(&out);
        assert!(r2.healthy, "resync: {}", r2.detail);
    }
}
