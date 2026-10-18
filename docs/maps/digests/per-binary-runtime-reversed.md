# Digest — per-binary census, runtime-families, reversed tooling (Task 5-a)

> Corpus: `easeus-drw-research` repo — `re-workspace/artifacts/per-binary/` (114 files),
> `re-workspace/artifacts/runtime-families/` (10 files), `re-workspace/reversed/` (13 REPORT.md files).
> All entries below are facts read from those files verbatim; no behavior was invented.
> Many `.mo` module stubs follow a template: identity table + Ghidra census + the same
> `IObject`/`GetModule` export boilerplate ("Component. Analyzed; see reversed evidence.") —
> for those the digest records the concrete identity/census numbers and any distinctive exports.
> DiskWizard (DW) context: Rust/Tauri 2 clean-room rebuild; mandate = no AI features,
> no kernel drivers, user-mode recovery.

## Part 1 — per-binary census (re-workspace/artifacts/per-binary/)

### re-workspace/artifacts/per-binary/AliyunWrap.md
- **Role**: Aliyun OSS cloud-upload wrapper (crash dumps etc.).
- **Key facts**:
  - `og-codebase/AliyunWrap.dll`, 624,144 bytes, x64, 281 imports / 21 DLLs, 18 exports, 8 RTTI classes (std/CRT only).
  - 2,045 functions / 79,448 decompiled lines (Ghidra batch `ghidra-misc`).
  - Export surface: `SendLogToCloud` (2 overloads, mangled), `AliyunOpenSession`, `AliyunCloseSession`, `AliyunSendInfo`, `AliyunAddParamToSessionA/W`, `AliyunGetKeyValue`/`AliyunSetKeyValue`, `AliyunGetUserUid`, `AliyunEnableUserInfoCollect`/`AliyunIsEnableUserInfoCollect`, `AliyunInstallConfigFilePath`, `AliyunInstallInitUid`, `AliyunEndCache`, `AliyunStopProcess`, `AliyunUninstallStart`/`AliyunUninstallEnd`.
- **DW relevance**: telemetry-only; DW drops the Aliyun crash-upload chain entirely (mandate: no equivalent cloud telemetry required).

### re-workspace/artifacts/per-binary/AliyunWrapExe.md
- **Role**: Standalone Aliyun OSS CLI upload exe.
- **Key facts**:
  - `og-codebase/AliyunWrapExe.exe`, 224,272 bytes, x64, 99 imports / 7 DLLs, 0 exports, 9 RTTI classes.
  - 919 functions / 29,519 lines (batch `ghidra-misc`).
  - Deepening batch 3 adds: uses `AliyunConfig.ini` + `ACCESSKEYSECRET`, AES encrypt/decrypt of payloads, `AllowSendUrl` whitelist gating; spawned by DRWUI at startup (Wine ground-truth census).
- **DW relevance**: telemetry host, out-of-process — excluded from DW.

### re-workspace/artifacts/per-binary/AutoUpdate.md
- **Role**: PartitionRecovery auto-update implementation.
- **Key facts**:
  - `og-codebase/PartitionRecovery/AutoUpdate.dll`, 158,440 bytes, x64, 89 imports / 3 DLLs, 4 exports (`GetAutoUpdateInstance` + CLogInBootICE/CLogWriter assignment operators), 0 RTTI.
  - 506 functions / 22,678 lines (batch `ghidra-partition`).
  - Deepening batch 3: update probe with WinSxS api-ms-* forwarder resilience (resolves `api-ms-win-core-synch-l1-2-0.dll`, `ext-ms-*`, `mscoree.dll`).
- **DW relevance**: self-updater is a packaging concern, not core recovery; DW can ignore or defer.

### re-workspace/artifacts/per-binary/BitLockerLib.md
- **Role**: BitLocker library — XTS-AES + FVE + manage-bde orchestration.
- **Key facts**:
  - `og-codebase/PartitionRecovery/BitLockerLib.dll`, 722,152 bytes, x64, 90 imports / 5 DLLs, 41 exports, 0 RTTI.
  - 2,320 functions / 83,772 lines (batch `ghidra-partition`).
  - `BitLockerManager` class export surface: `AesDecryptSectors`/`AesEncryptSectors`, `AdjustInformationOffset`, `BackupBootSectors`, `BuildMetadataEOWInformation`/`BuildMetadataInformation`, `CancelEncryption`, `Convert2BitLocker`/`Convert2BitLocker2`/`Convert2BitLockerAndPause`/`Convert2Normal`, `CreateBitLockerFveControl`, `Decryption`/`Encryption` (progress-callback signatures), `GenerateKeys`, `GetAutoUnlockKey`, `GetBackupBootOffset`, `DestroyBitLockerMgr`, plus "... 21 more".
- **DW relevance**: high-value spec for DW BitLocker support — but note the product pairs it with a Windows service (DRWService) for FVE auto-unlock keys; DW must stay user-mode.

### re-workspace/artifacts/per-binary/Boot.md
- **Role**: Boot sector handling module.
- **Key facts**:
  - `og-codebase/PartitionRecovery/Boot.dll`, 794,344 bytes, x64, 206 imports / 13 DLLs, 11 exports, 0 RTTI.
  - 843 functions / 33,066 lines (batch `ghidra-partition`).
  - Exports are the `IObject` COM-style interface set (`AddRef`/`Release`/`GetInterfaceEx`) plus `TBCanUnloadNow` / `TBCreateObject` — a TB (tool-box/plugin) module pattern.
- **DW relevance**: boot-sector parsing is core recovery input; the TB plugin pattern is a spec source for DW's engine abstraction.

### re-workspace/artifacts/per-binary/BootDriver.md
- **Role**: Per identity table, "Driver install helpers for boot components"; per deepening-batch3, actually the BOOT MEDIA BUILDER (user-mode MOM module, class `CBootDriver`) — the "Create Recovery USB/CD" feature.
- **Key facts**:
  - `og-codebase/PartitionRecovery/BootDriver.dll`, 133,352 bytes, x64, 89 imports / 11 DLLs, 8 exports: `BootDriverExcute`, `BootDriverGetProgressInfo`, `CheckFormat`, `DeleteGetBootDriverImp`, `GetBootDriverImp`, `SetBootDriverCallBack`, `TBCanUnloadNow`, `TBCreateObject`.
  - 457 functions / 10,723 lines (batch `ghidra-partition`).
  - Deepening-batch3 evidence: `CBootDriver BootType:%d, nDriver:%d`, `CBootDriver Execute Start/End! Err = 0x%x`, burn path `CBurnBootCD CallBack Result/Type` (IMAPI2 chain from Burn), config from `../res/urls.ini` + `..\res\UIConfig.ini`, handles `.iso`/`.ini` assets via `CopyFileW`, checkpoints `"BootDriver: Error = 0x%x, Line = %d"`.
- **DW relevance**: misleading name — it is user-mode WinPE media building; a legitimate optional DW feature (no kernel driver involved).

### re-workspace/artifacts/per-binary/BtrfsSearchDLL.md
- **Role**: Btrfs file-system search engine (SearchDLL family).
- **Key facts**:
  - `og-codebase/BtrfsSearchDLL.dll`, 150,760 bytes, x64, 70 imports / 8 DLLs, 2 exports: `EUCreateBtrfsComplateSearcher` (sic) and `EUCreateBtrfsQuickSearcher`.
  - 452 functions / 18,626 lines (batch `ghidra-searchdll`); capa (14 caps): IOCTL driver interaction, XOR encode, ZLIB inflate/deflate, RC4 PRGA, adler32, CRC32, PE-export resolution, PE-section enumeration.
  - 17 RTTI classes: `CBtrfsFileProerties` (sic), `CBtrfsQuickSearch`, `CBtrfsNoResidentDataRunList`, `CFileNode`, `CFileSystemNode`, `CNoneResidentNTFSDataRunList`, `CResidentNTFSDataRunList`, `CSearchedBtrfsPartitionInfo`, `CSearchedPartitionInfo`, `IDataRunList`, `IFileProperties`, `IFileSystemSearchController`, `IQuickFileSystemSearchController`.
- **DW relevance**: direct spec for a DW Btrfs engine (quick + complete searcher, DataRunList abstraction); RC4+ZLIB capa notes hint at Btrfs zlib datasets/encrypted pools.

### re-workspace/artifacts/per-binary/Burn.md
- **Role**: ISO/media burning.
- **Key facts**:
  - `og-codebase/PartitionRecovery/Burn.dll`, 165,608 bytes, x64, 92 imports / 8 DLLs, exports only `TBCanUnloadNow`/`TBCreateObject`, 0 RTTI.
  - 1,029 functions / 9,707 lines (batch `ghidra-partition`).
  - Pairs with BootDriver's `CBurnBootCD` IMAPI2 chain (deepening-batch3).
- **DW relevance**: recovery-media burning, optional DW feature; user-mode (IMAPI2).

### re-workspace/artifacts/per-binary/CallbackOperator.md
- **Role**: MOM callback/progress operator module.
- **Key facts**:
  - `og-codebase/PartitionRecovery/CallbackOperator.mo`, 41,704 bytes, x64, 43 imports / 7 DLLs, 29 exports.
  - 110 functions / 2,831 lines (batch `ghidra-mo`).
  - `CCallBackOperate` class surface: constructor taking `P6AKPEAUOC_PARAM@@PEAX@Z` callback, `CallFunction`, `DoMessageLoop`, `EndCurChildOperation`, `EndCurOperation`, `EndFileSystemOperation`, `IncrementCurChildOperation`, "... 9 more".
- **DW relevance**: the progress-cancellation/percent plumbing every long scan needs; a clean Rust trait/port pattern reference.

### re-workspace/artifacts/per-binary/CheckVersion.md
- **Role**: MOM module (version check) — template stub.
- **Key facts**:
  - `og-codebase/PartitionRecovery/CheckVersion.mo`, 135,912 bytes, x64, 127 imports / 10 DLLs, 10 exports (IObject set + `GetModule`).
  - 539 functions / 16,170 lines (batch `ghidra-mo`).
- **DW relevance**: minimal — licensing/version gates live elsewhere (EuRepairHost, DRWUI activation dialogs).

### re-workspace/artifacts/per-binary/CloneModule.md
- **Role**: Disk clone + ISO9660 + WinPE/BCD builder.
- **Key facts**:
  - `og-codebase/PartitionRecovery/CloneModule.dll`, 2,260,712 bytes, x64, 619 imports / 28 DLLs, 123 exports, 0 RTTI.
  - 15,273 functions / 169,891 lines (batch `ghidra-partition`).
  - Exports include classes `CEUCloneMgr`, `CEUPreOsMgr`, `CPartitionDevice`, interfaces `IDiskDevice`, `IPartitionDevice`, `IStorageDevice`, `IObject`.
- **DW relevance**: disk-clone feature spec; note PreOS/WinPE paths — DW clone must be user-mode only (no boot-driver trickery).

### re-workspace/artifacts/per-binary/Common.md
- **Role**: MOM module — template stub.
- **Key facts**: `PartitionRecovery/Common.mo`, 129,768 bytes, x64, 125 imports / 7 DLLs, 10 exports (IObject + GetModule); 617 functions / 13,642 lines (batch `ghidra-mo`).
- **DW relevance**: negligible beyond the MOM module map.

### re-workspace/artifacts/per-binary/Config.md
- **Role**: MOM module — template stub.
- **Key facts**: `PartitionRecovery/Config.mo`, 127,720 bytes, x64, 123 imports / 7 DLLs, 10 exports (IObject + GetModule); 605 functions / 13,311 lines (batch `ghidra-mo`).
- **DW relevance**: negligible; configuration is config-file territory (covered by other artifacts).

### re-workspace/artifacts/per-binary/DRW.md
- **Role**: Supervisor launcher — DRWLoader watchdog.
- **Key facts**:
  - `og-codebase/DRW.exe`, 127,208 bytes, x64, 130 imports / 14 DLLs, 0 exports, 11 RTTI classes (`CFileMappingLog`, `IEsLog`, ATL classes).
  - 174 functions / 5,661 lines (batch `ghidra-core`); capa (19): create process, mutex create/check, registry open/query, read .ini, extract resource via kernel32, compare SIDs.
  - Role line: "spawns DRWUI, crash events, restart, DRWReport handoff"; deepening-batch3 cross-link: drains the PubLog 2 MB shared log buffer every 5 s.
- **DW relevance**: supervisory watchdog pattern (spawn/restart UI, crash handoff) — optional DW robustness feature; DW has no AI features to guard.

### re-workspace/artifacts/per-binary/DRWReport.md
- **Role**: Crash reporter.
- **Key facts**:
  - `og-codebase/DRWReport.exe`, 4,805,352 bytes, x64, 812 imports / 18 DLLs, 0 exports, 614 RTTI classes (MFC heavy).
  - 19,008 functions / 612,091 lines (batch `ghidra-core`) — the 2nd-largest core binary.
  - Role: minidump collection, Elapsed/Error/Result JSON, upload via `send_dmp_with_aliyun` path.
- **DW relevance**: crash telemetry chain — DW replaces with its own minimal error reporting (no Aliyun).

### re-workspace/artifacts/per-binary/DRWService.md
- **Role**: BitLocker key service — "BitlockerReadFVEService".
- **Key facts**:
  - `og-codebase/DRWService.exe`, 40,168 bytes, x64, 93 imports / 10 DLLs, 0 exports, 4 RTTI classes.
  - 118 functions / 3,031 lines (batch `ghidra-core`); capa (17): create/start/modify service, persist via Windows service, registry ops.
  - FVE auto-unlock keys served over `\\.\Pipe\DRWBitlocker`.
- **DW relevance**: service-based FVE key retrieval is a Windows-only interop pattern; DW can adopt the pipe/service contract or use user-mode APIs — no kernel driver needed.

### re-workspace/artifacts/per-binary/DRWUI.md
- **Role**: Main application — all UI, orchestration, recovery manager.
- **Key facts**:
  - `og-codebase/DRWUI.exe`, 30,571,752 bytes, x64, 3,610 imports / 49 DLLs, 9 exports (IObject set), 1,073 RTTI classes.
  - 119,182 functions / 2,381,187 decompiled lines (dedicated batch `ghidra-drwui`) — largest corpus item.
  - RTTI shows the UI surface: `AIAssistantTipDialog`, `AIDeduplicationTipMenu` (AI features DW must NOT replicate), `AES_CBC256`, `ActivateResultDialog`/`ActivateViewProxy`/`ApowerActivateDialog` (licensing UI), `ArchivePreview*`/`ArchiveRepairThread`, `AutoRecoverDataSource`, `AvoidAmendDataRunLock`, `BadSectorTipsDlg`, `BitLockerMSGuideDlg`, `AVPreviewStub`, `AnalysisGridWidget`, `AdToolPopWindow`, `BaseWorker`, `BasicTitleBar`.
- **DW relevance**: the 1,073-class map is the product feature inventory; DW deliberately excludes the AI/activation-ad classes; Tauri 2 replaces the Qt widgets stack.

### re-workspace/artifacts/per-binary/Device.md
- **Role**: MOM device module (storage device abstraction) — stub with export surface.
- **Key facts**: `PartitionRecovery/Device.mo`, 740,072 bytes, x64, 285 imports / 14 DLLs, 35 exports; 3,474 functions / 68,987 lines (batch `ghidra-mo`); exports include `IStorageDevice` (dual vtables `IBlockDevice`+`IObject`), `CNTFSLib` assignment ops, `CCallBackOperate`/`CLogWriter` ops, "... 15 more".
- **DW relevance**: the IStorageDevice/IBlockDevice abstraction is exactly DW's `BlockDevice` trait to design.

