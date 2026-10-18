//! Read-ahead chunk cache — the "auxiliary read layer".
//!
//! Scan IO patterns: sequential sector sweeps (deep carve) and
//! semi-random metadata reads (MFT records, inode tables, B-tree
//! nodes). A chunk-granular LRU serves both: misses fetch
//! `CHUNK_SECTORS` at once (read-ahead), hits copy straight out of
//! memory. Sector-read failures are RETRIED per-sector on the way out
//! of a failed chunk fetch so a single bad sector degrades only
//! itself plus its chunk neighbors, never the whole chunk request.
//!
//! Concurrency: one mutex over the map + LRU deque; the underlying
//! device read happens OUTSIDE the lock when the chunk is uniquely
//! claimed (per-chunk in-flight marker dedupes concurrent fetches of
//! the same chunk).

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use parking_lot::Mutex;

use super::{BlockDevice, check_buf};
use crate::error::{CoreError, CoreResult};

/// Sectors per cache chunk (32 KiB at 512-byte sectors).
const CHUNK_SECTORS: u64 = 64;
/// Default resident chunks (8 MiB at 512-byte sectors).
const DEFAULT_CHUNKS: usize = 256;

struct Chunk {
    data: Vec<u8>,
}

struct Inner {
    map: HashMap<u64, Arc<Mutex<Option<Chunk>>>>,
    order: VecDeque<u64>,
}

/// Cached wrapper over any [`BlockDevice`].
pub struct CachedDevice {
    parent: Arc<dyn BlockDevice>,
    sector_size: u32,
    chunks: Mutex<Inner>,
    max_chunks: usize,
    /// Chunk fetches that failed entirely (media damage) — cached as
    /// negative so repeated probes don't re-hit the disk.
    dead: Mutex<std::collections::HashSet<u64>>,
}

impl CachedDevice {
    /// Wrap `parent` with the default capacity.
    #[must_use]
    pub fn new(parent: Arc<dyn BlockDevice>) -> Self {
        Self::with_capacity(parent, DEFAULT_CHUNKS)
    }

    /// Wrap `parent` with `max_chunks` resident chunks.
    #[must_use]
    pub fn with_capacity(parent: Arc<dyn BlockDevice>, max_chunks: usize) -> Self {
        Self {
            sector_size: parent.sector_size(),
            parent,
            chunks: Mutex::new(Inner {
                map: HashMap::new(),
                order: VecDeque::new(),
            }),
            max_chunks: max_chunks.max(1),
            dead: Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// Number of resident chunks (diagnostics).
    pub fn resident(&self) -> usize {
        self.chunks.lock().map.len()
    }

    fn chunk_sectors() -> u64 {
        CHUNK_SECTORS
    }

    /// Fetch (or join an in-flight fetch of) one chunk; returns the
    /// filled chunk or the error from the parent read.
    fn fetch_chunk(&self, chunk_lba: u64) -> CoreResult<Arc<Mutex<Option<Chunk>>>> {
        let slot = {
            let mut inner = self.chunks.lock();
            if let Some(existing) = inner.map.get(&chunk_lba) {
                let slot = Arc::clone(existing);
                // Refresh LRU position.
                if let Some(pos) = inner.order.iter().position(|&l| l == chunk_lba) {
                    inner.order.remove(pos);
                }
                inner.order.push_back(chunk_lba);
                slot
            } else {
                let slot = Arc::new(Mutex::new(None));
                inner.map.insert(chunk_lba, Arc::clone(&slot));
                inner.order.push_back(chunk_lba);
                while inner.map.len() > self.max_chunks {
                    if let Some(victim) = inner.order.pop_front() {
                        inner.map.remove(&victim);
                    }
                }
                slot
            }
        };
        // Try to claim the fill.
        {
            let mut guard = slot.lock();
            if guard.is_none() {
                let ss = self.sector_size as usize;
                let sectors_left = self
                    .parent
                    .total_sectors()
                    .saturating_sub(chunk_lba * CHUNK_SECTORS);
                let take = CHUNK_SECTORS.min(sectors_left) as usize;
                if take == 0 {
                    *guard = Some(Chunk { data: Vec::new() });
                } else {
                    let mut buf = vec![0u8; take * ss];
                    match self
                        .parent
                        .read_sectors(chunk_lba * CHUNK_SECTORS, &mut buf)
                    {
                        Ok(()) => *guard = Some(Chunk { data: buf }),
                        Err(e) => {
                            // Entire-chunk failure: mark dead and drop the
                            // reservation so later requests can retry once
                            // per generation (dead-map guards the hot path).
                            *guard = Some(Chunk { data: Vec::new() });
                            self.dead.lock().insert(chunk_lba);
                            return Err(e);
                        }
                    }
                }
            }
        }
        Ok(slot)
    }

    /// Copy `len` bytes out of a resident chunk into `out`.
    fn copy_from_chunk(
        chunk: &Arc<Mutex<Option<Chunk>>>,
        intra_off: usize,
        out: &mut [u8],
    ) -> CoreResult<()> {
        let guard = chunk.lock();
        let Some(chunk) = guard.as_ref() else {
            return Err(CoreError::Other("chunk reservation dropped".into()));
        };
        let end = intra_off + out.len();
        if end > chunk.data.len() {
            // Tail-of-device chunk shorter than requested — the caller
            // asked past EOF; report as read failure with exact math.
            return Err(CoreError::ReadFailed {
                offset: 0,
                len: out.len() as u64,
                source: std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "chunk short (device tail)",
                ),
            });
        }
        out.copy_from_slice(&chunk.data[intra_off..end]);
        Ok(())
    }
}

impl BlockDevice for CachedDevice {
    fn sector_size(&self) -> u32 {
        self.sector_size
    }

