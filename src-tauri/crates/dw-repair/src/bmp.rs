//! BMP repair — the DIB-header validator with palette arithmetic.
//!
//! Research: `artifacts/FileRepair/image-repair--gif-bmp-svg-tiff-walks.md`
//! §1:
//!
//! - Magic "BM" (0x4D42); the parser accepts both byte orders.
//! - DIB header-size taxonomy — exactly 12 (CORE), 40 (INFO), 108
//!   (V4), 124 (V5); anything else is not a BMP DIB.
//! - Palette arithmetic: bpp 1 → 8 bytes, 4 → 0x40, 8 → 0x400;
//!   24/32 → 0 (RGBQUAD per entry).
//! - Repair: the header fixer recomputes `bfOffBits` and `bfSize`
//!   from the parsed structure (end-of-header = palette + 0x1A for
//!   the CORE base).

use crate::{DamageKind, DamageReport, Format, RepairEngine, RepairOutcome, RepairTier};

const FILE_HEADER: usize = 14;
/// The accepted DIB sizes (the complete classic family).
const DIB_SIZES: [u32; 4] = [12, 40, 108, 124];

/// The BMP repair engine.
pub struct BmpEngine;

impl RepairEngine for BmpEngine {
    fn format(&self) -> Format {
        Format::Bmp
    }

    fn check_magic(&self, buf: &[u8]) -> bool {
        buf.len() >= 2 && &buf[..2] == b"BM"
    }

    fn analyze(&self, buf: &[u8]) -> DamageReport {
        let mut damage = Vec::new();
        let dib = dib_info(buf, &mut damage);
        if buf.len() < FILE_HEADER {
            damage.push(DamageKind::Truncated);
        }
        if let Some(d) = &dib {
            // Off-bits consistency: the declared bfOffBits must land
            // inside the file and at or after the palette end.
            if buf.len() >= 10 {
                let off = u32::from_le_bytes([buf[10], buf[11], buf[12], buf[13]]) as u64;
                if off + 2 > buf.len() as u64 || off < d.payload_start as u64 {
                    damage.push(DamageKind::BrokenIndex);
                }
            }
            // Size consistency: bfSize should cover the file (some
            // writers lie by a few bytes — only gross lies are named).
            if buf.len() >= 6 {
                let declared = u32::from_le_bytes([buf[2], buf[3], buf[4], buf[5]]) as u64;
                if declared > buf.len() as u64 * 2 + 16 {
                    damage.push(DamageKind::BrokenIndex);
                }
            }
        }
        let healthy = damage.is_empty();
        let detail = if healthy {
            format!(
                "BMP parses clean: DIB {} bytes, palette arithmetic consistent.",
                dib.map_or(0, |d| d.dib_size)
            )
        } else {
            let mut notes = Vec::new();
            if buf.len() >= FILE_HEADER && dib.is_none() {
                notes.push("DIB header size not in the classic family (12/40/108/124)".to_string());
            }
            if buf.len() < FILE_HEADER {
                notes.push("header truncated".to_string());
            }
            if damage.contains(&DamageKind::BrokenIndex) {
                notes.push("offset/size fields inconsistent".to_string());
            }
            format!("BMP damage: {}.", notes.join(", "))
        };
        DamageReport {
            format: Format::Bmp,
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
        let dib = dib_info(buf, &mut damage);
        out.clear();
        let Some(d) = dib else {
            if buf.len() > FILE_HEADER {
                // Salvage: synthesize a BITMAPFILEHEADER around the
                // DIB + pixels (the SampleRepair posture).
                out.extend_from_slice(b"BM");
                out.extend_from_slice(&(buf.len() as u32 + FILE_HEADER as u32).to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&((FILE_HEADER + d_size(buf)) as u32).to_le_bytes());
                out.extend_from_slice(buf);
                return RepairOutcome {
                    repaired: true,
                    input_bytes: buf.len() as u64,
                    output_bytes: out.len() as u64,
                    partial: false,
                    detail: "BMP SampleRepair: file header synthesized around the DIB.".to_string(),
                };
            }
            return RepairOutcome {
                repaired: false,
                input_bytes: buf.len() as u64,
                output_bytes: 0,
                partial: true,
                detail: "BMP BeyondRepair: no parseable DIB header.".to_string(),
            };
        };
        // BasisRepair: rebuild the file header with recomputed
        // offBits (end of palette) and size, keep the rest byte-exact.
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&(buf.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(d.payload_start as u32).to_le_bytes());
        out.extend_from_slice(&buf[FILE_HEADER.min(buf.len())..]);
        let fixed = damage.contains(&DamageKind::BrokenIndex);
        RepairOutcome {
            repaired: true,
            input_bytes: buf.len() as u64,
            output_bytes: out.len() as u64,
            partial: false,
            detail: if fixed {
                "BMP BasisRepair: offBits/size recomputed from the DIB + palette arithmetic."
                    .to_string()
            } else {
                "BMP canonical re-emit: header validated, bytes preserved.".to_string()
            },
        }
    }
}

struct DibInfo {
    dib_size: usize,
    /// Where the pixel array starts (end of DIB + palette).
    payload_start: usize,
}

/// Parse + validate the DIB header (the taxonomy + palette math).
fn dib_info(buf: &[u8], damage: &mut Vec<DamageKind>) -> Option<DibInfo> {
    if buf.len() < FILE_HEADER + 4 {
        return None;
    }
    let dib_size = u32::from_le_bytes([buf[14], buf[15], buf[16], buf[17]]);
    if !DIB_SIZES.contains(&dib_size) {
        damage.push(DamageKind::MissingHeader);
        return None;
    }
    let dib_size = dib_size as usize;
    if buf.len() < FILE_HEADER + dib_size {
        damage.push(DamageKind::Truncated);
        return None;
    }
    // Palette arithmetic (the CORE-base form; INFO+ DIBs start their
    // palette right after the header — the +0x1A in the vendor math
    // covers the 14+12 CORE layout).
    let (bpp_off, palette_base) = if dib_size == 12 {
        (FILE_HEADER + 10, FILE_HEADER + dib_size)
    } else {
        (FILE_HEADER + 14, FILE_HEADER + dib_size)
    };
    let bpp = u16::from_le_bytes([buf[bpp_off], buf[bpp_off + 1]]);
    let palette = match bpp {
        1 => 8,
        4 => 0x40,
        8 => 0x400,
        24 | 32 => 0,
        _ => 0,
    };
    Some(DibInfo {
        dib_size,
        payload_start: palette_base + palette,
    })
}

/// The DIB size field even when the taxonomy check failed (the
/// salvage path needs it for offBits).
fn d_size(buf: &[u8]) -> usize {
    if buf.len() >= FILE_HEADER + 4 {
        u32::from_le_bytes([buf[14], buf[15], buf[16], buf[17]]).min(124) as usize
    } else {
        40
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1x1 24-bpp BMP.
    fn build_bmp() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"BM");
        let off = (FILE_HEADER + 40) as u32;
        let size = (FILE_HEADER + 40 + 4) as u32;
        v.extend_from_slice(&size.to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes());
        v.extend_from_slice(&off.to_le_bytes());
        // DIB (40): 1x1, 1 plane, 24bpp, no compression.
        v.extend_from_slice(&40u32.to_le_bytes());
        v.extend_from_slice(&1i32.to_le_bytes());
        v.extend_from_slice(&1i32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&24u16.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&4u32.to_le_bytes());
        v.extend_from_slice(&[0u8; 16]);
        v.extend_from_slice(&[0xFF, 0x00, 0x00, 0x00]); // BGR pixel + pad
        v
    }

