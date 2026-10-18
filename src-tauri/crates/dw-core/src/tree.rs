//! The found-file tree — ingestion, classification, path assembly,
//! merge (structured wins), the special branches.
//!
//! The arena model: nodes live in a `Vec` (DENSE ids — the frozen
//! UI contract indexes `nodes[id]`), directories roll sizes up,
//! `parent_hint` chains (path strings or `@mft:` refs) resolve into
//! the tree, and carved entries attach to the "File Path Lost" /
//! synthesized-name branches (`artifacts/UIUX/09-results-phase-deep-dive.md`).
//!
//! Merge policy: a structured entry absorbs a carved duplicate when
//! their first extents coincide (the ~65% essential-mode noise
//! reduction: carved copies of files the FS engine already found are
//! the bulk of the noise; "users can switch to complete scan results
//! at any time" — essential mode hides `lost-name`/`version` origins
//! client-side).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::{
    Category, Extent, FileAttrs, FoundEntry, FsKind, NodeOrigin, Recoverability, Timestamps,
};

/// One node in the results tree (the wire shape mirrors the UI's
/// `FileNode` — dense `id` = arena index).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub name: String,
    pub is_dir: bool,
    pub category: Category,
    pub size: u64,
    pub modified: i64,
    pub origin: NodeOrigin,
    pub recoverability: Recoverability,
    pub path: String,
    #[serde(skip)]
    pub timestamps: Timestamps,
    #[serde(skip)]
    pub attrs: FileAttrs,
    /// Volume-relative content extents (recovery IO).
    #[serde(skip)]
    pub extents: Vec<Extent>,
    /// Engine confidence (grading input).
    #[serde(skip)]
    pub confidence: u8,
    /// Image dimensions when known.
    #[serde(skip)]
    pub dimensions: Option<(u32, u32)>,
    #[serde(skip)]
    pub source: FsKind,
}

/// The tree builder.
pub struct TreeBuilder {
    nodes: Vec<TreeNode>,
    /// path/parent-ref → dir node id.
    dirs: HashMap<String, u64>,
    /// first-extent offset → file node id (carved-vs-structured merge).
    by_first_extent: HashMap<u64, u64>,
    /// Per-category live counters.
    counts: HashMap<Category, (u64, u64)>,
    root_name: String,
}

impl Default for TreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeBuilder {
    /// New builder (root label = the scan target's display name).
    #[must_use]
    pub fn new() -> Self {
        Self::with_root("Scan")
    }

    #[must_use]
    pub fn with_root(root_name: &str) -> Self {
        let mut b = Self {
            nodes: Vec::new(),
            dirs: HashMap::new(),
            by_first_extent: HashMap::new(),
            counts: HashMap::new(),
            root_name: root_name.to_string(),
        };
        // Node 0 = the synthetic root.
        b.push_dir(None, root_name.to_string(), NodeOrigin::Existing);
        b
    }

    fn push_dir(&mut self, parent: Option<u64>, name: String, origin: NodeOrigin) -> u64 {
        let id = self.nodes.len() as u64;
        let path = match parent {
            None => self.root_name.clone(),
            Some(p) => format!("{}\\{}", self.nodes[p as usize].path, name),
        };
        self.nodes.push(TreeNode {
            id,
            parent_id: parent,
            name,
            is_dir: true,
            category: Category::Other,
            size: 0,
            modified: 0,
            origin,
            recoverability: Recoverability::Excellent,
            path,
            timestamps: Timestamps::default(),
            attrs: FileAttrs::default(),
            extents: Vec::new(),
            confidence: 100,
            dimensions: None,
            source: FsKind::Unknown,
        });
        id
    }

