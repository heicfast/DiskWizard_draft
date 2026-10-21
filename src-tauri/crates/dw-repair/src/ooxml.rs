//! OOXML (DOCX/XLSX/PPTX) repair — the OPC container rebuild.
//!
//! Research: `artifacts/FileRepair/deepening-batch2--pdf-office-repair.md`
//! §2.2 + `architecture/06-repair-chain.md` §4:
//!
//! - `EuOfficeRepairWin32` "can synthesize a fresh OOXML package
//!   shell around salvaged parts" — the `[Content_Types].xml`
//!   rebuild with byte-exact entry templates
//!   `"<Default Extension=\"%s\" ContentType=\"%s\"/>"` and
//!   `"<Override PartName=\"%s\" ContentType=\"%s\"/>"`.
//! - Part-name registry: `word/document`, `word/fontTable`,
//!   `word/header`, `word/footer`, `word/footnotes`,
//!   `word/endnotes`, `word/styles.xml`, `word/media/` (the xl/ppt
//!   sets are the same registry's siblings).
//! - Damaged local file headers are re-synced against the central
//!   directory; the manifest is re-derived from the parts that
//!   survive.
//!
//! Repair strategy (the index-repair family — same as PDF xref / MOV
//! moov): **the central directory is the truth**; when it survives we
//! rewrite every local header from it, when it is gone we rebuild it
//! from the surviving local headers; the manifest is re-derived from
//! the actual part inventory either way. Part payloads are preserved
//! byte-exact (still-deflated — no re-compression, no re-inflation of
//! healthy streams).

use crate::{DamageKind, DamageReport, Format, RepairEngine, RepairOutcome, RepairTier};
use memchr::memmem;

/// Local file header magic `PK\x03\x04`.
const LFH_MAGIC: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
/// Central directory entry magic `PK\x01\x02`.
const CDE_MAGIC: [u8; 4] = [0x50, 0x4B, 0x01, 0x02];
/// End-of-central-directory magic `PK\x05\x06`.
const EOCD_MAGIC: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];

/// One part recovered from the container.
#[derive(Debug, Clone)]
struct Part {
    name: String,
    /// The still-compressed payload (byte-exact preservation).
    data: Vec<u8>,
    /// Compression method (0 stored / 8 deflate).
    method: u16,
    crc32: u32,
    /// Uncompressed size (from whichever record was trusted).
    uncompressed: u32,
}

/// The OOXML repair engine.
pub struct OoxmlEngine;

impl RepairEngine for OoxmlEngine {
    fn format(&self) -> Format {
        Format::Ooxml
    }

    fn check_magic(&self, buf: &[u8]) -> bool {
        // OOXML = a ZIP whose first entry is typically
        // [Content_Types].xml stored. Accept the ZIP magic with an
        // OOXML telltale nearby.
        if buf.len() >= 4 && buf[..4] == LFH_MAGIC {
            return memmem::find(&buf[..buf.len().min(4096)], b"[Content_Types].xml").is_some()
                || memmem::find(&buf[..buf.len().min(4096)], b"word/document.xml").is_some()
                || memmem::find(&buf[..buf.len().min(4096)], b"xl/workbook.xml").is_some()
                || memmem::find(&buf[..buf.len().min(4096)], b"ppt/presentation.xml").is_some();
        }
        false
    }

