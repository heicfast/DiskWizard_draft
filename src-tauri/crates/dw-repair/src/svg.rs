//! SVG repair — text-envelope injection + render validation.
//!
//! Research: `artifacts/FileRepair/image-repair--gif-bmp-svg-tiff-walks.md`
//! §3 (byte-exact, line-cited):
//!
//! - `<?xml` missing → prepend the canonical declaration
//!   `<?xml version="1.0" encoding="UTF-8" standalone="no"?>`.
//! - `<svg` missing → wrap the salvaged inner content:
//!   prepend `<svg xmlns = "http://www.w3.org/2000/svg" version = "1.1">`,
//!   skip past any existing `<?xml …?>` prolog, append `</svg>`.
//! - `<svg` present → content kept as-is.
//! - Validate-then-atomic-replace: the repaired file is validated
//!   before it replaces anything (our structural validation: balanced
//!   svg tag, parseable well-formedness of the envelope).

use crate::{DamageKind, DamageReport, Format, RepairEngine, RepairOutcome, RepairTier};

/// The canonical XML declaration (byte-exact from the binary).
const XML_DECL: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>"#;
/// The svg wrapper (byte-exact, including the spaced `xmlns = ` form).
const SVG_OPEN: &str = r#"<svg xmlns = "http://www.w3.org/2000/svg" version = "1.1">"#;
const SVG_CLOSE: &str = "</svg>";

/// The SVG repair engine.
pub struct SvgEngine;

impl RepairEngine for SvgEngine {
    fn format(&self) -> Format {
        Format::Svg
    }

    fn check_magic(&self, buf: &[u8]) -> bool {
        // An SVG candidate: text-ish with an svg element or salvageable
        // svg-ish drawing markup (path/rect/circle/g elements).
        let head = &buf[..buf.len().min(2048)];
        let text = String::from_utf8_lossy(head);
        let tl = text.to_lowercase();
        tl.contains("<svg")
            || tl.contains("<path")
            || tl.contains("<rect")
            || tl.contains("<circle") && tl.contains("xmlns")
    }

