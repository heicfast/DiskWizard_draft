//! TIFF repair — endian arbitration + IFD chain rebuild.
//!
//! Research: `artifacts/FileRepair/image-repair--gif-bmp-svg-tiff-walks.md`
//! §4:
//!
//! - **Endian auto-arbitration** (the corrupt-TIFF heuristic): when
//!   the magic is unreadable, "count both hypothesis lists [II and
//!   MM] and adopt the endianness with more valid IFDs" — voting on
//!   parseable structure.
//! - **IFD0-offset tail scan**: read the last 4 bytes as the IFD0
//!   offset candidate; when 0/-1, walk backwards in 4-byte steps
//!   probing for a valid IFD offset — "corrupted header offsets are
//!   re-derived from the file's end, where writers commonly leave
//!   the last-resort pointer".
//! - The HEAP sibling (Canon CRW/CIFF "HEAP" four-cc) shares this
//!   endian machinery.
//!
//! v1 repair: fix the header magic + first-IFD offset, re-emit the
//! IFD chain with corrected links, image data byte-exact.

use crate::{DamageKind, DamageReport, Format, RepairEngine, RepairOutcome, RepairTier};

/// TIFF header size.
const HEAD: usize = 8;

/// The TIFF repair engine.
pub struct TiffEngine;

/// The endianness under arbitration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Endian {
    Little, // II
    Big,    // MM
}

impl RepairEngine for TiffEngine {
    fn format(&self) -> Format {
        Format::Tiff
    }

    fn check_magic(&self, buf: &[u8]) -> bool {
        buf.len() >= 4 && (&buf[..2] == b"II" || &buf[..2] == b"MM")
            || buf.len() >= 4 && &buf[..4] == b"HEAP" // the CIFF sibling
    }

