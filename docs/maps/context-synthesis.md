# DiskWizard — Phase 0 Context Synthesis

> **Phase 0 exit deliverable** (MASTER_PROMPT §10). Written from memory after
> full-corpus ingestion (see [doc-tree.md](doc-tree.md): 299/299 files covered;
> 49 FULL + 250 DIGEST with recorded digests). Every constant below is traceable
> to a research artifact; the governing file is named inline. This is the
> single-document memory of the studied product that DiskWizard's clean-room
> Rust core implements against.

## 1. What was studied (one paragraph)

The corpus is a multi-tool reverse-engineering of **EaseUS Data Recovery
Wizard** ("DRWIntelligentScan" internally): ~90 binaries, >10 M lines of
decompiled C, 6,903 RTTI classes, 7 kernel drivers, 9 filesystem scan engines,
a 190-format RAW carving engine, a transactional partition-recovery module
system (MOM/.mo), a repair suite (Fixo + in-house video repair), .rsf session
persistence, a WinPE bootable edition, cloud/NAS/BitLocker paths, and a
five-file config layer driving filtering/categorization. The architecture docs
(`architecture/00-07`) cross-link every artifact; raw evidence lives in
`re-workspace/reversed/` and `re-workspace/raw-evidence/`.

## 2. Product architecture (the studied shape → the DiskWizard mapping)

```
STUDIED PRODUCT                                    DISKWIZARD (clean-room, driverless)
─────────────────────────────────────────────     ─────────────────────────────────────
DRW.exe loader/watchdog + DRWUI.exe (Qt5)         Tauri 2 shell + React/TS Canvas UI (cherry-blossom theme)
10 SearchDLL engines (plugin factories)           dw-core engine set behind a trait seam
RAWSearchDLL (190 analysers + raw_signs.json)     dw-core carve engine (signature registry, bounded validators)
MOM .mo module system + pending-op engine         dw-core partition layer + restore queue
EPMNtDrv driver \\.\EPRDTDRV\d\p raw sector IO    user-mode \\.\PhysicalDriveN + volume handles
  (eprdtdrv/ebrntdrv/epmdkdrv, 4 builds)            (FILE_FLAG_NO_BUFFERING|OVERLAPPED, aligned, IOCP) — ADR-1 rev2
DRWService (FVE auto-unlock pipe)                 dw-win bitlocker (manage-bde + FVE registry)
EuRepairHost → Fixo repair EXEs (x86)             Phase-2 repair suite (TODO/phase-2-advanced-repair.md)
SaveScan .rsf (zlib-chunked records)              dw-core sessions (same architecture per ADR-4)
DiskHealth.exe (CrystalDiskInfo lineage, QML)     dw-win SMART (SAT/ATA + NVMe health log)
euactiveonline 3.1 → activation.easeus.com       DiskGenie licensing architecture ported 1:1
```

**The single most important architectural fact**: the studied product's scan
pipeline is *engine-per-filesystem with a uniform plugin contract*
(`EUCreate<FS>Searcher` / `EUCreate<FS>QuickSearcher` factories; interfaces
`IQuickFileSystemSearchController` / `ICompleteFileSystemSearchController` /
`IDataRunList` / `IFileProperties`; shared node classes
`CFileNode`/`CFileSystemNode`). The engine controller IS the tree builder AND
the bitmap builder (multiple inheritance). DiskWizard's dw-core mirrors this:
one engine module per filesystem behind a common controller trait, feeding a
shared node/property model (`SearchDLL/searchdll-family--architecture.md`,
`engine-matrix--factory-contract-verification.md`).

## 3. The scan pipeline (data flow, step by step)

1. **Device enumeration** (WMI + SetupAPI + IOCTL in the study; dw-win:
   PhysicalDrive + volume enumeration) → partition table parse (MBR/EBR/GPT) →
   lost-partition sweep of unallocated space (`PartitionRecoveryD` location
   generators: complete vs registry-guided; block parsers per FS; results as
   lostpart.dat/partinfo.dat) → each candidate validated by a filesystem
   analyser (boot sector / superblock / MFT sanity).