    fn analyze(&self, buf: &[u8]) -> DamageReport {
        let mut damage = Vec::new();
        let (parts, cd_survives) = inventory(buf, &mut damage);
        let has_manifest = parts.iter().any(|p| p.name == "[Content_Types].xml");
        if !cd_survives {
            damage.push(DamageKind::BrokenIndex);
        }
        if !has_manifest {
            damage.push(DamageKind::MissingHeader);
        }
        if parts.is_empty() {
            damage.push(DamageKind::Corruption);
        }
        let healthy = damage.is_empty();
        let detail = if healthy {
            format!(
                "OOXML package parses clean: {} parts, central directory intact.",
                parts.len()
            )
        } else {
            let mut notes = Vec::new();
            if !cd_survives {
                notes.push("central directory damaged/missing");
            }
            if !has_manifest {
                notes.push("[Content_Types].xml missing (will be re-derived)");
            }
            format!(
                "OOXML damage: {} — {} parts survive.",
                notes.join(", "),
                parts.len()
            )
        };
        DamageReport {
            format: Format::Ooxml,
            healthy,
            tier: if healthy || parts.is_empty() {
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
        let (parts, _) = inventory(buf, &mut damage);
        out.clear();

        // The re-derived manifest (§2.2): Default entries by extension
        // + Override entries for the known part registry.
        let manifest = synthesize_manifest(&parts);

        // Rebuild order: [Content_Types].xml first (the OPC
        // convention), then every surviving part (stable, original
        // order), then the fresh central directory + EOCD.
        let mut names: Vec<(String, u16, u32, u32, Vec<u8>)> = Vec::new();
        if !parts.iter().any(|p| p.name == "[Content_Types].xml") {
            names.push((
                "[Content_Types].xml".to_string(),
                8,
                crc32fast::hash(manifest.as_bytes()),
                manifest.len() as u32,
                deflate_bytes(manifest.as_bytes()),
            ));
        }
        for p in &parts {
            let data = p.data.clone();
            names.push((p.name.clone(), p.method, p.crc32, p.uncompressed, data));
        }

        let mut central: Vec<u8> = Vec::new();
        let mut offset: u32 = 0;
        for (name, method, crc, uncompressed, data) in &names {
            // Local file header, rewritten from the trusted inventory.
            let name_b = name.as_bytes();
            out.extend_from_slice(&LFH_MAGIC);
            out.extend_from_slice(&20u16.to_le_bytes()); // version
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // time
            out.extend_from_slice(&0u16.to_le_bytes()); // date
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&uncompressed.to_le_bytes());
            out.extend_from_slice(&(name_b.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            out.extend_from_slice(name_b);
            out.extend_from_slice(data);
            // Central directory entry (the full 46-byte fixed part).
            central.extend_from_slice(&CDE_MAGIC);
            central.extend_from_slice(&20u16.to_le_bytes()); // made by
            central.extend_from_slice(&20u16.to_le_bytes()); // needed
            central.extend_from_slice(&0u16.to_le_bytes()); // flags
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&uncompressed.to_le_bytes());
            central.extend_from_slice(&(name_b.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes()); // extra
            central.extend_from_slice(&0u16.to_le_bytes()); // comment
            central.extend_from_slice(&0u16.to_le_bytes()); // disk
            central.extend_from_slice(&0u16.to_le_bytes()); // int attrs
            central.extend_from_slice(&0u32.to_le_bytes()); // ext attrs
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name_b);
            offset = out.len() as u32;
        }
        let cd_offset = out.len() as u32;
        let cd_size = central.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&EOCD_MAGIC);
        out.extend_from_slice(&0u16.to_le_bytes()); // disk
        out.extend_from_slice(&0u16.to_le_bytes()); // cd disk
        out.extend_from_slice(&(names.len() as u16).to_le_bytes());
        out.extend_from_slice(&(names.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment len

        let detail = if parts.is_empty() {
            "OOXML BeyondRepair: no parts recoverable.".to_string()
        } else {
            format!(
                "OOXML package rebuilt: {} parts preserved byte-exact, central directory + manifest re-derived.",
                names.len()
            )
        };
        RepairOutcome {
            repaired: !names.is_empty(),
            input_bytes: buf.len() as u64,
            output_bytes: out.len() as u64,
            partial: parts.is_empty(),
            detail,
        }
    }
}

/// Recover the part inventory: central-directory-first (the truth),
/// local-header fallback when the CD is gone (the resync the vendor
/// performs against the CD; reversed when the CD is the casualty).
fn inventory(buf: &[u8], damage: &mut Vec<DamageKind>) -> (Vec<Part>, bool) {
    // Pass 1: the central directory.
    let mut cd_parts: Vec<Part> = Vec::new();
    let mut cd_ok = false;
    let mut i = 0usize;
    while let Some(p) = memmem::find(&buf[i..], &CDE_MAGIC) {
        let abs = i + p;
        if let Some(part) = parse_cde(buf, abs) {
            cd_ok = true;
            cd_parts.push(part);
            i = abs + 46;
        } else {
            i = abs + 4;
        }
        if cd_parts.len() >= 0x10_000 {
            break;
        }
    }
    if cd_ok {
        // Materialize the payloads via the CD's offsets.
        let mut parts = Vec::new();
        for p in &cd_parts {
            if let Some(data) = payload_at(buf, &p.name, p.method) {
                parts.push(p.clone_with_data(data));
            } else {
                damage.push(DamageKind::Corruption);
            }
        }
        if !parts.is_empty() {
            return (parts, true);
        }
    }
    // Pass 2: local headers.
    let mut parts = Vec::new();
    let mut i = 0usize;
    while let Some(p) = memmem::find(&buf[i..], &LFH_MAGIC) {
        let abs = i + p;
        if let Some(part) = parse_lfh(buf, abs) {
            parts.push(part);
            i = abs + 4;
        } else {
            damage.push(DamageKind::Corruption);
            i = abs + 4;
        }
        if parts.len() >= 0x10_000 {
            break;
        }
    }
    (parts, false)
}

impl Part {
    fn clone_with_data(&self, data: Vec<u8>) -> Self {
        Self {
            name: self.name.clone(),
            data,
            method: self.method,
            crc32: self.crc32,
            uncompressed: self.uncompressed,
        }
    }
}

/// Parse one central-directory entry into a Part (data filled later).
fn parse_cde(buf: &[u8], at: usize) -> Option<Part> {
    if at + 46 > buf.len() {
        return None;
    }
    let method = u16::from_le_bytes([buf[at + 10], buf[at + 11]]);
    let crc = u32::from_le_bytes([buf[at + 16], buf[at + 17], buf[at + 18], buf[at + 19]]);
    let uncompressed = u32::from_le_bytes([buf[at + 24], buf[at + 25], buf[at + 26], buf[at + 27]]);
    let name_len = u16::from_le_bytes([buf[at + 28], buf[at + 29]]) as usize;
    if at + 46 + name_len > buf.len() {
        return None;
    }
    let name = String::from_utf8_lossy(&buf[at + 46..at + 46 + name_len]).into_owned();
    if name.is_empty() || name.contains('\u{0}') {
        return None;
    }
    Some(Part {
        name,
        data: Vec::new(),
        method,
        crc32: crc,
        uncompressed,
    })
}

/// Parse one local file header into a Part with its payload.
fn parse_lfh(buf: &[u8], at: usize) -> Option<Part> {
    if at + 30 > buf.len() {
        return None;
    }
    let method = u16::from_le_bytes([buf[at + 8], buf[at + 9]]);
    let crc = u32::from_le_bytes([buf[at + 14], buf[at + 15], buf[at + 16], buf[at + 17]]);
    let csize =
        u32::from_le_bytes([buf[at + 18], buf[at + 19], buf[at + 20], buf[at + 21]]) as usize;
    let uncompressed = u32::from_le_bytes([buf[at + 22], buf[at + 23], buf[at + 24], buf[at + 25]]);
    let name_len = u16::from_le_bytes([buf[at + 26], buf[at + 27]]) as usize;
    let extra_len = u16::from_le_bytes([buf[at + 28], buf[at + 29]]) as usize;
    let data_start = at + 30 + name_len + extra_len;
    if data_start + csize > buf.len() {
        return None; // truncated payload — this entry does not survive
    }
    let name = String::from_utf8_lossy(&buf[at + 30..at + 30 + name_len]).into_owned();
    if name.is_empty() || name.contains('\u{0}') {
        return None;
    }
    Some(Part {
        name,
        data: buf[data_start..data_start + csize].to_vec(),
        method,
        crc32: crc,
        uncompressed,
    })
}

/// Fetch a part's payload by name: the local header whose name
/// matches (the CD-vs-LFH resync — the CD's offsets are trusted for
/// ordering, the LFH for the actual bytes).
fn payload_at(buf: &[u8], name: &str, _method: u16) -> Option<Vec<u8>> {
    let mut i = 0usize;
    while let Some(p) = memmem::find(&buf[i..], &LFH_MAGIC) {
        let abs = i + p;
        if let Some(part) = parse_lfh(buf, abs)
            && part.name == name
        {
            return Some(part.data);
        }
        i = abs + 4;
    }
    None
}

/// Synthesize `[Content_Types].xml` from the surviving parts — the
/// byte-exact entry templates from the decompilation.
fn synthesize_manifest(parts: &[Part]) -> String {
    let mut defaults: Vec<String> = Vec::new();
    let mut overrides: Vec<String> = Vec::new();
    let mut seen_default: Vec<String> = Vec::new();
    for p in parts {
        let Some(ext) = p.name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()) else {
            continue;
        };
        let default_type = match ext.as_str() {
            "rels" => "application/vnd.openxmlformats-package.relationships+xml",
            "xml" => "application/xml",
            _ => continue,
        };
        let entry = format!("<Default Extension=\"{ext}\" ContentType=\"{default_type}\"/>");
        if !seen_default.contains(&ext) {
            seen_default.push(ext.clone());
            defaults.push(entry);
        }
    }
    for p in parts {
        let ct = part_content_type(&p.name);
        if let Some(ct) = ct {
            overrides.push(format!(
                "<Override PartName=\"/{}\" ContentType=\"{ct}\"/>",
                p.name
            ));
        }
    }
    // rels+xml must exist even when no .rels part survived (the
    // package always carries the relationship default).
    if !seen_default.contains(&"rels".to_string()) {
        defaults.insert(
            0,
            "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>"
                .to_string(),
        );
    }
    if !seen_default.contains(&"xml".to_string()) {
        defaults.insert(
            0,
            "<Default Extension=\"xml\" ContentType=\"application/xml\"/>".to_string(),
        );
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">{}{}</Types>",
        defaults.concat(),
        overrides.concat()
    )
}

/// The part-name registry (§2.2): the canonical content types for the
/// parts the Office apps require to open a package.
fn part_content_type(name: &str) -> Option<&'static str> {
    Some(match name {
        "word/document.xml" => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
        }
        "word/styles.xml" => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"
        }
        "word/fontTable.xml" => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"
        }
        "word/settings.xml" => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"
        }
        "word/footnotes.xml" => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"
        }
        "word/endnotes.xml" => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml"
        }
        "xl/workbook.xml" => {
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"
        }
        "xl/sharedStrings.xml" => {
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"
        }
        "xl/styles.xml" => "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml",
        "ppt/presentation.xml" => {
            "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"
        }
        "docProps/core.xml" => "application/vnd.openxmlformats-package.core-properties+xml",
        "docProps/app.xml" => {
            "application/vnd.openxmlformats-officedocument.extended-properties+xml"
        }
        _ => return None,
    })
}