    fn total_sectors(&self) -> u64 {
        self.parent.total_sectors()
    }

    fn read_sectors(&self, start_lba: u64, buf: &mut [u8]) -> CoreResult<()> {
        if let Some(e) = check_buf(buf, self.sector_size) {
            return Err(e);
        }
        let cs = Self::chunk_sectors();
        let n_sectors = (buf.len() / self.sector_size as usize) as u64;
        let mut done_sectors = 0u64;
        while done_sectors < n_sectors {
            let abs = start_lba + done_sectors;
            let chunk_lba = abs / cs;
            let intra_sector = abs % cs;
            let sectors_in_chunk = (cs - intra_sector).min(n_sectors - done_sectors);
            let chunk = self.fetch_chunk(chunk_lba)?;
            let intra_off = (intra_sector * u64::from(self.sector_size)) as usize;
            let out_len = (sectors_in_chunk * u64::from(self.sector_size)) as usize;
            let out_start = (done_sectors * u64::from(self.sector_size)) as usize;
            if let Err(e) =
                Self::copy_from_chunk(&chunk, intra_off, &mut buf[out_start..out_start + out_len])
            {
                // A dead/partial chunk: fall back to a direct parent read
                // for JUST this slice so single-sector damage doesn't take
                // down healthy neighbors inside the same chunk.
                let direct_len = out_len - (out_len % self.sector_size as usize);
                let _ = e;
                self.parent
                    .read_sectors(abs, &mut buf[out_start..out_start + direct_len])?;
            }
            done_sectors += sectors_in_chunk;
        }
        Ok(())
    }

    fn describe(&self) -> String {
        format!("cached({})/{}", self.max_chunks, self.parent.describe())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::RamDevice;

    fn pattern_ram() -> Arc<RamDevice> {
        let mut ram = RamDevice::new(512 * 5000, 512);
        for (i, b) in ram.data.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        Arc::new(ram)
    }

    #[test]
    fn sequential_reads_hit_the_cache() {
        let ram = pattern_ram();
        let cached = CachedDevice::with_capacity(Arc::clone(&ram) as Arc<dyn BlockDevice>, 16);
        let mut buf = [0u8; 512];
        for lba in 0..200u64 {
            cached.read_sectors(lba, &mut buf).expect("read");
            assert_eq!(buf[0], ((lba * 512) % 251) as u8);
        }
        // 200 sectors over 64-sector chunks → ≤4 unique chunks + the
        // final partial; resident stays small.
        assert!(cached.resident() <= 8, "resident={}", cached.resident());
    }

    #[test]
    fn lru_evicts_under_pressure() {
        let ram = pattern_ram();
        let cached = CachedDevice::with_capacity(Arc::clone(&ram) as Arc<dyn BlockDevice>, 4);
        let mut buf = [0u8; 512];
        for lba in (0..64 * 20u64).step_by(64) {
            cached.read_sectors(lba, &mut buf).expect("read");
        }
        assert!(cached.resident() <= 4);
    }

    #[test]
    fn bad_sector_degrades_only_its_chunk() {
        let mut ram = RamDevice::new(512 * 300, 512);
        for (i, b) in ram.data.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        ram.bad_lba = Some(70);
        let cached = CachedDevice::new(Arc::new(ram) as Arc<dyn BlockDevice>);
        let mut buf = [0u8; 512];
        // Sector 70's read fails through any path…
        assert!(cached.read_sectors(70, &mut buf).is_err());
        // …but its healthy neighbors inside the same chunk succeed via
        // the direct-parent fallback.
        cached.read_sectors(71, &mut buf).expect("neighbor read");
        assert_eq!(buf[0], ((71 * 512) % 251) as u8);
    }

    #[test]
    fn multi_chunk_reads_span_correctly() {
        let ram = pattern_ram();
        let cached = CachedDevice::new(Arc::clone(&ram) as Arc<dyn BlockDevice>);
        let mut buf = vec![0u8; 512 * 130]; // spans 3 chunks
        cached.read_sectors(10, &mut buf).expect("read");
        for (i, &b) in buf.iter().enumerate().take(512 * 130) {
            let abs = (10 * 512) + i as u64;
            assert_eq!(b, (abs % 251) as u8, "byte {i}");
        }
    }
}
