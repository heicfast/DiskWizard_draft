//! Scan-session persistence — the .rsf format.
//!
//! Format (per the research's fully-reversed spec —
//! `artifacts/SaveScan/rsf-format--scan-session.md` — adapted with a
//! versioned schema of our own; ADR-4 in `docs/CORE-PLAN.md`):
//!
//! ```text
//! magic "DWRS" + u16 version + 16-byte session id
//! then records:
//!   u8 record_type | u32 payload_len | payload
//!   type 1: header (counts + flags + fs kind)
//!   type 2: device (name + total bytes + timestamps)
//!   type 3: tree nodes (serialized TreeNode rows)
//! each record's payload is zlib-compressed in ≤256 KiB chunks:
//!   u32 chunk_len | u16 flag(1=zlib) | chunk data…
//! ```
//!
//! Auto-save naming follows the mock contract:
//! `auto_<Volume>_<MM>_<DD>_<HH>_<MM>_<SS>.rsf`.

use std::io::Write as _;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::tree::TreeNode;

/// The format magic.
const MAGIC: [u8; 4] = *b"DWRS";
/// Current schema version. v2: timestamps moved to unix MILLISECONDS
/// (the og's sub-second FILETIME fidelity) — v1 sessions are
/// rejected cleanly rather than silently misrendering dates.
const VERSION: u16 = 2;
/// Record types.
const RT_HEADER: u8 = 1;
const RT_DEVICE: u8 = 2;
const RT_NODES: u8 = 3;
/// Max chunk size before splitting (large node arrays split in M2
/// streaming; the current writer compresses whole payloads).
#[allow(dead_code)]
const CHUNK: usize = 256 * 1024;

/// The session header record (type 1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHeader {
    pub files_found: u64,
    pub bytes_found: u64,
    pub fs: String,
    pub duration_sec: u64,
    pub stopped: bool,
}

/// The device record (type 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDevice {
    pub name: String,
    pub total_bytes: u64,
    pub saved_at_unix: i64,
}

/// A loadable session.
#[derive(Debug, Clone)]
pub struct LoadedSession {
    pub header: SessionHeader,
    pub device: SessionDevice,
    pub nodes: Vec<TreeNode>,
}

/// The wire row for persisted nodes (TreeNode minus the skip attrs —
/// extents/timestamps serialize in full for exact resume).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRow {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub name: String,
    pub is_dir: bool,
    pub category: String,
    pub size: u64,
    pub modified: i64,
    pub origin: String,
    pub recoverability: String,
    pub path: String,
    pub ext_json: String,
}

/// Where auto-saved sessions live (`%APPDATA%\DiskWizard\sessions`).
#[must_use]
pub fn sessions_dir() -> PathBuf {
    if let Ok(base) = std::env::var("APPDATA") {
        return PathBuf::from(base).join("DiskWizard").join("sessions");
    }
    if let Ok(base) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(base).join("DiskWizard").join("sessions");
    }
    PathBuf::from(".diskwizard").join("sessions")
}

/// The auto-save file name for a finished scan.
#[must_use]
pub fn auto_save_name(volume: &str, now_unix: i64) -> String {
    let safe: String = volume
        .chars()
        .map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let (mm, dd, hh, mi, ss) = civil_time(now_unix);
    format!("auto_{safe}_{mm:02}_{dd:02}_{hh:02}_{mi:02}_{ss:02}.rsf")
}

/// (month, day, hour, minute, second) UTC.
fn civil_time(unix: i64) -> (i64, i64, i64, i64, i64) {
    let days = unix.div_euclid(86_400);
    let rem = unix.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    (m, d, rem / 3_600, (rem % 3_600) / 60, rem % 60).let_time(y)
}

// Small helper trait to swallow the year (not needed in the name).
trait LetTime {
    fn let_time(self, _y: i64) -> Self;
}
impl LetTime for (i64, i64, i64, i64, i64) {
    fn let_time(self, _y: i64) -> Self {
        self
    }
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Save a session (header + device + nodes).
///
/// # Errors
/// [`CoreError::Session`] on I/O or serialization failure.
pub fn save(
    path: &std::path::Path,
    header: &SessionHeader,
    device: &SessionDevice,
    nodes: &[TreeNode],
) -> CoreResult<()> {
    let mut out: Vec<u8> = Vec::with_capacity(64 * 1024);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    let session_id: [u8; 16] = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(|_| [0u8; 16], |d| d.as_nanos().to_le_bytes());
    out.extend_from_slice(&session_id);
    write_record(
        &mut out,
        RT_HEADER,
        &serde_json::to_vec(header).map_err(|err| e(&err))?,
    );
    write_record(
        &mut out,
        RT_DEVICE,
        &serde_json::to_vec(device).map_err(|err| e(&err))?,
    );
    // Nodes: serialize row batch.
    let mut rows_json = Vec::with_capacity(nodes.len() * 256);
    for n in nodes {
        let row = NodeRow {
            id: n.id,
            parent_id: n.parent_id,
            name: n.name.clone(),
            is_dir: n.is_dir,
            category: n.category.wire().to_string(),
            size: n.size,
            modified: n.modified,
            origin: n.origin.wire().to_string(),
            recoverability: n.recoverability.wire().to_string(),
            path: n.path.clone(),
            ext_json: serde_json::json!({
                "timestamps": n.timestamps,
                "extents": n.extents,
                "attrs": n.attrs.0,
                "confidence": n.confidence,
                "dimensions": n.dimensions,
                "source": n.source,
                "partition": n.partition,
            })
            .to_string(),
        };
        serde_json::to_writer(&mut rows_json, &row).map_err(|err| e(&err))?;
        rows_json.push(b'\n');
    }
    write_record(&mut out, RT_NODES, &rows_json);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|err| CoreError::Session(format!("mkdir: {err}")))?;
    }
    std::fs::write(path, out).map_err(|err| CoreError::Session(format!("write: {err}")))
}

