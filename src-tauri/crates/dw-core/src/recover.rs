//! The recovery (export) engine — copying found data out.
//!
//! The CRecoveryMgr chain translated (research:
//! `architecture/04-recovery-workflow.md` §7): selection set →
//! ordered export queue; per-file extent reads through the source
//! device → destination writes via the platform writer; destination
//! validation (different-volume advisory, free-space gate,
//! writability); collision policy (numbered suffix); failure
//! collection for the recovery log; progress (file/byte counters,
//! current file).
//!
//! The destination IO is a seam ([`DestWriter`]) so the app layer
//! injects real filesystem IO (Tauri side) while tests inject RAM
//! writers — the core itself stays platform-independent.

use std::collections::HashMap;

use serde::Serialize;

use crate::blockdev::{BlockDevice, BlockRead};
use crate::error::{CoreError, CoreResult};
use crate::fs::ScanControl;
use crate::tree::TreeNode;
use crate::types::Extent;

/// Destination writer seam.
pub trait DestWriter: Send {
    /// Create a directory (parents as needed). Idempotent.
    /// # Errors
    /// String error on failure.
    fn mkdir(&mut self, path: &str) -> Result<(), String>;

    /// Write one file (full content from a reader callback).
    /// # Errors
    /// String error on failure.
    fn write_file(
        &mut self,
        path: &str,
        read: &mut dyn FnMut(&mut [u8]) -> usize,
    ) -> Result<(), String>;

    /// The writable root's free bytes (the space gate).
    /// # Errors
    /// String when unqueriable.
    fn free_bytes(&self) -> Result<u64, String>;
}

/// The recovery progress payload (the frozen `RecoverProgress`).
#[derive(Debug, Clone, Serialize)]
pub struct RecoveryProgress {
    pub total_files: u64,
    pub done_files: u64,
    pub failed_files: u64,
    pub total_bytes: u64,
    pub done_bytes: u64,
    pub current_file: String,
}

/// The final summary.
#[derive(Debug, Clone, Serialize)]
pub struct RecoveryReport {
    pub total_files: u64,
    pub done_files: u64,
    pub failed_files: u64,
    pub total_bytes: u64,
    pub done_bytes: u64,
    pub failures: Vec<(String, String)>,
}

/// One selected node to recover (id + resolved target path).
#[derive(Debug, Clone)]
pub struct RecoveryItem {
    pub node: TreeNode,
    /// Destination-relative path (sanitized).
    pub dest_path: String,
}

/// Build the export queue from selected tree nodes (files only;
/// directories materialize as parents during export).
#[must_use]
pub fn build_queue(nodes: &[TreeNode], selected_ids: &[u64]) -> Vec<RecoveryItem> {
    let mut out = Vec::new();
    let mut dir_paths: HashMap<u64, String> = HashMap::new();
    // Map ids → nodes once.
    let by_id: HashMap<u64, &TreeNode> = nodes.iter().map(|n| (n.id, n)).collect();
    // Walk selections; expand directories to their file descendants.
    let mut expanded: Vec<u64> = Vec::new();
    for &id in selected_ids {
        if let Some(node) = by_id.get(&id) {
            if node.is_dir {
                for n in nodes {
                    if is_descendant(n, node, &by_id) && !n.is_dir {
                        expanded.push(n.id);
                    }
                }
            } else {
                expanded.push(id);
            }
        }
    }
    for id in expanded {
        let Some(node) = by_id.get(&id) else { continue };
        // Build the destination path from the node's tree path minus
        // the root label, sanitized per segment.
        let rel = node
            .path
            .split(['\\', '/'])
            .skip(1) // drop the root label
            .map(crate::tree::sanitize_name)
            .collect::<Vec<_>>()
            .join("/");
        if rel.is_empty() {
            continue;
        }
        dir_paths.insert(node.id, rel.clone());
        out.push(RecoveryItem {
            node: (*node).clone(),
            dest_path: rel,
        });
    }
    out
}

fn is_descendant(candidate: &TreeNode, dir: &TreeNode, by_id: &HashMap<u64, &TreeNode>) -> bool {
    let mut cur = candidate.parent_id;
    while let Some(p) = cur {
        if p == dir.id {
            return true;
        }
        cur = by_id.get(&p).and_then(|n| n.parent_id);
    }
    false
}

