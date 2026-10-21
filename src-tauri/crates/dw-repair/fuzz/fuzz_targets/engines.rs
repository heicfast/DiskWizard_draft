//! The hostile-corpus repair fuzz: arbitrary bytes through the
//! registry dispatch AND through every engine directly (a selector
//! byte picks the engine so magic-rejecting inputs still cover each
//! grammar). Invariants: no panic, bounded output, repair never
//! touches the input.
#![no_main]

use dw_repair::RepairEngine;
use libfuzzer_sys::fuzz_target;

const JPEG: dw_repair::jpeg::JpegEngine = dw_repair::jpeg::JpegEngine;
const PNG: dw_repair::png::PngEngine = dw_repair::png::PngEngine;
const GIF: dw_repair::gif::GifEngine = dw_repair::gif::GifEngine;
const BMP: dw_repair::bmp::BmpEngine = dw_repair::bmp::BmpEngine;
const TIFF: dw_repair::tiff::TiffEngine = dw_repair::tiff::TiffEngine;
const WEBP: dw_repair::webp::WebpEngine = dw_repair::webp::WebpEngine;
const SVG: dw_repair::svg::SvgEngine = dw_repair::svg::SvgEngine;
const PDF: dw_repair::pdf::PdfEngine = dw_repair::pdf::PdfEngine;
const OOXML: dw_repair::ooxml::OoxmlEngine = dw_repair::ooxml::OoxmlEngine;
const MP4: dw_repair::mp4::Mp4Engine = dw_repair::mp4::Mp4Engine;

fn engines() -> [&'static dyn RepairEngine; 10] {
    [
        &JPEG, &PNG, &GIF, &BMP, &TIFF, &WEBP, &SVG, &PDF, &OOXML, &MP4,
    ]
}

fuzz_target!(|data: &[u8]| {
    // Registry path — the hostile dispatch (signature-based).
    if let Some(e) = dw_repair::engine_for(data) {
        let _ = e.analyze(data);
        let mut out = Vec::new();
        let _ = e.repair_into(data, &mut out);
    }
    // Forced per-engine path — the selector byte guarantees every
    // grammar sees hostile input even when check_magic would decline.
    let (sel, body) = match data.split_first() {
        Some((s, rest)) => (usize::from(*s) % 10, rest),
        None => (0, data),
    };
    let e = engines()[sel];
    let _ = e.analyze(body);
    let mut out = Vec::new();
    let outcome = e.repair_into(body, &mut out);
    // The honesty bound: outputs are bounded by the input's size
    // class (a repair may not inflate a hostile file unboundedly —
    // PNG sample synthesis and OOXML manifests add at most a small
    // constant; allow 64 KiB of container overhead).
    let overhead = out.len().saturating_sub(body.len());
    assert!(
        overhead <= 65_536 + body.len() / 2,
        "engine {:?} inflated {} bytes to {}",
        e.format(),
        body.len(),
        out.len()
    );
    if outcome.repaired {
        assert!(!out.is_empty(), "repaired output must be non-empty");
    }
});