### re-workspace/artifacts/per-binary/DeviceAdapter.md
- **Role**: MOM device adapter module.
- **Key facts**: `PartitionRecovery/DeviceAdapter.mo`, 37,096 bytes, x64, 35 imports / 5 DLLs, 76 exports; 126 functions / 2,599 lines (batch `ghidra-mo`); exports include `CDiskDevice`, `CPartitionDevice`, `IDiskDevice`, `IPartitionDevice`, `IStorageDevice` ctors/dtors/assigns, "... 56 more".
- **DW relevance**: thin adapter layer between MOM and device model — confirms the layered design DW should mirror.

### re-workspace/artifacts/per-binary/DeviceManager.md
- **Role**: MOM device manager module.
- **Key facts**: `PartitionRecovery/DeviceManager.mo`, 777,448 bytes, x64, 268 imports / 21 DLLs, 47 exports; 3,428 functions / 70,679 lines (batch `ghidra-mo`); exports include device class ctors/assigns + `CNTFSLib`/`CLogWriter` boilerplate, "... 27 more".
- **DW relevance**: device enumeration/management core; direct architectural reference.

### re-workspace/artifacts/per-binary/DiskHealth.md
- **Role**: Qt/QML disk health monitor — separate product.
- **Key facts**: `og-codebase/diskhealth/DiskHealth.exe`, 1,781,480 bytes, **x86** (machine 0x14c), 529 imports / 27 DLLs, 0 exports, 0 RTTI; 4,636 functions / 93,316 lines (batch `ghidra-wave2`); sole consumer of the Qt Quick/QML runtime family.
- **DW relevance**: out of scope — companion product, not the recovery app; proves the QML stack is separable.

### re-workspace/artifacts/per-binary/EFIBoot.md
- **Role**: MOM EFI boot module — template stub.
- **Key facts**: `PartitionRecovery/EFIBoot.mo`, 58,600 bytes, x64, 64 imports / 7 DLLs, 10 exports (IObject + GetModule); 189 functions / 6,018 lines (batch `ghidra-mo`).
- **DW relevance**: EFI boot-partition handling name only; details live in deeper artifacts.

### re-workspace/artifacts/per-binary/EUAdobeRepair.md
- **Role**: Adobe format repair + cloud client.
- **Key facts**:
  - `og-codebase/EUAdobeRepair.exe`, 434,408 bytes, **x86**, CONSOLE subsystem, 264 imports / 13 DLLs, 0 exports, 98 RTTI classes.
  - 2,209 functions / 81,535 lines (batch `ghidra-repair`).
  - RTTI: `CPsdbInfo`, `IAdobeInfo`, easylogging++-style `Logger`/`LogBuilder`/`TypedConfigurations`/`VRegistry`, concurrency `_CancellationTokenRegistration`.
- **DW relevance**: repair-suite reference (PSD repair + cloud relay); DW repair scope TBD, cloud relay path excluded.

### re-workspace/artifacts/per-binary/EUArchiveRepair.md
- **Role**: Archive repair — 7z next-header recovery + archive7z.
- **Key facts**:
  - `og-codebase/EUArchiveRepair.exe`, 602,344 bytes, x86, CONSOLE, 290 imports / 14 DLLs, 0 exports, 104 RTTI classes.
  - 3,338 functions / 117,834 lines (batch `ghidra-repair`).
  - RTTI: `CArchive7zInfo`, `CArchiveRar4Info`, `CArchiveRar5Info`, `CArchiveRarInfo`, `CArchiveZipInfo`, `IArchiveInfo`, `BitException` (bit7z lineage).
- **DW relevance**: archive repair spec (7z/RAR4/RAR5/ZIP); DW repair scope decision item.

### re-workspace/artifacts/per-binary/EUDCPEPM.md
- **Role**: Kernel disk-copy hook — "epmdiskcopydrv", disk-class UpperFilter (BOOT_START) in WinPE.
- **Key facts**:
  - `og-codebase/BUILDPE/x64/Windows/system32/drivers/EUDCPEPM.sys`, 72,144 bytes, x64 NATIVE driver, 61 imports / 1 DLL, 0 exports.
  - 176 functions / 10,057 lines (batch `ghidra-drivers`); capa (11): IRP completion, thread create/terminate, device object, PDB path.
  - Deep-dive decoded dispatch table: hooks ALL 28 `MajorFunction[]` IRPs via `FUN_000120c4`; special-cases `IRP_MJ_CREATE` (+0x70), `IRP_MJ_PNP` (+0x148), `IRP_MJ_POWER` (+0x120); `DriverExtension->AddDevice = FUN_00012014`; `DriverUnload = FUN_00011fb4`.
  - Also attaches to `\Driver\VolSnap` (VSS volume-snapshot driver) to coordinate volume locks with shadow copies during disk copy.
  - IOCTL path subtracts registry-configured `IoctlBase` (read at DriverEntry via ZwQueryValueKey on `…\Services\…\Parameters`; regdrv.reg sets `DeviceName=epmdiskcopydrv`, `IoctlBase=0x10000`) and dispatches custom range **0x800C2B00+** (volume lock/dismount-class); trace `"MainDispatch %d code=0x%X"`.
  - Exact trace strings: `"DrvHKSetNewHookFuns type=%d pfn=0x%x"`, `"DrvHKSelfAddDevice eHKNotHOOK return"`, `"DrvHookNextAddDevice driver=0x%x"`, `"InitSectorSize m_pTargetDeviceObject=%I64u pDisk=%I64u"`.
- **DW relevance**: EXCLUDED by DW mandate (kernel driver). Its purpose — exclusive-ish volume access during clone — must be achieved in DW via user-mode means (file-lock/volume-dismount APIs or decline in-use volumes).

### re-workspace/artifacts/per-binary/EUEDKEPM.md
- **Role**: Kernel hook — EUDCPEPM twin (drivers/ drop).
- **Key facts**: `BUILDPE/.../drivers/EUEDKEPM.sys`, 19,408 bytes, x64 NATIVE, 35 imports / 1 DLL, 0 exports; 35 functions / 1,799 lines (batch `ghidra-drivers`); capa (8): IRP completion, registry, device object, PDB path.
- **DW relevance**: excluded (kernel driver), same as EUDCPEPM.

### re-workspace/artifacts/per-binary/EUHwpFilePathCheckerModule.md
- **Role**: HWP file validation helper.
- **Key facts**: `og-codebase/EUHwpFilePathCheckerModule.dll`, 155,880 bytes, x86, 96 imports / 2 DLLs, 1 export: `IsAccessiblePath`; 668 functions / 29,099 lines (batch `ghidra-repair`); 6 RTTI classes (CRT only).
- **DW relevance**: trivial helper pattern (path accessibility check) for out-of-proc viewers.

### re-workspace/artifacts/per-binary/EUHwpViewer.md
- **Role**: Korean Hangul (HWP) word-processor viewer, out-of-proc preview.
- **Key facts**: `og-codebase/EUHwpViewer.exe`, 422,120 bytes, x86 GUI, 336 imports / 12 DLLs, 0 exports, 123 RTTI (MFC: `CEUHwpViewerApp`, `CEUHwpViewerDlg`, ...); 2,129 functions / 72,182 lines (batch `ghidra-misc`).
- **DW relevance**: preview-process isolation pattern; DW uses Tauri-side preview instead.

### re-workspace/artifacts/per-binary/EUImageEnhance.md
- **Role**: Image enhance — OpenCV 2.4 static; enhances repaired photos.
- **Key facts**:
  - `og-codebase/EUImageEnhance.dll`, 3,056,872 bytes, x86, 142 imports / 3 DLLs, 4 exports: `BmpBlur`, `EUCalcDamageProportion`, `EUbeSimilarPictures`, `EnhanceMemData`; 293 RTTI classes (OpenEXR `B44Compressor`, `Imf`-style `ArgExc`/`BaseExc`, codec `BmpDecoder`/`BmpEncoder`).
  - 9,841 functions / 404,642 lines (batch `ghidra-repair`).
- **DW relevance**: post-repair photo enhancement; `EUCalcDamageProportion`/`EUbeSimilarPictures` are similarity-scoring concepts DW could re-derive (deterministic, not AI).

### re-workspace/artifacts/per-binary/EUImgRepair.md
- **Role**: Image repair CLI — "Fixo image repair entry".
- **Key facts**: `og-codebase/EUImgRepair.exe`, 203,496 bytes, x86 CONSOLE, 130 imports / 14 DLLs, 0 exports; 564 functions / 23,519 lines (batch `ghidra-repair`); 8 RTTI classes: `CGIFAnalyzer`, `CJPGAnalyzer`, `CRAWAnalyzer`, `IImgAnalyzer` + CRT.
- **DW relevance**: repair-chain dispatcher shape (analyzer-per-format interface).

### re-workspace/artifacts/per-binary/EUImgRepairFile.md
- **Role**: Image repair engine — "37 source files; 21 RAW formats + BMP/GIF/JPEG/JXL/PNG/SVG/WebP".
- **Key facts**:
  - `og-codebase/EUFileRepair/EUImgRepairFile.exe`, 5,226,728 bytes, x86 CONSOLE, 428 imports / 18 DLLs, 580 exports, 0 RTTI.
  - 20,627 functions / 829,490 lines (batch `ghidra-repair`).
  - Export surface is LibRaw's C++ API: `LibRaw`, `LibRaw_abstract_datastream`, `LibRaw_bigfile_buffered_datastream`, `LibRaw_bigfile_datastream`, `LibRaw_buffer_datastream`, `LibRaw_windows_datastream`, `libraw_memmgr`, "... 560 more".
- **DW relevance**: image repair engine inventory (LibRaw = 21 RAW formats); Rust equivalents exist (rawloader etc.) for a DW repair module.

### re-workspace/artifacts/per-binary/EUOfficeViewer.md
- **Role**: Office preview host (MFC) — template stub with census.
- **Key facts**: `og-codebase/EUOfficeViewer.exe`, 3,088,104 bytes, x64 GUI, 643 imports / 18 DLLs, 0 exports, 412 RTTI (MFC panes/docking: `CAutoHideDockSite`, `CBaseTabbedPane`...); 9,855 functions / 372,749 lines (batch `ghidra-misc`).
- **DW relevance**: preview host, replaced by Tauri UI in DW.

### re-workspace/artifacts/per-binary/EURawImg.md
- **Role**: RAW container ops tool.
- **Key facts**: `og-codebase/EURawImg.exe`, 736,488 bytes, x64 CONSOLE, 156 imports / 15 DLLs, 467 exports, 12 RTTI (LibRaw class family + `enum LibRaw_exceptions`); 784 functions / 82,155 lines (batch `ghidra-repair`).
- **DW relevance**: RAW container handling reference (LibRaw surface).

### re-workspace/artifacts/per-binary/EUUnZip.md
- **Role**: Repair-tolerant unzip (MFC) — per deepening-batch3, used to VERIFY repaired archives (repair → extract → user confirms).
- **Key facts**:
  - `og-codebase/EUUnZip.exe`, 255,208 bytes, x86 CONSOLE, 251 imports / 8 DLLs, 0 exports, 58 RTTI (MFC 9.0 / VS2008).
  - 1,325 functions / 48,871 lines (batch `ghidra-repair`).
  - Batch-3 evidence: `AfxWnd90su`, `AfxFrameOrView90su`, `AfxMDIFrame90su` class registration; "Fatal Error: MFC initialization failed"; activation-context aware (CreateActCtxW); multi-monitor aware (EnumDisplayDevicesW/EnumDisplayMonitors).
- **DW relevance**: verification-by-extraction pattern for archive repair results.

### re-workspace/artifacts/per-binary/EUVideoRepair.md
- **Role**: Video repair engine — "weighted atom grammar + regroup".
- **Key facts**:
  - `og-codebase/EUVideoRepair.dll`, 948,456 bytes, x64 GUI, 201 imports / 14 DLLs, 1 export: `GetIMovMp4Repair`; 64 RTTI classes.
  - 7,154 functions / 164,316 lines (batch `ghidra-repair`).
  - RTTI atom grammar: `Box`, `BoxContainer`, `BoxList`, `Co64Box`, `ContainerBox`, `FtypBox`, `HdlrBox`, `MdatBox`, `PaddingBox`, `SampleEntryBox`, `StcoBox`, `StscBox`, `StsdBox`, `StszBox`, `TkhdBox`, `UuidBox`, `MovMp4Repair`, `QTFeatureAnalyse`, `MJpegCodec`, `IMediaDeviceIdentify`, `IUserInfoMediaDeviceIdentify`, `ChangeDataRunListHelper`, `IDataRunList`.
- **DW relevance**: MP4/MOV repair spec (ISO box grammar + sample tables); a DW video-repair module would mirror the box-walker classes.

### re-workspace/artifacts/per-binary/EUinApp.md
- **Role**: In-app purchase — store/licensing bridge.
- **Key facts**: `og-codebase/EUinApp.exe`, 37,608 bytes, x86 CONSOLE, 86 imports / 5 DLLs, 0 exports, 6 RTTI (CRT only); 209 functions / 3,898 lines (batch `ghidra-misc`).
- **DW relevance**: one of the two licensing bridge binaries; DW licensing design references it (user-mode, no service).

### re-workspace/artifacts/per-binary/EXT4SearchDLL.md
- **Role**: EXT4 file-system search engine (SearchDLL family).
- **Key facts**:
  - `og-codebase/EXT4SearchDLL.dll`, 129,256 bytes, x64, 84 imports / 9 DLLs, 2 exports: `EUCreateEXT4QuickSearcher`, `EUCreateEXT4Searcher`.
  - 475 functions / 18,071 lines (batch `ghidra-searchdll`); capa (14): IOCTL, RWX alloc + indirect call ("JIT-style"), PE-export resolution.
  - 21 RTTI classes: `CEXT4CatchBufferDeviceProxy`, `CEXT4CompleteSearch`, `CEXT4FileProperties`, `CEXT4QuickSearch`, `CExt4DataRunList`, `EXT4CatchBufferObj`, `Ext4Volume`, `ICompleteFileSystemSearchController`, plus shared `CFileNode`/`CFileSystemNode`/`IStorageDevice`/NTFS DataRunList classes.
- **DW relevance**: direct spec for DW ext4 engine; note capa flags RWX+indirect-call — DW Rust build must avoid such patterns.

### re-workspace/artifacts/per-binary/EaseUS_MediaGraph.md
- **Role**: Media pipeline for viewer/repair preview.
- **Key facts**: `og-codebase/VideoViewer/EaseUS_MediaGraph.dll`, 99,560 bytes, x86, 167 imports / 20 DLLs, 3 exports: `CreateMediaGraph`, `GatherPreviewBmpData`, `GetPreviewDataWithFile`; 572 functions / 12,228 lines (batch `ghidra-wave2`).
- **DW relevance**: preview data plumbing (BMP gathering) — DW thumbnail/preview contract reference.

### re-workspace/artifacts/per-binary/EaseUS_VideoViewer.md
- **Role**: ffmpeg-based media player for preview (out of core scope).
- **Key facts**: `og-codebase/VideoViewer/EaseUS_VideoViewer.exe`, 121,064 bytes, x86, 194 imports / 16 DLLs, 0 exports, 0 RTTI; 685 functions / 15,269 lines (batch `ghidra-wave2`).
- **DW relevance**: preview player — replaced by Tauri/HTML5 video in DW.

### re-workspace/artifacts/per-binary/Error.mo (Error.md)
- **Role**: MOM error module — template stub.
- **Key facts**: `PartitionRecovery/Error.mo`, 59,112 bytes, x64, 57 imports / 5 DLLs, 10 exports (IObject + GetModule); 282 functions / 5,615 lines (batch `ghidra-mo`).
- **DW relevance**: error-code plumbing module name only.

### re-workspace/artifacts/per-binary/EuDownloader.md
- **Role**: Component/file downloader — "PE tools, agent zips, holiday res".
- **Key facts**:
  - `og-codebase/EuDownloader.exe`, 1,381,608 bytes, x86 CONSOLE, 142 imports / 4 DLLs, 62 exports, 26 RTTI (`CEudDownload`, `CEudRunable`, `CEudThread`, `EudData2ParentProcess`).
  - 3,646 functions / 181,150 lines (batch `ghidra-misc`).
  - Exports embed a full libcurl easy API (`curl_easy_*`, `curl_formadd`, `curl_getdate`, ...).
