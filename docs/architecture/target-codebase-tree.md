# DiskWizard — Target Codebase Tree (scratch → final production state)

> **Phase 2 deliverable** (MASTER_PROMPT §10 PHASE 2.1). This is the
> authoritative module map: every planned module with its **governing
> research artifact** (§2.4 cross-link mandate). Supersedes the
> CORE-PLAN §2 workspace sketch (which remains valid as the session-3
> precursor). Status legend: ✅ shipped · 🔶 partial (gap noted) · ⬜ planned.

```
DiskWizard_draft/
├── src/                          # React/TS Canvas UI (cherry-blossom theme)
│   ├── shell/                    # TitleBar, Sidebar, splash, TourDriver ✅
│   ├── pages/                    # recover/ results/ tools/ cloud/ settings/ help/ ✅
│   ├── state/                    # zustand stores: view scan results devices license settings ✅
│   ├── mock/                     # TS mock core (browser-dev ONLY — never bundled) ✅
│   └── lib/                      # ipc seam, licenseKey (DB prefix), preview decoding ✅
│
├── src-tauri/                    # Cargo workspace root + the Tauri app crate
│   ├── src/
│   │   ├── commands/
│   │   │   ├── real/             # THE shipped command surface ✅
│   │   │   │   ├── devices.rs    #   enumeration, SMART, SDR, BitLocker ✅
│   │   │   │   ├── scan.rs       #   start/pause/resume/stop/progress/results ✅
│   │   │   │   ├── recover.rs    #   destinations, start_recovery, preview ✅
│   │   │   │   │                 #   + §5.1-8 source-disk exclusion ✅ (this session)
│   │   │   │   ├── session.rs    #   .rsf list/resume + dev hooks ✅
│   │   │   │   ├── license.rs    #   activate/validate/deactivate + L6 gate +
│   │   │   │   │                 #   ci-tour seed (feature-gated) ✅ (this session)
│   │   │   │   └── settings.rs   #   persisted settings ✅
│   │   │   └── mock/             # sim feature (dev/screenshot builds ONLY) ✅
│   │   └── lib.rs, main.rs       # invoke_handler + bootstrap ✅
│   │
│   ├── crates/
│   │   ├── dw-core/              # platform-independent engine (no OS deps)
│   │   │   └── src/
│   │   │       ├── blockdev.rs   # BlockDevice seam, AlignedBuf, sparse reads ✅
│   │   │       │                  #   gov: architecture/05-scan-engine §8; eprdtdrv artifact
│   │   │       ├── partition.rs  # MBR/EBR/GPT + lost sweep ✅
│   │   │       │                  #   gov: PartitionRecovery digests (5-b) + mo-module-map
│   │   │       ├── fs/
│   │   │       │   ├── mod.rs    #   create_engine dispatch + FsKind ✅
│   │   │       │   ├── ntfs/     #   MFT walk, fixups, runlists ✅ 🔶 ($ATTRIBUTE_LIST, hole-aware
│   │   │       │   │              #   extents, $Bitmap viability = M2 items; resident recovery ✅)
│   │   │       │   │              #   gov: SearchDLL/ntfs-engine--deep-dive + algorithms-v2
│   │   │       │   ├── fat/      #   FAT12/16/32 + LFN + 0xE5 ✅
│   │   │       │   │              #   gov: SearchDLL/fat-engine--deep-dive + fat-exfat-algorithms-v2
│   │   │       │   ├── exfat/    #   dentry triples + InUse mask ✅ 🔶 (chain re-derivation tests)
│   │   │       │   │              #   gov: SearchDLL/exfat-engine--deep-dive + algorithms-v2
│   │   │       │   ├── ext/      #   superblock+GDT+inode+extents+orphans ✅ 🔶 (journal reanimation)
│   │   │       │   │              #   gov: SearchDLL/ext4-engine--deep-dive + ext4-hfs-algorithms-v2
│   │   │       │   ├── hfsplus/  #   catalog B-tree + cnid + forks + overflow ✅ 🔶 (journal replay)
│   │   │       │   │              #   gov: SearchDLL/hfsplus-engine--deep-dive + algorithms-v2
│   │   │       │   └── refs/     #   DBR gate + SUPB/CHKP/MSB census 🔶 (MSB tree records = M3;
│   │   │       │                  #   honest boundary; content via carving)
│   │   │       │                  #   gov: SearchDLL/refs-engine--deep-dive + refs-btrfs-zfs-v2
│   │   │       ├── carve/        # signature registry + bounded streaming validators ✅
│   │   │       │                  #   gov: rawsearch-carving--deep-dive + registry census + zip doc
│   │   │       ├── tree.rs       # node arena, merge (structured wins), classify, grade ✅
│   │   │       │                  #   gov: architecture/05 §7 + filtering-priority config layer
│   │   │       ├── scan.rs       # quick→deep controller, phases, progress snapshot ✅
│   │   │       │                  #   gov: architecture/04 §3-§5 (FS codes, engine set)
│   │   │       ├── recover.rs    # queue, extents, DestWriter, space gates ✅
│   │   │       │                  #   gov: architecture/04 §7 (CRecoveryMgr chain)
│   │   │       ├── session.rs    # .rsf save/load (zlib-chunked records) ✅ 🔶 (record-3 context)
│   │   │       │                  #   gov: SaveScan/rsf-format--scan-session (ADR-4)
│   │   │       ├── dedup.rs      # content-key Version marking + same-region absorb ✅
│   │   │       │                  #   gov: changelog ~65% claim + FilterFolder/noise layer
│   │   │       ├── repair.rs     # RawFixer model: boot diagnosis + fix plans ✅ (SDR path)
│   │   │       │                  #   gov: PartitionRecovery/rawfixer artifacts (5-b digest)
│   │   │       └── (M3) io/      # ⬜ IOCP worker pool + adaptive QD (QA-11) — the §5.1-3/5 push
│   │   │                          #   gov: §5.1 + eprdtdrv MDL-async contract
│   │   │
│   │   ├── dw-win/               # Windows platform layer (cfg(windows))
│   │   │   └── src/
│   │   │       ├── blockdev.rs  # RawHandleDevice: positioned OVERLAPPED + NO_BUFFERING ✅
│   │   │       │                 #   gov: §5.1 items 1-2-4-9; eprdtdrv driver contract
│   │   │       ├── devices.rs   # inventory enumeration + SMART transports ✅
│   │   │       │                 #   gov: DiskHealth/diskhealth artifact (CrystalDiskInfo lineage)
│   │   │       ├── volume.rs    # device-id parsing, volume handles ✅
│   │   │       ├── smart.rs     # SAT + NVMe health-log ✅ 🔶 (threshold table breadth, QA-10)
│   │   │       ├── bitlocker.rs # FVE detection + unlock + BEK ✅
│   │   │       │                 #   gov: PartitionRecovery/bitlockerlib artifact
│   │   │       ├── repair.rs    # write-enabled handle + SDR apply/rollback ✅
│   │   │       └── license_plat.rs # MachineGuid/CPUID/DPAPI providers ✅
│   │   │
│   │   └── dw-license/           # licensing client (platform-independent)
│   │       └── src/
│   │           ├── config.rs    # ★ §8 ONE config module (this session) ✅
│   │           │                 #   gov: MASTER_PROMPT §8 + donor license.rs verbatim
│   │           ├── protocol.rs  # activate/validate/deactivate + HMAC ✅
│   │           ├── token.rs     # Ed25519 verify (iss/aud/kid checks) ✅
│   │           ├── posture.rs   # unlicensed/pro/grace/degraded ✅
│   │           ├── http.rs      # reqwest transport (UA pinned) ✅
│   │           ├── hwid.rs, store.rs # fingerprint + persistence seams ✅
│   │           └── (⬜) live-e2e tests vs the real worker (QA-16, feature-gated)
│   │
│   └── .github/workflows/        # ci.yml, windows-integration.yml (VHD truth),
│                                 # ui-screenshots.yml (sim tour), msix-build.yml,
│                                 # real-core-tour.yml (QA-1, this session) ✅
│
├── TODO/phase-2-advanced-repair.md  # §6.4 full spec (photo/doc/video) ✅
├── todo-uiux/                      # UI-agent task docs ✅ (+ register-fed updates)
├── docs/
│   ├── maps/                       # Phase 0: doc-tree, synthesis, digests ✅
│   ├── audit/                      # Phase 1: consistency + ui-gap registers ✅
│   ├── architecture/               # this tree + phase-build-plan (Phase 2) ✅
│   ├── standards/SOP.md            # Phase 2 ✅
│   ├── reviews/                    # multi-POV passes ✅ (one per phase boundary)
│   ├── benchmarks/                 # ⬜ io-backend numbers (M3 with the IOCP push)
│   ├── QA-REGISTER.md, CORE-PLAN.md, ARCHITECTURE.md, TODO.md, dependencies.md ✅
│   └── handoff/session-report.md   # per-session close-out ✅
└── worklog/open-questions.md       # ambiguity register ✅ (OQ-1..7)
```

## Module boundary rules (enforced by review)

1. `dw-core` never imports Win32; OS specifics live behind the
   `BlockDevice`/`StateStore`/`HwIdProvider` seams (dw-win injects).
2. The app crate owns IPC + threading only — no engine logic.
3. `dw-license` config values surface ONLY through `config.rs`.
4. The sim/mock surfaces mirror the real command contract 1:1
   (wire-shape parity, e.g. `deviceId` in results snapshots).
5. Every engine module cites its governing research artifact in the
   module doc-comment (already true across `fs/*`).
