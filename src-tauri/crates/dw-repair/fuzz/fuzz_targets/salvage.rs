//! The salvage-path fuzz: zlib/PNG-IDAT and OOXML-deflate payloads
//! under the tolerance functions (inflate_prefix / redeflate) plus
//! the name-based dispatch and the honest detail strings.
#![no_main]

use dw_repair::RepairEngine;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // The zlib-tolerant inflate: any bytes, decoded prefix, no panic.
    let png = dw_repair::png::PngEngine;
    let mut out = Vec::new();
    let _ = png.repair_into(data, &mut out);

    // The OOXML path with a stored-magic prefix: hostile payloads
    // through the ZIP inventory + manifest synthesis.
    let mut pkg = b"PK\x03\x04\x14\x00\x00\x00\x00\x00".to_vec();
    pkg.extend_from_slice(data);
    let ooxml = dw_repair::ooxml::OoxmlEngine;
    let _ = ooxml.analyze(&pkg);
    let mut out2 = Vec::new();
    let _ = ooxml.repair_into(&pkg, &mut out2);

    // The name dispatch: every extension the registry knows.
    for name in [
        "a.jpg", "a.jpeg", "a.png", "a.gif", "a.bmp", "a.tif", "a.webp", "a.svg", "a.pdf",
        "a.docx", "a.xlsx", "a.pptx", "a.mp4", "a.mov", "a.m4v", "nope.bin",
    ] {
        let _ = dw_repair::format_for_name(name);
    }
    // The honest salvage detail under degenerate fractions.
    let _ = dw_repair::salvage_detail("zlib", 0, 0);
    let _ = dw_repair::salvage_detail("zlib", 3, 10);
    let _ = dw_repair::salvage_detail("zlib", 10, 10);
});