- **DW relevance**: runtime download of components (NAS agent etc.) — DW can statically link or avoid.

### re-workspace/artifacts/per-binary/EuICloudRecovery.md
- **Role**: iCloud client — "esicloud namespace: full iCloud (idmsa/setup.ws/PCS)".
- **Key facts**:
  - `og-codebase/EuICloudRecovery.dll`, 7,187,688 bytes, x64 CONSOLE, 418 imports / 21 DLLs, **7,064 exports**, 81 RTTI.
  - 10,964 functions / 682,377 lines (batch `ghidra-misc`).
  - RTTI: `ICloudAuthenticator`, `ICloudRecovery`, `IESIcloud`, `IcloudContacts`, `IcloudDocuments`, `IcloudDrive` (+`ICloudDriveUploadTask`), `IcloudHealth`, `IcloudHomeKitSecureVideo`, `IcloudKeychain`, `IcloudMail`, `IcloudMessages`, `IcloudNotes`, `IcloudPhotos` (+`ICloudPhotosZone`), `IcloudReminders`, `IcloudSafari`; exports reference `eslog::Logger`, nlohmann `json_abi_v3_12_0`.
- **DW relevance**: cloud-recovery feature inventory (13 iCloud data classes); a DW cloud module would be a scope decision, pattern = full-client-in-one-DLL.

### re-workspace/artifacts/per-binary/EuMrx.md
- **Role**: Kernel NAS redirector — self-contained RDBSS mini-redirector.
- **Key facts**:
  - `og-codebase/EuShare/EuMrx.sys`, 215,704 bytes, x64 NATIVE driver, 184 imports / 2 DLLs, 0 exports.
  - 405 functions / 23,980 lines (batch `ghidra-drivers`); capa (23): IRP completion, threads, registry, **PEB access / check for PEB BeingDebugged flag** (anti-debug), file attributes, open process.
- **DW relevance**: EXCLUDED by DW mandate (kernel driver) — NAS recovery via user-mode (libssh2 agent transport) only, or dropped.

### re-workspace/artifacts/per-binary/EuOfficeRepairFile.md
- **Role**: Office repair engine — "HWPX + legacy HWP repair (42,215 fns - biggest repair engine)".
- **Key facts**:
  - `og-codebase/EUFileRepair/EuOfficeRepairFile.exe`, 10,537,704 bytes, x86 CONSOLE, 372 imports / 16 DLLs, 1 export: `_libiconv_version`, 0 RTTI.
  - 42,215 functions / 1,641,232 lines (batch `ghidra-repair`) — largest repair binary.
- **DW relevance**: Korean-office-format repair; largest single repair workload — a DW scope decision.

### re-workspace/artifacts/per-binary/EuOfficeRepairWin32.md
- **Role**: Office repair host — MFC host for office repair (Azure DevOps build).
- **Key facts**: `og-codebase/EuOfficeRepairWin32.exe`, 2,739,432 bytes, x64 GUI, 586 imports / 15 DLLs, 0 exports, 352 RTTI (MFC panes); 9,131 functions / 355,806 lines (batch `ghidra-repair`).
- **DW relevance**: repair host UI shell — DW equivalent would be a Tauri window + engine process.

### re-workspace/artifacts/per-binary/EuOfficeTools.md
- **Role**: Office file utilities — OOXML + BIFF format validation/repair toolkit.
- **Key facts**:
  - `og-codebase/EuOfficeTools.exe`, 1,653,480 bytes, x64 CONSOLE, 178 imports / 12 DLLs, 0 exports, 56 RTTI: `Cfb`, `Doc`, `Docx`, `Excel`, `FileExtension`, `Odf`, `Ooxml`, `Ppt`, `Pptx`, `Rtf`, `TreeWalker`, `Txt`, `Xlsb` + pcre2 `_Node_*` classes.
  - 3,633 functions / 111,636 lines (batch `ghidra-repair`).
  - Deepening-batch3: XPath OOXML surgery (`//dcterms:created`, `//dcterms:modified`, `//cp:lastModifiedBy`, `//w:numPr/w:ilvl`); BIFF record parser with `"biff_version="`, "NAME formula", "cell name"/"row number" diagnostics, "in external(?) file #".
- **DW relevance**: office repair detail-layer spec (formats enumerated above).

### re-workspace/artifacts/per-binary/EuPDFRepairFile.md
- **Role**: PDF repair engine — xref surgery + decrypt.
- **Key facts**: `og-codebase/EUFileRepair/EuPDFRepairFile.exe`, 1,896,680 bytes, x86 CONSOLE, 224 imports / 13 DLLs, 1 export: `_libiconv_version`; 4,649 functions / 133,274 lines (batch `ghidra-repair`).
- **DW relevance**: PDF repair approach = xref-table reconstruction + decryption; DW candidate crate-level work.

### re-workspace/artifacts/per-binary/EuRepairHost.md
- **Role**: Repair licensing host — "Activation bridge to repair engines".
- **Key facts**: `og-codebase/EuRepairHost.exe`, 23,784 bytes, x86 CONSOLE, 59 imports / 3 DLLs, 0 exports, 0 RTTI; only 50 functions / 985 lines (batch `ghidra-repair`).
- **DW relevance**: KEY licensing fact — repair-engine activation is a tiny separate host process; DW licensing should stay equally thin and user-mode.

### re-workspace/artifacts/per-binary/EuSendMessage.md
- **Role**: WinPE-side messaging helper (BUILDPE drop).
- **Key facts**: `og-codebase/BUILDPE/EaseUS-x64/EuSendMessage.exe`, 20,200 bytes, x86 GUI, 48 imports / 4 DLLs, 0 exports; 33 functions / 640 lines (batch `ghidra-wave2`).
- **DW relevance**: PE-environment IPC glue; DW WinPE story would need its own equivalent.

### re-workspace/artifacts/per-binary/EuUserRate.md
- **Role**: Rating/telemetry DLL.
- **Key facts**:
  - `og-codebase/EuUserRate.dll`, 1,619,176 bytes, x86 GUI, 365 imports / 12 DLLs, 60 exports, 28 RTTI (`CEuIniFileParser`, `CEuMainWindowsUI`, `CEuUserRateManager`, `IEuFeedbackInterface`, tinyxml2 classes, GDI+ `Image`/`SolidBrush`).
  - 4,731 functions / 198,259 lines (batch `ghidra-misc`).
  - Exports: `EuInitialize`, `EuShowWindow`, `EuUnInitialize` + a full embedded libcurl API.
- **DW relevance**: rating prompt + telemetry — excluded from DW scope (product decision).

### re-workspace/artifacts/per-binary/ExFatFileSystemAnalyser.md
- **Role**: exFAT analyser MOM module — stub with IStorageDevice surface.
- **Key facts**: `PartitionRecovery/ExFatFileSystemAnalyser.mo`, 67,816 bytes, x64, 59 imports / 5 DLLs, 32 exports incl. `IStorageDevice` vtables (`IBlockDevice`+`IObject`), `ConvertVolume`, `IObject` set, CLogWriter/CNTFSLib ops, "... 12 more"; 310 functions / 6,162 lines (batch `ghidra-mo`).
- **DW relevance**: per-FS analyser module naming convention (exFAT instance).

### re-workspace/artifacts/per-binary/ExtFileSystemAnalyser.md
- **Role**: ext analyser MOM module.
- **Key facts**: `PartitionRecovery/ExtFileSystemAnalyser.mo`, 65,768 bytes, x64, 58 imports / 5 DLLs, 27 exports incl. `DeviceIoControl`, `GetBitlockerInfo`, `GetBitlockerStatus`, `GetInformationOffset`, `ConvertVolume`, `IStorageDevice` vtables, "... 7 more"; 297 functions / 6,021 lines (batch `ghidra-mo`).
- **DW relevance**: shows the analyser base-interface includes BitLocker-awareness (`GetBitlockerInfo`/`GetBitlockerStatus`) — DW FS engines need the same hook.

### re-workspace/artifacts/per-binary/FATFileSystemAnalyser.md
- **Role**: FAT analyser MOM module.
- **Key facts**: `PartitionRecovery/FATFileSystemAnalyser.mo`, 69,864 bytes, x64, 59 imports / 6 DLLs, 30 exports incl. `DeviceIoControl`, `GetBitlockerInfo`, `IStorageDevice` vtables, `ConvertVolume`, "... 10 more"; 322 functions / 6,701 lines (batch `ghidra-mo`).
- **DW relevance**: FAT analyser naming instance; BitLocker-aware device surface.

### re-workspace/artifacts/per-binary/FATSearchDLL.md
- **Role**: FAT family search engine (FAT12/16/32).
- **Key facts**:
  - `og-codebase/FATSearchDLL.dll`, 469,224 bytes, x64, 160 imports / 11 DLLs, 2 exports: `EUCreateFATQuickSearcher`, `EUCreateFATSearcher`.
  - 1,265 functions / 64,658 lines (batch `ghidra-searchdll`); capa (15): IOCTL, runtime linking, obfuscated stackstrings, PEB access, PE header parse, ini reads.
  - 54 RTTI classes: `CDoubleFATTable`, `CFAT12Table`, `CFAT16Table`, `CFAT32Table`, `CFATDataRunList`, `CFATFileProperties`, `CFATQuickSearch` (+`CFATQuickSearchWorker`), `CFatCompleteSearch`, `CFatFolderAnalyzeCB`, `CFATSearchedPartitionInfo`, `Fat32CalcPartition`, `ICalcPartition`, `CRawFileInfo`, plus shared `CNTFSFileRecord*`/DataRunList/CFileNode/CFileSystemNode classes, `ICompleteFileSystemSearchController`.
- **DW relevance**: FAT engine class inventory — the FAT12/16/32 table split + `Fat32CalcPartition` is the layout-computation reference for DW.

### re-workspace/artifacts/per-binary/FatFormat.md
- **Role**: FAT format MOM module.
- **Key facts**: `PartitionRecovery/FatFormat.mo`, 60,136 bytes, x64, 76 imports / 10 DLLs, 41 exports incl. `IDiskDevice`/`IPartitionDevice`/`IStorageDevice` ctors/dtors/assigns, `CCallBackOperate`/`CLogWriter` ops, "... 21 more"; 238 functions / 5,719 lines (batch `ghidra-mo`).
- **DW relevance**: formatting (write-path) module — DW is read-recovery focused; low priority.

### re-workspace/artifacts/per-binary/FatLib.md
- **Role**: FAT library MOM module.
- **Key facts**: `PartitionRecovery/FatLib.mo`, 75,496 bytes, x64, 62 imports / 7 DLLs, 63 exports incl. device-interface boilerplate + `IPartitionDevice` vtable, "... 43 more"; 311 functions / 8,953 lines (batch `ghidra-mo`).
- **DW relevance**: FAT low-level library layer (read-path details live in deeper artifacts).

### re-workspace/artifacts/per-binary/FileSystem.md
- **Role**: FileSystem MOM module (dispatcher).
- **Key facts**: `PartitionRecovery/FileSystem.mo`, 133,352 bytes, x64, 72 imports / 7 DLLs, 32 exports incl. `ConvertVolume`, `IStorageDevice` vtables, CNTFSLib/CLogWriter ops, `IObject` set, "... 12 more"; 547 functions / 17,658 lines (batch `ghidra-mo`).
- **DW relevance**: the FS dispatch layer above per-FS analysers — DW's engine-registry design reference.

### re-workspace/artifacts/per-binary/FileSystemAnalyser.md
- **Role**: Base FileSystemAnalyser MOM module.
- **Key facts**: `PartitionRecovery/FileSystemAnalyser.mo`, 57,064 bytes, x64, 59 imports / 5 DLLs, 35 exports incl. `IPartitionDevice` vtable, `IStorageDevice` vtables, `IObject` set, "... 15 more"; 265 functions / 5,336 lines (batch `ghidra-mo`).
- **DW relevance**: base analyser abstraction (the per-FS modules subclass this contract).

### re-workspace/artifacts/per-binary/FileSystemCheck.md
- **Role**: File-system check MOM module.
- **Key facts**:
  - `PartitionRecovery/FileSystemCheck.mo`, 95,464 bytes, x64, 128 imports / 10 DLLs, 113 exports.
  - 419 functions / 9,964 lines (batch `ghidra-mo`).
  - Exports include `CHardDriveIO`, `CHardDriveIOSetting`, `CNtfsRunList`, `CNtfsVolume` (ctors/assigns), `MyTemplate::CSimpleArray<I>`, `CSimpleArray<SECTOR_RANGE>`, `CSimpleArray<tagDataRun>`, `IStorageDevice`, `IObject`.
- **DW relevance**: NTFS-volume validation classes (`CNtfsVolume`/`CNtfsRunList`/`CHardDriveIO`) — the FS-check (consistency) layer for DW.

### re-workspace/artifacts/per-binary/Fixup.md
- **Role**: Fixup MOM module — BCD/partition fix-ups.
- **Key facts**: `PartitionRecovery/Fixup.mo`, 69,352 bytes, x64, 87 imports / 11 DLLs, 36 exports incl. `Bcd_BcdProcPartition`, `Bcd_Init`, `Bcd_IsBcdPartition`, `Bcd_IsExistBcdFile`, `Bcd_IsUsingBcdFile`, `Bcd_IsVistaBoot`, `IObject` set, "... 16 more"; 243 functions / 7,502 lines (batch `ghidra-mo`).
- **DW relevance**: boot-config repair surface (`Bcd_*` family) — user-mode BCD surgery DW can reimplement.

### re-workspace/artifacts/per-binary/GetDriverInfo.md
- **Role**: Driver info — "user-mode side of EPMNtDrv contract".
- **Key facts**: `PartitionRecovery/GetDriverInfo.dll`, 50,920 bytes, x64 CONSOLE, 62 imports / 7 DLLs, 6 exports: `DeleteGetDriverInfoImp`, `GetDriverInfoImp`, `GetDriverInfo_GetDriverInfo`, `GetDriverInfo_Init`, `TBCanUnloadNow`, `TBCreateObject`; 207 functions / 3,865 lines (batch `ghidra-partition`).
- **DW relevance**: documents the user-mode↔driver contract DW replaces with a user-mode disk-access layer.

### re-workspace/artifacts/per-binary/HFSPlusSearchDLL.md
- **Role**: HFS+ search engine (SearchDLL family).
- **Key facts**:
  - `og-codebase/HFSPlusSearchDLL.dll`, 185,064 bytes, x64, 81 imports / 7 DLLs, 2 exports: `EUCreateHFSQuickSearcher`, `EUCreateHFSSearcher`.
  - 665 functions / 22,304 lines (batch `ghidra-searchdll`); capa minimal (3: loop, PE section enum, PDB path).
  - 32 RTTI classes: `CHFSPlusAttribute`, `CHFSPlusBTreeOptionBase` (+`IHFSPlusBTreeOption`), `CHFSPlusCatalog`, `CHFSPlusCompleteFileSystemSearchController`, `CHFSPlusExtents`, `CHFSPlusFileCache`, `CHFSPlusFileNode`, `CHFSPlusQuickFileSystemSearchController`, `CHFSPlusRawfile`, `CPartitionVirtualVolumeDevice`, `DetourDirectoryTree`, `HFSPlusFileDataRun`, `HFSPlusFileProperties`, `HFSPlusSystemNodeProperties`, `IDirecotryTreeBuilder` (sic), plus shared DataRunList/CFileNode classes.
- **DW relevance**: HFS+ engine spec — Catalog/Extents B-tree classes are the on-disk-structure map for DW's HFS+ engine.

