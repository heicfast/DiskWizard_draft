//! The signature table — the raw_signs.json model, compiled in.
//!
//! Each entry: magic bytes at an offset, an extension/category, size
//! bounds, confidence. `matches_at` probes ONE sector (the carver
//! calls it per sector); grammars refine beyond the magic on hit.

use crate::types::Category;

/// Stable analyser ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureId {
    Jpeg,
    Png,
    Gif,
    Bmp,
    Zip,
    Rar4,
    Rar5,
    Pdf,
    Mp4,
    Mov,
    Mkv,
    Mp3,
    Wav,
    Ogg,
    Flac,
    Ole2,
    Heic,
    Sqlite,
    Pst,
}

/// One signature row.
pub struct Signature {
    pub id: SignatureId,
    /// Display prefix for synthesized names.
    pub name_prefix: &'static str,
    /// File extension (with dot).
    pub ext: &'static str,
    /// Offset of the magic within the probed sector.
    pub offset: usize,
    pub magic: &'static [u8],
    /// Minimum plausible file size.
    pub min_size: u64,
    /// Maximum carve length (grammar-verified lengths only).
    pub max_size: u64,
    /// Session 8: the maximum number of DEVICE bytes a validator may
    /// READ for one hit — the probe bound that keeps hostile/garbage
    /// candidates bounded (the old code walked multi-GiB spans).
    pub probe_cap: u64,
    /// Session 8: the maximum carve length a WEAK (unvalidated /
    /// truncated) hit may claim; 0 = reject. Grammar-verified lengths
    /// are NOT bound by this — only the "probably truncated" fallback
    /// is (the TB-inflation fix: never carve the probe span).
    pub fallback_cap: u64,
    /// Base confidence (grammars adjust).
    pub confidence: u8,
    pub category: Category,
    /// True when the name comes from content (LostName branch).
    pub named_by_content: bool,
}

impl Signature {
    /// Does this signature match at the start of `sector`?
    #[must_use]
    pub fn matches_at(&self, sector: &[u8]) -> bool {
        sector.len() >= self.offset + self.magic.len()
            && &sector[self.offset..self.offset + self.magic.len()] == self.magic
    }
}