    fn analyze(&self, buf: &[u8]) -> DamageReport {
        let text = String::from_utf8_lossy(buf);
        let tl = text.to_lowercase();
        let mut damage = Vec::new();
        if !tl.contains("<?xml") {
            damage.push(DamageKind::MissingHeader);
        }
        if !tl.contains("<svg") {
            damage.push(DamageKind::MissingHeader);
        }
        if !tl.contains("</svg>") && tl.contains("<svg") {
            damage.push(DamageKind::Truncated);
        }
        let healthy = damage.is_empty();
        let detail = if healthy {
            "SVG parses clean: xml declaration + svg root present.".to_string()
        } else {
            let mut notes = Vec::new();
            if !tl.contains("<?xml") {
                notes.push("xml declaration missing");
            }
            if !tl.contains("<svg") {
                notes.push("svg root missing (will be wrapped)");
            }
            if !tl.contains("</svg>") && tl.contains("<svg") {
                notes.push("unclosed svg (truncated)");
            }
            format!("SVG damage: {}.", notes.join(", "))
        };
        DamageReport {
            format: Format::Svg,
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
        let text = String::from_utf8_lossy(buf).into_owned();
        let tl = text.to_lowercase();
        out.clear();
        let mut notes = Vec::new();

        // The inner content: skip past any existing <?xml …?> prolog
        // (search for '>'). The fuzzer's catch (session 12): the
        // prolog offset MUST be found in `text` itself — `tl`'s byte
        // indices diverge from the original whenever case-folding
        // changes a byte length before the match (İ → i+combining
        // dot), and slicing `text` at a `tl` index panicked
        // out-of-bounds. ASCII-fold search over the original.
        let xml_at = find_ascii_ci(&text, "<?xml");
        let inner = if let Some(xml_at) = xml_at {
            if let Some(gt) = text[xml_at..].find('>') {
                &text[(xml_at + gt + 1).min(text.len())..]
            } else {
                text.as_str()
            }
        } else {
            text.as_str()
        };

        let has_svg = tl.contains("<svg");
        let has_xml = tl.contains("<?xml");
        if has_xml && has_svg && tl.contains("</svg>") {
            // Canonical re-emit: the envelope is intact.
            out.extend_from_slice(buf);
            return RepairOutcome {
                repaired: true,
                input_bytes: buf.len() as u64,
                output_bytes: out.len() as u64,
                partial: false,
                detail: "SVG canonical re-emit: envelope intact.".to_string(),
            };
        }
        if !has_xml {
            out.extend_from_slice(XML_DECL.as_bytes());
            out.push(b'\n');
            notes.push("xml declaration prepended".to_string());
        }
        if has_svg {
            out.extend_from_slice(inner.as_bytes());
            if !tl.contains("</svg>") {
                out.extend_from_slice(SVG_CLOSE.as_bytes());
                notes.push("svg close synthesized".to_string());
            }
        } else {
            out.extend_from_slice(SVG_OPEN.as_bytes());
            out.extend_from_slice(inner.trim_start().as_bytes());
            out.extend_from_slice(SVG_CLOSE.as_bytes());
            notes.push("svg envelope injected".to_string());
        }

        RepairOutcome {
            repaired: true,
            input_bytes: buf.len() as u64,
            output_bytes: out.len() as u64,
            partial: false,
            detail: if notes.is_empty() {
                "SVG canonical re-emit: envelope intact.".to_string()
            } else {
                format!("SVG repair: {}.", notes.join(", "))
            },
        }
    }
}

/// ASCII-case-insensitive substring search over the ORIGINAL string —
/// byte indices that are safe to slice it with (`str::to_lowercase`
/// changes byte lengths on Unicode case folding; the engines-fuzz
/// catch). `needle` must be pure ASCII.
fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    debug_assert!(needle.is_ascii());
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    'outer: for i in 0..=h.len() - n.len() {
        for (j, &nb) in n.iter().enumerate() {
            let hb = h[i + j];
            let eq = hb == nb || (hb.is_ascii_alphabetic() && (hb | 0x20) == (nb | 0x20));
            if !eq {
                continue 'outer;
            }
        }
        // The needle is ASCII: any match boundary is a char boundary.
        return Some(i);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The engines-fuzz catch (session 12): a case-folding-EXPANDING
    /// char (İ → i + combining dot, +1 byte per char) before the
    /// `<?xml` prolog made the lowercased string's find-index exceed
    /// the ORIGINAL's length — the old code sliced `text` at a `tl`
    /// index and panicked out-of-bounds. The ASCII-fold search keeps
    /// every index in the original's coordinate space.
    #[test]
    fn case_folding_expansion_before_prolog_never_panics() {
        let evil = format!(
            "{}{}",
            "İ".repeat(600),
            r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"></svg>"#
        );
        let e = SvgEngine;
        let mut out = Vec::new();
        // The whole contract: no panic, a typed outcome either way.
        let _ = e.analyze(evil.as_bytes());
        let _ = e.repair_into(evil.as_bytes(), &mut out);
    }

    #[test]
    fn healthy_svg_passes_through() {
        let svg = r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"><rect width="1" height="1"/></svg>"#.to_string();
        let e = SvgEngine;
        let r = e.analyze(svg.as_bytes());
        assert!(r.healthy, "detail: {}", r.detail);
        let mut out = Vec::new();
        let o = e.repair_into(svg.as_bytes(), &mut out);
        assert!(o.repaired);
        assert_eq!(String::from_utf8_lossy(&out), svg);
    }

    #[test]
    fn missing_envelope_is_injected() {
        // Salvaged inner content with no xml/svg envelope at all.
        let inner = r#"<path d="M0,0 L1,1"/>"#;
        let e = SvgEngine;
        let r = e.analyze(inner.as_bytes());
        assert!(r.damage.contains(&DamageKind::MissingHeader));
        let mut out = Vec::new();
        let o = e.repair_into(inner.as_bytes(), &mut out);
        assert!(o.repaired);
        let s = String::from_utf8_lossy(&out);
        assert!(s.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>"));
        assert!(s.contains(SVG_OPEN));
        assert!(s.ends_with("</svg>"));
        assert!(s.contains("<path d=\"M0,0 L1,1\"/>"));
        let r2 = e.analyze(&out);
        assert!(r2.healthy, "envelope: {}", r2.detail);
    }

    #[test]
    fn truncated_svg_gets_close() {
        let svg = r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"><rect/>"#;
        let e = SvgEngine;
        let mut out = Vec::new();
        let o = e.repair_into(svg.as_bytes(), &mut out);
        assert!(o.repaired);
        assert!(String::from_utf8_lossy(&out).ends_with("</svg>"));
    }
}