### re-workspace/artifacts/per-binary/InfoForSetup.md
- **Role**: Installer helper — pre-install environment probing.
- **Key facts**: `og-codebase/InfoForSetup.exe`, 117,776 bytes, x64, 71 imports / 2 DLLs, 0 exports, 3 RTTI; 404 functions / 13,717 lines (batch `ghidra-core`); capa (9): runtime linking, PE export resolution, command-line args, PE header parse, analysis-tool strings.
- **DW relevance**: installer environment probing — packaging concern for DW installers.

### re-workspace/artifacts/per-binary/Japanese.md
- **Role**: Japanese resource DLL (localization).
- **Key facts**:
  - `og-codebase/Japanese.dll`, 4,206,312 bytes, x86, 30 imports / 2 DLLs, 0 exports, 0 RTTI; only 24 functions / 520 lines (resource-loader stubs).
  - Deepening-batch1: 4 MB dominated by `.rsrc`; resource language 1041 (ja-JP) bitmaps/icons/dialogs/strings; cursor resources language 2052 (zh-CN) — leftover from a zh-CN source project; ships ONLY in the bootable PE build.
- **DW relevance**: locale-resource strategy note; DW uses Tauri i18n resources instead.

### re-workspace/artifacts/per-binary/LdmManager.md
- **Role**: LDM (dynamic disks) manager MOM module.
- **Key facts**: `PartitionRecovery/LdmManager.mo`, 1,012,968 bytes, x64, 116 imports / 12 DLLs, 36 exports incl. `IPartitionDevice`/`IStorageDevice` boilerplate, `CCallBackOperate`/`CLogWriter` ops, `IObject` set, "... 16 more"; 3,622 functions / 91,475 lines (batch `ghidra-mo`).
- **DW relevance**: Windows dynamic-disk (LDM) support is a real DW engine-scope question; this is the module that owns it.

### re-workspace/artifacts/per-binary/Log.md
- **Role**: MOM log module (CLogWriter/CLogInBootICE).
- **Key facts**: `PartitionRecovery/Log.mo`, 48,360 bytes, x64, 77 imports / 10 DLLs, 37 exports incl. `CLogInBootICE` ctor/dtor, `CLogWriter::AppPath`, `CLogWriter::CurrentDate`, `CheckMemOverloaded`, `EndMemMonitor`, `CLogWriter::GetOS4MajorVersion10`, `IObject` set, "... 17 more"; 139 functions / 3,460 lines (batch `ghidra-mo`).
- **DW relevance**: logging + memory-monitor hooks; DW uses tracing crate equivalents.

### re-workspace/artifacts/per-binary/Logger.md
- **Role**: PR logging DLL.
- **Key facts**: `PartitionRecovery/Logger.dll`, 87,784 bytes, x64, 136 imports / 6 DLLs, 14 exports — a `Logger` singleton class: `getInstance`, `log`, `formatLogMessage`, `checkLogFileSize`, `processLogs`, `setLogFile`, `setLogLevel`, `start`, `stop`, `writeLog`, `getLoggerInstance`; 498 functions / 9,720 lines (batch `ghidra-partition`).
- **DW relevance**: log-rotation/level API shape; DW tracing covers it.

### re-workspace/artifacts/per-binary/MainModule.md
- **Role**: MOM host + partition engine — "Module Object Model host; pending-op partition engine shared with EPM".
- **Key facts**:
  - `PartitionRecovery/MainModule.dll`, 1,826,536 bytes, x64, 514 imports / 23 DLLs, 268 exports, 0 RTTI.
  - 9,420 functions / 173,477 lines (batch `ghidra-partition`).
  - Exports enumerate the pending-operation engine: `CBitlockerTurnOffOperation`, `CConvertDisk`, `CCreateOperation` (several ctor forms incl. `VOLUME_SPEC`/`AddPartitionParam`), `CGPTToMBROperation`/`CGPTToMBRCompositeOperation`, `CMBRToGPTOperation`/`CMBRToGPTCompositeOperation`, `CMoveResizeOperation`, `CUSTOM_SPACE` vectors, `IManagerItem`, "... 248 more".
- **DW relevance**: the transactional pending-ops partition manager — DW's partition-tools design reference (composite/undo operation queue).

### re-workspace/artifacts/per-binary/Mom.md
- **Role**: MOM kernel module loader — template stub.
- **Key facts**: `PartitionRecovery/Mom.mo`, 76,520 bytes, x64, 58 imports / 5 DLLs, 10 exports (IObject + `GetModule`); 331 functions / 6,720 lines (batch `ghidra-mo`).
- **DW relevance**: the module-loader piece of MOM (module discovery/loading contract).

### re-workspace/artifacts/per-binary/NTFSFileSystemAnalyser.md
- **Role**: NTFS analyser MOM module.
- **Key facts**: `PartitionRecovery/NTFSFileSystemAnalyser.mo`, 68,840 bytes, x64, 58 imports / 5 DLLs, 30 exports incl. `DeviceIoControl`, `GetBitlockerInfo`, `IStorageDevice` vtables, `IObject` set, "... 10 more"; 325 functions / 6,698 lines (batch `ghidra-mo`).
- **DW relevance**: NTFS analyser instance of the shared analyser contract.

### re-workspace/artifacts/per-binary/NTFSLib.md
- **Role**: NTFS library MOM module — the NTFS on-disk structure classes.
- **Key facts**:
  - `PartitionRecovery/NTFSLib.mo`, 197,352 bytes, x64, 73 imports / 10 DLLs, **310 exports**, 0 RTTI.
  - 832 functions / 25,791 lines (batch `ghidra-mo`).
  - Exports: `CHardDriveIO`(+`CHardDriveIOSetting(IManager*, IBlockDevice*)`), `CNtfsAttributeList`, `CNtfsFileRecord`, `CNtfsFileRecordAttribute`, `CNtfsMFT`, `MyTemplate::CSimpleArray<...>` variants (`G`, `tagAttriListElement`, `tagCNtfsFileRecordForMultiRecord`, `tagDataRun`, `CNtfsFileRecordAttribute`), "... 290 more".
- **DW relevance**: the canonical NTFS MFT/record/attribute class inventory for DW's `ntfs` engine.

### re-workspace/artifacts/per-binary/NTFSSearchDLL.md
- **Role**: NTFS search engine (SearchDLL family).
- **Key facts**:
  - `og-codebase/NTFSSearchDLL.dll`, 577,256 bytes, x64, 99 imports / 13 DLLs, 2 exports: `EUCreateNTFSQuickSearcher`, `EUCreateNTFSSearcher`.
  - 1,931 functions / 62,394 lines (batch `ghidra-searchdll`); capa (10): IOCTL, ini read, memory capacity, export resolution.
  - 27 RTTI classes: `CNTFSBuildVolumeBitmap` (+`IBuildVolumeBitmap`), `CNTFSFileProperties`, `CNTFSFileRecordAttribute`, `CNTFSFileRecordDataRunsAttribute`, `CNTFSFileRecordFileNameAttribute`, `CNTFSQuickSearch`(+`CNTFSQuickSearchBase`), `CNTFSSearchController`, `CStorageDeviceWithCache`, `CSearchedNTFSPartitionInfo`, `_Ref_count_base`, shared DataRunList/CFileNode interfaces.
  - ci-multitool addendum pins the IOCTL call site at `0x18002C0BA` — raw disk access via EPMNtDrv.
- **DW relevance**: NTFS engine spec; volume-bitmap builder + MFT attribute classes are the DW ntfs engine checklist.

### re-workspace/artifacts/per-binary/NTFSUtil.md
- **Role**: NTFS utilities MOM module.
- **Key facts**: `PartitionRecovery/NTFSUtil.mo`, 897,768 bytes, x64, 124 imports / 15 DLLs, 59 exports incl. `CHardDriveIO`(+Setting), device-interface boilerplate, `IObject` set, "... 39 more"; 2,871 functions / 132,414 lines (batch `ghidra-mo`).
- **DW relevance**: large NTFS utility surface — details in deeper artifacts.

### re-workspace/artifacts/per-binary/PDFViewer.md
- **Role**: PDFium-based preview host.
- **Key facts**: `og-codebase/PDFViewer.exe`, 2,974,952 bytes, x64 GUI, 601 imports / 16 DLLs, 0 exports, 361 RTTI (MFC); 9,206 functions / 352,504 lines (batch `ghidra-misc`).
- **DW relevance**: preview host — DW uses web-side PDF rendering.

### re-workspace/artifacts/per-binary/PE_Tools.md
- **Role**: PE/tool library — large helper DLL used by setup/tools.
- **Key facts**:
  - `og-codebase/PE_Tools.dll`, 976,104 bytes, x64, 294 imports / 18 DLLs, 10 exports, 51 RTTI (std/regex/system_error).
  - 3,286 functions / 136,012 lines (batch `ghidra-misc`).
  - Export surface (pet_* prefix): `pet_decompress_packet`, `pet_download_packet`, `pet_get_petools_dir`, `pet_install_driver`, `pet_is_exist_packet`, `pet_regist_progress_fun`, `pet_regist_status_fun`, `pet_set_download_path`, `pet_set_progress_data`, `pet_verify_packet`.
- **DW relevance**: packet download/verify/install machinery (WinPE tool packs); includes `pet_install_driver` — the driver-install path DW excludes.

### re-workspace/artifacts/per-binary/Partition.md
- **Role**: Partition analysis MOM module (GPT).
- **Key facts**: `PartitionRecovery/Partition.mo`, 406,760 bytes, x64, 92 imports / 11 DLLs, 47 exports incl. `CGPTDiskAnalyzer` ctor/dtor/assign, `IStorageDevice` vtables, `IObject` set, "... 27 more"; 2,179 functions / 38,246 lines (batch `ghidra-mo`).
- **DW relevance**: `CGPTDiskAnalyzer` — GPT layout analysis reference for DW.

### re-workspace/artifacts/per-binary/PartitionRecovery.md
- **Role**: PR core — partition recovery core DLL.
- **Key facts**: `PartitionRecovery/PartitionRecovery.dll`, 892,648 bytes, x64, 398 imports / 23 DLLs, 59 exports incl. device-class ctors (`CPartitionDevice`, `IDiskDevice`, `IPartitionDevice`, `IStorageDevice`), CNTFSLib/CLogWriter ops, `IObject` set, "... 39 more"; 3,182 functions / 70,706 lines (batch `ghidra-partition`).
- **DW relevance**: the core partition-recovery orchestrator DLL — DW's scan orchestration equivalent.

### re-workspace/artifacts/per-binary/PartitionRecoveryD.md
- **Role**: PR "D" variant MOM module — stub.
- **Key facts**: `PartitionRecovery/PartitionRecoveryD.mo`, 271,080 bytes, x64, 80 imports / 9 DLLs, 32 exports incl. `ConvertVolume`, `IStorageDevice` vtables, `IObject` set, "... 12 more"; 1,571 functions / 28,463 lines (batch `ghidra-mo`).
- **DW relevance**: naming suggests a deep-scan variant; details in deeper artifacts.

### re-workspace/artifacts/per-binary/PubLog.md
- **Role**: Shared publish-log facility — cross-process shared-memory log bus.
- **Key facts**:
  - `og-codebase/PubLog.dll`, 29,928 bytes, x64, 54 imports / 5 DLLs, 14 exports, 4 RTTI (`CDirectLog`, `CFileMappingLog`, `IEsLog`, `type_info`); 88 functions / 1,776 lines (batch `ghidra-misc`).
  - Exports: `DebugTrace` (2 forms), `InitLog`, `Log`, `LogError`, `LogInfo`, `LogWarning`, `ReleaseTrace` (each 2 forms), `FileNameToFileCode`.
  - Deepening-batch3: maps `./log/DefaultLog.txt` as a memory-mapped file; namespace GUIDs `DRWLOG_{1BFCF65F-0766-4a71-B7A3-2EB2CD1412DE}` (log mutex) and `DRW_{1E956C11-4D20-49d7-B4C7-016A342A2D39}` (data view); condition-variable sync (InitializeConditionVariable/SleepConditionVariableCS/WakeAllConditionVariable, dynamically resolved); the other end of DRWLoader's 2 MB shared log flush loop (drained every 5 s).
- **DW relevance**: multi-process log bus pattern — DW (Tauri + Rust engine processes) may want a simpler channel, but the concept (single shared log with a drainer) is reusable.

### re-workspace/artifacts/per-binary/RAWSearchDLL.md
- **Role**: RAW carving engine (SearchDLL family) — signature-registry-driven.
- **Key facts**:
  - `og-codebase/RAWSearchDLL.dll`, 644,840 bytes, x64, 186 imports / 14 DLLs, 6 exports, **198 RTTI classes** — largest analyser class set.
  - 1,921 functions / 80,614 lines (batch `ghidra-searchdll`); capa (16): runtime linking, obfuscated stackstrings, XOR encode, file R/W, ini read, HTTP status check, ZLIB inflate/deflate.
  - Exports: `EUCreateRAWSearcher`, `EUCreateRAWSearcherForCellTest`, `EUGetAnalyserList`, `EUGetAnalyserMap`, `EUGetAnalyserMultimap`, `GetRawUserInfoMap` — the signature-registry introspection API.
  - RTTI (first 26 shown) enumerates format analysers: `AEPAnalyser`, `C7ZIPAnalyser`, `CADTSAnalyser`(+`CADTSRawFileInfo`), `CAPEAnalyser`(+`CAPERawFileInfo`), `CAsfAnalyser`(+`CAsfRawFileInfo`), `CDWGAnalyser`(+`CDWGRawInfo`), `CDXFAnalyser`(+`CDXFRawFileInfo`), `CEPSAnalyser`, `CFCPEvtAnalyser`(+`CFCPEvtFileInfo`), `CFH11Analyser`, `CFileAACAnalyzer`, `CFileAIFAnalyzer`, `CFileOggAnalyser`, `CGPXAnalyser`, `CGifAnalyser`(+`CGifFileInfo`), `CHFPAnalyser`, "... 172 more" plus CAtl classes.
- **DW relevance**: THE RAW-carving signature-registry spec — the `EUGetAnalyserList/Map/Multimap` API + per-format `C*Analyser` classes define DW's carving engine registry (the full 198-class list is in the artifact).

### re-workspace/artifacts/per-binary/RawFixer.md
- **Role**: FS repair — "Transactional NTFS/FAT/exFAT structure repair".
- **Key facts**:
  - `PartitionRecovery/RawFixer.dll`, 706,792 bytes, x64 CONSOLE, 210 imports / 11 DLLs, 39 exports incl. `CRawFixer` assignment ops, `IStorageDevice`/`IObject` boilerplate, "... 19 more".
  - 3,446 functions / 86,575 lines (batch `ghidra-partition`); deep-dive artifact `artifacts/PartitionRecovery/rawfixer-fs-repair.md`.
- **DW relevance**: transactional file-system structure repair (fix MFT/FAT chains) — a DW "repair" feature reference, user-mode by design.

### re-workspace/artifacts/per-binary/ReFSSearchDLL.md
- **Role**: ReFS search engine (SearchDLL family).
- **Key facts**:
  - `og-codebase/ReFSSearchDLL.dll`, 154,856 bytes, x64, 49 imports / 6 DLLs, 2 exports: `EUCreateReFSQuickSearcher`, `EUCreateReFSSearcher`.
  - 629 functions / 20,270 lines (batch `ghidra-searchdll`); capa (5): OS version get/check, export resolution.
  - 46 RTTI classes: `RefsCheckPoint`, `RefsDbr`, `RefsDirectoryValues`, `RefsFileProperties`, `RefsFileValues`, `RefsMSTreeHeaderDesc`, `RefsMetadataBlockHeader`, `RefsMetadataBlockReference`, `RefsNodeHeader`, `RefsNodeRecord`, `RefsPartitionInformation`, `RefsQuickSearchFiles`, `RefsSearchController`, `RefsSupperBlock` (sic), `CReFSDataRunList`, `CRefsResidentDataRunList`, shared DataRunList/CFileNode interfaces.