/// Deflate for the synthesized manifest.
fn deflate_bytes(data: &[u8]) -> Vec<u8> {
    use flate2::{Compression, write::DeflateEncoder};
    use std::io::Write;
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
    let _ = enc.write_all(data);
    enc.finish().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal DOCX-shaped package: [Content_Types].xml (stored) +
    /// word/document.xml (deflated) + central directory + EOCD.
    fn build_docx() -> Vec<u8> {
        let manifest = "<?xml version=\"1.0\"?><Types><Default Extension=\"xml\" ContentType=\"application/xml\"/></Types>";
        let doc = b"<w:document xmlns:w=\"ns\"><w:body><w:p/></w:body></w:document>";
        let doc_deflated = deflate_bytes(doc);

        let mut v = Vec::new();
        let mut central = Vec::new();
        let entries: [(&str, u16, u32, u32, &[u8]); 2] = [
            (
                "[Content_Types].xml",
                0,
                crc32fast::hash(manifest.as_bytes()),
                manifest.len() as u32,
                manifest.as_bytes(),
            ),
            (
                "word/document.xml",
                8,
                crc32fast::hash(doc),
                doc.len() as u32,
                &doc_deflated,
            ),
        ];
        let mut offset = 0u32;
        for (name, method, crc, unc, data) in entries {
            v.extend_from_slice(&LFH_MAGIC);
            v.extend_from_slice(&20u16.to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes());
            v.extend_from_slice(&method.to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes());
            v.extend_from_slice(&crc.to_le_bytes());
            v.extend_from_slice(&(data.len() as u32).to_le_bytes());
            v.extend_from_slice(&unc.to_le_bytes());
            v.extend_from_slice(&(name.len() as u16).to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes());
            v.extend_from_slice(name.as_bytes());
            v.extend_from_slice(data);
            central.extend_from_slice(&CDE_MAGIC);
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes()); // flags
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&unc.to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
            offset = v.len() as u32;
        }
        v.extend_from_slice(&central);
        v.extend_from_slice(&EOCD_MAGIC);
        v.extend_from_slice(&0u16.to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&(central.len() as u32).to_le_bytes());
        v.extend_from_slice(&((v.len() - central.len()) as u32).to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes());
        v
    }

    #[test]
    fn healthy_package_passes_through() {
        let pkg = build_docx();
        let e = OoxmlEngine;
        assert!(e.check_magic(&pkg));
        let r = e.analyze(&pkg);
        assert!(r.healthy, "detail: {}", r.detail);
        let mut out = Vec::new();
        let o = e.repair_into(&pkg, &mut out);
        assert!(o.repaired && !o.partial);
        let r2 = e.analyze(&out);
        assert!(r2.healthy, "rebuilt: {}", r2.detail);
        // The document payload survives byte-exact (still deflated).
        assert!(
            memmem::find(
                &out,
                &deflate_bytes(b"<w:document xmlns:w=\"ns\"><w:body><w:p/></w:body></w:document>")
            )
            .is_some()
        );
    }

    #[test]
    fn destroyed_central_directory_is_rebuilt() {
        let pkg = build_docx();
        // Truncate the CD + EOCD away entirely.
        let cd_at = memmem::find(&pkg, &CDE_MAGIC).unwrap();
        let cut = &pkg[..cd_at];
        let e = OoxmlEngine;
        let r = e.analyze(cut);
        assert!(r.damage.contains(&DamageKind::BrokenIndex));
        let mut out = Vec::new();
        let o = e.repair_into(cut, &mut out);
        assert!(o.repaired);
        let r2 = e.analyze(&out);
        assert!(r2.healthy, "rebuilt: {}", r2.detail);
        assert!(memmem::find(&out, b"word/document.xml").is_some());
    }

    #[test]
    fn missing_manifest_is_resynthesized() {
        let pkg = build_docx();
        // Cut the FIRST local header ([Content_Types].xml) out — the
        // package starts at word/document.xml's LFH.
        let second_at = memmem::find(&pkg, b"PK\x03\x04\x14\x00\x00\x00\x08\x00").unwrap();
        let cut = &pkg[second_at..];
        let e = OoxmlEngine;
        // check_magic needs the OOXML telltale — document.xml counts.
        assert!(e.check_magic(cut));
        let r = e.analyze(cut);
        assert!(r.damage.contains(&DamageKind::MissingHeader));
        let mut out = Vec::new();
        let o = e.repair_into(cut, &mut out);
        assert!(o.repaired);
        let r2 = e.analyze(&out);
        assert!(r2.healthy, "manifest re-derived: {}", r2.detail);
        // The manifest must carry the document Override — it rides
        // DEFLATED in the rebuilt package, so inflate it first.
        let manifest = extract_part(&out, "[Content_Types].xml").expect("manifest part");
        let plain = inflate(&manifest);
        let text = String::from_utf8_lossy(&plain);
        assert!(
            text.contains("wordprocessingml.document.main+xml"),
            "override missing: {text}"
        );
    }

    /// Extract a part's (still-compressed) payload from a package.
    fn extract_part(pkg: &[u8], name: &str) -> Option<Vec<u8>> {
        let mut i = 0usize;
        while let Some(p) = memmem::find(&pkg[i..], &LFH_MAGIC) {
            let abs = i + p;
            if let Some(part) = parse_lfh(pkg, abs)
                && part.name == name
            {
                return Some(part.data);
            }
            i = abs + 4;
        }
        None
    }

    fn inflate(data: &[u8]) -> Vec<u8> {
        use std::io::Read;
        let mut out = Vec::new();
        let _ = flate2::read::DeflateDecoder::new(data).read_to_end(&mut out);
        out
    }

    #[test]
    fn manifest_templates_are_byte_exact() {
        let parts = vec![Part {
            name: "word/document.xml".to_string(),
            data: Vec::new(),
            method: 8,
            crc32: 0,
            uncompressed: 0,
        }];
        let m = synthesize_manifest(&parts);
        assert!(m.contains("<Default Extension=\"xml\" ContentType=\"application/xml\"/>"));
        assert!(m.contains("<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>"));
    }
}
