# Phase 2 — Advanced Repair Engines (Full Spec)

> Status: **queued for the next session** (owner decision, session 1:
> "the repair phase ships next, once the core idea and implementation
> are production-ready"). This document captures the complete
> specification so nothing is lost — every algorithm below is
> evidence-grounded in the research repository.
>
> Scope per `MASTER_PROMPT.md` §6.4: repair is an **integrated
> function of DiskWizard, NOT a separate product/service**. The
> interface contracts and UI wiring already exist (the
> `enable-file-repair` toggle persists through the recovery funnel;
> the completion-summary counters include repair-during-recovery).
>
> Research index:
> `re-workspace/artifacts/FileRepair/repair-architecture.md`
> (the component map), `image-repair--format-grammar.md` +
> `--state-machines.md` + `--gif-bmp-svg-tiff-walks.md` (photo),
> `deepening-batch2--pdf-office-repair.md` (documents),
> the video-repair source-tree decode (§2 below),
> `archive-repair--7z-engine.md` (archives),
> `x3f-repair--section-checkers-walk.md` (RAW).

## 1. Advanced Photo Repair

**Formats**: JPEG/JPG (JFIF/EXIF), PNG, GIF, BMP, TIFF, SVG, WebP,
JPEG XL, plus RAW families (3FR/ARW/AVIF/CR2/CR3/CRW/DNG/ERF/GPR/
HEIC/MRW/NEF/ORF/PEF/RAF/RAW/RW2/SR2/SRF/SRW/X3F).

**Algorithm (the vendor's grammar-repair model):**

1. **JPEG** (`jpeg/jpeg.cpp` — `JPEG::CJpegInfo`, the flagship):
   marker-grammar walk (SOI → APPn → DQT → SOF → DHT → SOS → EOI);
   repair = re-derive the marker chain: drop corrupt
   markers, re-sync at the next valid marker after damage,
   re-emit a canonical marker stream around the entropy-coded
   data; truncated files get a synthesized EOI (decodable prefix).
2. **PNG**: chunk-grammar (IHDR…IDAT…IEND) with CRC re-stamping —
   damaged chunk lengths are re-derived from the following chunk
   signature scan; IDAT streams re-inflated and re-deflated when
   the zlib stream breaks mid-way.
3. **GIF/BMP/TIFF/SVG/WebP/JXL**: header+data rebuild per the
   per-format walks (`image-repair--gif-bmp-svg-tiff-walks.md`).
4. **RAW**: LibRaw-class demosaic containers; embedded-JPEG
   scanning inside RAW containers (`otherType/scanJpeg.cpp` —
   carve the embedded preview JPEG when the RAW body is lost);
   X3F section-checker walks (`x3f-repair--section-checkers-walk.md`).
5. **Preview-before-recovery**: repaired bytes render in the
   existing preview pipeline (before/after compare view per the
   repair UI flows).

**Rust core plan**: `dw-repair` crate; per-format `RepairEngine`
trait (`analyze -> DamageReport`, `repair(src, dst) -> Outcome`);
bounded streaming parsers (the §5.1.7 memory rules apply); the
existing carver's signature grammar is shared infrastructure.

## 2. Document Repair

**Formats**: PDF; DOC/DOCX, XLS/XLSX, PPT/PPTX; HWP/HWPX (the
vendor's Korean-market surface — optional, last).

**Algorithms:**

1. **PDF** (`EuPDFRepairFile` — the decoded xref/trailer
   synthesis): scan the whole body for `N M obj` headers; rebuild
   `xref` tables from discovered objects; synthesize the `trailer`
   (`/Size` from max obj, `/Root` from the catalog scan, preserving
   `/Encrypt` when present — byte-exact canonical format strings
   documented in `deepening-batch2--pdf-office-repair.md`); linearize
   on save.
2. **OOXML (DOCX/XLSX/PPTX)**: OPC zip-container rebuild — the
   `[Content_Types].xml` + `_rels/.rels` manifest is re-derived
   from the parts that survive; damaged local file headers are
   re-synced against the central directory; part streams
   re-inflated where possible.
3. **Legacy Office (DOC/XLS/PPT)**: OLE compound-file rebuild —
   FAT sector re-derivation, directory-entry re-chain from stream
   signatures.

## 3. Video Repair

**Formats**: MP4/MOV (box-grammar), AVI (regroup), MTS/M2TS (raw
stream finding + regroup), MXF (regroup).

**Algorithm (the vendor's deepest chain, source-tree decoded):**

1. Parse the MP4 box tree; validate `ftyp`/`moov`/`mdat` (the "file
   has no moov" / "is bad moov" damage classes).
2. Walk fragmented-MP4 (`moof`) fragments with pre-align indices.
3. Verify each data run against the index ("org data run is
   correct, will not fix it" — only damaged runs are rebuilt).
4. **Regroup**: extract raw streams from `mdat`, rebuild a fresh
   `moov` around them (`regroupfile.cpp` + `integrationalg.cpp`
   — the integration algorithm recombines stream fragments).
5. **Advanced mode**: transplant — a healthy sample file's box
   template reconstructs the damaged file's index ("Add Sample
   Video for Advanced Repair" — the sample-file flow already in
   the UI research).
6. Frame-bitmap enhancement for thumbnails
   (`enhancebitmap.cpp` class of work) feeds the before/after
   compare preview.

## 4. Archive Repair (stretch — after photos+docs+video)

**Formats**: ZIP, 7Z, RAR. 7z per `archive-repair--7z-engine.md`:
next-header (footer index) reconstruction via
`ReadNextHeaderPackedStreams` walks + folder/substream bounds
re-derivation; password-aware repair (header-crypto + body-CRC
candidate validation, the Dict dictionary); ZIP via the
EOCD-central-directory rescan the carver already implements
(reuse the ZIP analyser grammar).

## 5. Cross-cutting contracts (bind to the existing surface)

- **Invocation**: in-process (no per-tool exe spawning — our
  engines are Rust crates, not the vendor's out-of-proc
  `EUFileRepair` hosts). Commands: `repair_preview`,
  `repair_execute`, progress events on the existing
  `EV_RECOVER_PROGRESS`-style channel.
- **Licensing**: every repair function gates on a valid license
  (SMART remains the only free surface).
- **Recovery integration**: the `enable-file-repair` toggle
  repairs during recovery; the completion summary counts
  `repairedFiles`.
- **Safety**: repair writes ONLY to the user-selected destination
  (never in-place on the source — stricter than the vendor, which
  also works on copies); every engine gets fuzz + fixture tests
  (corrupted, truncated, partially-overwritten inputs) per §12.
- **Honesty**: `DamageReport` verdicts are exact — "repaired
  prefix, X% of stream recovered", never a silent half-file.

## 6. Delivery plan — EXECUTING (session 10)

1. ✅ `dw-repair` crate skeleton + trait + fixture harness.
2. ✅ JPEG + PNG (the marker grammar + the chunk grammar with the
   height-adjusted viewable-prefix salvage).
3. ✅ PDF xref synthesis + OOXML manifest rebuild (+ the /Root
   synthesis cascade + the per-part CRC audit).
4. ✅ MP4/MOV box repair (weighted grammar, two-scan, successor-
   anchored extents). The regroup + sample-transplant advanced mode
   remains open — the honest BeyondRepair verdict covers the class
   until it lands.
5. ✅ GIF/BMP/TIFF/WebP/SVG. The RAW embedded-JPEG path (the
   CScanJepgInfo salvage) remains open.
6. ⬜ MTS/MXF + the archive engines (7z next-header, ZIP EOCD).
7. ✅ Windows-runner end-to-end: the repair-e2e workflow — REAL
   encoders (Pillow/PDF-writer/python-docx/ffmpeg), four damage
   classes, REAL decoders as the gates (Pillow/pypdf/zipfile/
   ffprobe), 20/20 checks passing locally and on the runner.

App wiring: ✅ repair_analyze_file + repair_files (license-gated),
repair-during-recovery (the enable-file-repair toggle + real
repairedFiles counts), the by-name dispatch (destroyed headers),
the dw-cli repair junction, the fuzz suite (engines + salvage).