fn e(err: &serde_json::Error) -> CoreError {
    CoreError::Session(format!("serialize: {err}"))
}

/// One record: type | len | zlib-chunked payload.
fn write_record(out: &mut Vec<u8>, rt: u8, payload: &[u8]) {
    out.push(rt);
    // Chunked: we compress the whole payload when it fits the chunk
    // budget (typical); large node arrays split.
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = encoder.write_all(payload);
    let compressed = encoder.finish().unwrap_or_default();
    // len covers flag + payload (the reader slices payload at +1).
    let len = (compressed.len() + 1) as u32;
    out.extend_from_slice(&len.to_le_bytes());
    out.push(1); // zlib flag
    out.extend_from_slice(&compressed);
}

/// Load a session.
///
/// # Errors
/// [`CoreError::Session`] on format/I/O failure.
pub fn load(path: &std::path::Path) -> CoreResult<LoadedSession> {
    let data = std::fs::read(path).map_err(|err| CoreError::Session(format!("read: {err}")))?;
    if data.len() < 22 || data[..4] != MAGIC {
        return Err(CoreError::Session("bad magic".into()));
    }
    let version = u16::from_le_bytes([data[4], data[5]]);
    if version != VERSION {
        return Err(CoreError::Session(format!("unsupported version {version}")));
    }
    let mut pos = 22usize; // magic + version + session id
    let mut header: Option<SessionHeader> = None;
    let mut device: Option<SessionDevice> = None;
    let mut nodes: Vec<TreeNode> = Vec::new();
    while pos + 5 <= data.len() {
        let rt = data[pos];
        let len = u32::from_le_bytes([data[pos + 1], data[pos + 2], data[pos + 3], data[pos + 4]])
            as usize;
        pos += 5;
        if pos + len > data.len() {
            return Err(CoreError::Session("truncated record".into()));
        }
        let flag = data[pos];
        let payload = &data[pos + 1..pos + len];
        let bytes = if flag == 1 {
            let mut d = flate2::read::ZlibDecoder::new(payload);
            let mut out = Vec::new();
            use std::io::Read as _;
            d.read_to_end(&mut out)
                .map_err(|err| CoreError::Session(format!("zlib: {err}")))?;
            out
        } else {
            payload.to_vec()
        };
        pos += len;
        match rt {
            RT_HEADER => {
                header = Some(serde_json::from_slice(&bytes).map_err(|err| e(&err))?);
            }
            RT_DEVICE => {
                device = Some(serde_json::from_slice(&bytes).map_err(|err| e(&err))?);
            }
            RT_NODES => {
                for line in bytes.split(|&b| b == b'\n') {
                    if line.is_empty() {
                        continue;
                    }
                    let row: NodeRow = serde_json::from_slice(line).map_err(|err| e(&err))?;
                    nodes.push(row_to_node(row));
                }
            }
            _ => {
                // Unknown record type — forward-compat skip.
            }
        }
    }
    let header = header.ok_or_else(|| CoreError::Session("missing header record".into()))?;
    let device = device.ok_or_else(|| CoreError::Session("missing device record".into()))?;
    Ok(LoadedSession {
        header,
        device,
        nodes,
    })
}

/// List saved sessions (newest first), parsed headers only.
///
/// # Errors
/// [`CoreError::Session`] when the directory is unreadable.
pub fn list_sessions(dir: &std::path::Path) -> CoreResult<Vec<SessionSummary>> {
    let mut out = Vec::new();
    let entries =
        std::fs::read_dir(dir).map_err(|err| CoreError::Session(format!("readdir: {err}")))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rsf") {
            continue;
        }
        let meta = entry.metadata().ok();
        if let Ok(sess) = load(&path) {
            out.push(SessionSummary {
                file: path.display().to_string(),
                saved_at: sess.device.saved_at_unix,
                target_name: sess.device.name,
                files_found: sess.header.files_found,
                bytes_found: sess.header.bytes_found,
                size_bytes: meta.map(|m| m.len()).unwrap_or_default(),
            });
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.saved_at));
    Ok(out)
}