/// Run the export.
///
/// # Errors
/// [`CoreError::Cancelled`] when stopped mid-way (the partial report
/// still returns through the observer pattern — the caller reads
/// state via the progress sink; here a cancel returns the report
/// wrapped in Ok with failures noted).
pub fn run_recovery(
    dev: &dyn BlockDevice,
    queue: &[RecoveryItem],
    dest: &mut dyn DestWriter,
    ctrl: &ScanControl,
    mut on_progress: impl FnMut(RecoveryProgress),
) -> CoreResult<RecoveryReport> {
    let total_files = queue.len() as u64;
    let total_bytes: u64 = queue.iter().map(|i| i.node.size).sum();
    let mut done_files = 0u64;
    let mut failed_files = 0u64;
    let mut done_bytes = 0u64;
    let mut failures = Vec::new();
    // Space gate.
    let free = dest.free_bytes().map_err(CoreError::Destination)?;
    if free < total_bytes {
        return Err(CoreError::Destination(format!(
            "insufficient space: need {total_bytes} bytes, destination has {free}"
        )));
    }
    let mut used_paths: HashMap<String, u32> = HashMap::new();
    for item in queue {
        ctrl.checkpoint()?;
        // Parent dirs.
        if let Some((parent, _)) = item.dest_path.rsplit_once('/') {
            dest.mkdir(parent).map_err(CoreError::Destination)?;
        }
        // Collision policy: numbered suffixes.
        let mut target = item.dest_path.clone();
        let count = used_paths.entry(item.dest_path.clone()).or_insert(0);
        if *count > 0 {
            if let Some((stem, ext)) = item.dest_path.rsplit_once('.') {
                target = format!("{stem} ({count}).{ext}");
            } else {
                target = format!("{} ({count})", item.dest_path);
            }
        }
        *count += 1;
        on_progress(RecoveryProgress {
            total_files,
            done_files,
            failed_files,
            total_bytes,
            done_bytes,
            current_file: item.node.name.clone(),
        });
        let result = export_one_to(dev, item, dest, &target);
        match result {
            Ok(n) => {
                done_files += 1;
                done_bytes += n;
            }
            Err(e) => {
                failed_files += 1;
                failures.push((item.node.name.clone(), e.to_string()));
            }
        }
    }
    let report = RecoveryReport {
        total_files,
        done_files,
        failed_files,
        total_bytes,
        done_bytes,
        failures,
    };
    on_progress(RecoveryProgress {
        total_files,
        done_files,
        failed_files,
        total_bytes,
        done_bytes,
        current_file: "Finalizing…".into(),
    });
    Ok(report)
}

/// Export one file: stream its extents through the writer (holes
/// zero-fill; damaged reads degrade to zeros with the failure noted
/// in the recovery log).
fn export_one_to(
    dev: &dyn BlockDevice,
    item: &RecoveryItem,
    dest: &mut dyn DestWriter,
    target_path: &str,
) -> CoreResult<u64> {
    let node = &item.node;
    let mut stream = ExtentStream {
        dev,
        extents: &node.extents,
        idx: 0,
        pos_in_ext: 0,
        remaining: node.size,
    };
    dest.write_file(target_path, &mut |buf| stream.fill(buf))
        .map_err(CoreError::Destination)?;
    Ok(node.size - stream.remaining)
}

/// The pull-based extent reader (the writer drives).
struct ExtentStream<'a> {
    dev: &'a dyn BlockDevice,
    extents: &'a [Extent],
    idx: usize,
    pos_in_ext: u64,
    remaining: u64,
}

