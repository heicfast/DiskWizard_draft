//! Shared fuzz-harness helpers: a read-only block device backed by
//! the arbitrary input bytes, a counting scan sink, and a
//! drive-to-completion runner. Every fuzz target runs a WHOLE engine
//! over the fuzzed "disk" — the same hostile-media threat model the
//! production scanner faces (SOP §12.3: zero panics, zero unbounded
//! allocations; libFuzzer's `-rss_limit_mb`/`-timeout` enforce the
//! resource side).

use std::sync::Arc;

use dw_core::blockdev::{BlockDevice, check_buf};
use dw_core::error::{CoreError, CoreResult};
use dw_core::fs::{ScanControl, ScanEngine, ScanSink, ScanStats};
use dw_core::types::FoundEntry;

/// A read-only 512-byte-sector device whose contents ARE the fuzz
/// input (the hostile disk image).
pub struct FuzzRam {
    pub data: Vec<u8>,
}

impl BlockDevice for FuzzRam {
    fn sector_size(&self) -> u32 {
        512
    }
    fn total_sectors(&self) -> u64 {
        (self.data.len() / 512) as u64
    }
    fn read_sectors(&self, start_lba: u64, buf: &mut [u8]) -> CoreResult<()> {
        if let Some(e) = check_buf(buf, 512) {
            return Err(e);
        }
        let start = start_lba as usize * 512;
        let end = start + buf.len();
        if end > self.data.len() {
            return Err(CoreError::ReadFailed {
                offset: start_lba * 512,
                len: buf.len() as u64,
                source: std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "past end"),
            });
        }
        buf.copy_from_slice(&self.data[start..end]);
        Ok(())
    }
    fn describe(&self) -> String {
        format!("fuzzram://{}x512", self.total_sectors())
    }
}

/// Wrap fuzz bytes as a device (empty input → an empty device).
pub fn ram(data: &[u8]) -> Arc<dyn BlockDevice> {
    Arc::new(FuzzRam {
        data: data.to_vec(),
    })
}

/// A sink that touches every entry field (names, extents, sizes —
/// exercise the whole pipeline's data path without allocating result
/// trees).
#[derive(Default)]
pub struct CounterSink {
    pub entries: u64,
    pub bytes: u64,
    pub names: u64,
}

impl ScanSink for CounterSink {
    fn on_entry(&mut self, e: FoundEntry) {
        self.entries += 1;
        self.bytes = self.bytes.wrapping_add(e.size);
        self.names = self.names.wrapping_add(e.name.len() as u64);
    }
    fn on_progress(&mut self, _done: u64, _total: u64) {}
}

/// Drive one engine's quick + deep passes to completion, ignoring
/// typed errors (invalid media is a normal outcome, not a crash).
pub fn drive(engine: &mut dyn ScanEngine) -> ScanStats {
    let ctrl = ScanControl::new();
    let mut sink = CounterSink::default();
    let mut stats = engine.quick_scan(&mut sink, &ctrl).unwrap_or_default();
    let deep = engine.deep_scan(&mut sink, &ctrl).unwrap_or_default();
    stats.entries += deep.entries;
    stats.bytes += deep.bytes;
    stats
}