    /// Ingest one engine entry.
    pub fn add(&mut self, e: FoundEntry) {
        if e.is_dir {
            self.add_directory(&e);
            return;
        }
        // Merge: same first extent + already structured → absorbed.
        let first_off = e.extents.first().map_or(u64::MAX, |x| x.offset);
        if first_off != u64::MAX
            && let Some(&existing_id) = self.by_first_extent.get(&first_off)
        {
            let existing = &mut self.nodes[existing_id as usize];
            if !existing.is_dir
                && matches!(existing.origin, NodeOrigin::Existing | NodeOrigin::Deleted)
            {
                // Structured wins: the carved copy is noise.
                return;
            }
        }
        let category = e.category_hint.unwrap_or_else(|| classify(&e.name));
        let (dir_id, lost_branch) = self.resolve_parent(&e);
        let id = self.nodes.len() as u64;
        let path = match lost_branch {
            Some(branch) => format!("{}\\{}", branch, e.name),
            None => format!("{}\\{}", self.nodes[dir_id as usize].path, e.name),
        };
        let recoverability = grade_from(&e);
        self.nodes.push(TreeNode {
            id,
            parent_id: Some(dir_id),
            name: e.name,
            is_dir: false,
            category,
            size: e.size,
            modified: e.timestamps.modified.unwrap_or_default(),
            origin: e.origin,
            recoverability,
            path,
            timestamps: e.timestamps,
            attrs: e.attrs,
            extents: e.extents,
            confidence: e.confidence,
            dimensions: e.dimensions,
            source: e.source,
        });
        if first_off != u64::MAX {
            self.by_first_extent.insert(first_off, id);
        }
        let entry = &self.nodes[id as usize];
        let c = self.counts.entry(entry.category).or_insert((0, 0));
        c.0 += 1;
        c.1 += entry.size;
        self.roll_up(entry.id, entry.size, entry.modified);
    }

    fn roll_up(&mut self, mut id: u64, size: u64, modified: i64) {
        while let Some(parent) = self.nodes[id as usize].parent_id {
            let p = &mut self.nodes[parent as usize];
            p.size += size;
            if modified > p.modified {
                p.modified = modified;
            }
            id = parent;
        }
    }

    fn add_directory(&mut self, e: &FoundEntry) {
        let (parent_id, name, synthetic) = match &e.parent_hint {
            Some(hint) if hint.starts_with('@') => {
                // NTFS parent-ref: resolved later (pass 2) or lost.
                let _ = hint;
                (0u64, sanitize_name(&e.name), true)
            }
            Some(hint) if !hint.is_empty() => {
                let parent = self.ensure_dir_path(hint);
                (parent, sanitize_name(&e.name), false)
            }
            _ => (0u64, sanitize_name(&e.name), false),
        };
        if name == "." {
            return; // the root re-entry
        }
        if synthetic {
            // Directories with unresolvable parents still surface
            // under their own name at root level (salvage).
            let _ = parent_id;
        }
        let key = format!("{parent_id}\\{name}");
        if self.dirs.contains_key(&key) {
            return;
        }
        let id = self.push_dir(Some(parent_id), name, e.origin);
        self.dirs.insert(key, id);
        self.nodes[id as usize].timestamps = e.timestamps;
    }

    /// Resolve an entry's parent: path-hint dirs materialize on
    /// demand; carved entries land on the lost branches.
    fn resolve_parent(&mut self, e: &FoundEntry) -> (u64, Option<String>) {
        match (&e.parent_hint, e.origin) {
            (_, NodeOrigin::LostName) => (0, Some("Tags Extracted".into())),
            (None, NodeOrigin::LostPath | NodeOrigin::Tags) => (0, Some("File Path Lost".into())),
            (Some(_), NodeOrigin::LostPath) => (0, Some("File Path Lost".into())),
            (Some(hint), _) if hint.starts_with('@') => {
                // NTFS ref unresolved in pass 1 → the lost-keep branch
                // keeps the name (better than dropping).
                (0, Some("File Path Lost".into()))
            }
            (Some(hint), _) => (self.ensure_dir_path(hint), None),
            (None, _) => (0, None),
        }
    }