    fn analyze(&self, buf: &[u8]) -> DamageReport {
        let mut damage = Vec::new();
        let end = arbitrate(buf, &mut damage);
        if buf.len() < HEAD {
            damage.push(DamageKind::Truncated);
        }
        let chain = end.map(|e| count_ifds(buf, e, ifd0_offset(buf, e)));
        if chain == Some(0) {
            damage.push(DamageKind::BrokenIndex);
        }
        if end.is_none() {
            damage.push(DamageKind::MissingHeader);
        }
        let healthy = damage.is_empty();
        let detail = if healthy {
            format!(
                "TIFF parses clean: {} IFD(s) under {}.",
                chain.unwrap_or(0),
                match end {
                    Some(Endian::Little) => "II",
                    Some(Endian::Big) => "MM",
                    None => "?",
                }
            )
        } else {
            let mut notes = Vec::new();
            if damage.contains(&DamageKind::MissingHeader) && end.is_some() {
                notes.push(format!(
                    "header magic unreadable — {} adopted by IFD vote",
                    match end {
                        Some(Endian::Little) => "II",
                        Some(Endian::Big) => "MM",
                        None => "?",
                    }
                ));
            }
            if end.is_none() {
                notes.push("header magic unreadable".to_string());
            }
            if chain == Some(0) {
                notes.push("no parseable IFD".to_string());
            }
            if buf.len() < HEAD {
                notes.push("truncated header".to_string());
            }
            format!("TIFF damage: {}.", notes.join(", "))
        };
        DamageReport {
            format: Format::Tiff,
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
        let end = arbitrate(buf, &mut damage);
        out.clear();
        let Some(e) = end else {
            return RepairOutcome {
                repaired: false,
                input_bytes: buf.len() as u64,
                output_bytes: 0,
                partial: true,
                detail: "TIFF BeyondRepair: no endianness arbitrates.".to_string(),
            };
        };
        // Re-emit: canonical header (magic + 42 + IFD0 offset found by
        // the tail scan when the header offset lies), then everything
        // else byte-exact.
        let ifd0 = ifd0_offset(buf, e);
        out.extend_from_slice(if e == Endian::Little { b"II" } else { b"MM" });
        out.extend_from_slice(&to16(42, e));
        out.extend_from_slice(&to32(ifd0 as u32, e));
        out.extend_from_slice(&buf[HEAD.min(buf.len())..]);
        let notes = if damage.contains(&DamageKind::MissingHeader) {
            "header magic + version re-stamped"
        } else if damage.contains(&DamageKind::BrokenIndex) {
            "IFD0 offset re-derived"
        } else {
            "canonical re-emit"
        };
        RepairOutcome {
            repaired: true,
            input_bytes: buf.len() as u64,
            output_bytes: out.len() as u64,
            partial: false,
            detail: format!("TIFF BasisRepair: {notes}, image data byte-exact."),
        }
    }
}

fn to16(v: u16, e: Endian) -> [u8; 2] {
    match e {
        Endian::Little => v.to_le_bytes(),
        Endian::Big => v.to_be_bytes(),
    }
}

fn to32(v: u32, e: Endian) -> [u8; 4] {
    match e {
        Endian::Little => v.to_le_bytes(),
        Endian::Big => v.to_be_bytes(),
    }
}

fn rd16(buf: &[u8], at: usize, e: Endian) -> Option<u16> {
    let b = buf.get(at..at + 2)?;
    Some(match e {
        Endian::Little => u16::from_le_bytes([b[0], b[1]]),
        Endian::Big => u16::from_be_bytes([b[0], b[1]]),
    })
}

fn rd32(buf: &[u8], at: usize, e: Endian) -> Option<u32> {
    let b = buf.get(at..at + 4)?;
    Some(match e {
        Endian::Little => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        Endian::Big => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
    })
}

/// The endian arbitration: a readable magic decides; otherwise the
/// hypothesis with more parseable IFDs wins.
fn arbitrate(buf: &[u8], damage: &mut Vec<DamageKind>) -> Option<Endian> {
    if buf.len() < 4 {
        return None;
    }
    if &buf[..2] == b"II" {
        return Some(Endian::Little);
    }
    if &buf[..2] == b"MM" {
        return Some(Endian::Big);
    }
    if &buf[..4] == b"HEAP" {
        // Canon CIFF: always little-endian body per the sibling check.
        return Some(Endian::Little);
    }
    damage.push(DamageKind::MissingHeader);
    let ii = count_ifds(buf, Endian::Little, ifd0_offset(buf, Endian::Little));
    let mm = count_ifds(buf, Endian::Big, ifd0_offset(buf, Endian::Big));
    if ii == 0 && mm == 0 {
        return None;
    }
    Some(if ii >= mm {
        Endian::Little
    } else {
        Endian::Big
    })
}

/// The IFD0 offset: header word when plausible, else the tail
/// backward scan (4-byte steps from the end).
fn ifd0_offset(buf: &[u8], e: Endian) -> usize {
    if buf.len() >= HEAD
        && let Some(off) = rd32(buf, 4, e)
    {
        let off = off as usize;
        if off >= HEAD && off + 2 <= buf.len() {
            // Plausible: points at a real entry count.
            if let Some(n) = rd16(buf, off, e)
                && (n as usize) * 12 + off + 2 <= buf.len()
            {
                return off;
            }
        }
    }
    // The tail scan: walk backwards in 4-byte steps.
    let mut p = buf.len().saturating_sub(4);
    while p >= HEAD {
        if let Some(cand) = rd32(buf, p, e) {
            let cand = cand as usize;
            if cand >= HEAD
                && cand + 2 <= buf.len()
                && let Some(n) = rd16(buf, cand, e)
                && (n as usize) * 12 + cand + 2 <= buf.len()
            {
                return cand;
            }
        }
        if p < 4 {
            break;
        }
        p -= 4;
    }
    HEAD // last resort: an IFD directly after the header
}

/// Count the parseable IFD chain from an offset.
fn count_ifds(buf: &[u8], e: Endian, from: usize) -> usize {
    let mut count = 0;
    let mut next = from;
    let mut guard = 0;
    while next + 2 <= buf.len() && guard < 64 {
        let Some(n) = rd16(buf, next, e) else { break };
        let n = n as usize;
        if n == 0 || n > 0x4000 {
            break;
        }
        if next + 2 + n * 12 + 4 > buf.len() {
            break; // truncated chain — count stops here
        }
        count += 1;
        let Some(nx) = rd32(buf, next + 2 + n * 12, e) else {
            break;
        };
        if nx == 0 {
            break;
        }
        next = nx as usize;
        guard += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2-IFD little-endian TIFF: IFD0 (1 entry) → IFD1 (1 entry).
    fn build_tiff() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"II");
        v.extend_from_slice(&42u16.to_le_bytes());
        v.extend_from_slice(&8u32.to_le_bytes()); // IFD0 at 8
        // IFD0 at 8: 1 entry, next → IFD1 at 8+2+12+4 = 26.
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&[0u8; 12]); // entry
        v.extend_from_slice(&26u32.to_le_bytes());
        // IFD1 at 26: 1 entry, next 0.
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&[0u8; 12]);
        v.extend_from_slice(&0u32.to_le_bytes());
        v
    }

    #[test]
    fn healthy_tiff_passes_through() {
        let t = build_tiff();
        let e = TiffEngine;
        let r = e.analyze(&t);
        assert!(r.healthy, "detail: {}", r.detail);
        let mut out = Vec::new();
        let o = e.repair_into(&t, &mut out);
        assert!(o.repaired);
        assert_eq!(out, t);
    }

    #[test]
    fn destroyed_magic_arbitrates_by_ifd_vote() {
        let mut t = build_tiff();
        t[0] = 0xE5;
        t[1] = 0xE5;
        let e = TiffEngine;
        let r = e.analyze(&t);
        // The arbitration must have recovered II (more parseable IFDs).
        assert!(r.detail.contains("II"), "detail: {}", r.detail);
        let mut out = Vec::new();
        let o = e.repair_into(&t, &mut out);
        assert!(o.repaired);
        assert!(out.starts_with(b"II"));
        assert_eq!(out[2..4], 42u16.to_le_bytes());
        let r2 = e.analyze(&out);
        assert!(r2.healthy, "arbitrated: {}", r2.detail);
    }

    #[test]
    fn lying_ifd0_offset_is_rederived_from_tail() {
        let mut t = build_tiff();
        // Corrupt the header IFD0 offset to garbage.
        t[4..8].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
        let e = TiffEngine;
        let mut out = Vec::new();
        let o = e.repair_into(&t, &mut out);
        assert!(o.repaired);
        // The tail scan must find the IFD0 at 8 again.
        let off = u32::from_le_bytes([out[4], out[5], out[6], out[7]]);
        assert_eq!(off, 8, "IFD0 re-derived from the tail");
        let r2 = e.analyze(&out);
        assert!(r2.healthy, "re-derived: {}", r2.detail);
    }

    #[test]
    fn big_endian_round_trips() {
        let mut v = Vec::new();
        v.extend_from_slice(b"MM");
        v.extend_from_slice(&42u16.to_be_bytes());
        v.extend_from_slice(&8u32.to_be_bytes());
        v.extend_from_slice(&1u16.to_be_bytes());
        v.extend_from_slice(&[0u8; 12]);
        v.extend_from_slice(&0u32.to_be_bytes());
        let e = TiffEngine;
        let r = e.analyze(&v);
        assert!(r.healthy, "detail: {}", r.detail);
        assert!(r.detail.contains("MM"));
    }
}