- **DW relevance**: ReFS on-disk structure vocabulary (DBR/superblock/checkpoint/metadata-block B-tree) — DW's refs engine checklist; OS-version gating noted by capa.

### re-workspace/artifacts/per-binary/RecoveryModule.md
- **Role**: Recovery engine — "Recovery module of PR".
- **Key facts**: `PartitionRecovery/RecoveryModule.dll`, 1,348,840 bytes, x64, 369 imports / 19 DLLs, 39 exports incl. device-class boilerplate, `IObject` set, "... 19 more"; 8,811 functions / 91,972 lines (batch `ghidra-partition`).
- **DW relevance**: the file-recovery (lost-file scan/restore) engine core — DW's main scan pipeline reference.

### re-workspace/artifacts/per-binary/RefsFileSystemAnalyser.md
- **Role**: ReFS analyser MOM module.
- **Key facts**: `PartitionRecovery/RefsFileSystemAnalyser.mo`, 453,864 bytes, x64, 113 imports / 9 DLLs, 31 exports incl. `DeviceIoControl`, `ConvertVolume`, `IStorageDevice` vtables, `IObject` set, "... 11 more"; 1,752 functions / 42,013 lines (batch `ghidra-mo`) — the biggest analyser module.
- **DW relevance**: ReFS analyser instance; large size hints at ReFS metadata complexity.

### re-workspace/artifacts/per-binary/RegLib.md
- **Role**: Offline registry-hive access library.
- **Key facts**:
  - `PartitionRecovery/RegLib.mo`, 93,928 bytes, x64, 52 imports / 10 DLLs, 28 exports; 302 functions / 10,974 lines (batch `ghidra-mo`).
  - `RegAPI` namespace exports: `OpenHiveA/W`, `CloseHive`, `OpenHiveByVolumeA/W`, `CloseHiveByVolume` (with `CRegVolumeDevice`), plus a full offline registry API: `RegCloseKey`, `RegCreateKeyExA/W`, `RegDeleteKeyA/W`, `RegDeleteValueA/W`, `RegEnumKeyExA/W`, `RegEnumValueA/W`, `RegFlushKey`, `RegOpenKeyExA/W`, "... 8 more".
- **DW relevance**: offline registry hive read/edit (volume-attached) — user-mode and directly reusable concept for DW (e.g., recovering registry-backed config).

### re-workspace/artifacts/per-binary/SetupUE.md
- **Role**: Installer — installation/uninstall, geo detection, retention triggers.
- **Key facts**: `og-codebase/SetupUE.exe`, 177,384 bytes, x86 GUI, 79 imports / 5 DLLs, 0 exports, 8 RTTI (CRT only); 670 functions / 24,545 lines (batch `ghidra-core`); capa (14): create process, registry, geographical location, system info, OS version.
- **DW relevance**: installer behavior inventory (geo/retention) — DW packaging omits ad/retention triggers.

### re-workspace/artifacts/per-binary/Thread.mo (Thread.md)
- **Role**: MOM thread module — template stub.
- **Key facts**: `PartitionRecovery/Thread.mo`, 74,472 bytes, x64, 81 imports / 6 DLLs, 10 exports (IObject + `GetModule`); 342 functions / 6,676 lines (batch `ghidra-mo`).
- **DW relevance**: worker-thread module name only.

### re-workspace/artifacts/per-binary/ToolsModule.md
- **Role**: Tools engine — "DM pending operations, tools".
- **Key facts**:
  - `PartitionRecovery/ToolsModule.dll`, 2,578,664 bytes, x64, 468 imports / 23 DLLs, 98 exports, 0 RTTI.
  - 17,876 functions / 187,572 lines (batch `ghidra-partition`) — largest PR DLL by function count.
  - Exports: `CChangeLetterOperation`, `CDelAllPartitionOperation`, `CDeletePartitionOperation`, device-class ctors, `IManagerItem`, "... 78 more".
- **DW relevance**: the disk-management tools operation set (change letter, delete partitions...) — DW partition-tools scope reference.

### re-workspace/artifacts/per-binary/UILogic.md
- **Role**: PR UI logic layer.
- **Key facts**: `PartitionRecovery/UILogic.dll`, 1,448,168 bytes, x64, 410 imports / 20 DLLs, 39 exports (device-class + IObject boilerplate, "... 19 more"); 9,516 functions / 103,011 lines (batch `ghidra-partition`).
- **DW relevance**: UI-logic separation precedent — DW keeps logic in Rust commands, UI in Tauri webview.

### re-workspace/artifacts/per-binary/VideoFileRepairer.md
- **Role**: VideoViewer-side video repair entry.
- **Key facts**:
  - `og-codebase/VideoViewer/VideoFileRepairer.exe`, 1,919,208 bytes, x86 CONSOLE, 389 imports / 18 DLLs, 14 exports, 0 RTTI.
  - 7,019 functions / 347,662 lines (batch `ghidra-wave2`).
  - Exports are the faad2 decoder API: `NeAACDecOpen/Init/Init2/Decode/Decode2/Close/...` (14 functions).
- **DW relevance**: AAC decode inside the video repair verification path — DW would use a Rust AAC crate or skip.

### re-workspace/artifacts/per-binary/Win32Bcd.md
- **Role**: Windows BCD editor.
- **Key facts**:
  - `PartitionRecovery/Win32Bcd.dll`, 261,864 bytes, x64, 82 imports / 2 DLLs, 96 exports, 0 RTTI.
  - 996 functions / 38,357 lines (batch `ghidra-partition`).
  - Export classes: `CWin32BCDItemBoolean`, `CWin32BCDItemStr`, `CWin32BCDManager`, `CWin32BootLoaderItem`, `CWin32BootMgrItem`, `CWin32DeviceItem`, `_Win32_BootLoader_Item_`, `_Win32_BootMgr_Item_`, `_Win32_Device_Des_`, `_Win32_Device_Option_`, "... 76 more".
- **DW relevance**: user-mode BCD store editing — directly reusable concept for DW boot-repair features.

### re-workspace/artifacts/per-binary/WinChkdsk.md
- **Role**: chkdsk wrapper — launches/monitors Windows chkdsk (legacy mod).
- **Key facts**:
  - `PartitionRecovery/WinChkdsk.exe`, 72,936 bytes, x64 CONSOLE, 166 imports / 12 DLLs, 0 exports, 0 RTTI; 289 functions / 6,188 lines (batch `ghidra-partition`).
  - Deepening-batch1: build path `i:\_drw_main\partitionrecovery\legacy_mod\mod.winchkdsk\main.cpp`; runs real chkdsk from `\System32\`; config `WinChkdsk.ini`; results via `LogMsg.txt`/`LogVss.txt`; VSS coordination ("Volume Snapshot name: "); `SectionFlags`/`GlobalSettings` config keys; uses `ulib.dll`; reports into module message system (AddErrorMsgIDs).
- **DW relevance**: delegating FS check to Windows chkdsk is a user-mode pattern DW can adopt unchanged.

### re-workspace/artifacts/per-binary/ZfsSearchDLL.md
- **Role**: ZFS search engine (SearchDLL family).
- **Key facts**:
  - `og-codebase/ZfsSearchDLL.dll`, 275,176 bytes, x64 CONSOLE, 119 imports / 7 DLLs, 2 exports: `EUCreateZFSQuickSearcher`, `EUCreateZFSSearcher`.
  - 981 functions / 45,364 lines (batch `ghidra-searchdll`); capa (6): delay execution (polling), file R/W, export resolution.
  - 43 RTTI classes: `CZfsQuickSearch`, `CZfsSearchController`, `ZfsDataRunlist`, `ZfsFileProperties`, `ZfsQuickSearchStream`, `ZplNodeFile`, `ZplNodeFolder`, `ZplNode_`, `CStreamBase`, shared DataRunList/CFileNode interfaces, `bad_any_cast`/`bad_weak_ptr` (std-heavy).
- **DW relevance**: ZFS/ZPL object model classes — DW ZFS engine vocabulary (ZPL node tree, runlist).

### re-workspace/artifacts/per-binary/archive.md
- **Role**: libarchive DLL — extraction for archive preview/repair.
- **Key facts**: `og-codebase/archive.dll`, 688,128 bytes, x64 CONSOLE, 190 imports / 15 DLLs, 427 exports, 0 RTTI; 1,353 functions / 89,903 lines (batch `ghidra-misc`); exports are the libarchive C API (`archive_bzlib_version`, `archive_entry_acl_*`, `archive_entry_atime`, ...).
- **DW relevance**: archive preview/extraction — Rust `zip`/`tar`/`compress` crates cover DW needs.

### re-workspace/artifacts/per-binary/bitlockerhelp.md
- **Role**: BitLocker help MOM module — stub.
- **Key facts**: `PartitionRecovery/bitlockerhelp.mo`, 344,808 bytes, x64, 206 imports / 16 DLLs, 14 exports (IObject set + `GetModule`); 1,239 functions / 34,879 lines (batch `ghidra-mo`).
- **DW relevance**: BitLocker UX/assist layer name; pairs with BitLockerLib/DRWService.

### re-workspace/artifacts/per-binary/cloud_session.md
- **Role**: Cloud OAuth sessions — cpprestsdk OAuth2 loopback for Dropbox/Google/OneDrive.
- **Key facts**:
  - `og-codebase/cloud_session.dll`, 943,336 bytes, x64, 290 imports / 14 DLLs, 4 exports: `create_con`, `getLastError`, `readLastError`, `recordCloudError`; 91 RTTI (cpprest `http_request`/`http_response`, PPL task classes, `RecycleBin`, `IListAllItems`, `IHasThumbnail`, `Default_DownloadTask`).
  - 4,608 functions / 137,230 lines (batch `ghidra-misc`).
  - Companion cpprest artifact: SRP-6a handshake, token management, session upload/download over cpprest http_client.
- **DW relevance**: cloud-recovery OAuth pattern (loopback redirect + token store) if DW ever adds cloud sources; otherwise out of scope.

### re-workspace/artifacts/per-binary/cpprest141_2_10.md
- **Role**: Microsoft C++ REST SDK (Casablanca) — the cloud_session HTTP engine.
- **Key facts**:
  - `og-codebase/cpprest141_2_10.dll`, 3,756,032 bytes, 353 exports, 451 imports; single verified consumer: `cloud_session.dll` (only importer across the whole tree).
  - Not used by DRWUI telemetry (that's Qt5Network + AliyunWrapExe), not used by repair tools (libcurl), not in the bootable PE build (BUILDPE lacks cpprest; cloud_session is live-tree-only).
  - Provides async http_client, JSON, OAuth, WebSocket primitives.
- **DW relevance**: proves three separate HTTP stacks per subsystem (cpprest=ccloud, Qt5Network=telemetry, libcurl=repair); DW consolidates on one Rust HTTP stack.

### re-workspace/artifacts/per-binary/deepening-batch1--japanese-winchkdsk-setupdrv-legacy.md
- **Role**: Thin-binary deepening batch 1 — Japanese.dll, WinChkdsk, setupdrvx64.exe, legacy.dll decoded.
- **Key facts**:
  - Japanese.dll: ja-JP resource satellite, PE-only, zh-CN (2052) cursor leftovers fingerprint a Chinese source project.
  - WinChkdsk (mod.winchkdsk): real chkdsk + VSS coordination, ini-driven.
  - setupdrvx64.exe: PDB `D:\Project_Application\_EPM_Main_\mod.WinDiskAccessDriverSetup\setupempdrv03\x64\Release\setupempdrvx64.pdb`; full SCM lifecycle (CreateService/OpenService/StartService/ControlService); installs to `\System32\Drivers\%s.sys`, `\SystemRoot\System32\drivers\%s.sys`, `%s\System32\Drivers\%s%d.sys` — the `%s%d` variant is the driver versioning scheme (eprdtdrv2.sys-style coexisting versions); "Boot File System" service group; `-Uninstall` mode; registry `SYSTEM\CurrentControlSet\Services\%s` write+verify.
  - legacy.dll: libtomcrypt fingerprint (`des_get_ctx_params`, `kdf_pbkdf1_derive`, `cipher_generic_init_internal`, `alg_id_param`) — legacy crypto substrate, most plausibly old .rsf/scan-session decryption or legacy-license blobs; modern paths use CryptoAPI/OpenSSL.
- **DW relevance**: driver-deploy details are exactly what DW must NOT do (no kernel drivers); legacy crypto = DW scan-session encryption design note.

### re-workspace/artifacts/per-binary/deepening-batch3--eight-stubs-resolved.md
- **Role**: Thin-stub resolution batch — 8 binaries decoded with verbatim strings.
- **Key facts**:
  - send_dmp_with_aliyun.exe: packs crash dumps (zlib + base64) and drives AliyunWrap's session API; owns namespace GUID `ALIYUN_{C69BDD99-9D0B-43f2-A794-9B70DAD4A96A}`; invoked by DRWReport after writing DRWDump0/1.DMP.
  - PubLog.dll: `DRWLOG_{1BFCF65F-...}` / `DRW_{1E956C11-...}` GUID namespaces; condition-variable sync.
  - BootDriver.dll: user-mode WinPE media builder (see BootDriver entry above).
  - AutoUpdate.exe: api-ms-* forwarder resilience.
  - firebasefetch.exe: downloads `https://update.easeus.com.cn/update/drw_cn/Yiwo_remote_config_data.data` (CN host; `_abroad` twin from same host); shells through firebaseonlineconfig.dll; feeds DRWUI's ~600 Firebase feature flags.
  - AliyunWrapExe.exe: `AliyunConfig.ini` + `ACCESSKEYSECRET`, AES payload encryption, `AllowSendUrl` whitelist.
  - EUUnZip.exe: MFC 9.0 verification extractor.
  - EuOfficeTools.exe: OOXML XPath surgery + BIFF record parser (see EuOfficeTools entry).
- **DW relevance**: mostly telemetry/config chain (excluded); the driver-setup and office-repair facts feed DW scope decisions.

### re-workspace/artifacts/per-binary/ebrntdrv.md
- **Role**: Kernel EPMNtDrv — BUILDPE drop.
- **Key facts**: `BUILDPE/x64/Windows/system32/ebrntdrv.sys`, 27,272 bytes, x64 NATIVE, 41 imports / 1 DLL, 0 exports; 39 functions / 2,192 lines (batch `ghidra-drivers`); capa (7): IRP completion, open process, device object, PDB path; SHA256-short `4370aae1ee1cdeab` — identical to eprdtdrv_win7older.sys.
- **DW relevance**: excluded by DW mandate (kernel raw-IO driver); its user-mode contract (documented in GetDriverInfo/eprdtdrv artifacts) is what DW replaces with user-mode sector reads.

### re-workspace/artifacts/per-binary/epmdkdrv.md
- **Role**: Kernel EPMNtDrv — system32 drop.
- **Key facts**: `BUILDPE/x64/Windows/system32/epmdkdrv.sys`, 27,272 bytes, x64 NATIVE, 41 imports / 1 DLL, 0 exports; 39 functions / 2,191 lines (batch `ghidra-drivers`); capa (7) same family; SHA256-short `486f65c894d5f23c`.
- **DW relevance**: excluded (kernel driver); same EPMNtDrv binary family.

### re-workspace/artifacts/per-binary/eprdtdrv.md
- **Role**: Kernel EPMNtDrv — raw IO driver, `\Device\EPMNTDRV`, user path `\\.\EPRDTDRV\d\p`.
- **Key facts**: `PartitionRecovery/eprdtdrv.sys`, 27,728 bytes, x64 NATIVE, 41 imports / 1 DLL, 0 exports; 39 functions / 2,192 lines (batch `ghidra-drivers`); capa (7): IRP completion, open process, delay, device object, PDB path; deep-dive artifact `artifacts/PartitionRecovery/eprdtdrv-sys--driver-internals.md`.
- **DW relevance**: THE raw-sector access driver DW will not reimplement — user path `\\.\EPRDTDRV\d\p` (disk\partition addressing) documents the access model DW replaces (e.g., Windows handle-based sector reads).

