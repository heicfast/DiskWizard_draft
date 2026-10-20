//! The found-file tree — ingestion, classification, path assembly,
//! merge (structured wins), the special branches.
//!
//! The arena model: nodes live in a `Vec` (DENSE ids — the frozen
//! UI contract indexes `nodes[id]`), directories roll sizes up,
//! `parent_hint` chains (path strings or `@mft:` refs) resolve into
//! the tree, and carved entries attach to the "File Path Lost" /
//! synthesized-name branches (`artifacts/UIUX/09-results-phase-deep-dive.md`).
//!
//! ## Session 8 — the NTFS folder-structure fix
//!
//! The old builder never resolved `@mft:<record>` parent refs (the
//! "resolved later (pass 2)" that didn't exist): every NTFS file
//! collapsed onto "File Path Lost" and whole-disk partition prefixes
//! corrupted the refs into literal `Partition 1\@mft:1234` path
//! dirs. The builder now keeps an MFT-record → dir-node map, defers
//! unresolvable entries to pending buffers, and runs a fixpoint
//! resolution ([`TreeBuilder::resolve_pending`]) at unit/phase
//! boundaries + [`TreeBuilder::finish`] (leftovers → the honest
//! lost branch). Partition prefixes nest via `FoundEntry::partition`
//! (never string-munged into refs).
//!
//! Merge policy: a structured entry absorbs a carved duplicate when
//! their first extents coincide (the ~65% essential-mode noise
//! reduction: carved copies of files the FS engine already found are
//! the bulk of the noise; "users can switch to complete scan results
//! at any time" — essential mode hides `lost-name`/`version` origins
//! client-side).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::blockdev::BlockDevice;
use crate::types::{
    Category, Extent, FileAttrs, FoundEntry, FsKind, NodeOrigin, Recoverability, Timestamps,
};

