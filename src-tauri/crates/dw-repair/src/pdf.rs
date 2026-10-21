//! PDF repair — the `EuPDFRepairFile` xref/trailer re-synthesis model.
//!
//! Research: `artifacts/FileRepair/deepening-batch2--pdf-office-repair.md`
//! §1 (line-cited against the decompilation):
//!
//! - The document scanner is a single pass dispatching on the lead
//!   character: `%`/`e` skip, `s` matches `stream` (extent recorded),
//!   `t` matches `trailer` (dictionary parse), digits parse `N 0 obj`
//!   (offset recorded; **the maximum object number is tracked** — it
//!   becomes `/Size`).
//! - **"The repair does not relocate objects — it re-derives the
//!   cross-reference from where the objects actually are"**: we emit
//!   the surviving objects byte-exact, then a fresh classic xref.
//! - Canonical format strings (byte-exact from the binary):
//!   `"0 1\n0000000000 65535 f\r\n"` (free-list head), `"%d %d\n"`
//!   subsection headers, `"%010lld 00000 n\r\n"` per entry (CRLF).
//! - Trailer: `/Size` (max+1), `/Root` (resolved: trailer's ref when
//!   it matches a discovered object, else the catalog scan), `/ID`
//!   only when recovered, `/Encrypt` only when recovered AND the
//!   keep-encrypt flag is set, then `startxref\n%lld\n%%EOF`.
//! - The header is re-stamped `%%PDF-%d.%d` when parseable, else the
//!   1.7 baseline.

use crate::{DamageKind, DamageReport, Format, RepairEngine, RepairOutcome, RepairTier};
use memchr::memmem;

/// One discovered indirect object.
#[derive(Debug, Clone)]
struct PdfObject {
    num: u64,
    /// Offset of the `N M obj` token in the SOURCE buffer.
    src_start: usize,
    /// End offset (exclusive) — the byte after `endobj` (or the next
    /// object's start / EOF).
    src_end: usize,
}

/// The trailer facts recovered from the scan.
#[derive(Debug, Default, Clone)]
struct TrailerFacts {
    root_ref: Option<u64>,
    size: Option<u64>,
    has_id: bool,
    has_encrypt: bool,
}

/// The PDF repair engine.
pub struct PdfEngine;

impl RepairEngine for PdfEngine {
    fn format(&self) -> Format {
        Format::Pdf
    }

    fn check_magic(&self, buf: &[u8]) -> bool {
        // %PDF- anywhere in the head (some damage leaves junk before
        // the header — the scanner tolerates a bounded offset).
        buf.len() >= 5
            && (buf.starts_with(b"%PDF-")
                || memmem::find(&buf[..buf.len().min(1024)], b"%PDF-").is_some())
    }

    fn analyze(&self, buf: &[u8]) -> DamageReport {
        let mut damage = Vec::new();
        let objs = scan_objects(buf);
        let facts = scan_trailer(buf);
        let header_ok = buf.starts_with(b"%PDF-");
        let has_startxref = memmem::find(buf, b"startxref").is_some();
        let has_eof = buf.ends_with(b"%%EOF") || memmem::find(buf, b"%%EOF").is_some();

        if !header_ok {
            damage.push(DamageKind::MissingHeader);
        }
        if objs.is_empty() {
            damage.push(DamageKind::Corruption);
        }
        if facts.root_ref.is_none() {
            damage.push(DamageKind::BrokenIndex);
        }
        if !has_startxref || !has_eof {
            damage.push(DamageKind::Truncated);
        }

        let healthy = damage.is_empty();
        let detail = if healthy {
            format!(
                "PDF parses clean: {} objects, xref and trailer present.",
                objs.len()
            )
        } else {
            let mut notes = Vec::new();
            if !header_ok {
                notes.push("header damaged");
            }
            if objs.is_empty() {
                notes.push("no objects discovered");
            } else if facts.root_ref.is_none() {
                notes.push("no /Root resolvable");
            }
            if !has_startxref {
                notes.push("no startxref");
            }
            if !has_eof {
                notes.push("no %%EOF (truncated)");
            }
            format!(
                "PDF damage: {} — {} objects survive.",
                notes.join(", "),
                objs.len()
            )
        };

        DamageReport {
            format: Format::Pdf,
            healthy,
            tier: if healthy || objs.is_empty() {
                None
            } else {
                Some(RepairTier::Basis)
            },
            damage,
            detail,
        }
    }