2. **Quick scan** = FS-typed searcher on the selected volume (FS codes: 0-2
   FAT, 3 NTFS, 4-5 EXT2, 7 exFAT, 8 HFS, 9 EXT4, 10 ReFS, 13 ZFS).
   **Advanced scan** = the full searcher set + `EUCreateRAWSearcher` (carving)
   in parallel, plus lost partitions inserted as scan targets
   (`architecture/04-recovery-workflow.md` §3).
3. **Per-FS engines** walk metadata (below §4) producing nodes with
   {name, size, flags (0x10 folder / 0x20 deleted), timestamps, anchor id,
   datarun/extent list}.
4. **RAW carving** runs multi-MB windows across unallocated/free regions:
   u32 fourcc + u16 magic-key registries (70 + 39 keys decoded), three-layer
   signature sources (hardcoded analysers, raw_signs.json 139 ext, EUUserData
   111), two-generation validator chains (modern + legacy fallback),
   index-first carving for containers (ZIP CD walk), brute-force frame-sync
   walkers for stream formats, confidence scoring, sector-aligned
   zero-padding extension (≤ 5.24 MB) for ZIP tails
   (`rawsearch-carving--deep-dive.md`, `rawsearchdll--zip-analyser-carving.md`).
5. **Merge & organize**: structured wins; carved nodes attach under
   "Lost"/"Other deleted files" branches; classification via FileFilter.xml
   (8 categories / 1,489 extensions) + PriorityDisplayType.xml ordering +
   FilterFolder.ini noise suppression (localized "New Folder" filters, 10
   locales) + SumaryFileType family grouping; validation threads grade
   recoverability; this config layer is the "65 % reduction" essential-mode
   mechanism (essential = filter noise + duplicates; complete = everything)
   (`config-files/filtering-priority--config-layer.md`).
6. **Preview**: per-type previewers; caps 100 MB input / 15 s video (free
   tier); damage-aware grading ("Recovery Chances: High"); streaming
   IInWindowStream (`UIUX/10-preview-subsystem.md`).
7. **Recovery**: destination dialog (Local/Cloud tabs + "Enable file repair"
   toggle ON by default; different-drive recommendation) → validation by
   opening `\\.\X:` of destination (same-volume detection) → export loop:
   engine-stream reads (raw sector IO under damaged volumes) → normal file
   writes; per-file failure log; progress forced 100 % on completion
   (`architecture/04-recovery-workflow.md` §7).
8. **Session**: .rsf = magic `EA F4 B3 1B…`, 16-byte session ids, 36-byte
   record headers, zlib-chunked payloads (4 KB input blocks, level 5, stored
   fallback); records 1 (session state) / 2 (volume + SYSTEMTIME + stats,
   147 B) / 3 (partition 0x6F5 struct + volume-device 0x1518 blob incl.
   512-byte boot snapshot) / 4 (BFS tree grammar, marker 0=CFileNode 34 B /
   1=CFileSystemNode; per-FS property types NTFS 0/FAT 1/RAW 3/exFAT 4/HFS+ 5/
   ReFS 6/Btrfs 7/EXT4 8/Generic 9/ZFS 0xC; type 0xB filtered both ways)
   (`SaveScan/rsf-format--scan-session.md`).

## 4. Filesystem engine constants (the implementation contract)