/// The NTFS root-directory MFT record (self-parented).
const MFT_ROOT_RECORD: u64 = 5;

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
    /// content key → first node id carrying that content (the
    /// duplicate/nonessential filter: repeats mark as Version).
    by_content_key: HashMap<u64, u64>,
    /// Per-category live counters.
    counts: HashMap<Category, (u64, u64)>,
    root_name: String,
    /// Session 8: MFT record → materialized dir node id (NTFS).
    mft_dirs: HashMap<u64, u64>,
    /// Session 8: partition label → dir node id (whole-disk scans).
    partitions: HashMap<String, u64>,
    /// Session 8: entries whose `@mft:` parent hasn't materialized
    /// yet — resolved by the fixpoint at phase boundaries / finish.
    pending_files: Vec<FoundEntry>,
    pending_dirs: Vec<FoundEntry>,
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
            by_content_key: HashMap::new(),
            counts: HashMap::new(),
            root_name: root_name.to_string(),
            mft_dirs: HashMap::new(),
            partitions: HashMap::new(),
            pending_files: Vec::new(),
            pending_dirs: Vec::new(),
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
        // `@mft:` parent refs that can't resolve YET defer to the
        // pending buffer (pass 2 / finish materializes them) — the
        // session-8 folder-structure fix.
        if let Some(hint) = e.parent_hint.as_deref()
            && hint.starts_with('@')
            && self.resolve_mft_dir(e.partition.as_deref(), hint).is_none()
        {
            self.pending_files.push(e);
            return;
        }
        self.materialize_file(e);
    }

    /// Materialize one file entry against the CURRENT tree state
    /// (merge maps, classification, roll-up).
    fn materialize_file(&mut self, e: FoundEntry) {
        // Merge: same first extent + already structured → absorbed.
        // Same first extent + same size + already carved → absorbed
        // too (the same on-disk region carved twice is redundant,
        // not a distinct copy).
        let first_off = e.extents.first().map_or(u64::MAX, |x| x.offset);
        if first_off != u64::MAX
            && let Some(&existing_id) = self.by_first_extent.get(&first_off)
        {
            let existing = &self.nodes[existing_id as usize];
            if !existing.is_dir
                && (matches!(existing.origin, NodeOrigin::Existing | NodeOrigin::Deleted)
                    || (matches!(existing.origin, NodeOrigin::LostPath | NodeOrigin::LostName)
                        && existing.size == e.size))
            {
                // Structured wins / region-identical carve: noise.
                return;
            }
        }
        // Same content elsewhere on disk: keep the node (complete
        // mode shows every copy) but mark it Version — the essential
        // mode hides repeats (the ~65% reduction contract).
        let mut origin = e.origin;
        if let Some(key) = e.content_key
            && self.by_content_key.contains_key(&key)
        {
            origin = NodeOrigin::Version;
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
            origin,
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
        if let Some(key) = e.content_key {
            self.by_content_key.entry(key).or_insert(id);
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
        // The NTFS root record: check the RAW name — sanitize_name
        // strips the trailing dot and turns "." into "_" (the phantom
        // '_' dir that swallowed every root-level file on the VHD).
        if e.name == "." {
            if let Some(rec) = e.record_id {
                let base = self.ensure_partition(e.partition.as_deref());
                self.mft_dirs.entry(rec).or_insert(base);
            }
            return;
        }
        let name = sanitize_name(&e.name);
        match &e.parent_hint {
            // NTFS parent-ref: resolve NOW when the parent dir already
            // materialized; otherwise defer to pending_dirs (pass 2).
            Some(hint) if hint.starts_with('@') => {
                if self.resolve_mft_dir(e.partition.as_deref(), hint).is_some()
                    || e.record_id.is_none()
                {
                    self.materialize_mft_dir(e, &name);
                } else {
                    self.pending_dirs.push(e.clone());
                }
            }
            Some(hint) if !hint.is_empty() => {
                let parent = self.ensure_dir_path(hint);
                let key = format!("{parent}\\{name}");
                if self.dirs.contains_key(&key) {
                    return;
                }
                let id = self.push_dir(Some(parent), name, e.origin);
                self.dirs.insert(key, id);
                self.nodes[id as usize].timestamps = e.timestamps;
            }
            _ => {
                let key = format!("0\\{name}");
                if self.dirs.contains_key(&key) {
                    return;
                }
                let id = self.push_dir(Some(0), name, e.origin);
                self.dirs.insert(key, id);
                self.nodes[id as usize].timestamps = e.timestamps;
            }
        }
    }

    /// Materialize a directory whose `@mft:` parent RESOLVED (or is
    /// root-salvage): create under the resolved parent, register the
    /// record → node map.
    fn materialize_mft_dir(&mut self, e: &FoundEntry, name: &str) {
        let parent = self
            .resolve_mft_dir(
                e.partition.as_deref(),
                e.parent_hint.as_deref().unwrap_or(""),
            )
            .unwrap_or_else(|| self.ensure_partition(e.partition.as_deref()));
        let key = format!("{parent}\\{name}");
        let id = if let Some(&id) = self.dirs.get(&key) {
            id
        } else {
            let id = self.push_dir(Some(parent), name.to_string(), e.origin);
            self.dirs.insert(key, id);
            self.nodes[id as usize].timestamps = e.timestamps;
            id
        };
        if let Some(rec) = e.record_id {
            self.mft_dirs.entry(rec).or_insert(id);
        }
    }

    /// Resolve an `@mft:<record>` parent hint (session 8): the root
    /// record (5) maps to the partition base (the volume root),
    /// materialized records to their dirs, `None` when not (yet)
    /// resolvable — the fixpoint retries those.
    fn resolve_mft_dir(&mut self, partition: Option<&str>, hint: &str) -> Option<u64> {
        let rec = hint.strip_prefix("@mft:")?.parse::<u64>().ok()?;
        if let Some(&id) = self.mft_dirs.get(&rec) {
            return Some(id);
        }
        // The root record self-references (parent 5 == record 5) and
        // everything parented at it: the partition base (tree root
        // for single-volume scans).
        if rec == MFT_ROOT_RECORD {
            return Some(self.ensure_partition(partition));
        }
        None
    }

    /// Materialize the partition dir (whole-disk scans) — lazily
    /// created, stable id; `None` = the tree root.
    fn ensure_partition(&mut self, partition: Option<&str>) -> u64 {
        let Some(p) = partition else { return 0 };
        if let Some(&id) = self.partitions.get(p) {
            return id;
        }
        let id = self.push_dir(Some(0), p.to_string(), NodeOrigin::Existing);
        self.partitions.insert(p.to_string(), id);
        id
    }

    /// Pass-2 fixpoint (session 8): materialize every pending dir
    /// whose parent chain just became resolvable; repeat until a
    /// full pass resolves nothing. Non-final calls keep unresolved
    /// entries pending (they may resolve after the next unit/phase).
    pub fn resolve_pending(&mut self) {
        loop {
            let mut progressed = false;
            let mut still = Vec::new();
            let pending = std::mem::take(&mut self.pending_dirs);
            for e in pending {
                let hint = e.parent_hint.clone().unwrap_or_default();
                if self
                    .resolve_mft_dir(e.partition.as_deref(), &hint)
                    .is_some()
                {
                    let name = sanitize_name(&e.name);
                    if name != "." {
                        self.materialize_mft_dir(&e, &name);
                    }
                    progressed = true;
                } else {
                    still.push(e);
                }
            }
            self.pending_dirs = still;
            if !progressed || self.pending_dirs.is_empty() {
                break;
            }
        }
        // Files: one pass (their parents are dirs — the fixpoint
        // above already settled everything resolvable).
        let pending = std::mem::take(&mut self.pending_files);
        for e in pending {
            let hint = e.parent_hint.clone().unwrap_or_default();
            if self
                .resolve_mft_dir(e.partition.as_deref(), &hint)
                .is_some()
            {
                self.materialize_file(e);
            } else {
                // Still unresolved mid-scan: keep pending.
                self.pending_files.push(e);
            }
        }
    }

    /// Final pass: resolve everything resolvable, then land the
    /// leftovers on the honest lost branches ("File Path Lost" for
    /// files; root-level salvage for dirs). Called once, after all
    /// engines finish.
    pub fn finish(&mut self) {
        self.resolve_pending();
        // Leftover dirs: root-level salvage (the old behavior).
        let pending = std::mem::take(&mut self.pending_dirs);
        for e in pending {
            let name = sanitize_name(&e.name);
            if name != "." {
                let base = self.ensure_partition(e.partition.as_deref());
                let key = format!("{base}\\{name}");
                if !self.dirs.contains_key(&key) {
                    let id = self.push_dir(Some(base), name, e.origin);
                    self.dirs.insert(key, id);
                    self.nodes[id as usize].timestamps = e.timestamps;
                    if let Some(rec) = e.record_id {
                        self.mft_dirs.entry(rec).or_insert(id);
                    }
                }
            }
        }
        // Leftover files: the lost branch (with partition context).
        let pending = std::mem::take(&mut self.pending_files);
        for mut e in pending {
            let branch = match e.partition.as_deref() {
                Some(p) => format!("{p}\\File Path Lost"),
                None => "File Path Lost".to_string(),
            };
            e.parent_hint = None;
            e.partition = None;
            self.materialize_lost(e, &branch);
        }
    }

    /// Materialize a file directly onto a lost branch (path assembled
    /// from the branch, not a parent dir).
    fn materialize_lost(&mut self, e: FoundEntry, branch: &str) {
        let first_off = e.extents.first().map_or(u64::MAX, |x| x.offset);
        if first_off != u64::MAX
            && let Some(&existing_id) = self.by_first_extent.get(&first_off)
        {
            let existing = &self.nodes[existing_id as usize];
            if !existing.is_dir
                && (matches!(existing.origin, NodeOrigin::Existing | NodeOrigin::Deleted)
                    || (matches!(existing.origin, NodeOrigin::LostPath | NodeOrigin::LostName)
                        && existing.size == e.size))
            {
                return;
            }
        }
        let mut origin = e.origin;
        if let Some(key) = e.content_key
            && self.by_content_key.contains_key(&key)
        {
            origin = NodeOrigin::Version;
        }
        let category = e.category_hint.unwrap_or_else(|| classify(&e.name));
        let dir_id = self.ensure_lost_branch(branch);
        let id = self.nodes.len() as u64;
        let path = format!("{}\\{}", self.nodes[dir_id as usize].path, e.name);
        let recoverability = grade_from(&e);
        self.nodes.push(TreeNode {
            id,
            parent_id: Some(dir_id),
            name: e.name,
            is_dir: false,
            category,
            size: e.size,
            modified: e.timestamps.modified.unwrap_or_default(),
            origin,
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
        if let Some(key) = e.content_key {
            self.by_content_key.entry(key).or_insert(id);
        }
        let entry = &self.nodes[id as usize];
        let c = self.counts.entry(entry.category).or_insert((0, 0));
        c.0 += 1;
        c.1 += entry.size;
        self.roll_up(entry.id, entry.size, entry.modified);
    }

    /// The lost branch dir ("File Path Lost" / "Partition N\File Path
    /// Lost"), materialized on demand.
    fn ensure_lost_branch(&mut self, branch: &str) -> u64 {
        if let Some(&id) = self.dirs.get(branch) {
            return id;
        }
        // Nested form ("Partition N\File Path Lost").
        let (parent, name) = match branch.rsplit_once('\\') {
            Some((p, n)) => (self.ensure_dir_path(p), n),
            None => (0, branch),
        };
        let id = self.push_dir(Some(parent), name.to_string(), NodeOrigin::LostPath);
        self.dirs.insert(branch.to_string(), id);
        id
    }

    /// Resolve an entry's parent: path-hint dirs materialize on
    /// demand; carved entries land on the lost branches; `@mft:` refs
    /// resolve through the record map (session 8 — the add path
    /// guarantees resolution here; the defensive arm keeps the lost
    /// fallback for direct callers).
    fn resolve_parent(&mut self, e: &FoundEntry) -> (u64, Option<String>) {
        match (&e.parent_hint, e.origin) {
            (_, NodeOrigin::LostName) => (0, Some("Tags Extracted".into())),
            (None, NodeOrigin::LostPath | NodeOrigin::Tags) => (0, Some("File Path Lost".into())),
            (Some(_), NodeOrigin::LostPath) => (0, Some("File Path Lost".into())),
            (Some(hint), _) if hint.starts_with('@') => {
                match self.resolve_mft_dir(e.partition.as_deref(), hint) {
                    Some(dir) => (dir, None),
                    None => (0, Some("File Path Lost".into())),
                }
            }
            (Some(hint), _) if hint.is_empty() => {
                // Volume-root file on a partition unit.
                (self.ensure_partition(e.partition.as_deref()), None)
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

    /// The ORGANIZE-phase recoverability probe (the og
    /// `CCheckNodeThread` model — research:
    /// `DRWUI-subsystems--assistant-grading-preview.md` §2): read the
    /// FIRST sector of up to `cap` deleted files' first extents; a
    /// read failure means the media cannot serve the content — the
    /// grade drops to Poor no matter what the arithmetic said.
    /// Returns the number of nodes probed (diagnostics).
    ///
    /// Structured grades below Poor are never raised, only lowered —
    /// reading the first sector cannot prove the whole file is
    /// intact, but failing to read it proves the file is not
    /// recoverable as-is.
    pub fn apply_readability_grades(&mut self, dev: &dyn BlockDevice, cap: usize) -> usize {
        let ss = u64::from(dev.sector_size()).max(1);
        let mut probed = 0usize;
        for node in &mut self.nodes {
            if probed >= cap {
                break;
            }
            if node.is_dir || node.origin != NodeOrigin::Deleted || node.extents.is_empty() {
                continue;
            }
            // The resident-marker (zero-length extent at the record
            // offset) is metadata the MFT walk already read — skip.
            let Some(first) = node.extents.first() else {
                continue;
            };
            if first.len == 0 {
                continue;
            }
            // One sector through the cache layer: chunk-resident for
            // neighbors, a single positioned read on the miss path.
            let mut buf = vec![0u8; ss as usize];
            let readable = dev.read_sectors(first.offset / ss, &mut buf).is_ok();
            probed += 1;
            if !readable && node.recoverability != Recoverability::Poor {
                node.recoverability = Recoverability::Poor;
            }
        }
        probed
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
    fn same_region_carved_duplicate_absorbed() {
        let mut b = TreeBuilder::new();
        b.add(file(
            "JPG_0000C350.jpg",
            800,
            NodeOrigin::LostPath,
            Some(50_000),
        ));
        // A second carve of the SAME on-disk region (same first
        // extent + same size) is redundant, not a distinct copy.
        b.add(file(
            "JPG_0000C350 (2).jpg",
            800,
            NodeOrigin::LostPath,
            Some(50_000),
        ));
        let (files, bytes) = b.totals();
        assert_eq!((files, bytes), (1, 800), "region-identical carve absorbed");
    }

    #[test]
    fn same_content_elsewhere_marks_version() {
        let mut b = TreeBuilder::new();
        let mut first = file("JPG_0000C350.jpg", 800, NodeOrigin::LostPath, Some(50_000));
        first.content_key = Some(0xABCD);
        let mut second = file("JPG_0001A000.jpg", 800, NodeOrigin::LostPath, Some(106_496));
        second.content_key = Some(0xABCD);
        b.add(first);
        b.add(second);
        let nodes = b.nodes();
        let dup = nodes
            .iter()
            .find(|n| n.name == "JPG_0001A000.jpg")
            .expect("duplicate kept for complete mode");
        assert_eq!(dup.origin, NodeOrigin::Version, "repeat marked Version");
        let first = nodes
            .iter()
            .find(|n| n.name == "JPG_0000C350.jpg")
            .expect("original present");
        assert_eq!(first.origin, NodeOrigin::LostPath, "first copy unchanged");
        let (files, _) = b.totals();
        assert_eq!(files, 2, "both nodes exist (essential hides client-side)");
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

    // ── Session 8: the @mft pass-2 resolution ──────────────────────

    fn mft_entry(record: u64, parent: u64, name: &str, is_dir: bool) -> FoundEntry {
        let mut e = FoundEntry::structured(name.to_string(), is_dir, FsKind::Ntfs);
        e.parent_hint = Some(format!("@mft:{parent}"));
        e.record_id = Some(record);
        e
    }

    fn mft_file(record: u64, parent: u64, name: &str, size: u64, at: u64) -> FoundEntry {
        let mut e = mft_entry(record, parent, name, false);
        e.size = size;
        e.extents = vec![Extent::new(at, size)];
        e
    }

    #[test]
    fn mft_refs_build_the_folder_structure() {
        // Root (5), Photos dir (100), file inside (101) — parent
        // BEFORE child (the record order NTFS actually produces).
        let mut b = TreeBuilder::with_root("Data (D:)");
        b.add(mft_file(101, 100, "vacation.jpg", 500, 8192));
        b.add(mft_entry(100, 5, "Photos", true));
        b.add(mft_entry(5, 5, ".", true));
        b.finish();
        let nodes = b.nodes();
        let photo = nodes
            .iter()
            .find(|n| n.name == "vacation.jpg")
            .expect("file present");
        assert!(
            photo.path.contains("Photos"),
            "nested under its dir, got {}",
            photo.path
        );
        assert!(!photo.path.contains("File Path Lost"));
        let photos = nodes.iter().find(|n| n.name == "Photos").unwrap();
        assert_eq!(photos.size, 500, "sizes roll up the resolved chain");
    }

    #[test]
    fn mft_child_before_parent_resolves_via_fixpoint() {
        // Child emitted first, its dir later — the pending buffer +
        // fixpoint must still nest correctly.
        let mut b = TreeBuilder::with_root("Vol");
        b.add(mft_file(50, 40, "report.pdf", 100, 4096));
        b.add(mft_file(51, 40, "notes.txt", 50, 8192));
        b.add(mft_entry(40, 5, "Docs", true));
        b.add(mft_entry(5, 5, ".", true));
        b.finish();
        let nodes = b.nodes();
        for name in ["report.pdf", "notes.txt"] {
            let n = nodes.iter().find(|n| n.name == name).unwrap();
            assert!(n.path.contains("Docs"), "{name} nested, got {}", n.path);
        }
    }

    #[test]
    fn mft_partition_prefixes_nest_without_corrupting_refs() {
        // Whole-disk scan: partition rides the dedicated field.
        let mut b = TreeBuilder::with_root("Disk 0");
        let mut f = mft_file(101, 100, "a.jpg", 10, 1024);
        f.partition = Some("Partition 2".into());
        b.add(f);
        let mut d = mft_entry(100, 5, "Photos", true);
        d.partition = Some("Partition 2".into());
        b.add(d);
        let mut r = mft_entry(5, 5, ".", true);
        r.partition = Some("Partition 2".into());
        b.add(r);
        b.finish();
        let nodes = b.nodes();
        let photo = nodes.iter().find(|n| n.name == "a.jpg").unwrap();
        assert!(photo.path.contains("Partition 2"), "{}", photo.path);
        assert!(photo.path.contains("Photos"), "{}", photo.path);
        assert!(
            !nodes.iter().any(|n| n.name.contains('@')),
            "no literal @mft dirs"
        );
    }

    #[test]
    fn mft_unresolvable_parents_land_on_the_lost_branch() {
        // A file whose parent record never appears (damaged MFT).
        let mut b = TreeBuilder::with_root("Vol");
        b.add(mft_file(77, 12345, "orphan.doc", 300, 2048));
        b.finish();
        let nodes = b.nodes();
        let orphan = nodes.iter().find(|n| n.name == "orphan.doc").unwrap();
        assert!(orphan.path.contains("File Path Lost"), "{}", orphan.path);
        assert_eq!(orphan.origin, NodeOrigin::Existing);
    }

    #[test]
    fn resolve_pending_mid_scan_keeps_unresolved_pending() {
        // Mid-scan resolution materializes what it can; the rest stays
        // pending for the next boundary (never lost prematurely).
        let mut b = TreeBuilder::with_root("Vol");
        b.add(mft_file(10, 20, "waiting.txt", 10, 512));
        b.resolve_pending();
        assert!(
            b.nodes().iter().all(|n| n.name != "waiting.txt"),
            "unresolved stays pending mid-scan"
        );
        b.add(mft_entry(20, 5, "Later", true));
        b.add(mft_entry(5, 5, ".", true));
        b.resolve_pending();
        assert!(
            b.nodes().iter().any(|n| n.name == "waiting.txt"),
            "resolved after the parent arrives"
        );
    }

    #[test]
    fn sanitize_blocks_traversal() {
        assert_eq!(sanitize_name("..\\..\\evil"), ".._.._evil");
        assert_eq!(sanitize_name("ok.txt"), "ok.txt");
        assert_eq!(sanitize_name("   "), "_");
        assert_eq!(sanitize_name("con:trol*"), "con_trol_");
    }
}