    /// Materialize a directory path ("a\\b\\c") → node id.
    fn ensure_dir_path(&mut self, hint: &str) -> u64 {
        let mut parent = 0u64;
        for part in hint.split(['\\', '/']).filter(|s| !s.is_empty()) {
            let key = format!("{parent}\\{part}");
            if let Some(&id) = self.dirs.get(&key) {
                parent = id;
            } else {
                let id = self.push_dir(Some(parent), part.to_string(), NodeOrigin::Existing);
                self.dirs.insert(key, id);
                parent = id;
            }
        }
        parent
    }

    /// The assembled snapshot (nodes in id order).
    #[must_use]
    pub fn nodes(&self) -> &[TreeNode] {
        &self.nodes
    }

    /// Category counters (the UI chips).
    #[must_use]
    pub fn category_counts(&self) -> Vec<CategoryCountWire> {
        let mut out: Vec<CategoryCountWire> = self
            .counts
            .iter()
            .map(|(c, (n, s))| CategoryCountWire {
                category: *c,
                count: *n,
                size: *s,
            })
            .collect();
        out.sort_by_key(|c| c.category.wire());
        out
    }

    /// Total found (files + bytes).
    #[must_use]
    pub fn totals(&self) -> (u64, u64) {
        self.counts
            .values()
            .fold((0, 0), |(n, s), (cn, cs)| (n + cn, s + cs))
    }
}

/// The wire counter shape (UI `CategoryCount`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryCountWire {
    pub category: Category,
    pub count: u64,
    pub size: u64,
}

/// Sanitize names for display + destination safety (path traversal).
#[must_use]
pub fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        "_".into()
    } else {
        trimmed.to_string()
    }
}

/// Extension → category classification (the FileFilter.xml model —
/// research: `artifacts/config-files/config-data-reference.md`:
/// 8 categories / 1489 extensions; the production subset).
#[must_use]
pub fn classify(name: &str) -> Category {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if !name.contains('.') || ext.len() > 6 {
        return Category::Other;
    }
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "tif" | "webp" | "heic" | "heif"
        | "avif" | "svg" | "ico" | "psd" | "raw" | "cr2" | "nef" | "arw" | "dng" | "orf"
        | "rw2" | "x3f" => Category::Images,
        "mp4" | "mov" | "avi" | "mkv" | "webm" | "m4v" | "mpg" | "mpeg" | "wmv" | "flv" | "3gp"
        | "mts" | "m2ts" => Category::Videos,
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "wma" | "m4a" | "opus" | "aiff" | "mid"
        | "amr" => Category::Audio,
        "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "pdf" | "txt" | "rtf" | "odt"
        | "ods" | "odp" | "md" | "csv" | "tsv" | "json" | "xml" | "html" | "htm" | "epub"
        | "hwp" | "hwpx" | "pages" | "numbers" | "key" => Category::Documents,
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "iso" | "cab" | "arj"
        | "lzh" => Category::Archives,
        "pst" | "ost" | "eml" | "msg" | "mbox" | "dbx" => Category::Email,
        _ => Category::Other,
    }
}

