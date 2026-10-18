# Digest — MOM module system, PartitionRecovery, WinPE (Task 5-b)

Source: `/home/z/my-project/repos/easeus-drw-research/re-workspace/artifacts/{MOM-modules(32),PartitionRecovery(14),WinPE(3)}` — every file read completely. Facts below are verbatim from those artifacts (class names, constants, IOCTL codes, file counts). DW = DiskWizard (user-mode, driverless Rust/Tauri 2 reimplementation target).

---

## re-workspace/artifacts/MOM-modules/Mom.md
- **Role**: MOM (Module Object Model) core client-side glue `.mo` module.
- **Key facts**:
  - Decompilation `reversed/ghidra-mo/Mom_decompiled.c` — 6,720 lines, 331 functions.
  - No RTTI classes (C-style or stripped); vftable census: `CErrorObject` ×2, `CMyError` ×2, `IMomTest` ×1.
  - Embedded build path `i:\_drw_main\partitionrecovery\mod.mom\module.cpp`; imports kernel32 / api-ms-win-core-synch-l1-2-0.
  - Loaded by MainModule.dll via the MOM contract (name → `<Name>.mo` → LoadLibraryA → GetModule/CreateObject).
- **DW relevance**: The plugin-host runtime itself is Windows LoadLibrary machinery; DW replaces it with a static Rust workspace (crates + traits) — architectural reference only, not ported code.

## re-workspace/artifacts/MOM-modules/Common.md
- **Role**: shared utilities for the whole `.mo` module set.
- **Key facts**:
  - 13,642 lines / 617 functions.
  - RTTI: `CCommonModule`, `CConfig`, `CConfigSection`, `ConfigFormatError`, plus the whole Mom object-model template family (`?$CModuleImpl`, `?$CMomObject`, `?$CMomAggObject`, `?$CMomEventObject`, `?$CMomStackObject`, `?$CMomCloneObject`, functor objects).
  - No source path strings (release build); imports kernel32 + fstream STL family.
- **DW relevance**: confirms the shared config/error/object substrate; DW's equivalent is Rust serde config + `thiserror` taxonomy.

## re-workspace/artifacts/MOM-modules/Config.md
- **Role**: MOM module configuration persistence.
- **Key facts**:
  - 13,311 lines / 605 functions (near-identical census to Common.mo — shared code family).
  - Classes: `CConfig`, `CConfigModule`, `CConfigSection`, `ConfigFormatError`; INI-style section model implied by CConfigSection ×4 instantiations.
  - No build-path strings.
- **DW relevance**: config persistence patterns map to a TOML/JSON file in DW; no algorithm content worth porting.

## re-workspace/artifacts/MOM-modules/Thread.md
- **Role**: worker-thread infrastructure for modules.
- **Key facts**:
  - 6,676 lines / 342 functions.
  - Vftable census: `CSyncObjectImpl` ×12, `CCriticalSection` ×5, `CMutex` ×3, `CEvent` ×1, `CTimer` ×2, `CThread` ×2 — a complete Win32 sync wrapper kit.
  - No build-path strings.
- **DW relevance**: threading model only; DW uses Rust std/rayon — reference for progress-callback concurrency shape, no port.

## re-workspace/artifacts/MOM-modules/Error.md
- **Role**: error codes/objects shared across modules.
- **Key facts**:
  - 5,615 lines / 282 functions.
  - RTTI: `CErrorManager`, `CErrorModule`, `CErrorTrackManager` + interfaces `IErrorBase`, `IErrorManager`, `IErrorModule`, `IErrorProc`, `IErrorTrackManager` — error tracking is a tracked/propagated object model, not just codes.
- **DW relevance**: the "error track manager" pattern maps to a Rust error-type hierarchy + operation error log; user-mode portable concept.

## re-workspace/artifacts/MOM-modules/Log.md
- **Role**: module-side logging sink (EPMLOG).
- **Key facts**:
  - 3,460 lines / 139 functions.
  - Classes: `IEsLog` ×3, `CFileMappingLog` ×2, `CMemMonitorManager` ×2 — file-mapping-backed log + a memory monitor.
  - String evidence: `\EPMLOG.log`, `\HD%03d.dpt`, `fsclog.dll`, ntdll.dll.
- **DW relevance**: log-file naming and memory-monitor concept portable to Rust `tracing`; `\HD%03d.dpt` per-disk data file is vendor-specific persistence worth noting for scan-session file design.

## re-workspace/artifacts/MOM-modules/CallbackOperator.md
- **Role**: MOM event/callback operators.
- **Key facts**:
  - 2,831 lines / 110 functions; largest function 5,237 lines (census outlier).
  - Only RTTI: `struct IObject` — this module IS the base event contract (COM-like IObject family).
  - No file/registry literals extracted.
- **DW relevance**: the `INotifyObserver`/callback plumbing maps to Rust channels/callbacks; per mom-modules UI-bridge file, CallbackOperator.mo is one of the two DAG roots every module depends on.

## re-workspace/artifacts/MOM-modules/CheckVersion.md
- **Role**: finds installed Windowses in offline hives for boot repair.
- **Key facts**:
  - 16,170 lines / 539 functions.
  - Classes: `CheckVersion(I)`, `DiskContainer(I)`, `FindPartitionPredicatorI` ×5, `IsWindowsServerInstalled` ×3, `RegistryReader`, `RegValue(I)`, `Partition(I)`.
  - Strings: `\\$$$PROTO.HIV`, `\\CONTROLSET00`, `\\CONTROL\PRODUCTOPTIONS\PRODUCTTYPE`, `\\SELECT\CURRENT`, `\\windows`, `\\winnt`, `\\.\PhysicalDrive`, `checkversion.log`.
  - Workflow: enumerate `\\.\PhysicalDrive` → per-partition hive read → ControlSet select → ProductType → server-vs-client verdict.
- **DW relevance**: fully user-mode replicable (read hives from partitions, detect OS type) — directly feeds a DW "boot-repair guidance" feature; $$$PROTO.HIV/SELECT\CURRENT keys are the concrete recipe.

## re-workspace/artifacts/MOM-modules/Device.md
- **Role**: disk/partition device objects (the device layer that talks to drivers).
- **Key facts**:
  - 68,987 lines / 3,474 functions — largest module of the infra family.
  - Classes: `CWindowsStorageDevice`, `CDiskEntry`, `CStorage{Volume,Disk,PhysicalDisk,VirtualDisk,Object,ResiliencySetting,Pool,SubSystem}`, `CWindowsDynamicVolumeDevice`, `CDiskDriveLayoutReadWriter` (raw layout R/W), `BitLockerManager`.
  - String evidence: `\\.\EPRDTDRV\%d\%d`, `\\.\EPRDTDRV\%d\Volume\%d`, `\\.\HarddiskVolume%d`, `\\.\PHYSICALDRIVE%d`, `\\.\ROOT\microsoft\windows\storage:MSFT_{Disk,PhysicalDisk,StoragePool,StorageSubSystem,VirtualDisk,Volume}` (WMI storage namespace objects), `fsclog.dll`.
  - Build path: `i:\_drw_main\partitionrecovery\legacy_mod\include\momnew.h`.
- **DW relevance**: the WMI/`\\.\PhysicalDrive%` enumeration half is user-mode portable and IS DW's device layer; the `\\.\EPRDTDRV\...` raw paths are kernel-driver-bound — DW uses direct volume/disk handles instead (documented contradiction to copy).

## re-workspace/artifacts/MOM-modules/DeviceAdapter.md
- **Role**: adapts Win32 devices to the MOM device model.
- **Key facts**:
  - 2,599 lines / 126 functions.
  - RTTI: `CDiskDevice`/`CPartitionDevice` + interfaces `IBlockDevice`, `IDiskDevice`, `IDiskGeometry`, `IFileSystemPartition`, `IFileSystemPartitionExtendInfo(Editor)`, `IPartitionEntry`, `IStorageDevice`, `IVolumeDevice`, `IVolumeEntryInfo`.
  - Vftable census: `IDiskDevice` ×5, `IPartitionDevice` ×4.
- **DW relevance**: the cleanest interface inventory in the set — a ready-made Rust trait list for DW's device/partition abstraction (`BlockDevice`, `DiskGeometry`, `PartitionEntry`…).

## re-workspace/artifacts/MOM-modules/DeviceManager.md
- **Role**: device enumeration/coordination for the module system.
- **Key facts**:
  - 70,679 lines / 3,428 functions.
  - Classes: `CPartitionManager` ×9, **`CPartitionManagerGPT` ×9**, `CVolumeManager`, `CManagerItem`, `CPartitionBootInfo`, `CDeviceUtils`, `CBaseReg`.
  - String evidence: `\Boot\bcd`, `\ControlSet001`, `\Device\Harddisk%d\Partition%d`, `\DosDevices\%C:`, `\MountedDevices`, `\Registry\Machine\SYSTEM\Setup`, `\GLOBAL??\%s`, `\StringFileInfo\%08lx\{CompanyName,FileDescription,FileVersion,InternalName}`.
  - Per mo-module-map: also embeds `CTbFile` (Todo-Backup .tbi image wrapper) and `CUpdateGptSystemBootPartition`.