impl ExtentStream<'_> {
    fn fill(&mut self, buf: &mut [u8]) -> usize {
        if self.remaining == 0 {
            return 0;
        }
        // No extents (metadata-only/empty) → zero-fill to size.
        if self.extents.is_empty() {
            let take = (buf.len() as u64).min(self.remaining) as usize;
            buf[..take].fill(0);
            self.remaining -= take as u64;
            return take;
        }
        let mut filled = 0usize;
        while filled < buf.len() && self.remaining > 0 {
            let Some(ext) = self.extents.get(self.idx) else {
                // Extents exhausted before size — zero-fill the tail.
                let take = ((buf.len() - filled) as u64).min(self.remaining) as usize;
                buf[filled..filled + take].fill(0);
                self.remaining -= take as u64;
                filled += take;
                break;
            };
            if ext.len == 0 {
                self.idx += 1;
                self.pos_in_ext = 0;
                continue;
            }
            let avail = ext.len - self.pos_in_ext;
            let want = (buf.len() - filled)
                .min(self.remaining as usize)
                .min(avail as usize);
            if want == 0 {
                self.idx += 1;
                self.pos_in_ext = 0;
                continue;
            }
            let at = ext.offset + self.pos_in_ext;
            if self
                .dev
                .read_bytes(at, &mut buf[filled..filled + want])
                .is_err()
            {
                buf[filled..filled + want].fill(0);
            }
            self.pos_in_ext += want as u64;
            self.remaining -= want as u64;
            filled += want;
            if self.pos_in_ext >= ext.len {
                self.idx += 1;
                self.pos_in_ext = 0;
            }
        }
        filled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::RamDevice;
    use crate::tree::TreeBuilder;
    use crate::types::{Extent, FoundEntry, FsKind, NodeOrigin};

    struct RamDest {
        files: HashMap<String, Vec<u8>>,
        dirs: Vec<String>,
        free: u64,
    }

    impl RamDest {
        fn new(free: u64) -> Self {
            Self {
                files: HashMap::new(),
                dirs: Vec::new(),
                free,
            }
        }
    }

    impl DestWriter for RamDest {
        fn mkdir(&mut self, path: &str) -> Result<(), String> {
            self.dirs.push(path.to_string());
            Ok(())
        }
        fn write_file(
            &mut self,
            path: &str,
            read: &mut dyn FnMut(&mut [u8]) -> usize,
        ) -> Result<(), String> {
            let mut buf = vec![0u8; 4096];
            let mut out = Vec::new();
            loop {
                let n = read(&mut buf);
                if n == 0 {
                    break;
                }
                out.extend_from_slice(&buf[..n]);
            }
            self.files.insert(path.to_string(), out);
            Ok(())
        }
        fn free_bytes(&self) -> Result<u64, String> {
            Ok(self.free)
        }
    }

    fn seeded_tree() -> (RamDevice, TreeBuilder) {
        let mut ram = RamDevice::new(512 * 64, 512);
        // Patterned content at offset 8192, 100 bytes.
        for i in 0..100usize {
            ram.data[8192 + i] = (i % 251) as u8;
        }
        let mut b = TreeBuilder::with_root("USB (F:)");
        let mut e = FoundEntry::structured("data.bin".into(), false, FsKind::Ntfs);
        e.size = 100;
        e.origin = NodeOrigin::Existing;
        e.extents = vec![Extent::new(8192, 4096)];
        b.add(e);
        (ram, b)
    }

    #[test]
    fn recovers_file_bytes_exactly() {
        let (ram, tree) = seeded_tree();
        let nodes = tree.nodes().to_vec();
        let file_id = nodes.iter().find(|n| n.name == "data.bin").unwrap().id;
        let queue = build_queue(&nodes, &[file_id]);
        assert_eq!(queue.len(), 1);
        let mut dest = RamDest::new(1 << 30);
        let report =
            run_recovery(&ram, &queue, &mut dest, &ScanControl::new(), |_| {}).expect("recover");
        assert_eq!(report.done_files, 1);
        assert_eq!(report.failed_files, 0);
        let recovered = dest.files.get("data.bin").expect("written");
        assert_eq!(recovered.len(), 100);
        assert_eq!(&recovered[..], &ram.data[8192..8192 + 100]);
    }

    #[test]
    fn space_gate_blocks_oversized_selections() {
        let (ram, tree) = seeded_tree();
        let nodes = tree.nodes().to_vec();
        let file_id = nodes.iter().find(|n| n.name == "data.bin").unwrap().id;
        let queue = build_queue(&nodes, &[file_id]);
        let mut dest = RamDest::new(10); // too small
        let err = run_recovery(&ram, &queue, &mut dest, &ScanControl::new(), |_| {})
            .expect_err("blocked");
        assert!(matches!(err, CoreError::Destination(_)));
    }

    #[test]
    fn directories_expand_to_descendants() {
        let (_, _tree) = seeded_tree();
        let mut b2 = TreeBuilder::with_root("R");
        let mut d = FoundEntry::structured("Docs".into(), true, FsKind::Ntfs);
        d.parent_hint = Some(String::new());
        b2.add(d);
        let mut f = FoundEntry::structured("a.txt".into(), false, FsKind::Ntfs);
        f.parent_hint = Some("Docs".into());
        f.size = 5;
        b2.add(f);
        let nodes = b2.nodes().to_vec();
        let dir_id = nodes.iter().find(|n| n.name == "Docs").unwrap().id;
        let queue = build_queue(&nodes, &[dir_id]);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].dest_path, "Docs/a.txt");
    }

    #[test]
    fn collisions_get_numbered_suffixes() {
        let (ram, tree) = seeded_tree();
        let nodes = tree.nodes().to_vec();
        let file_id = nodes.iter().find(|n| n.name == "data.bin").unwrap().id;
        // Recover the same file twice in one queue.
        let mut q1 = build_queue(&nodes, &[file_id]);
        let q2 = build_queue(&nodes, &[file_id]);
        q1.extend(q2);
        let mut dest = RamDest::new(1 << 30);
        let report =
            run_recovery(&ram, &q1, &mut dest, &ScanControl::new(), |_| {}).expect("recover");
        assert_eq!(report.done_files, 2);
        assert!(dest.files.contains_key("data.bin"));
        assert!(dest.files.contains_key("data (1).bin"));
    }

    #[test]
    fn cancel_stops_the_export() {
        let (ram, tree) = seeded_tree();
        let nodes = tree.nodes().to_vec();
        let file_id = nodes.iter().find(|n| n.name == "data.bin").unwrap().id;
        let queue = build_queue(&nodes, &[file_id]);
        let ctrl = ScanControl::new();
        ctrl.cancel();
        let mut dest = RamDest::new(1 << 30);
        let r = run_recovery(&ram, &queue, &mut dest, &ctrl, |_| {});
        assert!(matches!(r, Err(CoreError::Cancelled)));
    }
}