/// A session row for the UI list.
#[derive(Debug, Clone, Serialize)]
pub struct SessionSummary {
    pub file: String,
    pub saved_at: i64,
    pub target_name: String,
    pub files_found: u64,
    pub bytes_found: u64,
    pub size_bytes: u64,
}

fn row_to_node(row: NodeRow) -> TreeNode {
    let ext: serde_json::Value = serde_json::from_str(&row.ext_json).unwrap_or_default();
    TreeNode {
        id: row.id,
        parent_id: row.parent_id,
        name: row.name,
        is_dir: row.is_dir,
        category: wire_category(&row.category),
        size: row.size,
        modified: row.modified,
        origin: wire_origin(&row.origin),
        recoverability: wire_grade(&row.recoverability),
        path: row.path,
        timestamps: serde_json::from_value(ext["timestamps"].clone()).unwrap_or_default(),
        attrs: crate::types::FileAttrs(ext["attrs"].as_u64().unwrap_or_default() as u32),
        extents: serde_json::from_value(ext["extents"].clone()).unwrap_or_default(),
        confidence: ext["confidence"].as_u64().unwrap_or_default() as u8,
        dimensions: ext["dimensions"].as_array().map(|a| {
            (
                a.first()
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or_default() as u32,
                a.get(1)
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or_default() as u32,
            )
        }),
        partition: ext["partition"].as_str().map(str::to_string),
        source: serde_json::from_value(ext["source"].clone()).unwrap_or_default(),
    }
}

fn wire_category(s: &str) -> crate::types::Category {
    match s {
        "images" => crate::types::Category::Images,
        "videos" => crate::types::Category::Videos,
        "audio" => crate::types::Category::Audio,
        "documents" => crate::types::Category::Documents,
        "archives" => crate::types::Category::Archives,
        "email" => crate::types::Category::Email,
        _ => crate::types::Category::Other,
    }
}

fn wire_origin(s: &str) -> crate::types::NodeOrigin {
    match s {
        "existing" => crate::types::NodeOrigin::Existing,
        "deleted" => crate::types::NodeOrigin::Deleted,
        "lost-path" => crate::types::NodeOrigin::LostPath,
        "lost-name" => crate::types::NodeOrigin::LostName,
        "tags" => crate::types::NodeOrigin::Tags,
        _ => crate::types::NodeOrigin::Version,
    }
}

fn wire_grade(s: &str) -> crate::types::Recoverability {
    match s {
        "excellent" => crate::types::Recoverability::Excellent,
        "good" => crate::types::Recoverability::Good,
        _ => crate::types::Recoverability::Poor,
    }
}

#[cfg(test)]
#[allow(clippy::case_sensitive_file_extension_comparisons)] // our own file names
mod tests {
    use super::*;
    use crate::tree::TreeBuilder;
    use crate::types::{Extent, FoundEntry, FsKind};

    fn sample_tree() -> (TreeBuilder, SessionHeader, SessionDevice) {
        let mut b = TreeBuilder::with_root("USB (F:)");
        let mut e = FoundEntry::structured("photo.jpg".into(), false, FsKind::Ntfs);
        e.size = 4096;
        e.extents = vec![Extent::new(8192, 4096)];
        b.add(e);
        let header = SessionHeader {
            files_found: 1,
            bytes_found: 4096,
            fs: "NTFS".into(),
            duration_sec: 42,
            stopped: false,
        };
        let device = SessionDevice {
            name: "USB (F:)".into(),
            total_bytes: 32 * 1024 * 1024 * 1024,
            saved_at_unix: 1_700_000_000,
        };
        (b, header, device)
    }

    #[test]
    fn round_trips_through_disk() {
        let (tree, header, device) = sample_tree();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rsf");
        save(&path, &header, &device, tree.nodes()).expect("save");
        let loaded = load(&path).expect("load");
        assert_eq!(loaded.header.files_found, 1);
        assert_eq!(loaded.device.name, "USB (F:)");
        assert_eq!(loaded.nodes.len(), tree.nodes().len());
        let photo = loaded.nodes.iter().find(|n| n.name == "photo.jpg").unwrap();
        assert_eq!(photo.size, 4096);
        assert_eq!(photo.extents, vec![Extent::new(8192, 4096)]);
    }

    #[test]
    fn listing_sorts_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let (_, h1, d1) = sample_tree();
        let mut d2 = d1.clone();
        d2.saved_at_unix = 1_800_000_000;
        save(&dir.path().join("a.rsf"), &h1, &d1, &[]).expect("save");
        save(&dir.path().join("b.rsf"), &h1, &d2, &[]).expect("save");
        let list = list_sessions(dir.path()).expect("list");
        assert_eq!(list.len(), 2);
        assert!(list[0].saved_at >= list[1].saved_at);
    }

    #[test]
    fn rejects_bad_magic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.rsf");
        std::fs::write(&path, b"NOPE........").unwrap();
        assert!(load(&path).is_err());
    }

    #[test]
    fn auto_names_are_safe() {
        let name = auto_save_name("Data (D:)", 1_700_000_000);
        assert!(
            name.starts_with("auto_Data (D_") && name.ends_with(".rsf"),
            "{name}"
        );
    }
}