- **DW relevance**: enumerate/mount-point/BCD-awareness logic is user-mode portable (registry + `\\?\` namespace); CTbFile image-as-device concept maps to DW scanning image files (a feature decision, not a driver need).

## re-workspace/artifacts/MOM-modules/Partition.md
- **Role**: partition objects + table parsing (MBR/GPT).
- **Key facts**:
  - 38,246 lines / 2,179 functions.
  - Classes: `CPartitionEntryGPT` ×16, `CPartitionEntry` ×8, `CFileSystemPartition` ×6, `CRangeDevice` ×6, `CMBR` ×3, **`CExtendMBR` ×2** (extended/EBR chain), `IFileSystemPartition(Editor)`, `CDiskDeviceType`.
  - Build path `legacy_mod\include\momnew.h`; imports fsclog.dll.
- **DW relevance**: the MBR/EBR/GPT parse+model layer is the heart of DW's lost-partition scan — 100% user-mode implementable in Rust (2,179 vendor functions = scope ruler).

## re-workspace/artifacts/MOM-modules/PartitionRecoveryD.md
- **Role**: the orchestrating module of the lost-partition scan.
- **Key facts**:
  - 28,463 lines / 1,571 functions.
  - Classes: `CRecoverySearchEngine` ×18, `IRecoverySearchEngine` ×10, `IBlockParser`, `CBlockParserLinux`, `CBlockParseWin32`, `CExtXBlockParser`, `CFatxBlockParser`, `IRealFATTable`, `CGeneralGeneratorSetImpl`, **`CGeneratorSetCompleteSearch`**, **`CGeneratorSetRegSearch`**, `IBlockparserSet`, `ILocationGeneratorSet`.
  - String evidence: **`lostpart.dat`**, **`partinfo.dat`** — persisted lost-partition and partition-info scan state.
  - Per mo-module-map also contains `BitlockerParser`, `CFAT12/16/32Table`.
  - "Location generator sets" = candidate-offset generators; "CompleteSearch" vs "RegSearch" = exhaustive vs registry-guided scan strategies.
- **DW relevance**: THE lost-partition engine to reimplement — all user-mode (read raw sectors, generate candidate locations, parse block headers); `lostpart.dat`/`partinfo.dat` are the vendor's scan-result persistence format names.

## re-workspace/artifacts/MOM-modules/FileSystem.md
- **Role**: filesystem abstraction layer (volume/file objects).
- **Key facts**:
  - 17,658 lines / 547 functions.
  - Classes: `IFSInterface` ×15, `CExt2File` ×14, `CExt4File` ×12, `CWindowsFS` ×3, `CExFatFs`/`CExFatPartition`, `CExt2FS`/`CExt4FS`, `CFATDBR`, `CFATFS`, `CFATTable`, `CNTFSInterface`.
- **DW relevance**: the per-FS implementation-set behind the analyser interface — user-mode; DW already has ext/HFS+/ReFS engine tasks (QA-3) covering part of this surface.

## re-workspace/artifacts/MOM-modules/Fixup.md
- **Role**: repairs damaged partition tables (write side).
- **Key facts**:
  - 7,502 lines / 243 functions.
  - Classes: `CBcd` ×6, `CEFIFirstBcd` ×3 — BCD store surgery (per mo-module-map: "BCD repair/editing post-recovery & for boot media").
  - String evidence: `\ArcName`, `\BcdLog.txt`, `\Device\Harddisk%d\Partition0`, `\Registry\Machine\BCD00000000`, `\\.\PHYSICALDRIVE%d`.
- **DW relevance**: BCD/EFI boot repair is user-mode-possible (registry hive BCD00000000 + boot files); the `\Registry\Machine\BCD00000000` mount point is the concrete how-to.

## re-workspace/artifacts/MOM-modules/FileSystemAnalyser.md
- **Role**: the IFSAnalyser interface + shared analyser logic (read-side validation).
- **Key facts**:
  - 5,336 lines / 265 functions; `CFileSystemAnalyser` ×2 + `IPartitionDevice`.
  - Per mom-module-system architecture: "FileSystemAnalyser family validates each candidate (boot sector, superblock, MFT sanity)".
- **DW relevance**: the validation-pass trait — every candidate partition found by scan goes through a per-FS sanity check; direct Rust trait equivalent.

## re-workspace/artifacts/MOM-modules/NTFSFileSystemAnalyser.md
- **Role**: IFSAnalyser implementation for NTFS.
- **Key facts**: 6,698 lines / 325 functions; single worker class `CNTFSVolumeAnalyser` ×5; "scans a found partition and reconstructs its file tree for partition-recovery verdicts".
- **DW relevance**: NTFS candidate validation (boot sector + MFT sanity) — user-mode, pairs with DW's NTFS engine.

## re-workspace/artifacts/MOM-modules/FATFileSystemAnalyser.md
- **Role**: IFSAnalyser for FAT12/16/32.
- **Key facts**: 6,701 lines / 322 functions; `CFatVolumeAnalyser` ×6 only.
- **DW relevance**: FAT validation pass — user-mode.

## re-workspace/artifacts/MOM-modules/ExFatFileSystemAnalyser.md
- **Role**: IFSAnalyser for exFAT.
- **Key facts**: 6,162 lines / 310 functions; `CExFatFileSystemAnalyser` ×5 only.
- **DW relevance**: exFAT validation pass — user-mode (note DBR checksum validation exists on RawFixer side too).

## re-workspace/artifacts/MOM-modules/ExtFileSystemAnalyser.md
- **Role**: IFSAnalyser for ext2/3/4.
- **Key facts**: 6,021 lines / 297 functions; `CExtVolumeAnalyser` ×5 only.
- **DW relevance**: ext validation pass — user-mode; overlaps DW ext engine work.

## re-workspace/artifacts/MOM-modules/RefsFileSystemAnalyser.md
- **Role**: IFSAnalyser for ReFS (MSB metadata structures).
- **Key facts**:
  - 42,013 lines / 1,752 functions — by far the largest analyser.
  - Classes: `CRefsLogFileProc`, `CRefsMetaDirectory`, `CRefsFileProc`, `CRefsChildMsbProc`, `CRefsVaryChildMsbProc`, `CRefsChildMsbDupProc`, `CRefsMetaFileDataRun`, `CRefsMetaFileDirItem`, `CRefsMetaSymbolicDirItem`, `CRefsVolumeInfoProc`, `CRefsVolumeAnalyser`, `CRefsBitmapProc`, `CRefsContainerTable`, `CRefsFileItemEntry`, `CRefsAppDataRunItem` (per map: MSB = metadata switch blocks).
  - Build paths: `mod.librefs\refsmetaobjproc.cpp`, `mod.librefs\refsmsbitem.cpp`.
- **DW relevance**: ReFS MSB walk is user-mode parsing (relevant to DW's ReFS engine task); the 42K-line size shows ReFS is the heaviest analyser in the family.

## re-workspace/artifacts/MOM-modules/NTFSLib.md
- **Role**: NTFS on-disk structure library (MFT records, attributes, runlists) — the PartitionRecovery monorepo copy (separate from NTFSSearchDLL).
- **Key facts**:
  - 25,791 lines / 832 functions.
  - Classes: `CAttributeI` ×10, `CRWContextI` ×10, `CAttributeBase`, `CAttributeReadWriteBase`, `CRecordI`, `CRecordAttributeReader/Writer`, `CResidentAttribute`, `CNoresidentAttribute`, **`CExtendMFT`**, `CMultiRecordWrapper`, `ChunkCacheImplPiece`.
  - Build paths: `mod.ntfslib\{extendmft,mftwith0x20,ntfslib,ntfsmft,recordattributerw}.cpp`; string `\\?\GLOBALROOT\Device\Harddisk%d\Partition%d`.
- **DW relevance**: the MFT/attribute/runlist decode layer — user-mode, reusable knowledge for DW NTFS deep scan; the GLOBALROOT path form works from user mode too.

## re-workspace/artifacts/MOM-modules/NTFSUtil.md
- **Role**: high-level NTFS queries + EPM resize/move/merge workflows.
- **Key facts**:
  - 132,414 lines / 2,871 functions — biggest module in the set.
  - Classes: `CNtfsBaseItem`, `CNtfsFileItem`, `CVolumeInfo`, `CFatVirtualBaseItem` (per map: `CFatVirtual{View,BaseItem,DirItem,FileItem}` — virtual FS views over damaged disks), `CHardDriveIOEx`, `CFirst/NewVolumeMergeDeal`, `COld/NewVolumeResizeMoveDeal`, `CNtfs2FATWorkFlow`, `CResizeMoveAlgorithm`.
  - Build paths (17): `mod.ntfsutil\{attributeparser,cntfsclusterresize,decompress,harddriveioex,ntfsmerge,ntfsresizemove,resizemovebyrebuildingbitmap,...}.cpp`.
  - Strings: `pagefile.sys`, `new.bin`, `old.bin`, `fsclog.dll`.
- **DW relevance**: mostly EPM partition-editing (resize/merge/convert) — out of DW's data-recovery scope; `CFatVirtualView` (consistent in-memory FS over damaged content) is a portable, DW-relevant pattern.

## re-workspace/artifacts/MOM-modules/FatLib.md
- **Role**: FAT structure library (BPB/FAT table/dir entry primitives).
- **Key facts**: 8,953 lines / 311 functions; `IDirectoryEntry` ×4, `IFileAllocateTable` ×4, `CDirectoryEntry`, `CFileAllocateTable`, `IPartitionDevice`.
- **DW relevance**: FAT chain/entry primitives — user-mode, feeds both validation and RawFixer-style repair in DW.

## re-workspace/artifacts/MOM-modules/FatFormat.md
- **Role**: partition formatting support (EPM heritage).
- **Key facts**: 5,719 lines / 238 functions; `CFatFormat` ×3, references `IDiskDevice`/`IPartitionDevice`; largest functions 1,609 / 1,581 / 1,042 / 926 lines (the format writers).
- **DW relevance**: formatting is EPM territory, not DW recovery scope; only relevant if DW ever needs to prepare target media.

## re-workspace/artifacts/MOM-modules/LdmManager.md
- **Role**: Windows LDM dynamic-disk config parsing (spanned/striped/mirrored/RAID-5).
- **Key facts**:
  - 91,475 lines / 3,622 functions — second-largest module.
  - Classes: `MirrorVolumeStorage`, `Raid5VolumeStorage`, `SimpleVolumeStorage`, `SpanVolumeStorage`, `StripeVolumeStorage`, `Win32VolumeStorageDevice`, `DmrpVolume`, `Disk_EP_Cache`, `ILdmDisk/ILdmVolume`, `EditLdm2*` (delete/resize/move/update LDM database).
  - Build paths: `mod.ldmmanager\{dmrpdisk,dmrpmodule,dmrpvolume,editldm2*,ldmmanager,nativemodule,vdsdisk,vdsmodule,vdsnotify,vdsvolume}.cpp` — includes VDS interop.
- **DW relevance**: LDM database parsing is user-mode (see module-internals-wave2 for signatures) — DW could surface dynamic-disk volumes read-only; the EditLdm2 write side is out of scope.

## re-workspace/artifacts/MOM-modules/RegLib.md
- **Role**: offline registry HIVE parser (reads Windows state without booting it).
- **Key facts**:
  - 10,974 lines / 302 functions.
  - Classes: `Node`, `Key`, `SubValList`, `Hive`, `Indirect`, `IndxLeaf`, `FastLeaf`, `HashLeaf`, `Security`, `Val`, `MidData`, `BigData` — the full regf/hbin/leaf cell model.
- **DW relevance**: fully user-mode; needed if DW wants offline-OS/BitLocker-key/BMR detection — pairs with CheckVersion and bitlockerlib artifacts.

## re-workspace/artifacts/MOM-modules/bitlockerhelp.md
- **Role**: BitLocker detection/unlock assistance in the module system.
- **Key facts**:
  - 34,879 lines / 1,239 functions; `CBitlockerHelpModule`/`IBitlockerHelpModule` only.
  - Strings: `C:\Program Files\EaseUS\BL`, `x:\Program Files\EaseUS\BL` (PE path), `\ControlSet001\Control\FVEAutoUnlock\`, `\\.\root\CIMV2\Security\MicrosoftVolumeEncryption` (WMI), `\system32\bitlockerwizard.exe %1 U` and `%1 Y`, `temp.dat`.
- **DW relevance**: the module is mostly Windows-integration glue (WMI/FVEAutoUnlock/bitlockerwizard) — DW's driverless stance can still read `-FVE-FS-` volumes and offline FVE keys via user-mode parsing (see bitlockerlib digest).

## re-workspace/artifacts/MOM-modules/EFIBoot.md
- **Role**: EFI NVRAM entries, BCD stores, boot entry repair.
- **Key facts**: 6,018 lines / 189 functions; `CEFIBoot`, `CEFIBootModule`, `IEFIBootModule` — small module; function census shows several 500-700-line functions (the NVRAM/BCD writers).
- **DW relevance**: EFI NVRAM/BCD manipulation is user-mode possible (admin); relevant only if DW adds boot-repair guidance.

## re-workspace/artifacts/MOM-modules/FileSystemCheck.md
- **Role**: validates found partitions' filesystem consistency (chkdsk-style).
- **Key facts**:
  - 9,964 lines / 419 functions.
  - Classes: `CFatChain` ×14, `CCheckFAT`, `CCheckNTFSVolume`, `CFileAllocationTable`; build paths `mod.filesystemcheck\{checkfat,checkntfs}.cpp`.
  - String evidence: **`WinChkdsk.exe`** — shell-out to Windows chkdsk exists in this path.
- **DW relevance**: consistency checks (FAT chain, NTFS volume) are user-mode; the WinChkdsh.exe delegation is a Windows-only convenience DW should NOT copy (Rust-native checks instead).

## re-workspace/artifacts/MOM-modules/module-internals-wave2--ldm-reglib-ctbfile.md
- **Role**: algorithm internals wave — LDM database parser, RegLib hive engine (read+write), CTbFile image layer.
- **Key facts**:
  - **LDM VMDB parser** (FUN_180072790): signature `0x564D4442` ("VMDB"); version must be 4.10 (u16@+0x12==4, u16@+0x14==10) else abort; disk_group_id BE-swapped u64@+0x75; status u16@+0x10 (1=healthy); vblk count/offset/size at +0x08/+0x0C/+0x10, VBLK offset expected 0x200; TOCBLOCK signature `0x544F43424C4F434B`, **three-location search `[toc1+2, toc1, toc1+1]`**, first bitmap must be "config", second "log"; PRIVHEAD chain validation ("size doesn't match real disk size", "invalid GUID").
  - **RegLib**: `RegAPI::OpenHiveByVolumeW` — signature `0x66676572` ("regf") at hive+0x20, checksum at +0x21C, fail → return 5 (ERROR_ACCESS_DENIED); hbin (`0x6E696268`) constructor writes `_time64()` stamps, 0x1000-granular bin sizes — i.e. it can WRITE/repair hives, not just parse.
  - **CTbFile** in DeviceManager.mo: `{ vftable; qword -1; dword 0; ptr 0; dword 1 }` — .tbi (Todo Backup image) file wrapper surfacing backup images as storage devices.
  - Confidence 100% [CODE]; CTbFile product-role sentence marked INFERRED.
- **DW relevance**: exact LDM VMDB/TOCBLOCK signatures and regf offsets are directly reusable in DW's Rust LDM/hive readers — all user-mode parsing; CTbFile motivates a DW image-file scan feature.

## re-workspace/artifacts/MOM-modules/mom-modules--ui-bridge-and-dependency-matrix.md
- **Role**: engine→UI message bridge decode + cross-module dependency matrix + coupling census.
- **Key facts**:
  - **Message path**: engine thread → MOM service hub (`FUN_14026b8d0` singleton) → message-service → `CMessageProxy` (singleton, multiple inheritance QObject + `INotifyObserver` at vtable+0x10) → notify shim constructs a stack `CMessageSend` (24 B envelope) → `CMessageSend::dispatch(int, void*, int&)` Qt signal → `CMessageProxy::doDispatchMessage` → 15 UI subscriber classes.
  - **Thread safety**: ctor probes `QThread::currentThread() == QCoreApplication::instance()->thread()`; same thread → direct connect, other thread → queued-style marshaling (slot strings `2dispatch(int, void*,int&)` / `2doDispatchMessage(int, void*,int&)` byte-exact at RVA 0xe474d0/0xe47488); envelope is create→emit→destroy per message; symmetric disconnect before dtor.
  - **Import matrix** (all 37 binaries x64, machine 0x8664): ToolsModule.dll/PartitionRecovery.dll/CloneModule.dll import 7 modules each; MainModule 6; RecoveryModule/UILogic 5; DeviceManager.mo imports 6 (FatLib, NTFSLib, DeviceAdapter, Partition, RegLib, …).
  - **Centrality**: `Log.mo` imported by 15+, `CallbackOperator.mo` 8, `NTFSLib.mo` 5, DeviceAdapter/Fixup/FatLib 4-5, Partition.mo 3 — DAG rooted at Log + CallbackOperator, no cycles.
  - **Two coupling styles**: 26 of 31 `.mo` export factory `GetModule(HMODULE)` (runtime name-keyed, lazy); 5 export NO factory and are link-time shared libs — CallbackOperator.mo (29 C++ exports), DeviceAdapter.mo (76), Log.mo (37), Fixup.mo (36), **NTFSLib.mo (310)**; `IObject` vtable symbol family exported by every module.
- **DW relevance**: this is the canonical pattern for DW's Tauri bridge: Rust engine threads → serialized progress events → UI thread via Tauri events (the CMessageProxy thread-affinity marshaling is exactly the problem Tauri's event system solves); dependency DAG informs DW crate layering.

---

## re-workspace/artifacts/PartitionRecovery/mom-module-system--architecture.md
- **Role**: master architecture of the MOM engine (module system + host DLLs + workflow).
- **Key facts**:
  - `.mo` = PE DLLs ("Module Object") loaded by name: `FUN_1800017f0("DeviceManager")` → name+".mo" (`appends '.', 'm', 'o'` at 0x180001d80+40) → `LoadLibraryA` → `GetProcAddress("GetModule")` (line 571, REQUIRED export) → `GetModule(HMODULE)` self-registration; `CreateObject` per-class factory; path resolution tries `<exe_dir>\RecoveryModule.dll` then `<exe_dir>\PartitionRecovery\RecoveryModule.dll`; lazy + singleton-cached (DAT_ globals).
  - Object-model skeleton (RTTI-verified, all 31 modules): `CModuleImpl<T>`, `CMomObject` (IObject AddRef/Release/QueryInterface), `CMomEventObject`, `CMomStackObject`, `CMomAggObject` (XPCOM-style tearoffs), `CMomCloneObject`, functor system, `CErrorObject`.
  - Host DLLs: PartitionRecovery.dll (engine host, exports `CPartitionDevice`, `IDiskDevice`, `CCallBackOperate`, `CLogWriter`, `CLogInBootICE`), MainModule.dll (268 mangled op-class symbols — the partition-editing engine), RecoveryModule.dll (scan orchestration), ToolsModule.dll (2.5 MB; WinPE media building, burning, imaging), UILogic.dll (1.4 MB).
  - Internal build path `I:\_drw_main\PartitionRecovery\Output\Release_x64`; shared lineage with EPM (PDB `T:/source/repos/_EPM_main/legacy_mod/BitLockerLib`).
  - Workflow: DeviceManager enumerate (WMI + setupapi + IoCtl) → Partition.mo parse (MBR EBR chains, GPT) + LdmManager dynamic + lost-partition detection = **scan unallocated space for partition signatures** → FS validation → PartitionRecoveryD rebuilds partition records → "writes new table via MainModule.dll ops" → post-recovery repair (MBR↔GPT, boot repair CBcdAccess/GenUEFISystemFile).
  - "All device I/O goes through Device.mo → EPMNtDrv driver (\\.\EPRDTDRV\{d}\{p}) when raw access is needed."
- **DW relevance**: the whole orchestration EXCEPT the EPRDTDRV raw-IO leg is user-mode-replicable; the "scan unallocated space for partition signatures → validate → rebuild table" pipeline is the spec for DW's lost-partition feature; MainModule write-ops map to DW transactional table writes (see pending-op engine).

## re-workspace/artifacts/PartitionRecovery/mo-module-map.md
- **Role**: complete RTTI-verified module inventory/roles (companion to the architecture doc).
- **Key facts**:
  - 30 modules, 689K lines total; per-module function counts (Device 3,474; DeviceManager 3,428; LdmManager 3,622; Partition 2,179; PartitionRecoveryD 1,571; NTFSUtil 2,871; RefsFileSystemAnalyser 1,752; …).
  - PartitionRecoveryD block parsers: `CBlockParseWin32`/`CBlockParserLinux`/`CExFatBlockParser`/`CExtXBlockParser`/`CFatxBlockParser` (**FATX — Xbox-original FS**), `CFAT12/16/32Table`, `BitlockerParser`, `CGeneratorSetCompleteSearch`.
  - Architectural insights (verbatim claims): (1) two-layer recovery — FileSystemAnalysers recognize/assess → PartitionRecoveryD block parsers deep-scan → engine DLLs (SearchDLL) present results; (2) Windows-aware recovery via RegLib + CheckVersion (IsWindowsServerInstalled) + PartitionBootInfo; (3) dynamic disk (LDM) support "most consumer tools cannot touch"; (4) disk images first-class via CTbFile ("tb镜像mount" origin); (5) virtual FS views `CFatVirtualView/DirItem/FileItem`; (6) boot repair via Fixup.mo + EFIBoot.mo + Win32Bcd.dll + `CUpdateGptSystemBootPartition`; (7) FATX legacy surface.
- **DW relevance**: the module role table is DW's crate/feature checklist; two-layer (validate + deep-parse) and Windows-aware recovery are the two design patterns to adopt.

## re-workspace/artifacts/PartitionRecovery/pending-operation-engine.md
- **Role**: the transactional partition-editing engine shared with EPM (why a recovery product contains a partition manager).
- **Key facts**:
  - Rationale: whole-disk recovery, recover-to-new-disk, bootable media, system rescue all need repartition/copy/convert/boot-repair — "every user-visible action becomes a 'pending operation' that is composed, previewed, then applied — with undo semantics."
  - Architecture: Controller (validate+configure) → Operation (plan) → PendingOperation (execute/rollback), e.g. `CChangeLetterController → CChangeLetterOperation → CChangeLetterPendingOperation`; queue = `COperationManager` + iterator; composition = `CPendingOperationComposition`; base = `CPendingOperationCommon/Ex`; dynamic factories `CCreatePendingOperationDynamic`, `CMoveResizePendingOperationDynamic`; `CNullPendingOperation` placeholder; `IControllerEx`.
  - Operation vocabulary (RTTI, ToolsModule): CCreate, CDeletePartition, CDelAllPartition, CDiskRepartition, CMoveResize(+Dynamic), CSplitSpace, CAllocateSpace [vendor typo "Alloacte"], CMergePartition, CPartAlign, CChangeLetter, CPartitionLabel, CHide/CUnhide, CSetActive, CFormatPartition, CClusterSizeChange, CConvert2{Ntfs,Fat,Exfat}, CConvert2Dynamic, CInitToGPT, CInitToMBR, **CRebuildMBR**, **CMBRGPTBcdProc**, CCopyDisk{,Initialize,Finalize}, CCopyPartition, CWipeData{Disk,Partition}, **CExtendBootPart**, **CBRSysPartProc**, CBcdBootProc, CBcdEditProc, CBcdAccess, CBootExecute, **CGenUEFISystemFile/CGenBIOSSystemFile**, CChangeBIOSReg, **CRepairRaid5**, **CBitlockerTurnOff**, CSmartRM.
  - Shared epmpublic utilities in all four DLLs: `CAccessControlList`/`CAccessEntry`/`CFileDaclProc`/`CFileSecurityInfo` — "recovered files keep their original ACLs"; CBPIniParser, CConsoleProc.
  - Recovery tie-ins: CDiskRepartition + CCopyDisk (whole-disk), peandbcdproc/peandiniproc (bootable media), CExtendBootPart + CRebuildMBR + Fixup (system rescue), CBitlockerTurnOff (encrypted volumes), CRepairRaid5Controller (RAID-5 + LdmManager).
  - capa corroboration: MainModule/CloneModule/PartitionRecovery.dll match "delete drive layout via IOCTL" (IOCTL_DISK_DELETE_DRIVE_LAYOUT).
- **DW relevance**: the Controller→Operation→PendingOperation transactional pattern (compose → preview → apply → undo) is the model for DW's partition-table restore queue; ACL preservation during copy is a user-mode feature DW can adopt.

## re-workspace/artifacts/PartitionRecovery/rawfixer-fs-repair.md
- **Role**: RawFixer.dll — the pre-scan filesystem repair engine (repairs FS metadata so structured scans can proceed).
- **Key facts**:
  - Binary: 3,446 functions / 86,575 lines; source `i:\_drw_main\partitionrecovery\rawfixer\{rawfixer,ntfsfixer,fatfixer,exfatfixer,rawfixerlog,rawinfocollector}.cpp`.
  - Facade `CRawFixer::Start` / `CRawFixer::CheckPartition` (probe FS type + damage → dispatch to per-FS fixer).
  - Method inventories (byte-exact from scoped logger): NTFSFixer: CheckDBR, CheckMFT, CheckSector, FixByScanSector, FixDBR, FixFromDBR, IsValidRootDir, UpdateDBR, Undo. FATFixer: CheckBackupDBR, CheckCluster, CheckDBR, CheckFatTable, CheckRootDir, CheckSector, FixByScanSector, FixFromDBR, IsValidDBR, UpdateDBR, Undo. ExFATFixer: CheckDBR, CheckMustBeZero, CheckRoot, CheckSector, ChecksumDBR, CopySectors, FixByScanSector, FixDBR, FixFromDBR, ReadCluster, TryFixFromSectorsPerCluster, UpdateDBR.
  - Strategy flow: `Start → CheckPartition → per-FS: CheckDBR ok? → done; else FixDBR (in-place) / FixFromDBR (backup restore) / FixByScanSector (deep) → UpdateDBR → verify (IsValidRootDir/CheckMFT)`; "All paths: Undo available (transactional), RawFixerLog records each action."
  - Progress async via `Concurrency::message<uint64>` lambdas; pairs with DRWUI check classes CCheck{Ntfs,Fat,ExFat,Bitlocker}Partition; RawInfoCollector gathers geometry + candidate boot sectors.
  - Key positioning: "instead of only carving raw files from a dead NTFS, DRW first repairs the FS in place (reversibly) so the FULL directory tree, file names, timestamps, and attributes survive. Only volumes that resist RawFixer fall through to pure RAW carving."
- **DW relevance**: RawFixer is entirely user-mode sector read/write logic — the DBR-restore-then-deep-scan-else-carve waterfall is THE DW access-restore semantic to implement in Rust.

## re-workspace/artifacts/PartitionRecovery/rawfixer-v2--method-inventory.md
- **Role**: RawFixer v2 — complete method table, ENTER/EXIT trace framework, key validator algorithms.
- **Key facts**:
  - 33 named methods from `__FUNCTION__` traces; shared protocol across all three fixers: CheckDBR, FixFromDBR, UpdateDBR, FixByScanSector, CheckSector, Undo (exFAT undo via CopySectors).
  - Per-FS validators: FATFixer adds `CheckRootDir`, `IsValidDBR`, `CheckFatTable`, `CheckCluster`, `CheckBackupDBR`; NTFSFixer adds `IsValidRootDir`, `CheckMFT`; ExFATFixer adds `TryFixFromSectorsPerCluster`, `ReadCluster`, `CopySectors`, `ChecksumDBR`, `CheckRoot`, `CheckMustBeZero`.
  - `NTFSFixer::CheckMFT`: early-out if `u16@parsedBP+0x0B == 0`; MFT record sizing: `if (clusters_per_record < 0) recsize = 1 << (-clusters_per_record)` (log2 encoding) `else recsize = clusters_per_record * bytes_per_sector`; body runs in PPL agents (Concurrency, move_iterator); failure returns 0xFFFFFFFF sentinel.
  - `ExFATFixer::ChecksumDBR`: rolling per-byte `ucCRC & 1 ? 0x87>>7|<<1 : >>1` VHD-style CRC over bytes 11..209 + 510 boot-signature region, validating/repairing **both boot sectors (primary at 0, backup at LBA 12)**.
  - `ExFATFixer::TryFixFromSectorsPerCluster`: re-derives sectors-per-cluster from cluster-count invariants (two candidate paths at source lines 0x1AC/0x1A6 — FAT-chain vs cluster-heap bounds), re-validates via CheckRoot.
  - `FATFixer::CheckBackupDBR`: FAT32 backup boot sector is **sector 6**; FixFromDBR restores primary from it. `CopySectors` = undo-log (sectors copied aside before any write).
  - Confidence: 100% CODE for method table/framework/sizing; checksum byte-loop labeled INFERRED vs exFAT spec.
- **DW relevance**: exact repair heuristics (backup-sector offsets FAT32=6, exFAT=LBA 12; NTFS MFT sizing rule; SPC re-derivation) are directly portable Rust algorithms; undo-via-sector-copy defines DW's transactional repair.

## re-workspace/artifacts/PartitionRecovery/eprdtdrv-sys--driver-internals.md
- **Role**: EPMNtDrv kernel driver internals (the raw sector access door).
- **Key facts**:
  - Binaries: eprdtdrv.sys + eprdtdrv_win7older.sys + ebrntdrv.sys + epmdkdrv.sys — identical logic, 39 functions each, all PDB `epmntdrv.pdb`; pool tag `0x72647365` "esdr".
  - DriverEntry: device `\Device\EPMNTDRV` + symlink `\DosDevices\EPMNTDRV`, `FILE_DEVICE_UNKNOWN (0x22)`, `DO_DIRECT_IO (Flags |= 0x10)`; dispatch CREATE/CLOSE/READ/WRITE/DEVICE_CONTROL/CLEANUP/Unload.
  - CREATE Branch A `...\Volume\{N}`: parse N → `\Device\HarddiskVolume%u` → `IoGetDeviceObjectPointer` → top-of-stack; Branch B `...\{DiskNum}\{PartNum}`: DiskNum < 200 → `\Device\Harddisk{D}\Partition0` (whole-disk raw access) → fallback `\Device\Harddisk{D}\DR{P}` (legacy naming, retry once) → walks stack down verifying disk-class layer.
  - READ/WRITE: MDL mapping (`MmMapLockedPagesSpecifyCache`), `IoBuildAsynchronousFsdRequest`, completion `FltReadWriteSectorsCompletion`; **read length is in SECTORS of 512 bytes (hardcoded)**, file offset = absolute byte offset (LBA×512); "the scan engines compute partition-relative offsets themselves."
  - IOCTLs (CTL_CODE device 0x600, METHOD_BUFFERED): `0x60002400` (global target set), `0x60002404` GetFileObjName (out ≥ 0x208 = 260 WCHARs), `0x60002408` GetDiskContextMode; **any other IOCTL is passed through transparently to the disk stack** (geometry, SCSI passthrough).
  - No ACL on device — security via signed-driver install (setupdrvx64.exe); plain WDK C, no obfuscation; KeBugCheckEx(0xF7) GS path.
- **DW relevance**: kernel-driver-only capability (per DW's driverless constraint) — but its FUNCTIONAL contract (absolute-LBA sector reads/writes on whole disk) is achievable user-mode via `\\.\PhysicalDriveN` handles; DW must replicate the sector-addressing semantics, not the driver.

## re-workspace/artifacts/PartitionRecovery/epmntdrv-internals-v2--completion-engine-identity-ioctls.md
- **Role**: EPMNtDrv v2 — completion engine, process-identity IOCTLs, volume resolution, unload.
- **Key facts**:
  - `FltReadWriteSectorsCompletion` (FUN_00011844): frees MDL chain node-by-node (`MmUnlockPages` + `IoFreeMdl`), conditionally frees bounce buffer (flag 0x20), writes `{status, info}` to result_ptr and `KeSetEvent` to wake submitter, `IoFreeIrp` + returns `STATUS_MORE_PROCESSING_REQUIRED (0xC0000016)`; per-IRP extension layout +0x08 mdl_chain, +0x30 io_status, +0x38 io_info (bytes = sectors << 9), +0x48 result_ptr.
  - IOCTL `0x60002400` = **GetProcName**: caller-supplied PID → `NtOpenProcess(0x20000)` → `ZwQueryInformationProcess(0x1B ProcessImageFileName)` → returns image path ("attribute scan activity to DRWUI.exe / WinPE host processes"); `0x60002404` = GetFileObjName (HANDLE → `ObQueryNameString` device name + FileName = full NT path); `0x60002408` = GetDiskContextMode.
  - `\Volume\{N}` chain: scan L"\\Volume" → `RtlUnicodeStringToInteger` → `\\Device\\HarddiskVolume%u` → `IoGetDeviceObjectPointer` → pinned file object → top-of-stack; failure prints vendor-typo string `[GetVolumeDeviceObejct] %ws open error 0x%x,that's it!` → STATUS_INVALID_PARAMETER.
  - DriverUnload exists (`IoDeleteSymbolicLink` + `IoDeleteDevice`) → **hot-unloadable**, enabling no-reboot upgrades; GS cookie `KeBugCheckEx(0xF7)`; toolchain = "Visual Studio 2019 Release" per Ghidra library matches.
- **DW relevance**: kernel-only identity/IOCTL services have no DW equivalent (nor need — DW trusts its own process); the completion/MDL semantics inform how DW's async sector reads should report status+bytes to callers.

## re-workspace/artifacts/PartitionRecovery/epmntdrv-trio--naming-pid-handshake.md
- **Role**: EPMNtDrv trio — runtime device naming, PID-token handshake, WinPE failover pair.
- **Key facts**:
  - Device names are runtime-composed from the service registry path: `dev_name = "\Device\" + UPPERCASE(last path component)`, `dos_name = "\DosDevices\" + same`, fallback literal `"EPMNTDRV"`; so eprdtdrv→EPRDTDRV, ebrntdrv→EBRNTDRV, epmdkdrv→EPMDKDRV — and setupdrvx64's `%s%d` numbered variants (`eprdtdrv2`→EPRDTDRV2…) allow **in-place driver upgrades without reboot** (old instance keeps serving handles).
  - **PID-token handshake** (both sides CODE): driver computes `token = b0 | (b1+b0)<<8 | (b2+b1)<<16 | (b3+b2)<<24` from `PsGetCurrentProcessId()` bytes; client (Device.mo ~line 342) composes `"\\\\.\\EPRDTDRV\\%d\\Volume\\%d"` with the SAME transform of its own PID; mismatch → STATUS_INVALID_PARAMETER. "any process that wants raw-device access through EPMNtDrv must encode its own live PID into the device path."
  - Full path grammar: `\\.\{SYM}\<PID-TOKEN>\Volume\<N>` (volume top-of-stack) and `\\.\{SYM}\<PID-TOKEN>\<disk#>\<partition#>` (raw disk); disk# must be ≤ 199 (max 200 disks).
  - Disk branch: `\Device\Harddisk{D}\Partition0` → retry `\Device\Harddisk{D}\DR{P}` → stack walk with `RtlCompareUnicodeString(DriverName, "Disk", TRUE)` — **refuses non-disk.sys stacks** (mode stored, queryable via IOCTL 0x60002408).
  - IRP_MJ_DIRECTORY_CONTROL (+0x100) is a 6-line success stub (flat namespace).
  - WinPE carries ebrntdrv + epmdkdrv as **spare instances for failover** (policy INFERRED, mechanics CODE); Device.mo client fallback: when driver probe `FUN_1800577e0()` fails it opens `\\.\HarddiskVolume%d` directly — "plain volume access without the driver… the 'no-admin' degraded mode."
- **DW relevance**: the anti-abuse PID binding and failover instances are driver-world concerns DW explicitly does not adopt; the important DW fact is the fallback: **the product itself has a user-mode no-driver path** (direct `\\.\HarddiskVolume%d` opens) that DW can make primary.

## re-workspace/artifacts/PartitionRecovery/epmntdrv-trio--byte-diff-verification.md
- **Role**: byte-level verification that the three EPMNtDrv binaries are one source built three times.
- **Key facts**:
  - Sizes: eprdtdrv.sys 27,728 B; ebrntdrv.sys 27,272 B; epmdkdrv.sys 27,272 B; SHA-256/16 all distinct (`0f21a48ebc8e1077` / `4370aae1ee1cdeab` / `486f65c894d5f23c`).
  - ebrntdrv vs epmdkdrv: 2,907 differing bytes — PE checksum field, code register-allocation variance (`mov rbp,rsi` vs `mov rbp,rdi`), a ~5-byte extra branch block in epmdkdrv at 0x108B, Authenticode signature blocks.
  - eprdtdrv vs the pair: 9,703 differing bytes + 456 extra bytes — "the live-Windows build with additional functionality over the PE-edition builds."
  - Verdict: "three distinct compilations of one codebase… NOT interchangeable copies."
- **DW relevance**: none beyond confirming the driver family taxonomy; pure verification artifact (no DW-portable content).

## re-workspace/artifacts/PartitionRecovery/eudcpepm-euedkepm--hook-framework-urgent-cache.md
- **Role**: EUDCPEPM/EUEDKEPM v2 — hook framework, urgent COW cache, Todo-Backup variant.
- **Key facts**:
  - **DrvHK hook framework** (vendor's own mangled names): `DrvHookNextAddDevice`, `DrvHKHookSelfNewDevice`, `DrvHookSelfAddDeviceWith{Hook,NoHook}Version`, `DrvHKSetNewHookFuns(type, pfn)` installing type 1 (AddDevice replacement), type 2 (IRP dispatch interceptor), type 3 (post-AddDevice callback).
  - `DrvHookNextAddDevice` calls the victim's original AddDevice, resolves **`\Driver\VolSnap`**, walks its device chain and attaches the filter to every new snapshot device — "EuBackup rides on VSS snapshot devices."
  - IRP interceptor active ONLY for majors {0x03 READ, 0x04 WRITE, 0x09 FLUSH_BUFFERS, 0x0E DEVICE_CONTROL, 0x10 SHUTDOWN} — data-path-only interception.
  - Device extension: `IoCreateDevice(ext=0x130, FILE_DEVICE_DISK(7))`; **magic `0xD2C8E4C5` at ext+0x00** with the hook installer `DrvHKSetNewHookFuns` exposed at **ext+0x38** — an in-kernel plugin channel.
  - **Urgent COW cache** (traces verbatim): `CSession DataFirstSec=%I64u tSecs=%I64u spb=%u`, `CBitmap::SetSectors`, `AddUrgentIRP`, `cache CacheSrcData insert`, `ReadFromCache`, `CacheTransmitData0..5`, bitmap persistence `eumonSysReadWriteVolBmpFile` — a copy-on-write snapshot without VSS orchestration [model INFERRED, traces CODE].
  - Session boot FS detection: `*(u64*)(boot+0x52) == 0x2020203233544146` ("FAT32   ") and `*(u64*)(boot+3) == 0x202020205346544E` ("NTFS    "); registers a **PAGEFILE.SYS file-name watcher** for urgent caching.
  - Registry-configured identity: `DisplayName`, `DeviceName`, `IoctlBase` service values (multiple instances = different DeviceName/IoctlBase).
  - EUEDKEPM = "EaseUS Todo Backup" variant (DeviceName from registry, same dispatch shape, disk.sys verify + DR retry chain).
- **DW relevance**: kernel-filter COW snapshotting is fundamentally driver-bound — DW cannot (and per clean-room scope, will not) replicate; the only portable nugget is the boot-sector FS-label detection constants for DW's volume sniffer.

## re-workspace/artifacts/PartitionRecovery/eudcpepm-euedkepm--hook-drivers.md
- **Role**: EUDCPEPM/EUEDKEPM deep-dive — the WinPE disk-copy kernel engine + dormant raw-access door.
- **Key facts**:
  - EUDCPEPM PDB `d:\epm\_epm_main\sharelib\dc\...\EuBackup.pdb`; EUEDKEPM PDB `...EuDskAcs.pdb` (product "EaseUS Todo Backup 5.8.0.0", FileDescription "Disk Access Driver", CHENGDU YIWO Tech, PE timestamp 2024-02-04, VS2008 + /GS).
  - EUDCPEPM registers in WinPE as **epmdiskcopydrv, disk-class UpperFilter, Start=0 BOOT_START**; device extension magic 0xD2C8E4C5 + callback at +0x38; sector-size probe via `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX (0x700A0)`.
  - Custom IOCTL protocol (codes 0x800C2AC8…0x800C2B38, normalized as `code − IoctlBase` with IoctlBase=0x10000): GET_VERSION (writes 0x00010100 + "EASEUS Todo Backup Driver (c) Ea…"), OpenSession (0x840 B CSession), GETNEXTBLOCK (async data pump: marks IRP pending, queues, kicks worker with **retry budget 301 / 0x12D**), GET_PROGRESS (`percent = bytesDone×100/totalBytes`), CloseSession, direct sector read/write 0x800C2B0A/0xB12, sized transfers, CANCEL (session+0x38=3).
  - CSession layout: magic **0x74534298** at +0x28, OpenSession input {DataFirstSec, tSecs, extents, bitmap size/count, spb (0 → driver picks: tSecs < 0x20 → 1, else 0x10), 0x800 B block bitmap}, 1 MiB transfer buffer, secondary 111,744 B buffer pool tag "ECLU".
  - EUEDKEPM: PID-token gated opens (`TOKEN = b0 | (b0+b1)<<8 | (b1+b2)<<16 | (b2+b3)<<24`), disk# < 100, targets `\Device\Harddisk%u\Partition0`, 512-byte sector contract, clean IOCTL pass-through; **no in-tree component opens `\\.\EUDKEPM\...` (exhaustive search of DRWUI 2.38M + PartitionRecovery 1.17M + .mo 689K lines) — dormant in this product.**
  - WinPE rationale: BOOT_START UpperFilter guarantees presence in every disk stack from earliest PnP — "no timing races with late-attaching storage drivers."
- **DW relevance**: kernel copy engine + hook drivers — explicitly out of DW scope (driverless); the dormant-EUEDKEPM finding shows even the vendor ships unused driver surface, reinforcing that DW should not feel bound to driver parity.

## re-workspace/artifacts/PartitionRecovery/getdriverinfo--tb-module-decode.md
- **Role**: GetDriverInfo.dll — MOM-loadable drive-topology helper (TB contract), NOT a driver installer.
- **Key facts**:
  - 3,865-line decompilation, 6 exports: `TBCreateObject`/`TBCanUnloadNow` (the MOM module entry pair, "TB" = Todo Backup lineage), `GetDriverInfo_Init`, `GetDriverInfo_GetDriverInfo`, `GetDriverInfoImp`/`DeleteGetDriverInfoImp` (refcounted interface handout).
  - Imports: `GetLogicalDrives` + `GetDriveTypeW` (drive-letter topology), `LoadLibraryA`/`GetProcAddress`/`FreeLibrary` (late-binds sibling TB modules by constructing `<name>.dll` + resolving TBCreateObject), `CreateEventW`/`SetEvent`/`WaitForSingleObjectEx` (async completion).
  - **NO SCM APIs** (CreateService/StartService absent, verified) — despite the name, no driver installation happens here; driver registration belongs to installers/PE (regdrv.reg) and Device.mo's on-demand EPMNtDrv load.
  - Documented naming trap: "GetDriverInfo" = get drive-letter info, not kernel-driver info.
- **DW relevance**: pure user-mode Win32 topology enumeration — DW equivalent is direct (logical drives + types), plus the reminder to late-bind nothing (static Rust linking).

## re-workspace/artifacts/PartitionRecovery/bitlockerlib--bitlocker-implementation.md
- **Role**: BitLockerLib.dll — complete BitLocker implementation (detect, unlock, decrypt, author metadata).
- **Key facts**:
  - 2,320 functions / 83,772 lines; namespace `bitlocker::`.
  - API (symbol-exact): `GetBitLockerStatus` (fve_status_t: encrypted/locked/clear), `ParserBitLocker`/`GetVolumeIdentifier` (detect FVE volumes via **"-FVE-FS-" OEM signature memcmp**), `GetProtectors(protector_type_t, protector_t&)`, `GetProtectorsCount`, `GetAutoUnlockKey`, `PSUnlockBitLocker` (builds `Unlock-BitLocker -MountPoint '<drive>'` PowerShell), legacy `manage-bde -unlock` fallback, `GetFullVolumeEncryptionKey`, `SetProcessProtectorKey`, `SetProcessProtectorKeyFromBEK` (BEK = external-key file), `GenerateKeys`, `Encryption`, `Decryption`, `CancelEncryption`, `BuildMetadataEOWInformation` (most-referenced, 20 refs — FVE metadata authoring), `WriteRawData`/`ReadRawData` (raw sector IO "via EPMNtDrv path"), `VolumeSize`, `SectorSize`, `GetInformationOffset`, `GetBackupBootOffset`, `UnlockVolumeAndRefresh`, `LockVolume`, `IsBitLockerLocked/Encrypted`, `PercentageEncrypted`, `Convert`.
  - Key-recovery crypto: 8-round word-mixing chain `x' = ((x >> 11 | x << 21) ^ (x << 7 | x >> 25) ^ …)` in the key-derivation path (recovery-password stretching); OpenSSL EVP `aes_{128,256}_xts` is the volume-content cipher.
  - Unlock flow: user password/recovery key/BEK → GetProtectors → stretch/derive VMK → FVEK → engine reads sectors with AES-XTS decrypt; OR auto path via DRWService `\\.\Pipe\DRWBitlocker` FVEAutoUnlock keys; OR let Windows unlock (PSUnlockBitLocker/manage-bde).
  - Assessment: "a full third-party-clean-room BitLocker stack… with write capability… exists because locked BitLocker volumes are the #1 consumer data-loss scenario" and manage-bde is insufficient in WinPE.
- **DW relevance**: everything except the EPMNtDrv raw-IO leg is user-mode portable (FVE metadata parse, protector handling, XTS decrypt via a Rust crate); "-FVE-FS-" detection and the DRWService pipe idea (DW: a privileged helper service for auto-unlock keys) are the adoptable specifics.

## re-workspace/artifacts/PartitionRecovery/burn--imapi2-disc-writer.md
- **Role**: Burn.dll — disc writer engine (IMAPI2 wrapper for ISO/USB emergency media).
- **Key facts**:
  - 9,707-line decompilation; standard flow `fun_WriteImageToCD`, `fun_WriteOpen/Close`, `fun_WriteGetProgress`, `fun_AbortWrite`, `fun_GetSupportedWriteModes`, `fun_WriteGetCurrBurnFileName`.
  - Real-time streaming mode: `fun_RealTimeWrite_Begin/_End`, `fun_RealTimeWrite_CreateFile/_CloseFile`, `fun_RealTimeWrite_SetEstimateTotalWriteSize`, `fun_RealTimeWrite_GetDiscRemainCapacity` — "writing files to disc as they are produced" / multi-session append.
  - Error taxonomy strings: "CBurn Read track info error." / "CBurn Conv file start error." / "CBurn Conv file error."
  - Position in flow: PE_Tools media builder → EuDownloader fetches WAIK objects (dlconfig.xml byte-ranges + MD5) → stages WAIK tree (PEtools.xml) → DISM mounts/customizes WINPE.WIM + injects BUILDPE payload + drivers (regdrv.reg) → Burn.dll writes ISO/USB.
- **DW relevance**: optical burning is legacy IMAPI2/COM — Windows-only, out of DW's core scope; only relevant if DW ever ships a bootable-media builder (USB imaging suffices in 2025+).

---

## re-workspace/artifacts/WinPE/pe-payload-census--buildpe-easeus-x64.md
- **Role**: BUILDPE/EaseUS-x64 payload census — what the bootable (Emergency Disk) edition contains and omits.
- **Key facts**:
  - 180 files: product core (DRW.exe, DRWUI.exe, DRWService.exe, DRWReport.exe), **all 9 scan engines** (NTFS/FAT/exFAT/EXT4/HFSPlus/ReFS/Btrfs/Zfs/RAW SearchDLL.dll), full repair tool line, viewers, cloud (firebase pair), Qt 5 runtime with **qoffscreen.dll PE-only** (headless fallback).
  - Runtimes: three compiler generations (VC9 msvcp90/mfc90u, VC12, VC14) + ucrtbase + **49 api-ms-win-* forwarder DLLs** = UCRT self-carriage (WinPE has no UCRT).
  - Three OpenSSL generations coexist (0.9.8-lineage libeay32/ssleay32, 1.1, 3.x) — different subsystems link different ones.
  - PE-only binaries: `EuSendMessage.exe`, `d3d8thk.dll`, `d3d9.dll`, `d3d11.dll`, `dxgi.dll`, `mfc90u.dll`.
  - Live-tree-only: SetupUE.exe/unins000/InfoForSetup, AliyunWrap*, cloud_session.dll/cpprest141, EuUserRate.dll, and **directories `PartitionRecovery/` (31 .mo modules + 19 DLLs), `EuShare/`, `NasConfig/`, `License/`, `SaveScan/`, `EUFileRepair/`, `VideoViewer/`, `diskhealth/`**.
  - **Zero `.mo` and zero `.rsf` files anywhere under BUILDPE (find-verified)** → consequence #1: "No lost-partition search in the bootable edition… DRWUI in PE therefore cannot run the 'Lost Partition' scan mode — the PE product is scoped to file recovery from currently-visible volumes + RAW deep scan + repair."
  - PE is FREE edition by construction: `version=free`, `product-code=RE-DRW-FREE-WIN`; no License/ blobs; version 21.0.0.0 build pair with live tree.
- **DW relevance**: the most important boot-environment fact for DW: even the vendor's own bootable edition drops the entire MOM module layer — so a future bootable DiskWizard must bundle its (Rust) lost-partition engine INTO the PE payload to beat the vendor's PE limitation, or scope itself identically.

## re-workspace/artifacts/WinPE/boot-config--winpeshl-autobat-regdrv.md
- **Role**: the PE "OS configuration layer" — winpeshl.ini, auto.bat, regdrv.reg, software.reg, Unattend.xml.
- **Key facts**:
  - winpeshl.ini LaunchApps order: (1) `wpeinit.exe` (PnP, hives, network init), (2) `ChangeRes.exe setmax`, (3) `launch.exe 0 "...explorer\auto.bat"` (pseudo-desktop, synchronous), (4) `%systemdrive%\Program Files\easeus\DRW\DRW.exe` (the product — launched by the shell, not auto.bat), (5) `launch.exe 2` (keep-alive after DRW exits).
  - auto.bat phases: A ~25 `.reg` chrome imports (taskbar, PowerShell enable, MMC/CPL); B `regsvr32 /s` MMC snap-ins (devmgr, dmdskmgr, dmocx, dmdlgs, dmview.ocx, filemgmt, mycomput) → **Device Manager + Disk Management available in PE**; C shortcut creation; D OEM gate `reg query "HKLM\SOFTWARE\EASEUS\EPM\Version" /v OEM` → "Mainland" branch copies China-OEM shortcut pack, else creates "Data Recovery Wizard.lnk"; E deploys USB `Tools\` tree (DiskHealth/DiskMark/Chrome as .lnk + DLL copies, JPG-screenshot help); F `PENetwork.exe` (SMB + Wi-Fi) + `explorer.exe -winpe` + cleanup.
  - regdrv.reg (UTF-16): `epmdkdrv` DEMAND_START; `ebrntdrv` DEMAND_START, Group="Boot File System"; **`EUDCPEPM` BOOT_START, DeviceName="epmdiskcopydrv", IoctlBase=0x10000, and UpperFilters="EUDCPEPM" on GUID_DEVCLASS_DISKDRIVE {71A27CDD-812A-11D0-BEC7-08002BE2092F}**; `EUEDKEPM` SYSTEM_START, DeviceName/IoctlBase same scheme.
  - **eprdtdrv.sys and EuMrx.sys are absent from the PE image** — PE relies on ebrntdrv/epmdkdrv demand-started by the product plus the two boot-time hook drivers.
  - software.reg seeds only `HKLM\SOFTWARE\EASEUS\DRW\Language = "English"`; Unattend.xml display floor 1024×768@60Hz@32bpp (DRWUI verified geometry 1240×840).
  - Consistency verdict: "the emergency media is one codebase shared across EaseUS products, with DRW as the launched product" (EPM key + EPM-built drivers in same image).
- **DW relevance**: the wiring blueprint for a future bootable DW: shell-launch the product, self-carry the CRT, pre-register any drivers (DW: none needed — PhysicalDrive access works in PE), demand-start nothing, OEM-gate branding.

## re-workspace/artifacts/WinPE/eusendmessage--wm-copydata-messenger.md
- **Role**: EuSendMessage.exe — PE-only one-shot WM_COPYDATA notifier to the installer window.
- **Key facts**:
  - x86, VC9-era CRT, 640-line decompile, 3 real functions; imports only FindWindowW + SendMessageW + CommandLineToArgvW + WideCharToMultiByte.
  - Algorithm: argv[1..] concatenated into a 208-wchar buffer (spaces lost), two-phase WideCharToMultiByte (default char `_`, `WC_NO_BEST_FIT_CHARS` 0x200), `FindWindowW(NULL, L"DRW_Installer")` (by title, class-agnostic), `SendMessageW(hwnd, WM_COPYDATA 0x4A, 0, &cds)` with **`dwData = 999`**, payload ≤ 1040 B ANSI.
  - Exit −1 on missing args; silently no-ops if the window doesn't exist; target = the DRW installer wizard (SetupUE family — live-tree only); PE-only because the media build is scripted/unattended.
- **DW relevance**: trivial IPC utility; DW's Tauri app has no equivalent need, but the pattern (titled-window + WM_COPYDATA) is how Windows-only sidecar messaging was done — DW uses Tauri events instead.

---

## Cross-cutting observations

**MOM module system shape.** The "Module Object Model" is an in-house COM/XPCOM: 31 `.mo` files are renamed x64 PE DLLs implementing a common object skeleton (`CMomObject`/IObject AddRef/Release/QueryInterface, `CModuleImpl<T>`, event/stack/aggregate/clone object bases, functor late-binding). Lifecycle: engine requests a module **by name** → `name + ".mo"` → `LoadLibraryA` → required export `GetModule(HMODULE)` (self-registration) → `CreateObject` per-class factory; lazy load, singleton-cached in globals; host DLLs (PartitionRecovery/MainModule/RecoveryModule/ToolsModule/UILogic/CloneModule) are themselves modules of the same model. Two coupling styles coexist: 26 of 31 are runtime-discoverable factory plugins; 5 (CallbackOperator/DeviceAdapter/Log/Fixup/NTFSLib — 29/76/37/36/310 mangled exports) are link-time shared libraries; `IObject` symbols are the one universal export. The import graph is a **DAG rooted at Log.mo (15+ importers) + CallbackOperator.mo (8)**, no cycles. Message routing: engine worker threads call the MOM service-hub → `INotifyObserver` slot on the `CMessageProxy` singleton (QObject + observer multiple inheritance) → temporary `CMessageSend` envelope → `dispatch(int, void*, int&)` Qt signal with a **thread-affinity dual connect** (direct vs queued marshaling) → `doDispatchMessage` hub slot → 15 UI subscriber classes. For DiskWizard this whole layer is replaced by: a static Rust workspace (crates as "modules"), trait objects (DeviceAdapter's interface inventory is the natural trait list), channels/Tauri events (the CMessageProxy thread-marshaling problem is exactly what Tauri's event bridge solves), and a `tracing`-based log crate as the equivalent dependency root.

**PartitionRecovery: user-mode-implementable vs driver-bound.**
- *User-mode implementable (the recovery core)*: the lost-partition scan strategy — Partition.mo MBR/`CExtendMBR`/GPT entry parsing; PartitionRecoveryD's `CRecoverySearchEngine` with `ILocationGeneratorSet` location generators (CompleteSearch = exhaustive, RegSearch = registry-guided), per-FS block parsers (`CBlockParseWin32`, `CBlockParserLinux`, `CExtXBlockParser`, `CExFatBlockParser`, `CFatxBlockParser`, `BitlockerParser`), result persistence as `lostpart.dat`/`partinfo.dat`; candidate validation via the FileSystemAnalyser family (boot sector/superblock/MFT sanity); lost-partition restoration = "rebuilds partition records → writes new table via MainModule.dll ops" under the **transactional pending-operation engine** (Controller→Operation→PendingOperation, COperationManager apply/undo queue, preview semantics, ACL-preserving copy utilities); **RawFixer access-restore semantics** — CheckDBR→FixFromDBR→UpdateDBR with backup-DBR restore (FAT32 backup at sector 6, exFAT backup at LBA 12, exFAT ChecksumDBR rolling CRC, `TryFixFromSectorsPerCluster` geometry re-derivation, NTFS MFT sizing rule `1 << (-clusters_per_record)`), `FixByScanSector` deep fallback, `CopySectors` undo log — "only volumes that resist RawFixer fall through to pure RAW carving"; offline-registry reading (RegLib regf/hbin with 0x66676572 + checksum offsets), offline Windows detection (CheckVersion: `$$$PROTO.HIV`, `SELECT\CURRENT`, `PRODUCTOPTIONS\PRODUCTTYPE`), LDM dynamic-disk parsing (VMDB 0x564D4442 v4.10, TOCBLOCK 0x544F43424C4F434B three-slot search, "config"/"log" bitmaps), BitLocker detection/unlock/decrypt ("-FVE-FS-" signature, protector enumeration, AES-XTS content cipher), and BCD/EFI boot-repair guidance (Fixup.mo `\Registry\Machine\BCD00000000`).
- *Driver-bound (explicitly NOT portable to DW's driverless stance)*: the EPMNtDrv raw-sector door itself (\\.\EPRDTDRV paths, MDL async FSD passthrough, 512-byte-sector contract, PID-token anti-abuse handshake, disk.sys stack verification, hot-unload + numbered `%s%d` upgrade variants) — though its functional contract (absolute-LBA whole-disk reads) is achievable with user-mode `\\.\PhysicalDriveN` handles, and the vendor's own Device.mo fallback to direct `\\.\HarddiskVolume%d` opens proves a no-driver degraded mode exists; the EUDCPEPM kernel copy engine (BOOT_START disk-class UpperFilter, DrvHK AddDevice hooking, VolSnap attach, urgent COW cache with CacheSrcData/ReadFromCache, bitmap persistence) — COW snapshotting is unreplicable user-mode; EUEDKEPM (dormant even in the vendor tree); EuMrx NAS redirector (RDBSS, absent from PE); and RawFixer's/BitLockerLib's raw sector writes "via EPMNtDrv path" (DW must write through volume/disk handles instead, requiring dismount/lock semantics).

**WinPE implications for a future bootable DiskWizard.** The vendor's Emergency Disk is a full product-in-a-WIM but with load-bearing omissions: **zero .mo modules** (the entire lost-partition capability is live-Windows-only — DrwLostDiskPage unreachable in PE), no EuMrx/NAS, no License/ (PE = free edition by construction), no saved scans; the PE self-carries UCRT (49 api-ms-win-* + ucrtbase), three CRT and three OpenSSL generations, qoffscreen headless Qt, and D3D shims; boot wiring = winpeshl.ini shell-launch of DRW.exe after wpeinit + a registry-scripted pseudo-desktop with Device Manager/Disk Management, PENetwork SMB, and pre-registered drivers (one BOOT_START filter, two demand-start raw-access spares, eprdtdrv absent). For DW this means: (1) a bootable edition is feasible user-mode-only (PhysicalDrive access works in PE; no driver registration needed at all — simpler than the vendor's four-driver setup); (2) DW's differentiator would be shipping lost-partition recovery IN the boot environment, which the vendor does not do; (3) the practical checklist is: self-contained runtime, headless fallback, shell-launch via winpeshl.ini, display floor, and network via PE's own stack.
