# DiskWizard Core — Development Plan (scratch → production engine)

> **Status: ACTIVE BUILD PLAN.** This is the master plan for replacing the
> simulated core with the production Rust engine, derived line-by-line from
> the `easeus-drw-research` artifacts (architecture/00-07 + 299 artifact
> docs), the frozen frontend contract (`docs/ARCHITECTURE.md`), and the
> DiskGenie donor (licensing / release / CI architecture). Cross-links:
> [ARCHITECTURE.md](ARCHITECTURE.md) (the IPC contract), [TODO.md](TODO.md)
> (repair phase = next priority), `worklog.md` (journal).

## 0. Mission

Build the production data-recovery core: real device enumeration, real
filesystem scan engines (quick + deep), RAW carving, recoverability
grading, real file recovery, SMART disk health, BitLocker handling, scan
session persistence, and the DiskGenie-architecture licensing client —
all behind the **frozen IPC contract** so the shipped UI needs zero
changes. No mocks, no stubs, no hardcoded results in the shipped path.
The existing mock cores stay for development/tour only, behind cargo
features that never enter release builds.

## 1. Product scope (from the owner brief + research)

| Feature | Priority | Phase |
|---|---|---|
| Recover data (any file type) | P0 | M1 |
| Recover from any storage media (HDD/SSD/USB/SD/ISO/VHD) | P0 | M1 |
| All file systems: FAT12/16/32, NTFS/NTFS5, exFAT, ext2/3, HFS+, ReFS | P0 | M1-M2 |
| RAW carving (signature scan, sector-by-sector) | P0 | M1 |
| BitLocker data recovery (unlock → scan) | P0 | M2 |
| Preview files before recovery | P0 | M1 (core streams bytes) |
| Pause/resume/stop scans, progressive results | P0 | M1 |
| Scan session persistence (.rsf) | P0 | M2 |
| Lost-partition detection + scan targets | P0 | M1 (detect) / M2 (repair path) |
| SD Card Rescue (access-restore then recovery) | P1 | M2 |
| Disk health (SMART) — **FREE tier** | P0 | M1 |
| Essential/complete results modes (~65% noise cut) | P0 | M1 |
| Licensing: paid app, DW- keys, Ed25519 entitlements | P0 | M1 |
| Recoverability indicators (Excellent/Good/Poor) | P0 | M1 |
| File repair (photo/doc/video) | next priority | see TODO.md |
| ~~Cloud recovery~~ | removed | session 8 (owner directive) |

**Explicitly excluded for now**: AI features, robo mascot, AI pop-ups
(owner directive — ignore any such surfaces in the research corpus).

## 2. Workspace architecture