### re-workspace/artifacts/per-binary/eprdtdrv_win7older.md
- **Role**: Kernel EPMNtDrv legacy — Win7-and-older build of same driver.
- **Key facts**: `PartitionRecovery/eprdtdrv_win7older.sys`, 27,272 bytes, x64 NATIVE, 41 imports / 1 DLL, 0 exports; 39 functions / 2,192 lines (batch `ghidra-drivers`); SHA256-short `4370aae1ee1cdeab` (byte-identical to ebrntdrv.sys drop).
- **DW relevance**: excluded (kernel driver); OS-compat variant only.

### re-workspace/artifacts/per-binary/exFATSearchDLL.md
- **Role**: exFAT search engine (SearchDLL family).
- **Key facts**:
  - `og-codebase/exFATSearchDLL.dll`, 413,416 bytes, x64, 163 imports / 9 DLLs, 2 exports: `EUCreateEXFATQuickSearcher`, `EUCreateEXFATSearcher`.
  - 1,299 functions / 58,043 lines (batch `ghidra-searchdll`); capa (10): XOR encode, **FNV hash**, "parse credit card information" (capa regex F/P — actually carving signature scanning per the addendum), file R/W.
  - 65 RTTI classes: `CExFatCompleteSearch`, `CExFatDataRunList`, `CExFatFileProperties`, `CExFatPartition`(+`CExFatPartitionEx`), `CExFatQuickSearch`(+`CExFatQuickSearchEx`), `CExFatVolumeAnalyer` (sic), `CQuickSearchAndBuildBitmap`, `ExFatCalcPartition`, `ICalcPartition`, `ICompleteFileSystemSearchController`, shared NTFS-record/DataRunList classes.
- **DW relevance**: exFAT engine class inventory; `CQuickSearchAndBuildBitmap` shows allocation-bitmap reconstruction during quick scan.

### re-workspace/artifacts/per-binary/firebasefetch.md
- **Role**: Out-of-proc Firebase remote-config fetcher.
- **Key facts**: `og-codebase/firebasefetch.exe`, 90,344 bytes, x64 CONSOLE, 149 imports / 10 DLLs, 0 exports, 21 RTTI (std/filesystem); 449 functions / 9,603 lines (batch `ghidra-misc`); batch-3: fetch loop + retry/cache layer over firebaseonlineconfig.dll; URL `https://update.easeus.com.cn/update/drw_cn/Yiwo_remote_config_data.data`.
- **DW relevance**: remote-config chain — DW has no equivalent feature flags requirement (no AI features).

### re-workspace/artifacts/per-binary/firebaseonlineconfig.md
- **Role**: Firebase Remote Config wrapper (A/B flags).
- **Key facts**: `og-codebase/firebaseonlineconfig.dll`, 1,155,816 bytes, x64 GUI, 333 imports / 19 DLLs, 2 exports: `create_config`, `release_config`; 45 RTTI (`FilebaseOnlineConfig` (sic), `IFireBaseConfig`, `RemoteConfigRequest`/`RemoteConfigResponse`, `TransportCurl`, `ControllerCurl`, `ReferenceCountedFutureImpl`); 3,520 functions / 146,733 lines (batch `ghidra-misc`).
- **DW relevance**: config-fetch client — excluded from DW (product telemetry/flags).

### re-workspace/artifacts/per-binary/legacy.md
- **Role**: Legacy support DLL.
- **Key facts**: `og-codebase/legacy.dll`, 101,888 bytes, x64 GUI, 115 imports / 4 DLLs, 1 export: `OSSL_provider_init` (OpenSSL 3 provider entry); 275 functions / 7,080 lines (batch `ghidra-misc`); deepening-batch1: embedded libtomcrypt fingerprint (DES-era + PBKDF1) — "most plausibly the old .rsf / scan-session decryption or legacy-license blobs".
- **DW relevance**: legacy crypto substrate — scan-session (.rsf) decryption reference for DW scan persistence; DW uses Rust crypto crates.

### re-workspace/artifacts/per-binary/libcurl.md
- **Role**: Repair pipeline's HTTP client (EUFileRepair-local libcurl).
- **Key facts**:
  - `og-codebase/EUFileRepair/libcurl.dll`, 564,736 bytes, 100 exports, 419 imports; shipped only inside `EUFileRepair/`.
  - Zero PE-import consumers across all 1,470 inventoried files — loaded dynamically (LoadLibrary/GetProcAddress) or invoked as a tool by external repair helper processes.
  - Uses: sample-file fetching (Adobe repair cloud client + format-specific sample registries), cloud repair relay, possibly repair format-pack updates.
- **DW relevance**: repair-side HTTP is late-bound plugin-style — a DW repair module would use reqwest behind a Rust trait instead.

### re-workspace/artifacts/per-binary/libssh2.md
- **Role**: SSH library — libssh2 1.11.0 (NAS agent transport).
- **Key facts**: `og-codebase/libssh2.dll`, 246,784 bytes, x64, 208 imports / 13 DLLs, 135 exports (`libssh2_agent_*`, `libssh2_channel_*`, `libssh2_base64_decode`, ...); 550 functions / 29,444 lines (batch `ghidra-wave2`); consumer of OpenSSL family (per openssl-crypto-trio).
- **DW relevance**: NAS recovery transport (user-mode!) — the Rust `ssh2` crate is the direct DW equivalent.

### re-workspace/artifacts/per-binary/libvmdk.md
- **Role**: VMware VMDK disk access (libvmdk-derived, customized).
- **Key facts**: `PartitionRecovery/libvmdk.dll`, 1,108,712 bytes, x64, 81 imports / 8 DLLs, 50 exports (`libvmdk_check_file_signature*`, `libvmdk_extent_descriptor_*`, `libvmdk_handle_*`, `libvmdk_error_*`, `libvmdk_get_version`...); 1,352 functions / 47,690 lines (batch `ghidra-partition`); imports zlib family (per network-libraries consumer map).
- **DW relevance**: VMDK container support — disk-image input feature for DW.

### re-workspace/artifacts/per-binary/peformat.md
- **Role**: Partition/format helpers DLL.
- **Key facts**: `PartitionRecovery/peformat.dll`, 50,920 bytes, x64 CONSOLE, 65 imports / 7 DLLs, 46 exports incl. `CPartitionDevice`/`IDiskDevice`/`IPartitionDevice`/`IStorageDevice` boilerplate, "... 26 more"; 207 functions / 4,058 lines (batch `ghidra-partition`).
- **DW relevance**: low-level partition-table format helpers — spec source for DW MBR/GPT parsing.

### re-workspace/artifacts/per-binary/send_dmp_with_aliyun.md
- **Role**: Crash-dump uploader to Aliyun OSS.
- **Key facts**: `og-codebase/send_dmp_with_aliyun.exe`, 107,456 bytes, x64 CONSOLE, 156 imports / 5 DLLs, 1 export (a std mangled assign-op), 3 RTTI; 288 functions / 8,381 lines (batch `ghidra-misc`); batch-3: zlib+base64 dump packing, drives AliyunWrap session API, GUID namespace `ALIYUN_{C69BDD99-9D0B-43f2-A794-9B70DAD4A96A}`, invoked by DRWReport after DRWDump0/1.DMP.
- **DW relevance**: crash-upload chain — excluded from DW.

### re-workspace/artifacts/per-binary/setupdrvx64.md
- **Role**: Driver setup — installs EPMNtDrv drivers (x64).
- **Key facts**: `PartitionRecovery/setupdrvx64.exe`, 174,024 bytes, x64 GUI, 94 imports / 3 DLLs, 0 exports, 0 RTTI; 436 functions / 15,028 lines (batch `ghidra-partition`); deepening-batch1 details: PDB `mod.WinDiskAccessDriverSetup\setupempdrv03`, SCM lifecycle, `\System32\Drivers\%s.sys` + `%s%d.sys` versioned layouts, "Boot File System" group, `-Uninstall` mode.
- **DW relevance**: driver installer — the exact component class DW eliminates (no kernel drivers).

### re-workspace/artifacts/per-binary/unins000.md
- **Role**: Inno Setup uninstaller (3rd-party generated, customized).
- **Key facts**: `og-codebase/unins000.exe`, 1,259,408 bytes, x86 GUI, 298 imports / 12 DLLs, 0 exports, 0 RTTI; 4,389 functions / 148,953 lines (batch `ghidra-misc`).
- **DW relevance**: packaging artifact — DW uses its own installer (e.g., NSIS/MSI/WiX); nothing to port.

## Part 2 — runtime-families (re-workspace/artifacts/runtime-families/)

### re-workspace/artifacts/runtime-families/d3d-graphics-shims.md
- **Role**: Direct3D/DXGI shims — BUILDPE-only graphics portability.
- **Key facts**:
  - Pattern `^(d3d9|d3d11|d3d8thk|dxgi)\.dll$`; 4 copies, all in `BUILDPE/EaseUS-x64/`.
  - WinPE's reduced graphics stack lacks full D3D; shims let Qt's OpenGL scene-graph + video preview render; live Windows relies on the OS.
  - Consumers: `Qt5Gui.dll`, `d3d11.dll`/`d3d9.dll` (self), `diskhealth/Qt5Gui.dll`, `diskhealth/scenegraph/qsgd3d12backend.dll`.
  - Members: d3d9.dll (2 MB — D3D9 + D3DX runtime), d3d11.dll + dxgi.dll, d3d8thk.dll (legacy D3D8 thunk).
- **DW relevance**: Tauri 2 uses the OS WebView — no D3D shims needed; only relevant if DW ever ships a WinPE build.

### re-workspace/artifacts/runtime-families/imagemagick-modules.md
- **Role**: ImageMagick delegate modules (EUFileRepair).
- **Key facts**:
  - Pattern `^IM_MOD_RL_.*\.dll$`; 7 copies, all in `EUFileRepair/`: `heic_`, `jpeg_`, `png_`, `psd_`, `svg_`, `tiff_`, `webp_`.
  - Each format codec is a separate DLL loaded at runtime by the repair pipeline's ImageMagick usage.
  - No PE-import consumers found in inventoried binaries — loaded dynamically (LoadLibrary) or by external processes (possibly the runtime-downloaded connect.exe).
- **DW relevance**: repair image-codec set (7 formats) — DW image repair would use Rust image crates; format list is the spec.

### re-workspace/artifacts/runtime-families/media-codecs.md
- **Role**: Media codec set for preview + repair.
- **Key facts**:
  - Pattern covers `libx265|de265|heif|GDIPLUS|avcodec|avformat|avutil|swres|swscale|postproc`; 9 shipped copies (root, BUILDPE/EaseUS-x64, EUFileRepair/heif.dll).
  - libx265.dll = HEVC encoder (repair re-encode); de265.dll = HEVC decoder (preview); heif.dll = HEIF/HEIC codec; postproc-58.dll = ffmpeg postprocessing (repair).
  - Core consumers: `DRWReport.exe`, `DRWUI.exe`, `EUImageEnhance.dll`, `EUOfficeViewer.exe`, `EURawImg.exe`, `EuOfficeRepairWin32.exe`, `EuUserRate.dll`, `PDFViewer.exe`, `heif.dll`; plus BUILDPE twins, `EUFileRepair/EUImgRepairFile.exe`, VideoViewer pair.
- **DW relevance**: preview/repair codec dependency map; DW does preview in the webview and repair with Rust crates — no shipped codec DLLs.