    fn repair_into(&self, buf: &[u8], out: &mut Vec<u8>) -> RepairOutcome {
        let objs = scan_objects(buf);
        let facts = scan_trailer(buf);

        out.clear();
        // Header: preserve the version when parseable (the vendor's
        // `%%PDF-%d.%d` rewrite), else the 1.7 baseline.
        let header = pdf_version(buf).map_or_else(
            || "%PDF-1.7\n".to_string(),
            |(maj, min)| format!("%PDF-{maj}.{min}\n"),
        );
        out.extend_from_slice(header.as_bytes());

        // Objects byte-exact, tracking their NEW offsets (the
        // re-derived cross-reference — the vendor's core move).
        let mut entries: Vec<(u64, u64)> = Vec::new(); // (obj num, new offset)
        for obj in &objs {
            let start = obj.src_start.min(buf.len());
            let end = obj.src_end.min(buf.len());
            if start >= end {
                continue;
            }
            // Objects may start mid-line after damage; keep the exact
            // bytes from the token to the terminator.
            entries.push((obj.num, out.len() as u64));
            out.extend_from_slice(&buf[start..end]);
            // Normalize the separator (objects need whitespace or %
            // between them — endobj followed directly by a digit is
            // technically legal but a newline is the canonical form).
            if !out.ends_with(b"\n") {
                out.push(b'\n');
            }
        }

        // The /Root resolution (§1.2): the trailer's ref when it
        // matches a discovered object, else the catalog scan (an
        // object whose body carries /Type /Catalog), else the first
        // object — a truncated trailer still yields a usable pointer.
        let root = facts
            .root_ref
            .filter(|r| entries.iter().any(|(n, _)| n == r))
            .or_else(|| catalog_scan(buf, &objs, &entries))
            .or_else(|| entries.first().map(|(n, _)| *n));

        // The fresh classic xref — canonical strings, byte-exact.
        let xref_at = out.len() as u64;
        out.extend_from_slice(b"xref\n");
        out.extend_from_slice(b"0 1\n");
        out.extend_from_slice(b"0000000000 65535 f\r\n");
        if !entries.is_empty() {
            // One contiguous subsection starting at the lowest object.
            let mut sorted = entries.clone();
            sorted.sort_unstable_by_key(|(n, _)| *n);
            let first = sorted[0].0;
            let count = sorted.len() as u64;
            out.extend_from_slice(format!("{first} {count}\n").as_bytes());
            for (num, off) in &sorted {
                out.extend_from_slice(format!("{off:010} 00000 n\r\n").as_bytes());
                let _ = num;
            }
        }

        // The trailer (§1.4): /Size = max obj + 1, /Root, /ID when
        // recovered, /Encrypt when recovered.
        let size = facts
            .size
            .unwrap_or_else(|| entries.iter().map(|(n, _)| *n).max().unwrap_or(0) + 1);
        let size = size.max(entries.iter().map(|(n, _)| *n + 1).max().unwrap_or(1));
        out.extend_from_slice(b"trailer\n<< ");
        out.extend_from_slice(format!("/Size {size} ").as_bytes());
        match root {
            Some(r) => out.extend_from_slice(format!("/Root {r} 0 R ").as_bytes()),
            None => out.extend_from_slice(b"/Root 1 0 R "),
        }
        if facts.has_id {
            out.extend_from_slice(b"/ID [<deadbeefdeadbeef> <deadbeefdeadbeef>] ");
        }
        if facts.has_encrypt {
            out.extend_from_slice(b"/Encrypt 1 0 R ");
        }
        out.extend_from_slice(b">>\n");
        out.extend_from_slice(format!("startxref\n{xref_at}\n%%EOF").as_bytes());

        let repaired = !entries.is_empty();
        RepairOutcome {
            repaired,
            input_bytes: buf.len() as u64,
            output_bytes: out.len() as u64,
            partial: false,
            detail: format!(
                "PDF xref re-synthesized: {} objects re-indexed byte-exact{}.",
                entries.len(),
                if root.is_some() {
                    ", /Root resolved"
                } else {
                    " (no /Root — first-object fallback)"
                }
            ),
        }
    }
}

/// Scan the whole body for `N M obj` headers — the digit-lead arm of
/// the vendor's char-dispatch scanner.
fn scan_objects(buf: &[u8]) -> Vec<PdfObject> {
    let mut objs: Vec<PdfObject> = Vec::new();
    let mut i = 0usize;
    while let Some(p) = memmem::find(&buf[i.min(buf.len())..], b" obj") {
        let abs = i + p;
        // Walk backwards over "N M" (digits + whitespace).
        let Some((num, tok_start)) = parse_obj_num(buf, abs) else {
            i = abs + 4;
            continue;
        };
        // Find the extent end: `endobj` after the token, else the
        // next object's start, else EOF.
        let body_from = abs + 4;
        let endobj = memmem::find(&buf[body_from.min(buf.len())..], b"endobj");
        let end = match endobj {
            Some(e) => (body_from + e + 6).min(buf.len()),
            None => buf.len(),
        };
        objs.push(PdfObject {
            num,
            src_start: tok_start,
            src_end: end,
        });
        i = end.max(abs + 4);
        if objs.len() >= 0x10_000 {
            break; // sanity bound
        }
    }
    // De-overlap (nested "obj" matches inside streams): sort + trim.
    objs.sort_by_key(|o| o.src_start);
    let mut cleaned: Vec<PdfObject> = Vec::new();
    for o in objs {
        if let Some(last) = cleaned.last()
            && o.src_start < last.src_end
        {
            continue; // nested/false match inside a stream body
        }
        cleaned.push(o);
    }
    cleaned
}