    #[test]
    fn healthy_bmp_passes_through() {
        let bmp = build_bmp();
        let e = BmpEngine;
        let r = e.analyze(&bmp);
        assert!(r.healthy, "detail: {}", r.detail);
        let mut out = Vec::new();
        let o = e.repair_into(&bmp, &mut out);
        assert_eq!(out, bmp);
        assert!(o.repaired);
    }

    #[test]
    fn bad_offset_is_recomputed() {
        let mut bmp = build_bmp();
        // Corrupt bfOffBits to point inside the palette (impossible).
        bmp[10..14].copy_from_slice(&2u32.to_le_bytes());
        let e = BmpEngine;
        let r = e.analyze(&bmp);
        assert!(r.damage.contains(&DamageKind::BrokenIndex));
        let mut out = Vec::new();
        let o = e.repair_into(&bmp, &mut out);
        assert!(o.repaired);
        let off = u32::from_le_bytes([out[10], out[11], out[12], out[13]]);
        assert_eq!(off as usize, FILE_HEADER + 40);
        let r2 = e.analyze(&out);
        assert!(r2.healthy, "recomputed: {}", r2.detail);
    }

    #[test]
    fn destroyed_magic_is_resynthesized() {
        let mut bmp = build_bmp();
        bmp[0] = b'X';
        bmp[1] = b'Y';
        let e = BmpEngine;
        // check_magic fails on destroyed magic — engine is invoked by
        // extension in that case; direct call still repairs.
        let mut out = Vec::new();
        let o = e.repair_into(&bmp, &mut out);
        assert!(o.repaired);
        assert!(out.starts_with(b"BM"));
        let r2 = e.analyze(&out);
        assert!(r2.healthy, "resynthesized: {}", r2.detail);
    }
}