```
src-tauri/
├── Cargo.toml                # workspace root + the app package
├── crates/
│   ├── dw-core/              # Platform-INDEPENDENT engine (no windows deps)
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── types.rs      # FileEntry, FileId, Extent, Timestamps, DeviceInfo…
│   │   │   ├── blockdev/     # BlockDevice trait + ImageFile impl + cached reader
│   │   │   ├── partition/    # MBR, EBR, GPT, LDM probe; lost-partition scan
│   │   │   ├── fs/           # per-FS engines (trait ScanEngine)
│   │   │   │   ├── mod.rs    #   registry: fs code → engine factory
│   │   │   │   ├── ntfs/     #   MFT walk, $MFTMirr, attributes, runlists,
│   │   │   │   │             #   LZNT1 decompress, deleted viability, $Secure
│   │   │   │   ├── fat/      #   FAT12/16/32, cluster chains, LFN, deleted
│   │   │   │   ├── exfat/    #   exFAT: bitmap, FAT, dir entries, InUse triples
│   │   │   │   ├── ext/      #   ext2/3/4: superblock, group descs, inodes,
│   │   │   │   │             #   extents, dir recursion, journal-aware (ext3/4)
│   │   │   │   ├── hfsplus/  #   catalog + extents B-trees, journal
│   │   │   │   └── refs/     #   ReFS: superblock, B+ tree, object IDs
│   │   │   ├── raw/          # RAW carving: signature registry, analysers,
│   │   │   │   │             #   grammar walks (JPG/PNG/ZIP/RAR/PDF/MP4/MOV/
│   │   │   │   │             #   MP3/WAV/GIF/BMP/HEIC/DOC(X)/XLS(X)/PPT(X)…)
│   │   │   ├── tree/         # found-file tree, classification (FileFilter),
│   │   │   │   │             #   dedup/merge (structured wins), branch flags
│   │   │   ├── grade.rs      # recoverability grading (extent readability,
│   │   │   │                 #   size sanity, deletion damage → Poor/Good/Excellent)
│   │   │   ├── scan/         # ScanController: quick→deep phases, progress,
│   │   │   │                 #   cancel/pause token, generation guard
│   │   │   ├── recover/      # export engine: extent reads → dest writes,
│   │   │   │                 #   destination validation, collision policy
│   │   │   ├── session/      # .rsf scan-session writer/reader (zlib chunks)
│   │   │   ├── classify/     # extension → category tables (8 categories)
│   │   │   └── error.rs      # typed engine errors (0x5002xxxx-style codes)
│   ├── dw-win/               # Windows platform layer (windows-rs)
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── devices.rs    # physical disk + volume enumeration (IOCTL +
│   │   │   │                 #   SetupAPI + mount points; USB/SD/SSD classification)
│   │   │   ├── blockdev.rs   # \\.\PhysicalDriveN + \\.\X: BlockDevice impls
│   │   │   ├── volume.rs     # volume info: FS detect, TRIM, BitLocker status,
│   │   │   │                 #   recycle-bin resolution, free space
│   │   │   ├── smart/        # CAtaSmart-port: SAT ATA-pass-through, NVMe
│   │   │   │                 #   admin queue, CSMI; attribute parsing; health
│   │   │   ├── bitlocker.rs  # FVE status via native APIs + manage-bde fallback
│   │   │   └── admin.rs      # elevation check
│   └── dw-license/           # DiskGenie-architecture license client
│       ├── src/
│       │   ├── lib.rs        # protocol client (LicenseHttp seam), token verify
│       │   ├── token.rs      # Ed25519 entitlement verify (payload.sig)
│       │   ├── posture.rs    # unlicensed/pro/grace/degraded state machine
│       │   ├── hwid.rs       # machine fingerprint (seam; win impl in dw-win)
│       │   └── store.rs      # DPAPI-wrapped persistence (seam; win impl)
├── src/                      # the app crate — REAL command implementations
│   ├── lib.rs                #   (tauri builder, feature-routed commands)
│   └── commands/
│       ├── mod.rs            # dispatch: real ↔ sim (cargo features)
│       ├── devices.rs        # list_devices, refresh_devices, smart_report…
│       ├── scan.rs           # start/pause/resume/stop, progress, results
│       ├── recover.rs        # destinations, start_recovery, preview stream
│       ├── session.rs        # list/resume sessions
│       ├── license.rs        # license_status/activate/deactivate/validate
│       ├── settings.rs       # persisted settings
│       └── mock/             # THE MOCK (unchanged) — sim builds only
```

**Why a workspace**: `dw-core` has zero Windows dependencies — the engines
are pure algorithms over `&mut dyn BlockDevice` (or generic `R: ReadSeek`).
That means the entire engine surface compiles and unit-tests on Linux CI
(and locally), while `dw-win` compiles only on Windows. This is the same
seam the research describes (engines behind `EUCreate*Searcher` factories
over a volume-IO abstraction) translated to Rust idioms.

## 3. The frozen contract → core mapping

Every IPC command from `docs/ARCHITECTURE.md` maps to a real engine call:

| Command (frozen) | Real implementation |
|---|---|
| `list_devices` | dw-win::devices (physical + volumes + removable + BitLocker/TRIM/lost states) |
| `refresh_devices` | re-enumerate + WM_DEVICECHANGE subscription |
| `smart_report` | dw-win::smart multi-transport report (FREE — never gated) |
| `sdr_diagnose` | partition-table sanity check + guided repair (M2: RawFixer-lite) |
| `unlock_bitlocker` | dw-win::bitlocker (password / recovery key / FVE auto-unlock) |
| `start_scan` / `start_folder_scan` / `start_disk_image_scan` | dw-core::scan controller over the resolved BlockDevice |
| `start_cloud_scan` | REMOVED (session 8 — owner directive: local recovery only) |
| `pause/resume/stop_scan` | controller pause token / cancel flag |
| `get_scan_progress` | controller snapshot (phase machine preserved) |
| `get_scan_results` | tree snapshot (generation-guarded) |
| `list_sessions` / `resume_session` | dw-core::session (.rsf) |
| `list_recover_destinations` | dw-win::devices (writable volumes + space) |
| `start_recovery` | dw-core::recover export engine |
| `license_*` | dw-license (Ed25519 entitlement client) |
| `get_settings`/`set_settings` | persisted JSON (AppData) |
| `get_dev_hooks` | unchanged (tour/CI) |

Events preserved exactly: `scan-progress`, `scan-done`, `recover-progress`,
`recover-done`, `license-changed`, `devices-changed`.

**Payload shapes are frozen** — the real core must return the same JSON
field names the UI consumes (e.g. `ScanProgress.phase`:
`preparing|quick|deep|organizing|done|paused|stopped`; result categories;
recoverability `excellent|good|poor`). Where the mock invented realistic
values, the real core produces the real ones through the same schema.

## 4. Engine design (dw-core) — key algorithms

All algorithms below are specified in the research artifacts; the
summaries live in the session context and the artifacts remain the
authoritative reference (`re-workspace/artifacts/SearchDLL/*.md`).