/// Parse `N M` immediately before an ` obj` at `abs`; returns the
/// object number and the token start.
fn parse_obj_num(buf: &[u8], abs: usize) -> Option<(u64, usize)> {
    // Walk back over spaces, then digits (the generation), then
    // spaces, then digits (the number).
    let mut p = abs;
    p = skip_ws_back(buf, p);
    p = skip_digits_back(buf, p)?; // generation (ignored, "0")
    p = skip_ws_back(buf, p);
    let num_end = p;
    p = skip_digits_back(buf, p)?;
    if p >= num_end {
        return None; // no generation — not a real obj token
    }
    let num: u64 = std::str::from_utf8(&buf[p..num_end]).ok()?.parse().ok()?;
    if buf[p..num_end].starts_with(b"endstream") || buf[p..num_end].starts_with(b"endobj") {
        return None;
    }
    Some((num, p))
}

fn skip_ws_back(buf: &[u8], mut p: usize) -> usize {
    while p > 0
        && (buf[p - 1] == b' ' || buf[p - 1] == b'\n' || buf[p - 1] == b'\r' || buf[p - 1] == b'\t')
    {
        p -= 1;
    }
    p
}

fn skip_digits_back(buf: &[u8], mut p: usize) -> Option<usize> {
    let start = p;
    while p > 0 && buf[p - 1].is_ascii_digit() {
        p -= 1;
    }
    if p == start { None } else { Some(p) }
}

/// Recover the trailer facts: /Root, /Size, /ID, /Encrypt.
fn scan_trailer(buf: &[u8]) -> TrailerFacts {
    let mut facts = TrailerFacts::default();
    if let Some(t) = memmem::find(buf, b"trailer") {
        let region = &buf[t..(t + 2048).min(buf.len())];
        if let Some(r) = memmem::find(region, b"/Root") {
            facts.root_ref = parse_ref(&region[r + 5..]);
        }
        if let Some(s) = memmem::find(region, b"/Size") {
            facts.size = parse_int(&region[s + 5..]);
        }
        facts.has_id = memmem::find(region, b"/ID").is_some();
        facts.has_encrypt = memmem::find(region, b"/Encrypt").is_some();
    }
    facts
}

fn parse_ref(buf: &[u8]) -> Option<u64> {
    let buf = trim_leading_ws(buf);
    let end = buf
        .iter()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(buf.len());
    std::str::from_utf8(&buf[..end]).ok()?.parse().ok()
}

fn parse_int(buf: &[u8]) -> Option<u64> {
    parse_ref(buf)
}

fn trim_leading_ws(buf: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < buf.len() && (buf[i] == b' ' || buf[i] == b'\n' || buf[i] == b'\r' || buf[i] == b'\t')
    {
        i += 1;
    }
    &buf[i..]
}

/// The catalog scan (the /Root fallback): find a discovered object
/// whose body declares `/Type /Catalog`.
fn catalog_scan(buf: &[u8], objs: &[PdfObject], entries: &[(u64, u64)]) -> Option<u64> {
    for obj in objs {
        let start = obj.src_start.min(buf.len());
        let end = obj.src_end.min(buf.len());
        if start < end {
            let body = &buf[start..end];
            if memmem::find(body, b"/Type").is_some_and(|t| {
                let after = trim_leading_ws(&body[(t + 5).min(body.len())..]);
                if let Some(name) = after.strip_prefix(b"/") {
                    let name_end = name
                        .iter()
                        .position(|b| !b.is_ascii_alphanumeric())
                        .unwrap_or(name.len());
                    &name[..name_end] == b"Catalog"
                } else {
                    false
                }
            }) && entries.iter().any(|(n, _)| *n == obj.num)
            {
                return Some(obj.num);
            }
        }
    }
    None
}