### re-workspace/artifacts/runtime-families/misc-windows-shims.md
- **Role**: Windows system DLL shims shipped for portability.
- **Key facts**:
  - Pattern `^(dbghelp|ole32|msi|oledlg|legacy|PubLog)\.dll$`; 10 copies (root + BUILDPE + SysWOW64/system32 msi+oledlg).
  - dbghelp.dll = crash-dump symbol resolution (DRWReport path); msi.dll + oledlg.dll = WinPE-compat copies; legacy.dll + PubLog.dll are EaseUS-authored utility glue.
  - Broad consumer map: DRW.exe, DRWReport.exe, DRWUI.exe, EUHwpViewer.exe, EUOfficeViewer.exe, EuOfficeRepairWin32.exe, EuUserRate.dll, FATSearchDLL.dll, NTFSSearchDLL.dll, RAWSearchDLL.dll, PartitionRecovery/*.mo+dll set, Qt5* stack, firebasefetch/onlineconfig, unins000.exe, etc.
- **DW relevance**: legacy.dll and PubLog.dll are the only EaseUS-authored members (log bus + legacy crypto); DW equivalents live in Rust (tracing + RustCrypto).

### re-workspace/artifacts/runtime-families/network-libraries.md
- **Role**: Network protocol libraries family.
- **Key facts**:
  - Pattern `^(libcurl|libssh2|libzmq-mt|zlib1|zlib|liblzma)\.dll$`; 11 copies (root, BUILDPE/EaseUS-x64, EUFileRepair/libcurl, PartitionRecovery/zlib*).
  - libzmq-mt-4_3_5.dll = ØMQ 4.3.5 for VAPreviewListener IPC on ports 20171/20172; libssh2 = SSH2 (NAS agent push); libcurl = HTTP(S) client (EUFileRepair).
  - zlib/liblzma = "compression everywhere: rsf, scan sessions, image ops".
  - PE-import consumers: PartitionRecovery/libvmdk.dll, archive.dll, libssh2.dll (+BUILDPE twins).
- **DW relevance**: zlib = the compression substrate of scan-session files (.rsf) — DW scan persistence should note the format family; ZeroMQ preview IPC ports 20171/20172 are a DW-internal-contract candidate (or replaced by Tauri events).

### re-workspace/artifacts/runtime-families/openssl-crypto-trio.md
- **Role**: OpenSSL — THREE shipped generations.
- **Key facts**:
  - 16 copies across root and BUILDPE: libeay32/ssleay32 (0.9.8/1.0.x lineage names), libcrypto-1_1(-x64)/libssl-1_1(-x64) (1.1), libcrypto-3-x64/libssl-3-x64 (3.x), libssl32.
  - Generation→consumer mapping: libeay32/ssleay32 = "legacy modules (EuRepairHost licensing, older engine code)"; 1.1 = "cloud/licensing stack of the 2019-era modules"; 3.x = "newest consumers (firebasefetch, cloud recovery)".
  - Core consumers: DRWUI.exe, cloud_session.dll, legacy.dll, libssh2.dll, the ssl dlls themselves.
- **DW relevance**: KEY licensing fact — repair licensing (EuRepairHost) rides the oldest OpenSSL generation; DW should use a single modern Rust TLS stack (rustls/openssl crate) for everything.

### re-workspace/artifacts/runtime-families/qt5-core-stack.md
- **Role**: Qt 5 main-window framework powering DRWUI.
- **Key facts**:
  - Pattern `^(Qt5Core|Qt5Gui|Qt5Widgets|Qt5Network|Qt5Svg|Qt5WinExtras|Qt5Xml)\.dll$`; 18 copies (root, PartitionRecovery/Qt5Core, BUILDPE/EaseUS-x64, diskhealth).
  - Members: Qt5Core (event loop, signals/slots, QSettings), Qt5Gui (raster/OpenGL painting, QSS), Qt5Widgets ("QMainWindow/QTreeView/QStackedWidget — the entire results UI"), Qt5Network (QNetworkAccessManager — firebase/remote-config/telemetry HTTP), Qt5Svg + iconengines, Qt5WinExtras (taskbar progress/jump-list for scan progress).
  - Core consumers incl. PartitionRecovery DLLs: CloneModule, MainModule, PartitionRecovery, RecoveryModule, ToolsModule, UILogic + platform/imageformat plugins (qwindows, qminimal, qoffscreen, qgif, qjpeg, qsvg, qsvgicon, qwindowsvistastyle).
- **DW relevance**: fully replaced by Tauri 2 (WebView + Rust) in DW; the plugin list (platforms/imageformats) maps to what a bundled webview needs.

### re-workspace/artifacts/runtime-families/qt5-quick-qml-stack.md
- **Role**: Qt Quick 2 / QML engine — DiskHealth app only.
- **Key facts**:
  - 7 copies, all in `diskhealth/`: Qt5Qml, Qt5QmlModels, Qt5Quick, Qt5QuickControls2, Qt5QuickShapes, Qt5QuickTemplates2, Qt5RemoteObjects.
  - "DRWUI is widgets-only; DiskHealth is the QML-based companion"; no PE-import consumers found (late-bound/plugin-loaded).
  - Qt5RemoteObjects = cross-process model sync (DiskInfo service <-> UI); Quick/Controls.2 style plugins (Imagine/Material/Fusion/Universal).
- **DW relevance**: not needed by DW (main app widgets-only in the original; DW is webview-based).

### re-workspace/artifacts/runtime-families/ucrt-forwarders.md
- **Role**: UCRT api-set forwarders shipped because WinPE lacks the Universal CRT.
- **Key facts**:
  - Pattern `^api-ms-win-crt-.*\.dll$`; **75 copies** (BUILDPE/EaseUS-x64, EUFileRepair, VideoViewer, root, diskhealth) — "verified: BUILDPE ships 49 forwarders" plus ucrtbase.dll as the real CRT target.
  - Per-set members: conio, convert, environment, filesystem, heap, locale, math, multibyte, private, process, runtime, stdio, string, time, utility (l1-1-0 each).
  - Consumer map spans virtually every EaseUS binary (search DLLs, PR DLLs/mo files, core exes, Qt, repair).
- **DW relevance**: static Rust binaries eliminate the whole shipped-CRT problem; only relevant if DW builds a WinPE variant.

### re-workspace/artifacts/runtime-families/vc-runtime-generations.md
- **Role**: Visual C++ runtime — three compiler generations.
- **Key facts**:
  - 23 copies: VC9 (msvcp90/msvcr90/mfc90u + Microsoft.VC90.CRT.manifest), VC12 (msvcp120/msvcr120), VC14 (msvcp140/vcruntime140/concrt140); "The codebase spans ~12 years of MSVC: VC9 (2008 — engine/legacy modules), VC12 (2013 — mid-generation), VC14 (2015-2022 — current)".
  - msvcm90.dll = .NET mixed-mode, live tree only — "one mixed-mode module".
  - Consumers span every subsystem (see map: AliyunWrap, search DLLs, PR modules, DRW*, repair, Qt5, Japanese.dll, etc.).
- **DW relevance**: no C++ runtime shipping in DW (pure Rust toolchain); the VC9 provenance of engine modules explains their RTTI-poor, template-heavy export style.

## Part 3 — reversed tooling reports (re-workspace/reversed/)

### re-workspace/reversed/ci-multitool/REPORT.md
- **Role**: Multi-tool batch report — drivers (despite being the root ci-multitool file, its title is "Multi-tool batch: drivers").
- **Key facts**:
  - Generated 2026-10-17 07:31 UTC; per binary: rabin2 info OK, FLOSS strings line counts, capa match counts, retdec "n/a", pefile OK.
  - eprdtdrv.sys: 522 FLOSS lines, 4 capa matches; eprdtdrv_win7older.sys: 504/4; ebrntdrv.sys: 504/4; epmdkdrv.sys: 510/4; EUDCPEPM.sys: 1299/7; EUEDKEPM.sys: 370/5; EuMrx.sys: 4059 FLOSS lines, 16 capa matches.
- **DW relevance**: tooling provenance only; confirms the driver set is the smallest-analyzed corpus (as expected for DW-excluded components).

### re-workspace/reversed/ci-multitool/core/REPORT.md
- **Role**: Multi-tool batch: core (DRW.exe, DRWService.exe, DRWReport.exe, InfoForSetup.exe, SetupUE.exe, DRWUI.exe).
- **Key facts**:
  - Generated 2026-10-17 09:06 UTC.
  - DRW.exe: FLOSS 1516 lines, capa 13, retdec 7007 lines; DRWService.exe: 635/12/4156; DRWReport.exe: FLOSS 1 line, capa **MISSING**, retdec n/a; InfoForSetup.exe: 2008/7/10076; SetupUE.exe: 1424/9/15831; DRWUI.exe: FLOSS 2 lines, capa **MISSING**, retdec n/a.
  - capa/FLOSS MISSING on the two biggest binaries (DRWUI, DRWReport) — tool limits, not absence of capability.
- **DW relevance**: shows which core binaries lack capa coverage — DW digests relying on Ghidra REPORTs for those.

### re-workspace/reversed/ci-multitool/repair/REPORT.md
- **Role**: Multi-tool batch: repair (10 binaries).
- **Key facts**:
  - Generated 2026-10-17 10:01 UTC.
  - EUImgRepair.exe 1860/17/52247; EUVideoRepair.dll 10795/13/161264; EUArchiveRepair.exe 3659/41/139629; EUAdobeRepair.exe 2784/35/95939; EuOfficeRepairWin32.exe 28277/27/n-a; EuRepairHost.exe 410/9/491; EURawImg.exe 8023/17/153368; EUUnZip.exe 2104/22/38350; EUImageEnhance.dll 11/33/n-a; EuOfficeTools.exe 14315/11/139360 (FLOSS/capa/retdec-lines).
  - EUImageEnhance has only 11 FLOSS lines (static OpenCV) yet 33 capa matches.
- **DW relevance**: repair-suite analysis provenance; the capa counts hint at network/crypto capability inside repair exes (cloud relay).

### re-workspace/reversed/ci-multitool/searchdll/REPORT.md
- **Role**: Multi-tool batch: searchdll (9 engines) + Local Analysis Addendum.
- **Key facts**:
  - Generated 2026-10-17 08:15 UTC; FLOSS/capa/retdec per engine: NTFSSearchDLL 6672/8/76572; FATSearchDLL 6285/12/80856; exFATSearchDLL 5748/8/77730; EXT4SearchDLL 2089/10/23594; HFSPlusSearchDLL 2052/2/28324; RAWSearchDLL 7705/15/94450; ReFSSearchDLL 2092/3/23658; BtrfsSearchDLL 2110/12/24538; ZfsSearchDLL 3522/4/50471.
  - Addendum capa digest table (rule counts): Btrfs 14 (IOCTL, ZLIB, RC4 PRGA, XOR, adler32, CRC32); EXT4 14 (IOCTL, RWX+indirect call JIT-style); FAT 15 (IOCTL, runtime linking, stackstrings, PEB access, PE header parse, ini); HFS+ 3 (minimal, library code); NTFS 10 (IOCTL @ 0x18002C0BA — raw disk access via EPMNtDrv, ini, memory capacity); RAW 16 (runtime linking, stackstrings, XOR, file R/W, ini — "carving engine w/ obfuscation"); ReFS 5 (OS version checks); Zfs 6 (delay/polling, file R/W); exFAT 10 (XOR, FNV hash, "parse credit card information" = capa regex F/P, actually carving signature scanning).
  - "ALL 9 engines: interact with driver via IOCTL confirms EPMNtDrv (\\.\EPRDTDRV) raw-sector access from user mode".
  - "RAWSearchDLL obfuscated stackstrings = signature/copyright strings hidden from casual strings dumps".
  - RetDec .c output retained (~2.5 MB per engine); .ll/.dsm/.bc trimmed for repo size.
- **DW relevance**: THE engine-family capability matrix — every DW FS/caving engine is user-mode and must NOT need the IOCTL driver; the per-engine capa notes flag patterns (RC4, RWX, stackstrings) DW should avoid.

### re-workspace/reversed/ci-multitool/partition/REPORT.md
- **Role**: Multi-tool batch: partition (11 PR DLLs).
- **Key facts**:
  - Generated 2026-10-17 10:34 UTC.
  - FLOSS/capa/retdec: PartitionRecovery.dll 19940/44/64199; MainModule.dll 40229/45/145245; RecoveryModule.dll 22085/48/112916; ToolsModule.dll **3 lines**/53/n-a; UILogic.dll 23000/50/123990; RawFixer.dll 17397/12/55075; Boot.dll 9052/37/27778; BootDriver.dll 2997/13/8149; BitLockerLib.dll 14039/29/87012; CloneModule.dll **18 lines**/51/194421; Win32Bcd.dll 4770/9/43831; libvmdk.dll 12175/20/65076.
  - ToolsModule and CloneModule have near-zero FLOSS lines (packed/stripped) yet high capa counts.
- **DW relevance**: capa-heavy partition DLLs (44-53 matches) are the deep-analysis targets for DW's partition engine; low-FLOSS + high-capa flags obfuscation in clone/tools.

### re-workspace/reversed/ghidra-repair/REPORT.md
- **Role**: Ghidra batch: repair — decompilation census (19 items).
- **Key facts**:
  - Generated 2026-10-17 08:45 UTC; entries: EUImgRepair.exe 564 fns/23,519 lines (600K); EUVideoRepair.dll 7,154/164,316 (3.7M); EUArchiveRepair.exe 3,338/117,834 (2.9M); EUAdobeRepair.exe 2,209/81,535 (2.0M); EuOfficeRepairWin32.exe 9,131/355,806 (9.4M); EuRepairHost.exe 50/985 (28K); EURawImg.exe 784/82,155 (2.8M); EUUnZip.exe 1,325/48,871 (1.2M); EUImageEnhance.dll 9,841/404,642 (11M); EUHwpFilePathCheckerModule.dll 668/29,099 (712K); EuOfficeTools.exe 3,633/111,636 (2.8M).
  - EUFileRepair sub-batch: EUAdobeRepair.exe 2,209/81,535; EUArchiveRepair.exe 3,338/117,834; EUImgRepairFile.exe 20,627/829,490 (22M); EuOfficeRepairFile.exe 42,215/1,641,232 (37M); EuPDFRepairFile.exe 4,649/133,274 (3.3M).
  - **NO OUTPUT**: `EUFileRepair/EUZipRepairFile.exe`, `EUFileRepair/EURarRepairFile.exe`, `EUFileRepair/EUVideoRepairFile.exe` — "check warnings" (three repair engines never decompiled).
- **DW relevance**: repair-suite size ranking (Office > Image > VideoEnhance > OfficeHost...); the three NO-OUTPUT engines are coverage gaps — DW specs for ZIP/RAR/video repair must come from other artifacts.

### re-workspace/reversed/ghidra-drivers/REPORT.md
- **Role**: Ghidra batch: drivers census.
- **Key facts**:
  - Generated 2026-10-17 07:43 UTC; eprdtdrv.sys 39 fns/2,192 lines (64K); eprdtdrv_win7older.sys 39/2,192 (64K); ebrntdrv.sys 39/2,192 (64K); epmdkdrv.sys 39/2,191 (64K); EUDCPEPM.sys 176/10,057 (292K); EUEDKEPM.sys 35/1,799 (52K); EuMrx.sys 405/23,980 (672K).
- **DW relevance**: the whole kernel-driver surface is only ~29K decompiled lines (minus EuMrx) — small because DW excludes it anyway.

### re-workspace/reversed/ghidra-drwui/REPORT.md
- **Role**: Ghidra batch: drwui (dedicated).
- **Key facts**:
  - Generated 2026-10-17 08:28 UTC; single entry: DRWUI.exe — 119,182 functions, 2,381,187 lines decompiled (51M).
- **DW relevance**: the giant main-UI corpus; DW deliberately rebuilds only the non-AI, non-ad subset of its 1,073 RTTI classes.

### re-workspace/reversed/ghidra-partition/REPORT.md
- **Role**: Ghidra batch: partition census (19 DLL/EXEs).
- **Key facts**:
  - Generated 2026-10-17 08:10 UTC; PartitionRecovery.dll 3,182/70,706 (1.7M); MainModule.dll 9,420/173,477 (3.7M); RecoveryModule.dll 8,811/91,972 (2.0M); ToolsModule.dll 17,876/187,572 (3.9M); UILogic.dll 9,516/103,011 (2.2M); RawFixer.dll 3,446/86,575 (2.3M); Boot.dll 843/33,066 (908K); BootDriver.dll 457/10,723 (264K); BitLockerLib.dll 2,320/83,772 (2.3M); CloneModule.dll 15,273/169,891 (3.7M); Win32Bcd.dll 996/38,357 (1.1M); libvmdk.dll 1,352/47,690 (1.3M); AutoUpdate.dll 506/22,678 (608K); GetDriverInfo.dll 207/3,865 (84K); Logger.dll 498/9,720 (240K); peformat.dll 207/4,058 (100K); WinChkdsk.exe 289/6,188 (156K); Burn.dll 1,029/9,707 (188K); setupdrvx64.exe 436/15,028 (372K).
- **DW relevance**: exact decompiled-volume map of the PR layer — the DW partition/recovery engine spec source list.

### re-workspace/reversed/ghidra-core/REPORT.md
- **Role**: Ghidra batch: core census.
- **Key facts**:
  - Generated 2026-10-17 07:57 UTC; DRW.exe 174 fns/5,661 lines (152K); DRWService.exe 118/3,031 (80K); DRWReport.exe 19,008/612,091 (16M); InfoForSetup.exe 404/13,717 (344K); SetupUE.exe 670/24,545 (640K).
- **DW relevance**: core-process size ranking: DRWReport dominates (MFC crash reporter); DW keeps a minimal crash handler.

### re-workspace/reversed/ghidra-wave2/REPORT.md
- **Role**: Ghidra batch: wave2 census (aux tree).
- **Key facts**:
  - Generated 2026-10-17 09:19 UTC; EuSendMessage.exe 33/640 (16K); DiskHealth.exe 4,636/93,316 (2.7M); EaseUS_VideoViewer.exe 685/15,269 (356K); VideoFileRepairer.exe 7,019/347,662 (9.2M); EaseUS_MediaGraph.dll 572/12,228 (288K); libssh2.dll 550/29,444 (876K).
- **DW relevance**: secondary-wave binaries (viewer/health/ssh); DW preview handled by webview.

### re-workspace/reversed/ghidra-searchdll/REPORT.md
- **Role**: Ghidra batch: searchdll census (9 engines).
- **Key facts**:
  - Generated 2026-10-17 07:50 UTC; NTFSSearchDLL.dll 1,931 fns/62,394 lines (1.6M); FATSearchDLL.dll 1,265/64,658 (1.8M); exFATSearchDLL.dll 1,299/58,043 (1.6M); EXT4SearchDLL.dll 475/18,071 (488K); HFSPlusSearchDLL.dll 665/22,304 (628K); RAWSearchDLL.dll 1,921/80,614 (2.2M); ReFSSearchDLL.dll 629/20,270 (536K); BtrfsSearchDLL.dll 452/18,626 (532K); ZfsSearchDLL.dll 981/45,364 (1.2M).
- **DW relevance**: per-engine effort sizing for DW's nine (optional) FS engines — NTFS/FAT/exFAT/RAW are the big four by volume.

### re-workspace/reversed/ghidra-misc/REPORT.md
- **Role**: Ghidra batch: misc census (21 items).
- **Key facts**:
  - Generated 2026-10-17 08:35 UTC; PE_Tools.dll 3,286/136,012 (3.7M); PubLog.dll 88/1,776 (44K); Japanese.dll 24/520 (16K); legacy.dll 275/7,080 (220K); archive.dll 1,353/89,903 (2.6M); cloud_session.dll 4,608/137,230 (3.5M); firebaseonlineconfig.dll 3,520/146,733 (4.2M); AliyunWrap.dll 2,045/79,448 (2.1M); AliyunWrapExe.exe 919/29,519 (756K); EuDownloader.exe 3,646/181,150 (4.6M); EuICloudRecovery.dll 10,964/682,377 (31M); EuUserRate.dll 4,731/198,259 (5.2M); EUinApp.exe 209/3,898 (104K); firebasefetch.exe 449/9,603 (252K); send_dmp_with_aliyun.exe 288/8,381 (264K); EUHwpViewer.exe 2,129/72,182 (1.8M); unins000.exe 4,389/148,953 (3.7M); EUOfficeViewer.exe 9,855/372,749 (9.7M); PDFViewer.exe 9,206/352,504 (9.3M); EUVideoRepair.dll 7,154/164,316 (3.7M).
  - **NO OUTPUT**: `EuSendMessage.exe` — "check warnings" (yet ghidra-wave2 did produce EuSendMessage output — 33 fns/640 lines).