### 4.1 BlockDevice + volume abstraction
- `trait BlockDevice: Send { fn read_sectors(&self, lba: u64, buf: &mut [u8]) -> io::Result<()>; sector_size/total_sectors }`
- Implementations: `ImageFile` (any OS; .iso/.vhd/.img/flat), Windows
  `PhysicalDrive`/`VolumeHandle` (dw-win), `PartitionSlice` (offset view
  for scanning a partition inside a disk), `CachedDevice` (read-ahead
  buffer — the research's "auxiliary" read layer).
- Reader contract: sector-aligned reads, 512..4096 logical sector sizes,
  errors mapped to the typed `CoreError` (scan survives bad sectors —
  `Result` per extent, not fatal).

### 4.2 Partition layer
- MBR (protective + classic), EBR chains, GPT (header + entry array,
  CRC-checked, backup header), LDM probe (VMDB/TOCBLOCK magic per the
  research), plus **lost-partition scan**: sector-wise boot-signature
  sweep (NTFS BPB "NTFS", FAT "FAT", exFAT "EXFAT", ext magic at 0x438,
  HFS+ "H+"/"HX" at 1024, ReFS) → candidate partitions with StartSector/
  TotalSector (the research's "Insert lost partition %s…" path).
- Output: `PartitionInfo` list with fs hints, mounted-letter mapping,
  `is_lost` flag (no table entry / gap-resident).

### 4.3 NTFS engine (the deepest — artifact: ntfs-engine deep-dive + v2)
- Boot sector parse (OEM "NTFS", BPB, MFT LBA, clusters-per-MFT-record),
  MFT record walk: fixup array (USN apply), $STANDARD_INFORMATION,
  $FILE_NAME (parent ref → path build), $DATA (resident + non-resident
  runlists with nibble length/offset math), $ATTRIBUTE_LIST fork,
  $INDEX_ROOT/$INDEX_ALLOCATION for directory recursion, $BITMAP for
  used-cluster awareness.
- Deleted records: entry present but unlinked → keep, mark deleted;
  deleted-viability scoring (artifact v2): extent-intact ratio gates
  (intact==total → destroy/drop as overwritten, thresholds per research).
- Compressed/sparse files: LZNT1 decompressor for resident-compressed
  streams (recovered only when quick-scan safe; grading reflects risk).
- $MFTMirr fallback when record damage detected.
- Quick scan: MFT walk only. Deep scan: MFT walk + unallocated-aware
  pass + cross-check with RAW results during merge.

### 4.4 FAT engine
- FAT12/16/32 detection via BPB; cluster-chain walker with **cycle guard**
  (visited set — research v2 artifact), BAD-mark handling (0xFFFFFF7),
  EOC (≥0xFFFFFF8), deleted entries (0xE5 first byte), LFN reassembly
  (0x40 last-fragment + sequence checks), root directory (FAT12/16 fixed
  region, FAT32 cluster chain), orphan chain salvage.
- Deep: full directory-entry sweep including erased-entry reanimation.

### 4.5 exFAT engine
- Boot region (main + backup, checksum validation), FAT + cluster bitmap,
  directory entries (0x85 stream, 0xC0 name, 0xC1 name-hash, file/dir/tex
  entries), InUse-bit triple reconstruction (the v2 artifact's
  `(t−X)&0x7F` algorithm) for deleted files, chain re-derivation from
  hint when FAT damaged.

### 4.6 ext2/3/4 engine
- Superblock (magic 0xEF53 at 1024, feature flags, desc_size), group
  descriptors, inode table walk, extent trees (ext4) + classic indirect
  blocks (ext2/3), directory entries (linear + htree), deleted inodes
  (dtime==0 check reversed), journal awareness (ext3/4: .journal
  reconstruction = deep-only), dtime/ctime preservation.

### 4.7 HFS+ engine
- Volume header at 1024 ("H+", "HX" for case-sensitive), catalog B-tree
  (leaf records → threads → hard links), extents overflow B-tree,
  journal detection, deleted catalog records (deep pass: free-node
  reclamation from the B-tree free-list + unallocated space).

### 4.8 ReFS engine
- Superblock identification ("MSR" + ReFS identifiers per the deep-dive),
  object-ID B+ tree walk, checksummed metadata pages (the refs:: MSB
  semantics from v2 artifacts), identifier tables. Deep: page-chain
  recovery of orphaned trees. (ReFS is Windows Server/Dev-Drive — the
  research notes the smallest engine; ship after the majors.)

### 4.9 RAW carving engine
- Registry-driven: `Signature { magic, offset, tail?, min_size, max_size,
  category, confidence, analyser }`; per-format analysers implement
  "grammar walks" beyond magic bytes (the research's 106-analyser
  registry reduced to a curated production set):
  - JPEG (FFD8FF + EOI FFD9 walk, restart-marker aware)
  - PNG (IHDR walk + IEND)
  - GIF (header + trailer 0x3B, extent trees)
  - BMP (DIB header size taxonomy 12/40/108/124 + pixel math)
  - ZIP (PK\x03\x04 + central-directory walk — the index-first carving
    algorithm from the research), RAR (4/5 markers)
  - 7z, TAR, GZ
  - PDF (%PDF → %%EOF, xref-aware)
  - MP4/MOV (ftyp + moov/mdat atom walk — ISO-BMFF grammar)
  - MKV/WEBM (EBML vint walk)
  - MP3 (ID3 + frame sync), WAV/RIFF (fmt walk), FLAC, OGG
  - DOC/XLS/PPT (OLE2 CFB header + FAT walk), DOCX/XLSX/PPTX (ZIP
    grammar + [Content_Types].xml confirmation)
  - HEIC (ftyp heic/hevc/mif1 + ipco/ipma box walk)
  - SQLite, PST (0xA8/0xB8 header triples per research), DWG
- Carving output: nodes under "File Path Lost" branches with per-format
  confidence feeding the recoverability grade; carving-only files get
  `grade ≤ good` cap (mock contract preserved: carved = capped grade).
- Merge: structured results win (same first-sector → structured node
  absorbs carved duplicate; the ~65% essential-mode noise reduction is
  the UI toggle over the same merged tree).

### 4.10 Tree, classification, grading
- `FoundTree`: nodes with `branch` flags (Normal / File Path Lost / Tags
  Extracted / Reconstructed), category classification via extension
  tables (8 categories mirroring FileFilter.xml: Graphics, Video, Audio,
  Document, Email, Archive, Folders, Other), size roll-ups, dir counts.
- Grading (CCheckNodeThread analog): per-node extent readability probe
  (sample reads through the block device), size sanity, deletion damage
  → `Excellent | Good | Poor` (the UI's recoverability chips + preview
  gauge). Grading runs progressively so results fill while scanning.

### 4.11 Scan controller
- Phases preserved: `preparing → quick → deep → organizing → done`
  (+ paused/stopped control states) with the same progress semantics
  (quick ≈ 0-38%, deep ≈ 38-100% of the aggregate bar; counters:
  files found, bytes found, sectors read/total).
- `ScanToken`: atomic cancel + pause; engine loops check it per record
  batch (the research's per-record cancel contract).
- Generation guard: results snapshots are generation-tagged; stale
  progress events are dropped by the app layer.
- Deep-scan carve toggles per category (RawConfig.ini analog) — default
  on for all.

### 4.12 Recovery engine
- Selection set (files/folders) → ordered export queue; per-file extent
  reads through the source BlockDevice → destination writes (normal file
  APIs); destination validation: different-volume check (warning when
  same), free-space gate, writability probe; collision policy (numbered
  suffix); ACL preservation on capable destinations; failure collection
  → recovery log; progress events (file/byte counters, current file).
- BitLocker-unlocked volumes scan as NTFS post-unlock (the common path);
  damaged-metadata volumes: the deep scan works on the raw encrypted
  stream only when keys available (M3 hardening).

### 4.13 Session persistence (.rsf)
- Our production .rsf: zlib-chunked record framing (magic + 16-byte
  session id + 36-byte record headers — format per the research's
  rsf-format artifact, records 1-4: header/device/partitions/tree),
  written progressively (auto-save at scan completion — the
  auto_<Volume>_<ts>.rsf naming preserved from the mock contract).
- Reader: full validation, versioned schema, forward-compat unknown-
  record skip.

### 4.14 SMART / disk health (FREE tier — never gated)
- Transports (the CAtaSmart port): ATA PASS-THROUGH (SAT, 28/48-bit),
  NVMe IDENTIFY + SMART/HEALTH log via admin passthrough, CSMI for
  legacy controllers; USB bridge identification (SAT-capable query).
- Attributes: the standard 22-attribute table (ID/current/worst/threshold
  /raw/status) + temperature + spin-up + power-on hours + reallocated/
  pending sectors; health verdict Good/Caution/Bad (threshold rules);
  per-disk identity (model, serial, firmware, capacity, rotation vs SSD).

## 5. Licensing (dw-license) — the DiskGenie architecture, verbatim

Wire-identical to `diskgenie-v2`'s documented system (owner directive:
"exactly same and as it is"):
- Ed25519-signed entitlement tokens (payload.sig), client-side verify
  against the embedded public key; payload: iss/ver/jti/iat/exp(14d)/
  key/tier/name/email/hw/plat/lexp.
- Requests HMAC-SHA256-signed (ts + nonce + method + path + body hash);
  endpoints `/v1/activate|validate|deactivate|health`; typed error
  contract (KEY_NOT_FOUND, KEY_REVOKED, DEVICE_SLOT_TAKEN, RATE_LIMITED,
  LICENSE_EXPIRED…). Base URL + public key = compile-time consts, env
  override for tests (DISKWIZARD_LICENSE_API).
- Postures: unlicensed / pro / grace (offline ≤14d) / degraded;
  24h revalidation scheduler + launch check; DPAPI-wrapped state at
  rest; hardware fingerprint = SHA-256(MachineGuid ‖ system-volume
  serial ‖ CPUID) — frozen algorithm.
- Free tier boundary: ONLY disk health (SMART) + device listing +
  settings work unlicensed; every scan/recovery/SDR command re-checks
  posture IN RUST (L6 command gates), throwing the ACTIVATION_REQUIRED
  marker the frontend intercepts.
- Server: the owner will deploy the diskgenie-license-server worker for
  DiskWizard (same code, `dw-license` namespace) — client consts point
  at the deployment; demo keys remain for sim builds only.

## 6. The mock retention plan (dev-only simulation)

- `src/mock/` (TS): unchanged — browser dev + tests (already DEV-gated).
- `src-tauri/src/commands/mock/`: unchanged code, now registered ONLY
  when the cargo feature `sim` is on. Release builds (NSIS/MSIX) never
  enable `sim`; the tour + ui-screenshots workflow builds with
  `--features sim` (mirroring DiskGenie's ci-license-sim pattern).
- `license_sim_set` stays sim-gated. `get_dev_hooks` remains always-on
  (env-driven, no data).
- Real core = default feature set. This satisfies "two simulation cores
  during development, real core ships".

## 7. Testing & validation strategy (production grade)

1. **Unit tests (every OS, Linux CI included)**: each engine gets
   fixture-image tests — the test suite BUILDS filesystem images
   (mkfs-like writers in dw-core test utils: FAT12/16/32, exFAT, NTFS
   with crafted MFT records, ext4 via manual structures, GPT/MBR tables)
   then scans them: asserts found sets, deleted detection, path trees,
   runlist math, LZNT1 round-trip, fixup application, cycle guards,
   grading rules, .rsf round-trip, carving grammars (crafted fragments
   + fuzzed tails).
2. **Property/fuzz tests**: random-cut images (sector corruption
   patterns) — engines must degrade gracefully (no panics — panic-free
   guarantee on untrusted input is a hard requirement).
3. **Integration on Windows CI (real disks, real APIs)**:
   - VHD pipeline: create VHD → attach → format (NTFS/FAT32/exFAT) →
     seed files → delete some → detach/sleep → **scan the raw device** →
     assert recovered set incl. deleted → recover to folder → byte-
     compare against originals. This is the end-to-end truth test.
   - Device enumeration snapshot test (runner disks present, ≥1 NTFS).
   - SMART: read real attributes (runner disks), assert schema + verdict
     computed.
   - License: fake-transport unit tests + Ed25519 fixture verification
     (like DiskGenie's) — live E2E when the owner deploys the server.
4. **Gates in CI** (extend existing ci.yml): `cargo fmt --check`,
   `cargo clippy --workspace --all-targets -D warnings`, full workspace
   tests on Windows, **dw-core-only test job on ubuntu** (proves the
   platform-independence seam), frontend typecheck + vitest unchanged.
5. **VLM validation**: ui-screenshots tour with the REAL core build
   (separate step from the sim tour) on the Windows runner — walk
   recover→scan→results with a real (VHD) target, capture screenshots +
   app logs; VLM-decode the frames for phase/result sanity.
6. **Multi-POV review checkpoints** (per owner SOP): senior-dev (code
   review pass), PM (feature completeness vs contract), security
   researcher (raw-IO safety, path traversal in recovery, licensing
   bypass review), production manager (CI green, artifacts, logs),
   paying-user simulation (unlicensed funnel + Pro unlock).

## 8. Build phases (milestones)

- **M0 — Foundation** (this session, first push): workspace restructure,
  dw-core types/blockdev/partition + dw-win skeleton + license client
  port + app feature-routing. CI stays green.
- **M1 — The critical path** (this session): NTFS + FAT + exFAT engines,
  RAW carving core set, tree/classify/grade, scan controller, recovery
  engine, device enumeration, SMART, BitLocker detect+unlock, license
  client live, .rsf sessions, real commands wired end-to-end.
- **M2 — Coverage completion** (next sessions): ext2/3/4, HFS+, ReFS,
  SDR (access-restore), lost-partition direct-repair path, cloud scan
  (OAuth2), deeper carving set, RawFixer-lite repair pass.
- **M3 — Hardening**: performance tuning (parallel reads, cached
  device, 1M-node benchmarks), long-run stability (24h soak on runner),
  crash-resilience (scan journal), MSIX Store readiness.

## 9. SOPs & rules (binding for all core code)

1. **Latest stable crates only** (owner rule, no fallbacks): versions
   pinned to current crates.io stable at implementation time.
2. **No TODO/FIXME/stub markers** in shipped code (CI-enforced,
   DiskGenie R2.2 gate).
3. **Clippy**: workspace `-D warnings` with pedantic allowed-list per
   crate; `cargo fmt` enforced.
4. **Panic-free on untrusted input**: all parsers use checked reads
   (`try_into`, bounds checks); fuzz tests assert no panics.
5. **No blocking IPC**: long operations run on worker threads; IPC
   commands return immediately; events carry progress.
6. **Memory discipline**: bounded caches (LRU with cap), no unbounded
   Vec growth (chunked iteration), tree nodes arena-allocated.
7. **Cross-linking**: every module header cites its research artifact
   (`artifacts/SearchDLL/ntfs-engine--deep-dive.md` etc.) — reviewers
   can trace every algorithm decision to evidence.
8. **Commit cadence**: push at each green milestone (≤10 files).
9. **Worklog**: `worklog.md` in the repo (core sessions append; separate
   from the UI/UX agents' sections).
10. **Security**: recovery paths must sanitize destinations (no path
    traversal from carved names), command gates in Rust, no secrets in
    logs, DPAPI at rest for license state.

## 10. ADRs (architecture decision records)

- **ADR-1 (rev 2): high-performance user-mode direct I/O IS the
  product path.** The research documents the vendor's EPMNtDrv driver
  (IOCTLs 0x60002400/04/08, MDL async sector IO), but a kernel
  driver is NOT required for professional recovery throughput — the
  gains that matter come from large aligned reads, positioned
  overlapped requests (no shared file pointer), adaptive scheduling,
  low allocation overhead, and good carving algorithms, all
  achievable from user mode through `\\.\PhysicalDriveN`, volume
  handles, and storage IOCTLs. dw-win implements exactly that
  discipline:
  - handles open `FILE_FLAG_NO_BUFFERING | FILE_FLAG_OVERLAPPED`
    (buffered fallback when a filter stack rejects no-buffering);
  - every read carries its offset in the `OVERLAPPED` — concurrent
    calls race nowhere (no `seek`);
  - buffer addresses align to the media's logical sector
    (`STORAGE_ACCESS_ALIGNMENT` probe); the engine's cache chunks and
    the carver's 8 MiB windows allocate aligned, so the metadata hot
    path and the bulk sweep both read in place;
  - media errors never storm: a failed bulk read degrades to ONE
    attempt per sector (`read_sectors_sparse`), salvaging every
    readable sector and reporting the dead ranges;
  - the carve sweep reads multi-megabyte sparse windows with
    SIMD-friendly `memmem` matching, not per-sector calls.
  A driver becomes justified later only for capabilities user mode
  cannot safely provide (low-level filter behavior, tightly
  controlled imaging, very early-boot rescue). Until then the
  `BlockDevice` trait remains the seam a driver-backed implementation
  could slot into without engine changes — but it is NOT on the M1/M2
  path, and user-mode I/O is not a stopgap: it is the design.
- **ADR-2: engines as pure-Rust over Read+Seek.** Maximum testability
  (Linux CI), no unsafe in engines; all Windows-specific IO in dw-win.
- **ADR-3: keep both mocks behind features.** Owner requirement (dev
  simulation + tour determinism); release builds compile neither.
- **ADR-4: .rsf compatibility.** We implement the research-documented
  framing (magic, zlib chunks, record types) with a versioned schema —
  NOT byte-identical to the vendor's (our tree model differs), but the
  same architecture (records 1-4, auto-save naming, resume support).
- **ADR-5: BitLocker via OS unlock in M1/M2.** The common user path is
  unlock-then-scan (password/recovery key via FVE APIs → volume mounts
  decrypted → NTFS engine). Raw XTS-AES stream recovery (BitLockerLib
  port) is M3 hardening for damaged volumes.
