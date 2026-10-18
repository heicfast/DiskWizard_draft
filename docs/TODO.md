# DiskWizard — TODO / Roadmap

## Next priority (owner-directed): the repair phase

The advanced **file repair** surfaces ship next, once the core idea
and implementation are production-ready (owner note, session 1). The
research artifacts already define them:

- [ ] **Advanced Photo Repair** (integrated, not a separate product):
      auto-restore corrupted JPEG/JPG/PNG/GIF; preview the repaired
      image before final recovery.
- [ ] **Document Repair**: extract + repair all components of
      corrupted `.doc/.docx`, `.xls/.xlsx`, `.pdf/.pptx`.
- [ ] **Video repair wizard** (sample-file advanced repair,
      "Repairing (1/2): …" item rows, before/after compare preview).
- [ ] **Repair-during-recovery counters** in the completion summary
      (the enable-file-repair toggle already persists through the
      funnel).

## Core integration (replaces both mocks)

- [ ] Swap `src-tauri/src/commands/mock` for the production engine —
      the command surface + events in `docs/ARCHITECTURE.md` are the
      frozen contract.
- [ ] Wire real device enumeration (WMI + SetupAPI), real scan
      engines (per-FS searchers + RAW carving), real `.rsf` session
      save/load, and the licensing client (Ed25519 entitlement
      verification — the DiskGenie licensing server architecture).
- [ ] Real SMART telemetry (SAT/CSMI/NVMe transports).

## UI polish backlog

- [ ] Icon-view mode for results (grid toggle exists in the toolbar
      concept; list mode ships first).
- [ ] Context menu on result rows (Recover / Preview / Open location
      / Select unrecovered / Clear selection).
- [ ] Full keyboard navigation pass (arrow keys in the file table,
      tree focus management).
- [ ] Localization framework (English-first, tr()-style extraction).
- [ ] WinPE / bootable-media surface ("Crashed PC Rescue").

## Senior-review findings for the M2 engine session (2026-10-18)

Evidence-based items from the independent senior-dev review of the
core-interface session (fixed items live in the commit history; these
three are engine-scale work):

1. **NTFS $ATTRIBUTE_LIST forking** — base records whose attributes
   overflow into extension records (heavily fragmented / many-stream
   files) currently emit NO data streams: the file vanishes from
   results. Walk ATTR_ATTRIBUTE_LIST (0x20), merge extension-record
   $DATA/$FILE_NAME into the base entry.
2. **Hole-aware extent mapping** — sparse/compressed runlists drop
   holes silently; recovery concatenates extents positionally so
   content after a gap lands at the wrong file offset. Emit extents
   with explicit file offsets (or zero-length hole markers), make
   ExtentStream zero-fill gaps (not just the tail), fail whole-runlist
   on implausible mid-list runs instead of skipping.
3. **Resident-content recovery** — sub-~700-byte NTFS files (the
   resident-marker extents) currently recover as zeros. Read the
   record at the marker offset at export time (or inline content at
   walk time).

Also queued from the review (minor): read_bytes overflow guard +
decode_runlist LCN-vs-max_clusters bound; carver take-clamp + sparse
error propagation; staging-buffer reuse per chunk; records 0-3
visibility decision; Drop-invariant comment rewording.

## Engine M3 items (session 5, 2026-10-18)

The ext/HFS+/ReFS engines landed with the evidence-complete layers;
these are the next-depth items, each with its evidence trail:

1. **ext4 journal reanimation** — the vendor's TraverseJournalDir
   path (inode 8, `.journal`): walk journal transactions for
   recently-modified/deleted state not yet reflected in the main
   trees. Evidence: `remaining-engines--ext4-hfs-refs-btrfs-zfs.md`
   (GotJournalDir/TraverseJournalDir trace strings).
2. **HFS+ journal replay** — hfspluslib journal.cpp port: the
   circular hot-file journal holds recent catalog transactions.
   Evidence: HFSPlusSearchDLL class census (CHFSPlusRawfile +
   journal source-tree strings).
3. **ReFS MSB tree enumeration** — byte-level object-tree records
   (RefsDirectoryValuesImpl / RefsFileValuesImpl) inside MSB pages.
   Current evidence is RTTI-level only (the 42K-line
   RefsFileSystemAnalyser.mo sibling has the layouts but no
   byte-level doc). Plan: format a real ReFS VHD on the Windows
   runner, reverse the record layouts against ground truth, then
   implement. Engine v1 delivers the DBR gate + page census +
   checkpoint generations (byte-exact) and carving coverage.
4. **ext zeroed-inode name↔content association** — name-only
   deletions (kernel shape B) and orphaned inodes are emitted
   separately today; single-candidate matching would join them.