- **DW relevance**: sizing for the misc family; EuICloudRecovery is the largest misc binary (31M decompiled) — cloud is expensive, another reason DW scopes it out.

## Cross-cutting observations

### Binary family map — which DLL/EXE owns which capability

- **Product shell (user-mode)**: `DRW.exe` (launcher watchdog; spawns/restarts DRWUI, crash handoff, drains the 2 MB PubLog shared log every 5 s) → `DRWUI.exe` (119,182 fns, 1,073 RTTI classes — all UI + orchestration + recovery manager) → `DRWReport.exe` (19,008 fns MFC crash reporter) → `send_dmp_with_aliyun.exe` + `AliyunWrap.dll`/`AliyunWrapExe.exe` (Aliyun OSS telemetry chain).
- **Recovery core (PartitionRecovery tree, all user-mode)**: `PartitionRecovery.dll` (PR core), `RecoveryModule.dll` (recovery engine), `MainModule.dll` (MOM host + pending-op partition engine shared with EPM: CGPTToMBR/CMBRToGPT/CCreate/CMoveResize operations), `ToolsModule.dll` (DM tools ops, 17,876 fns), `UILogic.dll` (PR UI logic), `RawFixer.dll` (transactional NTFS/FAT/exFAT structure repair), `CloneModule.dll` (clone + ISO9660 + WinPE/BCD builder), `Burn.dll`/`BootDriver.dll` (bootable media builder + IMAPI2 burn), `Win32Bcd.dll` + `Fixup.mo` (BCD surgery: `Bcd_*`, `CWin32BCDManager`), `WinChkdsk.exe` (delegates to real Windows chkdsk), `BitLockerLib.dll` + `bitlockerhelp.mo` + `DRWService.exe` (BitLocker: XTS-AES/FVE + service pipe `\\.\Pipe\DRWBitlocker`).
- **File-system engines — the SearchDLL family (9 DLLs, root tree)**: `NTFSSearchDLL`, `FATSearchDLL`, `exFATSearchDLL`, `EXT4SearchDLL`, `HFSPlusSearchDLL`, `ReFSSearchDLL`, `BtrfsSearchDLL`, `ZfsSearchDLL` + `RAWSearchDLL` (carving). Each exports exactly 2 factory functions `EUCreate<FS>Searcher` + `EUCreate<FS>QuickSearcher` (Btrfs: `EUCreateBtrfsComplateSearcher` sic) and shares the interface set `IFileSystemSearchController` / `IQuickFileSystemSearchController` / `ICompleteFileSystemSearchController` / `IDataRunList` / `IFileProperties` / `CFileNode` / `CFileSystemNode` / `CSearchedPartitionInfo` — plus per-FS classes (e.g. `CNTFSBuildVolumeBitmap`, `CFAT12/16/32Table`, `CExFatVolumeAnalyer`, `Ext4Volume`, `CHFSPlusCatalog`, `RefsSupperBlock`, `ZplNode_`). FATSearchDLL/exFATSearchDLL also embed the NTFS record classes (`CNTFSFileRecord*`) — shared base code.
- **MOM (Module Object Model)**: the `.mo` family (`Mom.mo`, `MainModule.dll` as host) — every module exports the `IObject` interface (`??0IObject…`, `AddRef`, `Release`, `GetInterfaceEx`) + `GetModule`; DLL plugin modules additionally export `TBCanUnloadNow`/`TBCreateObject` (Boot, BootDriver, Burn, GetDriverInfo, PE_Tools-family). Analyser modules (`*FileSystemAnalyser.mo` per FS) expose `IStorageDevice` vtables with `ConvertVolume`, `DeviceIoControl`, `GetBitlockerInfo`/`GetBitlockerStatus` — FS engines are BitLocker-aware at the device layer.
- **Repair suite (EUFileRepair + repair exes, x86 CONSOLE mostly)**: `EuOfficeRepairFile.exe` (42,215 fns, HWPX/HWP — biggest repair engine), `EUImgRepairFile.exe` (20,627 fns, LibRaw, 21 RAW formats + BMP/GIF/JPEG/JXL/PNG/SVG/WebP), `EUImageEnhance.dll` (OpenCV 2.4), `EUVideoRepair.dll` (MP4/MOV box grammar; export `GetIMovMp4Repair`), `EUArchiveRepair.exe` (7z next-header + RAR4/5/ZIP), `EuPDFRepairFile.exe` (xref surgery + decrypt), `EUAdobeRepair.exe` (PSD + cloud client), `EuOfficeTools.exe` (OOXML XPath + BIFF surgery), hosts/viewers `EuOfficeRepairWin32.exe`, `EUOfficeViewer.exe`, `EUHwpViewer.exe`, `PDFViewer.exe` (PDFium), `EUUnZip.exe` (repair verification by extraction), `EuRepairHost.exe` (50 fns licensing bridge). **Coverage gap**: Ghidra produced NO OUTPUT for `EUZipRepairFile.exe`, `EURarRepairFile.exe`, `EUVideoRepairFile.exe`.
- **Cloud**: `cloud_session.dll` (cpprestsdk OAuth2 loopback for Dropbox/Google/OneDrive; SRP-6a) with `cpprest141_2_10.dll` as its single-consumer HTTP engine; `EuICloudRecovery.dll` (7,064 exports, esicloud: idmsa/setup.ws/PCS + 13 data-class clients). Cloud stack is live-tree-only (absent from BUILDPE).
- **Telemetry/config**: `firebasefetch.exe` + `firebaseonlineconfig.dll` (remote config, `update.easeus.com.cn`, ~600 DRWUI feature flags), `EuUserRate.dll` (rating + telemetry), `AutoUpdate.dll`, `EuDownloader.exe` (component downloader).
- **Kernel drivers (all DW-excluded)**: `eprdtdrv.sys`/`ebrntdrv.sys`/`epmdkdrv.sys`/`eprdtdrv_win7older.sys` (EPMNtDrv raw-IO driver, `\Device\EPMNTDRV`, user path `\\.\EPRDTDRV\d\p`; 39 fns each; ebrntdrv == eprdtdrv_win7older by SHA), `EUDCPEPM.sys`/`EUEDKEPM.sys` (disk-copy hook: disk-class UpperFilter, all-IRP hook + VolSnap attach + IoctlBase-relative control codes 0x800C2B00+), `EuMrx.sys` (RDBSS NAS mini-redirector, anti-debug PEB check). `setupdrvx64.exe` installs them (`mod.WinDiskAccessDriverSetup`, versioned `%s%d.sys` layouts, "Boot File System" group).
- **Support libs**: `libvmdk.dll` (VMDK), `libssh2.dll` 1.11.0 (NAS agent transport), `libcurl.dll` (EUFileRepair-local, zero PE-import consumers — late-bound), `archive.dll` (libarchive, 427 exports), `legacy.dll` (libtomcrypt + `OSSL_provider_init`), `PubLog.dll` (shared-memory log bus, `DRWLOG_{1BFCF65F-...}`/`DRW_{1E956C11-...}` GUID namespaces), `PE_Tools.dll` (`pet_*` packet download/verify/install), `RegLib.mo` (offline registry hives by volume), `unins000.exe` (Inno Setup), `SetupUE.exe`/`InfoForSetup.exe` (installer), `Japanese.dll` (ja-JP PE-only resource satellite).

### SearchDLL engine family — details that matter for DiskWizard

- Uniform factory contract (2 exports per DLL) → DW should mirror with a single Rust trait + registry, one crate per FS, `quick` and `complete` search modes per engine.
- Shared abstractions to port: `IDataRunList` (+ resident/non-resident variants), `IFileProperties`, `CFileNode`/`CFileSystemNode` tree, `CSearchedPartitionInfo`, `ICompleteFileSystemSearchController`.
- RAWSearchDLL's registry API — `EUGetAnalyserList`, `EUGetAnalyserMap`, `EUGetAnalyserMultimap`, `GetRawUserInfoMap`, plus `EUCreateRAWSearcherForCellTest` and 198 RTTI analyser classes (`CADTSAnalyser`, `C7ZIPAnalyser`, `CDWGAnalyser`, `CGPXAnalyser`, `CHFPAnalyser`, `CFH11Analyser`, …) — is the carving signature registry: a multimap of (extension/format → analyser) consulted at scan time. DW's rawsearch registry design should be a data-driven table equivalent.
- Raw-sector access: the ci-multitool addendum states ALL 9 engines interact with a driver via IOCTL (`\\.\EPRDTDRV` = EPMNtDrv) — DW must obtain sectors in user mode (e.g., \\.\PhysicalDrive reads) since no kernel drivers are allowed.
- Capability cautions from capa: obfuscated stackstrings (FAT, RAW), XOR encode (RAW, exFAT, Btrfs), RC4 PRGA (Btrfs), RWX+indirect call (EXT4), PEB access (FAT) — none of these patterns belong in a DW Rust engine.
- Volume bitmap reconstruction during quick scan (`CQuickSearchAndBuildBitmap` exFAT, `CNTFSBuildVolumeBitmap` NTFS) is part of the engine contract.
- Engine size ranking (decompiled lines): RAW 80.6K > FAT 64.7K > NTFS 62.4K > exFAT 58.0K > ZFS 45.4K > HFS+ 22.3K > ReFS 20.3K > EXT4 18.1K ≈ Btrfs 18.6K.

### Rawsearch registry keys / namespaces / config keys seen in this corpus

- RAWSearchDLL reads `.ini` files (capa) — engine config is ini-driven, not registry-driven; no rawsearch registry keys appear in these 137 files (registry use in this corpus is: DRW.exe registry query, DRWService service persistence, setupdrvx64 `SYSTEM\CurrentControlSet\Services\%s`, EUDCPEPM `…\Services\…\Parameters` with `DeviceName=epmdiskcopydrv` + `IoctlBase=0x10000`).
- GUID namespaces (shared-memory/mutex IPC): `ALIYUN_{C69BDD99-9D0B-43f2-A794-9B70DAD4A96A}` (Aliyun telemetry IPC), `DRWLOG_{1BFCF65F-0766-4a71-B7A3-2EB2CD1412DE}` (log mutex), `DRW_{1E956C11-4D20-49d7-B4C7-016A342A2D39}` (log data view).
- Named pipes: `\\.\Pipe\DRWBitlocker` (FVE auto-unlock key service).
- Device paths: `\Device\EPMNTDRV` / user path `\\.\EPRDTDRV\d\p` (raw IO), `\Driver\VolSnap` (EUDCPEPM VSS hook target).
- IPC ports: 20171/20172 (ØMQ `VAPreviewListener` preview IPC).

### Licensing binaries

- `EuRepairHost.exe` — "Repair licensing host. Activation bridge to repair engines" — only 50 functions / 985 lines; x86 console.
- `EUinApp.exe` — "In-app purchase. Store/licensing bridge" (209 fns).
- openssl-crypto-trio maps the old OpenSSL generation (libeay32/ssleay32) to "legacy modules (EuRepairHost licensing, older engine code)"; 1.1 = cloud/licensing stack of 2019-era modules; 3.x = firebasefetch/cloud recovery.
- DRWUI RTTI shows the activation UI: `ActivateResultDialog`, `ActivateViewProxy`, `ApowerActivateDialog`, plus `AES_CBC256`.
- legacy.dll (libtomcrypt, PBKDF1/DES) is the plausible legacy-license-blob / old .rsf scan-session decryption substrate.
- DW takeaway: licensing in the original is thin, user-mode, and crypto-generation-scattered — DW should centralize on one modern crypto stack.

### Repair binaries — family summary

- Engines (CONSOLE, x86 mostly): Office (HWPX/HWP, 42,215 fns), Image (LibRaw-based, 20,627 fns), PDF (xref+decrypt, 4,649 fns), Archive (7z next-header, RAR4/5/ZIP, 3,338 fns), Video (box grammar, 7,154 fns), Adobe (PSD + cloud client, 2,209 fns), plus enhancement (OpenCV) and utilities (EuOfficeTools, EUUnZip verification).
- Hosts/viewers are separate MFC processes (EuOfficeRepairWin32, EUOfficeViewer, EUHwpViewer, PDFViewer/PDFium); repair tools are invoked plugin-style (libcurl loaded dynamically; window-IPC launch chain with `EUImgRepair_Wndname` + registry key launch path referenced in libcurl artifact).
- `EuRepairHost.exe` gates activation for repair engines.
- ImageMagick delegates (7 formats) + media codecs (libx265/de265/heif/postproc-58) support the repair/preview chain; VideoFileRepairer embeds faad2 (NeAACDec*) for AAC verification.

### Ghidra / ci-multitool tooling notes

- Ghidra batches: `ghidra-core` (5), `ghidra-drivers` (7), `ghidra-drwui` (1: DRWUI 51M chars), `ghidra-partition` (19), `ghidra-repair` (19 incl. 3 NO-OUTPUT), `ghidra-searchdll` (9), `ghidra-misc` (21, 1 NO-OUTPUT), `ghidra-wave2` (6). `ghidra-mo` and `ghidra-local` directories exist but produced no REPORT.md in the reversed tree. Raw outputs per binary: `<Name>_decompiled.c` + `_functions.txt` + `_imports.txt` + `_blocks.txt`.
- ci-multitool pipeline per binary: rabin2 info, FLOSS strings, capa rules, retdec (retained `.c` ~2.5 MB/engine; `.ll/.dsm/.bc` trimmed), pefile. capa/FLOSS went MISSING on the two biggest binaries (DRWUI.exe, DRWReport.exe) — tool limits.
- Timestamps: 2026-10-17 07:31→10:34 UTC across batches (drivers → searchdll → core → repair → partition wave order; ghidra batches 07:43→09:19).
- RetDec totals per family give an independent size cross-check matching the Ghidra counts.
- Coverage gaps to remember: `EUZipRepairFile.exe`, `EURarRepairFile.exe`, `EUVideoRepairFile.exe` (ghidra-repair NO OUTPUT), `EuSendMessage.exe` in ghidra-misc (but present in ghidra-wave2), and capa gaps on DRWUI/DRWReport.

### DiskWizard-relevant conclusions (mandate filter)

- IN (user-mode, portable): the SearchDLL engine family contract, RawFixer-style transactional FS repair, MOM pending-op partition engine patterns, Win32Bcd/Fixup BCD surgery, WinChkdsk delegation, RegLib offline hives, libvmdk VMDK input, BitLockerLib + DRWService pipe pattern (user-mode service), libssh2-style NAS transport, archive.dll-style preview extraction.
- OUT (mandate): all 7 kernel `.sys` binaries + their installers (setupdrvx64, pet_install_driver), EUDCPEPM volume-lock trickery (clone must use user-mode locking or refuse in-use volumes), all AI classes in DRWUI (`AIAssistantTipDialog`, `AIDeduplicationTipMenu`), the Aliyun/Firebase telemetry chains, and ad/retention UI (`AdToolPopWindow`, SetupUE geo/retention triggers).
- REPLACED BY TAURI/RUST: Qt5 core+QML stacks, D3D shims, MFC viewers (PDF/Office/HWP), three-generations OpenSSL and three HTTP stacks (cpprest/Qt5Network/libcurl) → one Rust HTTP/TLS stack.