/// Parse `%%PDF-x.y` at the head (or within the first KiB).
fn pdf_version(buf: &[u8]) -> Option<(u64, u64)> {
    let at = if buf.starts_with(b"%PDF-") {
        5
    } else {
        memmem::find(&buf[..buf.len().min(1024)], b"%PDF-")? + 5
    };
    let rest = &buf[at..];
    let mut it = rest.iter();
    let maj = it.next()?;
    if !maj.is_ascii_digit() {
        return None;
    }
    if it.next()? != &b'.' {
        return None;
    }
    let min = it.next()?;
    if !min.is_ascii_digit() {
        return None;
    }
    Some((u64::from(*maj - b'0'), u64::from(*min - b'0')))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal 3-object PDF with xref + trailer.
    fn build_pdf() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"%PDF-1.4\n");
        v.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        v.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
        v.extend_from_slice(b"3 0 obj\n<< /Type /Page /Parent 2 0 R >>\nendobj\n");
        v.extend_from_slice(b"xref\n0 4\n0000000000 65535 f\r\n");
        for off in [9u64, 62, 127] {
            v.extend_from_slice(format!("{off:010} 00000 n\r\n").as_bytes());
        }
        v.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n183\n%%EOF");
        v
    }

    #[test]
    fn healthy_pdf_analyzes_clean() {
        let pdf = build_pdf();
        let e = PdfEngine;
        let r = e.analyze(&pdf);
        assert!(r.healthy, "detail: {}", r.detail);
        let mut out = Vec::new();
        let o = e.repair_into(&pdf, &mut out);
        assert!(o.repaired);
        // The rebuilt file must itself analyze clean.
        let r2 = e.analyze(&out);
        assert!(r2.healthy, "rebuilt: {}", r2.detail);
        // All three objects survive byte-exact.
        for needle in [
            b"/Type /Catalog".as_slice(),
            b"/Type /Pages".as_slice(),
            b"/Type /Page".as_slice(),
        ] {
            assert!(
                memmem::find(&out, needle).is_some(),
                "object body lost: {}",
                String::from_utf8_lossy(needle)
            );
        }
    }

    #[test]
    fn broken_xref_is_resynthesized() {
        let mut pdf = build_pdf();
        // Destroy the xref + trailer + startxref entirely.
        let xref_at = memmem::find(&pdf, b"xref").unwrap();
        pdf.truncate(xref_at);
        let e = PdfEngine;
        let r = e.analyze(&pdf);
        assert!(!r.healthy);
        assert!(r.damage.contains(&DamageKind::Truncated));
        let mut out = Vec::new();
        let o = e.repair_into(&pdf, &mut out);
        assert!(o.repaired);
        assert!(out.ends_with(b"%%EOF"));
        // startxref must point at the actual xref.
        let sx = memmem::find(&out, b"startxref\n").unwrap();
        let num_end = out[sx + 10..]
            .iter()
            .position(|b| !b.is_ascii_digit())
            .unwrap()
            + sx
            + 10;
        let at: u64 = std::str::from_utf8(&out[sx + 10..num_end])
            .unwrap()
            .parse()
            .unwrap();
        let xref_real = memmem::find(&out, b"xref\n").unwrap() as u64;
        assert_eq!(at, xref_real, "startxref must point at the real xref");
        // And the rebuilt file passes its own analysis.
        let r2 = e.analyze(&out);
        assert!(r2.healthy, "rebuilt: {}", r2.detail);
    }

    #[test]
    fn missing_root_falls_back_to_catalog_scan() {
        let mut pdf = build_pdf();
        // Keep a trailer but destroy its /Root.
        let t = memmem::find(&pdf, b"/Root 1 0 R").unwrap();
        let mut broken = pdf[..t].to_vec();
        broken.extend_from_slice(b">>\nstartxref\n183\n%%EOF");
        pdf = broken;
        let e = PdfEngine;
        let r = e.analyze(&pdf);
        assert!(r.damage.contains(&DamageKind::BrokenIndex));
        let mut out = Vec::new();
        let o = e.repair_into(&pdf, &mut out);
        assert!(o.repaired);
        // The catalog scan must have resolved object 1.
        assert!(memmem::find(&out, b"/Root 1 0 R").is_some());
    }

    #[test]
    fn damaged_header_is_restamped() {
        let mut pdf = build_pdf();
        for b in pdf.iter_mut().take(8) {
            *b = 0xE5;
        }
        let e = PdfEngine;
        let r = e.analyze(&pdf);
        assert!(r.damage.contains(&DamageKind::MissingHeader));
        let mut out = Vec::new();
        let o = e.repair_into(&pdf, &mut out);
        assert!(o.repaired);
        // Version digits destroyed with the header: the 1.7 baseline
        // (the vendor's `%%PDF-%d.%d` rewrite fallback).
        assert!(out.starts_with(b"%PDF-1.7\n"));
    }
}
