//! # dw-repair — the DiskWizard file-repair engine chain
//!
//! The reverse-engineered Fixo repair family translated to Rust as
//! in-process engines (phase-2 §5: "our engines are Rust crates, not
//! the vendor's out-of-proc EUFileRepair hosts").
//!
//! Research lineage (every engine cites its artifact):
//! - `artifacts/FileRepair/repair-architecture.md` — the component map
//!   + the two-tier philosophy (BasisRepair vs SampleRepair).
//! - `artifacts/FileRepair/image-repair--format-grammar.md` §4/§12 —
//!   the JPEG marker grammar + the PNG chunk/scan machinery.
//! - `artifacts/FileRepair/image-repair--gif-bmp-svg-tiff-walks.md` —
//!   BMP DIB taxonomy, GIF extent re-chain, SVG envelope, TIFF endian
//!   arbitration.
//! - `artifacts/FileRepair/deepening-batch2--pdf-office-repair.md` —
//!   PDF xref/trailer synthesis (canonical format strings byte-exact)
//!   + the OOXML `[Content_Types].xml` manifest rebuild.
//! - `architecture/06-repair-chain.md` §3 — the MP4 weighted atom
//!   grammar + two-scan architecture.
//!
//! ## The IImageInfo contract, translated
//!
//! The vendor's 5-slot virtual contract (state-machines §12.1) maps to
//! Rust as: `Drop` (dtor), [`RepairEngine::check_magic`] (CheckMagic),
//! [`RepairEngine::analyze`] (AnalysisData), a factory registry
//! ([`engine_for`]) and [`RepairEngine::repair_into`] (SaveData).
//!
//! ## The two-tier model
//!
//! Every engine offers **BasisRepair** (structure intact → revalidate
//! and patch: re-stamp CRCs, re-derive lengths, synthesize the missing
//! trailer) and **SampleRepair** (structure broken → salvage the
//! payload stream, synthesize a fresh container around it) — the
//! corruption-first philosophy the research documents across all
//! formats, "scan-then-validate, never trust structure".
//!
//! ## Honesty contract (phase-2 §5)
//!
//! [`DamageReport`] verdicts are exact — "repaired prefix, X% of
//! stream recovered", never a silent half-file. Undamaged payload
//! bytes are preserved byte-exact by every BasisRepair path; the
//! sample tier says precisely what it salvaged.

use serde::Serialize;

pub mod bmp;
pub mod gif;
pub mod jpeg;
pub mod mp4;
pub mod ooxml;
pub mod pdf;
pub mod png;
pub mod svg;
pub mod tiff;
pub mod webp;

/// The repairable format families (the per-type wizards of the
/// FileRepairHomePage, phase-2 §1-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Jpeg,
    Png,
    Gif,
    Bmp,
    Tiff,
    Webp,
    Svg,
    Pdf,
    Ooxml,
    Mp4,
}

impl Format {
    /// The human-facing family name (the repair-wizard tab labels).
    #[must_use]
    pub fn family(self) -> &'static str {
        match self {
            Format::Jpeg
            | Format::Png
            | Format::Gif
            | Format::Bmp
            | Format::Tiff
            | Format::Webp
            | Format::Svg => "Photo",
            Format::Pdf | Format::Ooxml => "Document",
            Format::Mp4 => "Video",
        }
    }
}

/// What is wrong (the analysis vocabulary; each engine reports the
/// kinds its grammar can name — the vendor's per-stage failure logs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DamageKind {
    /// The stream ends mid-structure (no trailer/EOI/IEND/%%EOF).
    Truncated,
    /// A structural marker is corrupt or inconsistent.
    BadMarker,
    /// The container index is broken (PNG chunk chain, PDF xref,
    /// ZIP central directory, MP4 box sizes).
    BrokenIndex,
    /// A checksum mismatch (PNG chunk CRC, ZIP CRC).
    ChecksumBad,
    /// The header/magic itself is damaged.
    MissingHeader,
    /// Mid-stream corruption the walk had to resync over.
    Corruption,
}

/// The repair tier the analysis selected (the vendor's BasisRepair vs
/// SampleRepair dispatch — state-machines §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RepairTier {
    /// Structure intact: revalidate + patch in place.
    Basis,
    /// Structure broken: salvage the payload, synthesize a container.
    Sample,
}

/// The exact analysis verdict (phase-2 §5 honesty).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DamageReport {
    pub format: Format,
    /// The file parses clean — nothing to do.
    pub healthy: bool,
    /// The selected tier when repair applies.
    pub tier: Option<RepairTier>,
    /// The named damage classes found.
    pub damage: Vec<DamageKind>,
    /// The honest one-line verdict (surfaces verbatim in the UI).
    pub detail: String,
}