/// The production signature set (ordered by hit-rate priority).
///
/// Session-8 caps (see `raw/mod.rs`'s module docs): `probe_cap`
/// bounds validator device reads; `fallback_cap` bounds weak-
/// validation carve lengths (0 = reject). Calibrations follow the
/// real-world size distribution of each format family — a truncated
/// JPEG still surfaces as a 1 MiB prefix, but garbage never claims
/// gigabytes.
#[allow(clippy::too_many_lines)] // A data table, not logic.
#[must_use]
pub fn signature_set() -> Vec<Signature> {
    let mib = 1024 * 1024u64;
    vec![
        Signature {
            id: SignatureId::Jpeg,
            name_prefix: "IMG",
            ext: ".jpg",
            offset: 0,
            magic: b"\xFF\xD8\xFF",
            min_size: 128,
            max_size: 256 * mib,
            probe_cap: 8 * mib,
            fallback_cap: mib,
            confidence: 90,
            category: Category::Images,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Png,
            name_prefix: "IMG",
            ext: ".png",
            offset: 0,
            magic: b"\x89PNG\r\n\x1A\n",
            min_size: 64,
            max_size: 256 * mib,
            probe_cap: 8 * mib,
            fallback_cap: 256 * 1024,
            confidence: 95,
            category: Category::Images,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Gif,
            name_prefix: "IMG",
            ext: ".gif",
            offset: 0,
            magic: b"GIF8",
            min_size: 32,
            max_size: 64 * mib,
            probe_cap: 4 * mib,
            fallback_cap: 256 * 1024,
            confidence: 88,
            category: Category::Images,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Bmp,
            name_prefix: "IMG",
            ext: ".bmp",
            offset: 0,
            magic: b"BM",
            min_size: 26,
            max_size: 512 * mib,
            probe_cap: 64 * 1024,
            fallback_cap: 0,
            confidence: 80,
            category: Category::Images,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Zip,
            name_prefix: "ARC",
            ext: ".zip",
            offset: 0,
            magic: b"PK\x03\x04",
            min_size: 64,
            max_size: 4 * 1024 * mib,
            // The EOCD hunt reads only the ~66 KiB tail zone.
            probe_cap: 2 * mib,
            fallback_cap: mib,
            confidence: 90,
            category: Category::Archives,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Rar4,
            name_prefix: "ARC",
            ext: ".rar",
            offset: 0,
            magic: b"Rar!\x1A\x07\x00",
            min_size: 32,
            max_size: 4 * 1024 * mib,
            probe_cap: 64 * mib,
            fallback_cap: mib,
            confidence: 92,
            category: Category::Archives,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Rar5,
            name_prefix: "ARC",
            ext: ".rar",
            offset: 0,
            magic: b"Rar!\x1A\x07\x01\x00",
            min_size: 32,
            max_size: 4 * 1024 * mib,
            probe_cap: 64 * mib,
            fallback_cap: mib,
            confidence: 92,
            category: Category::Archives,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Pdf,
            name_prefix: "DOC",
            ext: ".pdf",
            offset: 0,
            magic: b"%PDF-",
            min_size: 128,
            max_size: 2 * 1024 * mib,
            probe_cap: 4 * mib,
            fallback_cap: 256 * 1024,
            confidence: 92,
            category: Category::Documents,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Mp4,
            name_prefix: "VID",
            ext: ".mp4",
            offset: 4,
            magic: b"ftyp",
            min_size: 1024,
            max_size: 8 * 1024 * mib,
            probe_cap: 64 * mib,
            fallback_cap: 4 * mib,
            confidence: 85,
            category: Category::Videos,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Mov,
            name_prefix: "VID",
            ext: ".mov",
            offset: 4,
            magic: b"ftypqt",
            min_size: 1024,
            max_size: 8 * 1024 * mib,
            probe_cap: 64 * mib,
            fallback_cap: 4 * mib,
            confidence: 85,
            category: Category::Videos,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Mkv,
            name_prefix: "VID",
            ext: ".mkv",
            offset: 0,
            magic: b"\x1A\x45\xDF\xA3",
            min_size: 512,
            max_size: 8 * 1024 * mib,
            probe_cap: 32 * mib,
            fallback_cap: 4 * mib,
            confidence: 82,
            category: Category::Videos,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Mp3,
            name_prefix: "AUD",
            ext: ".mp3",
            offset: 0,
            magic: b"ID3",
            min_size: 256,
            max_size: 256 * mib,
            probe_cap: 64 * 1024,
            fallback_cap: 0,
            confidence: 85,
            category: Category::Audio,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Wav,
            name_prefix: "AUD",
            ext: ".wav",
            offset: 0,
            magic: b"RIFF",
            min_size: 44,
            max_size: 2 * 1024 * mib,
            probe_cap: 64 * 1024,
            fallback_cap: 0,
            confidence: 75,
            category: Category::Audio,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Ogg,
            name_prefix: "AUD",
            ext: ".ogg",
            offset: 0,
            magic: b"OggS",
            min_size: 512,
            max_size: 256 * mib,
            probe_cap: 4 * mib,
            fallback_cap: 0,
            confidence: 80,
            category: Category::Audio,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Flac,
            name_prefix: "AUD",
            ext: ".flac",
            offset: 0,
            magic: b"fLaC",
            min_size: 64,
            max_size: 256 * mib,
            probe_cap: 64 * 1024,
            fallback_cap: 256 * 1024,
            confidence: 88,
            category: Category::Audio,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Ole2,
            name_prefix: "DOC",
            ext: ".doc",
            offset: 0,
            magic: b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1",
            min_size: 512,
            max_size: 2 * 1024 * mib,
            probe_cap: 64 * 1024,
            fallback_cap: 0,
            confidence: 88,
            category: Category::Documents,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Heic,
            name_prefix: "IMG",
            ext: ".heic",
            offset: 4,
            magic: b"ftypheic",
            min_size: 1024,
            max_size: 2 * 1024 * mib,
            probe_cap: 64 * mib,
            fallback_cap: 4 * mib,
            confidence: 82,
            category: Category::Images,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Sqlite,
            name_prefix: "DB",
            ext: ".sqlite",
            offset: 0,
            magic: b"SQLite format 3\x00",
            min_size: 512,
            max_size: 2 * 1024 * mib,
            probe_cap: 64 * 1024,
            fallback_cap: 0,
            confidence: 90,
            category: Category::Other,
            named_by_content: false,
        },
        Signature {
            id: SignatureId::Pst,
            name_prefix: "MAIL",
            ext: ".pst",
            offset: 0,
            magic: b"!BDN",
            min_size: 512 * 1024,
            max_size: 64 * 1024 * mib,
            probe_cap: 64 * 1024,
            // The research's 1 MiB floor for unknown versions.
            fallback_cap: mib,
            confidence: 60,
            category: Category::Email,
            named_by_content: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_wellformed() {
        for s in signature_set() {
            assert!(!s.magic.is_empty(), "{} empty magic", s.name_prefix);
            assert!(s.max_size >= s.min_size, "{} size bounds", s.name_prefix);
            assert!(s.confidence > 0 && s.confidence <= 100);
            assert!(s.offset + s.magic.len() <= 512);
            // Session 8: probes are bounded and fallbacks never exceed
            // the max size.
            assert!(s.probe_cap > 0, "{} probe cap", s.name_prefix);
            assert!(
                s.fallback_cap <= s.max_size,
                "{} fallback cap",
                s.name_prefix
            );
        }
    }

    #[test]
    fn matches_at_respects_offset() {
        let mut sector = [0u8; 512];
        sector[0..4].copy_from_slice(b"PK\x03\x04");
        let zip = signature_set()
            .into_iter()
            .find(|s| s.id == SignatureId::Zip)
            .unwrap();
        assert!(zip.matches_at(&sector));
        let png = signature_set()
            .into_iter()
            .find(|s| s.id == SignatureId::Png)
            .unwrap();
        assert!(!png.matches_at(&sector));

        // ftyp signatures live at offset 4.
        let mut mp4_sector = [0u8; 512];
        mp4_sector[4..12].copy_from_slice(b"ftypisom");
        let mp4 = signature_set()
            .into_iter()
            .find(|s| s.id == SignatureId::Mp4)
            .unwrap();
        assert!(mp4.matches_at(&mp4_sector));
    }
}