/// Recoverability from engine confidence + origin + extent sanity
/// (the CCheckNodeThread analog; extent READABILITY probing is the
/// controller's async pass — see `scan`).
#[must_use]
pub fn grade_from(e: &FoundEntry) -> Recoverability {
    // Carved entries cap at Good (the mock realism contract — real
    // grading honors the same promise the UI made).
    let cap = match e.origin {
        NodeOrigin::Existing => 100u8,
        NodeOrigin::Deleted => 100,
        _ => 80,
    };
    let score = e.confidence.min(cap);
    if e.extents.is_empty() && !e.is_dir && e.size > 0 {
        // Size claimed but no extents — metadata-only knowledge.
        return Recoverability::Poor;
    }
    match score {
        85..=100 => Recoverability::Excellent,
        55..=84 => Recoverability::Good,
        _ => Recoverability::Poor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Extent;

    fn file(name: &str, size: u64, origin: NodeOrigin, ext_off: Option<u64>) -> FoundEntry {
        let mut e = FoundEntry::structured(name.to_string(), false, FsKind::Ntfs);
        e.size = size;
        e.origin = origin;
        e.extents = ext_off.map_or_else(Vec::new, |o| vec![Extent::new(o, size)]);
        e.timestamps.modified = Some(1_700_000_000);
        e
    }

    #[test]
    fn classification_table_covers_common_formats() {
        assert_eq!(classify("photo.jpg"), Category::Images);
        assert_eq!(classify("clip.MP4"), Category::Videos);
        assert_eq!(classify("song.flac"), Category::Audio);
        assert_eq!(classify("report.docx"), Category::Documents);
        assert_eq!(classify("backup.zip"), Category::Archives);
        assert_eq!(classify("inbox.pst"), Category::Email);
        assert_eq!(classify("unknown.xyz"), Category::Other);
        assert_eq!(classify("noext"), Category::Other);
    }

    #[test]
    fn tree_assembles_paths_and_rolls_sizes() {
        let mut b = TreeBuilder::with_root("USB (F:)");
        b.add(file("vacation.jpg", 100, NodeOrigin::Existing, Some(4_096)));
        let mut dir = FoundEntry::structured("Photos".into(), true, FsKind::Ntfs);
        dir.parent_hint = Some(String::new());
        b.add(dir);
        let mut nested = file("family.png", 50, NodeOrigin::Existing, Some(8_192));
        nested.parent_hint = Some("Photos".into());
        b.add(nested);
        let nodes = b.nodes();
        assert!(nodes.len() >= 4);
        assert!(
            nodes
                .iter()
                .any(|n| n.name == "family.png" && n.path.ends_with("Photos\\family.png"))
        );
        let photos = nodes.iter().find(|n| n.name == "Photos").unwrap();
        assert_eq!(photos.size, 50);
        let (files, bytes) = b.totals();
        assert_eq!((files, bytes), (2, 150));
    }

    #[test]
    fn carved_absorbs_into_structured_duplicate() {
        let mut b = TreeBuilder::new();
        b.add(file("report.pdf", 1000, NodeOrigin::Existing, Some(5_000)));
        let mut carved = file("DOC_0000C138.pdf", 1000, NodeOrigin::LostPath, Some(5_000));
        carved.category_hint = Some(Category::Documents);
        b.add(carved);
        let (_, bytes) = b.totals();
        assert_eq!(bytes, 1000, "carved duplicate absorbed");
    }

    #[test]
    fn carved_lands_on_lost_branch() {
        let mut b = TreeBuilder::new();
        let mut carved = file("IMG_00001000.jpg", 4096, NodeOrigin::LostPath, Some(0x1000));
        carved.parent_hint = None;
        b.add(carved);
        let nodes = b.nodes();
        let img = nodes.iter().find(|n| n.name.starts_with("IMG_")).unwrap();
        assert!(img.path.contains("File Path Lost"));
        assert_eq!(img.recoverability, Recoverability::Good); // carved cap
    }

    #[test]
    fn deleted_high_confidence_is_excellent() {
        let e = file("gone.log", 10, NodeOrigin::Deleted, Some(50));
        assert_eq!(grade_from(&e), Recoverability::Excellent);
        let mut poor = file("bad.dat", 10, NodeOrigin::LostPath, None);
        poor.size = 100;
        poor.confidence = 30;
        assert_eq!(grade_from(&poor), Recoverability::Poor);
    }

    #[test]
    fn sanitize_blocks_traversal() {
        assert_eq!(sanitize_name("..\\..\\evil"), ".._.._evil");
        assert_eq!(sanitize_name("ok.txt"), "ok.txt");
        assert_eq!(sanitize_name("   "), "_");
        assert_eq!(sanitize_name("con:trol*"), "con_trol_");
    }
}