/// The result of a repair pass.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairOutcome {
    /// A usable container was produced.
    pub repaired: bool,
    pub input_bytes: u64,
    pub output_bytes: u64,
    /// True when the output is a salvaged prefix rather than the
    /// whole original payload (the "repaired prefix" honesty flag).
    pub partial: bool,
    /// Exact statement of what the repair did.
    pub detail: String,
}

/// One per-format repair engine (the `C<Format>Info` contract).
pub trait RepairEngine: Send + Sync {
    /// The format this engine owns.
    fn format(&self) -> Format;

    /// CheckMagic: does this buffer belong to this format?
    fn check_magic(&self, buf: &[u8]) -> bool;

    /// AnalysisData: parse + validate, name the damage exactly.
    fn analyze(&self, buf: &[u8]) -> DamageReport;

    /// SaveData: produce the repaired container in `out` (cleared
    /// first). Never touches the input; writes only what the caller
    /// supplies (phase-2 §5 safety: repair never writes in place).
    fn repair_into(&self, buf: &[u8], out: &mut Vec<u8>) -> RepairOutcome;
}

/// Run one repair through the registry.
///
/// # Errors
/// String error when NO engine claims the buffer's signature (the
/// CHKRepairFileSign role — dispatch refuses unknown formats).
pub fn repair(buf: &[u8], out: &mut Vec<u8>) -> Result<RepairOutcome, String> {
    let engine = engine_for(buf).ok_or_else(|| {
        format!(
            "UNSUPPORTED_FORMAT. No repair engine claims this signature ({} bytes).",
            buf.len()
        )
    })?;
    out.clear();
    Ok(engine.repair_into(buf, out))
}

/// Analyze through the registry (the preview/preview-failed path).
///
/// # Errors
/// String error when no engine claims the signature.
pub fn analyze(buf: &[u8]) -> Result<DamageReport, String> {
    let engine =
        engine_for(buf).ok_or_else(|| "UNSUPPORTED_FORMAT. Unknown signature.".to_string())?;
    Ok(engine.analyze(buf))
}

/// The engine registry — signature dispatch in vendor priority order
/// (photo first, then documents, then video — the FileRepairHomePage
/// wizard ordering; MP4 last because ISO-BMFF magic is the loosest).
#[must_use]
pub fn engine_for(buf: &[u8]) -> Option<&'static dyn RepairEngine> {
    const JPEG: jpeg::JpegEngine = jpeg::JpegEngine;
    const PNG: png::PngEngine = png::PngEngine;
    const GIF: gif::GifEngine = gif::GifEngine;
    const BMP: bmp::BmpEngine = bmp::BmpEngine;
    const TIFF: tiff::TiffEngine = tiff::TiffEngine;
    const WEBP: webp::WebpEngine = webp::WebpEngine;
    const SVG: svg::SvgEngine = svg::SvgEngine;
    const PDF: pdf::PdfEngine = pdf::PdfEngine;
    const OOXML: ooxml::OoxmlEngine = ooxml::OoxmlEngine;
    const MP4: mp4::Mp4Engine = mp4::Mp4Engine;

    let engines: [&'static dyn RepairEngine; 10] = [
        &JPEG, &PNG, &GIF, &BMP, &TIFF, &WEBP, &SVG, &PDF, &OOXML, &MP4,
    ];
    engines.into_iter().find(|e| e.check_magic(buf))
}

/// Convenience: does a buffer carry ANY repairable signature?
#[must_use]
pub fn is_repairable_signature(buf: &[u8]) -> bool {
    engine_for(buf).is_some()
}

/// The file-extension → format map (the results-page repair context
/// action keys on FileFilter.xml categories; ours keys on names).
#[must_use]
pub fn format_for_name(name: &str) -> Option<Format> {
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => Some(Format::Jpeg),
        "png" | "apng" => Some(Format::Png),
        "gif" => Some(Format::Gif),
        "bmp" | "dib" => Some(Format::Bmp),
        "tif" | "tiff" => Some(Format::Tiff),
        "webp" => Some(Format::Webp),
        "svg" => Some(Format::Svg),
        "pdf" => Some(Format::Pdf),
        "docx" | "xlsx" | "pptx" => Some(Format::Ooxml),
        "mp4" | "mov" | "m4v" => Some(Format::Mp4),
        _ => None,
    }
}

/// Build the honest detail string shared by salvage paths: what
/// fraction of the payload survived (the "repaired prefix, X% of
/// stream recovered" contract).
#[must_use]
pub fn salvage_detail(kind: &str, recovered: u64, total: u64) -> String {
    if total == 0 {
        return format!("{kind}: nothing recoverable");
    }
    let pct = (recovered * 100 / total).min(100);
    format!("{kind}: repaired prefix, {pct}% of stream recovered")
}