**NTFS** (`SearchDLL/ntfs-engine--deep-dive.md` + algorithms-v2):
$MFT bootstrap via boot BPB (bytes/sector, sectors/cluster, $MFT LCN ×
cluster size — NOT sector size), record size from clusters-per-record (neg →
1 << -v), ≥ 0x400 clamp; "FILE" magic 0x454C4946; fixup chain (USN at
+0x1E, per-sector slots +0x1FE, min 1 KiB parse size, copy-on-wrap for shared
buffers); $ATTRIBUTE_LIST (0x20) fork for fragmented MFT; attribute walk with
$-name filter + flags&4 skip; in-use bit → deleted branch; deleted-viability:
for non-folder files ≥ 0x3FF, test EVERY data-run cluster against the volume
$Bitmap (sector = cluster × spc) — `intact == total` → destroy (all clusters
reallocated); partially-intact surfaces with damage grading; $MFTMirr
cross-reference (fallback: parse MFT record 1's own $DATA runlist); runlist
nibble math `len += (b>>4) + 1 + (b&0xF)`; fastsearch default ON.

**FAT12/16/32** (`fat-engine--deep-dive.md` + fat-exfat algorithms-v2):
OEM label at 0x36 (FAT12/16) / 0x52 (FAT32); IFATTable polymorphism
(CFAT12/16/32Table + CDoubleFATTable mirrored-FAT with healthy-copy
selection); normalized BAD = 0xFFFFFF7, EOC ≥ 0xFFFFFF8 (FAT32 EOC family
0x0FFFFFF8..F + 0x0FFFFFF0 forms tolerated); cluster→sector
`(c-2)*SPC + first_data_sector`; chain walk with visited-range RB-tree cycle
guard; 64 KiB chain-length sanity cap on deleted entries; 0xE5 deleted 8.3
entries with LFN checksum reassembly (0x40 flag, 0x0F attr); "." self-entry
directory signature (8-byte ".       " + dir attr); Recycle Bin
$RECYCLE.BIN/$I…/$R… pairing.

**exFAT** (`exfat-engine--deep-dive.md` + algorithms-v2):
dentry types: volume label 0x83, allocation bitmap 0x81/0x82, up-case table,
file 0x85 + stream-extension 0xC0 + filename 0xC1; InUse bit 0x80 cleared on
delete (type preserved → `(t-X) & 0x7F` classification); stream-ext field
validation (reserved zeros at +2/+6/+7/+0x10..0x13; first-cluster/EOC family
at +0x14); deleted triple reconstruction = BACKWARD walk from stream dentry
at 0x40 strides into three list builders; allocation bitmap mirrors volume
state for deleted-cluster reasoning; NoFatChain contiguous runs; FAT-chain
re-derivation as the damaged-FAT hint path; RawFixer: backup DBR at LBA 12
(rolling checksum), TryFixFromSectorsPerCluster geometry re-derivation.

**ext2/3/4** (`ext4-engine--deep-dive.md` + ext4-hfs algorithms-v2):
superblock at offset 0x400, magic 0xEF53 at +0x38; validation: inodes_count
≠ 0, blocks_count ≠ 0, blocks_per_group ≠ 0, inodes_per_group ≠ 0;
first_data_block(1 KiB blocks → 1, else 0) cross-checked against
log_block_size; block_size = 0x400 << (log & 0x1F); total_blocks = hi(+0x150)
<< 32 | lo(+0x04); groups = total/blocks_per_group with GDT sanity; 64-bit
feature flag 0x10 at +0xF8 → desc_size u16 at +0x196 (32 vs 64-byte GDT
entries); volume label strncpy 0xF; backup superblocks (8193, 32768…) probed
for damaged volumes; journal traversal (inode 8) for recently-deleted state;
extents (extent header/idx/leaf) with indirect-block fallback for ext3;
CatchBuffer device proxy: on read failure retry with shifted window
(0x400-backtrack re-read) — bad-sector salvage; deleted = dtime≠0 / inode
unlink state, orphan-file/list handling.

**HFS+** (`hfsplus-engine--deep-dive.md` + ext4-hfs algorithms-v2):
BIG-ENDIAN volume header; signature "H+" (0x482B) or "HX" (0x4858, HFSX);
blockSize % 512 == 0; version < 0x11; allocationFile/extentsFile block counts
≤ totalBlocks; catalog B-tree walk (node header 14 B, records grow backward
from node end, record-offset array; leaf record placement = base + 1 +
record_count + attr_count); cnid parent chains (root folder = 2, catalog
file = 4…) with thread records; extents-overflow B-tree for forks > 8 extents;
attributes B-tree (decmpfs); journal-aware raw file reads; deleted HFS+
files: catalog records overwritten on delete → orphaned-cnid sweep + RAW
carving supplement; resource forks surface (rsrc fork model).

**ReFS** (`refs-engine--deep-dive.md` + refs-btrfs-zfs algorithms-v2):
`refs::` metadata block chain with page-type signatures — 1 = "SUPB" page, 2 =
"CHKP+SUPB" checkpointed superblock, 3 = "MSB" metadata session block, all
followed by the `ReFSFSRS` filesystem identity; block-header parse
(refs::RefsMetadataBlockRefrenceImpl [vendor typo] at page+8, size−0x20,
fields {valid, block_id, flags 0x2C, size 0x30, next 0x38} chained through
RefsBaseDataImplManager); DBR boot interpretation (refs::RefsDbrImpl); MSB
tree headers (RefsMSTreeHeaderDescImpl); directory/file value records
(RefsDirectoryValuesImpl / RefsFileValuesImpl); 32-byte object IDs → identity
/ parent linkage; small files resident (CRefsResidentDataRunList — the only
per-FS resident runlist the vendor's .rsf re-materializes by name);
newest-transaction-wins crash model (CHKP→SUPB→MSB chain), OS-version-gated
metadata layouts.

**RAW carving signature anchors** (registry census):
JPEG 0xFFD8FF{E0,E1,DB,EE}; PNG 0x474E5089; GIF8; %PDF; Rar!+1A07+00 (RAR4);
PK{0304,0102,0506} (all three ZIP structures accepted as anchors); TIFF
II*/MM*/IU/IRO* + ORF IIRO/IIRS/MMOR + MRW 1A49/1A4D; RAF "FUJI"; X3F "FOVb";
BZh1 (DMG); OggS; .RMF; MXF 06 0E 2B 34; ASF 30 26 B2 75; EBML 1A 45 DF A3
(vint grammar, header ≤ 0x200); HEIC brand 0x68656963; PST ver 0x0E (ANSI,
size @0xA8) / 0x17 (Unicode, size @0xB8) / else 1 MiB floor; ICO 00
01/00 02 00 00; OLE D0 CF 11 E0 (doc/xls/ppt/msi); RIFF; FORM; SQLi; DWG
AC10xx; CAB MSCF/mscf; u16 keys: BM/MM/%!/ID/FL/<h/<H/<!/1F 8B/FF F1 (ADTS)…
MPEG sync families; ZIP carve = index-first CD walk, exact size from sum of
entries, zero-padding extension to sector alignment (last 16 B of each
sector zero) capped 5.24 MB, status 0xd for indexless; two-layer validation
(registry magic + in-analyser revalidation).

## 5. I/O and drivers (what the user-mode core must equal)

The studied drivers provide: raw sector access at `\\.\EPRDTDRV\d\p`
(absolute LBA, 512-byte sectors, MDL async, pass-through IOCTL proxy, PID
token handshake), volume-lock hooks (EUDCPEPM: all-28-IRP hook + VolSnap
attach for VSS-consistent locking), and a NAS redirector. **DiskWizard is
driverless by decision** (ADR-1 rev2): equivalent throughput comes from
aligned overlapped reads, queue depth, adaptive chunk sizes, and carving
quality (§5.1 of MASTER_PROMPT). The vendor's own Device.mo keeps a
`\\.\HarddiskVolumeN` direct-open fallback when the driver probe fails —
direct evidence the user-mode path is viable. RawFixer semantics
(FAT32 backup sector 6; exFAT backup LBA 12 + rolling checksum DBR; NTFS
"only volumes that resist RawFixer fall through to pure RAW carving")
belong to the SDR/access-restore path (QA-9).

## 6. User journey (three steps — must be preserved)

1. **Select a location**: landing = PeripheralHomeView (default landing of the
   4-page wizard stack: 0 DrwHomePage / 1 DrwLostDiskPage / 2 PeripheralHomeView /
   3 CDetailPage); device cards with capacity bars, TRIM/BitLocker/lost states;
   quick actions (Recycle Bin / Desktop / Select Folder / disk image); lost
   partitions surfaced.
2. **Scan**: stack switches to CDetailPage (scan runs INSIDE the results
   page); two-phase quick→advanced with live counters ("N files (S) found",
   Searching strip, sector tickers), pause/resume/stop-confirm; 3-branch tree
   (Windows (C:) / File Path Lost / Reconstructed); browse-while-scan;
   auto-save .rsf at completion.
3. **Preview and recover**: results page with Path/Type trees (one tree
   class, mode specialization), filter bar (Show/Type/Date/Size), recoverability
   chips, preview panel (double-click) + standalone preview dialog, essential
   vs complete result modes; recovery funnel with destination validation,
   space gates, different-drive guard; progress honest to the byte.

## 7. Licensing + free tier (DiskGenie port — master prompt §8)

Architecture (from donor digests, 5-d): Ed25519 entitlement tokens verified
client-side against an embedded public key; HMAC-SHA256 request signing with
a shared client secret; postures Unlicensed / Pro / Grace (14 days) /
Degraded; 24 h revalidation; DPAPI at rest; hardware fingerprint (MachineGuid
‖ volume serial ‖ CPUID); key format DB-prefix + 20 Crockford base32 chars.
DiskWizard's config module must carry the **DiskGenie** endpoint
(`https://diskbytes-license.heictojpg-pics.workers.dev/`), public key
`4e4f51ef…`, secret `333d7117…`, UA `DiskBytes-License-Client/1`, headers
`x-db-*`, iss `db-license` / aud `diskgenie` until the DiskWizard server
deploys (OQ-3). Free tier: SMART/disk-health only.

## 8. What DiskWizard explicitly EXCLUDES (logged skips)

All AI surfaces: AI assistant robot + tooltip ("Meet your recovery
assistant"), AI Recommend (boot toast + FAB), AI Deduplication
(DrwAIDedupWindow), AI Recovery Insight cards, ChatPDF, NeedHelpMenu
assistant menu, AI panel auto-open behavior (5-c digest: complete skip list).
Also skipped: ad popups (AdToolPopWindow), retention/win-back gates
(RetentionExitDlg family, FreezeAnimalDialog, PirateVictimsTipDlg, UserRate,
follow-us), moving buy-guide windows, embedded-IE purchase webviews,
Firebase remote config, Aliyun telemetry, holiday theming. Kept: the
deterministic recoverability grading (CRecoverability grades 0/2,
device.kind ∈ {1,4} offset −2) and category rollup IDs (5 Graphics / 6
Document / 7 Archive / 8 Video / 9 Audio / 0xB Other).

## 9. Repair suite (Phase 2 spec pointers — see TODO/phase-2-advanced-repair.md)

Video: weighted ISO-BMFF atom grammar (ftyp pdin moov moof mfra mdat free
skip meta meco styp sidx ssix prft, weight 1.0), two-scan architecture,
moov triage with sample-file moov substitution, self-built sector bitmap
fragment tracking, useOrgFsDatarun vs regrouped datarun choice, HD-frag
rollback, per-format regroup (AVI idx1, MTS raw stream at DEVICE offset, MXF
KLV). Photo: JPEG skeleton FFD8FF01 + EOI D9 marker walk (SOF/DHT/SOS/DQT/
DRI, 0x400 marker bound, 0x7FFF length clamps); PNG chunk table + IEND CRC
0xAE426082 + BasisRepair 20/30/50/80 + SampleRepair 15/25/61/85 + salvage
zlib IDAT re-headering; GIF extent RB-tree re-chain; BMP DIB 12/40/108/124 +
palette math; TIFF endian vote + backward IFD scan; SVG envelope injection +
render-validate. Documents: PDF xref synthesis ("%010lld 00000 n\r\n",
/Size /Root /ID /Encrypt rules, six structural tokens) + decrypt (Dict
228-password list); Office OOXML shell synthesis (Content_Types template) +
OLE stream repair; HWPX container + OpenSSL CFB. Archive: 7z next-header
recovery + password via header-crypto or body CRC.

## 10. Session-5 governing conclusions

1. The engine seam, node/property model, quick/complete duality, and merge
   semantics are fully specified by the research — dw-core's existing shape
   is correct; the gaps are engine coverage (ext/HFS+/ReFS — QA-3), deep
   deleted-viability ($Bitmap — QA-4), exFAT chain re-derivation coverage
   (QA-5), and end-to-end proof of the essential-mode reduction (QA-6).
2. The QA-1 real-core tour has a precise reference contract: the wizard
   stack semantics, dialog chain, and telemetry-anchored completion pipeline
   (§6 above; UIUX digests).
3. The licensing §8 mandate has exact donor values to port (§7 above; donor
   digest) — the current dw-license crate deviates on every axis (endpoint,
   key, secret, UA, headers, iss/aud, key prefix) and must be re-pointed in
   one config module.
4. Preview contract (100 MB / 15 s / damage grading) is the research bar
   (QA-2 decision logged in OQ-5).
5. Driverless parity bar: large aligned overlapped reads, adaptive profiles,
   bad-sector isolation, carve-quality validators — never 1-sector reads or
   unbounded validators (§5.1; QA-11 for the IOCP queue-depth push).
