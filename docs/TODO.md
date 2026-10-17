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
