# DiskWizard — Research Documentation Tree & Coverage Ledger

> **Phase 0 deliverable** (MASTER_PROMPT §10). Corpus:
> [easeus-drw-research](https://github.com/heicfast/easeus-drw-research)
> — the specification repository. Donor repos (diskgenie-v2,
> diskgenie-license-server-v2) are covered in
> [digests/donor-diskgenie.md](digests/donor-diskgenie.md).

**Coverage model** (worklog/open-questions.md OQ-6):

| Mark | Meaning |
|---|---|
| `FULL` | Read line-by-line by the session agent; governs implementation decisions directly |
| `DIGEST` | Read completely by a digestion sub-agent (5-a…5-e); per-file structured summary recorded in `digests/<name>.md`. Promoted to FULL on demand whenever a file becomes governing for code written |
| `UNCOVERED` | Not covered — a ledger failure to fix before Phase 0 exit |

**Digest files:**

| Digest | Files | Reader |
|---|---|---|
| `digests/per-binary-runtime-reversed.md` | per-binary census (114), runtime-families (10), reversed tooling (13) | agent 5-a |
| `digests/mom-partition-winpe.md` | MOM-modules (32), PartitionRecovery (14), WinPE (3) | agent 5-b |
| `digests/uiux-longtail.md` | UIUX long-tail (26) | agent 5-c |
| `digests/uiux-savescan-filerepair-config.md` | UIUX core (9), SaveScan (3), FileRepair (7), config-files (5), Cloud/EuShare/DRW binaries (13) | agent 5-e |
| `digests/donor-diskgenie.md` | diskgenie-v2 + diskgenie-license-server-v2 (licensing/workflows/theme/mock) | agent 5-d |


## (repo root)
- [x] **README.md** — FULL

## `architecture/`
- [x] **00-INDEX.md** — FULL
- [x] **01-build-tree-intelligence.md** — FULL
- [x] **02-kernel-driver-architecture.md** — FULL
- [x] **03-winpe-boot-environment.md** — FULL
- [x] **04-recovery-workflow.md** — FULL
- [x] **05-scan-engine-architecture.md** — FULL
- [x] **05-uiux-architecture.md** — FULL
- [x] **06-repair-chain.md** — FULL
- [x] **07-user-flow-uiux.md** — FULL

## `re-workspace/`
- [x] **README.md** — FULL

## `re-workspace/artifacts/Cloud/`
- [x] cloud-recovery-architecture.md — DIGEST → digests/uiux-savescan-filerepair-config.md

## `re-workspace/artifacts/`
- [x] DRW-exe--launcher.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] DRWReport-exe--crash-reporter.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] DRWService-exe--bitlocker-service.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] DRWUI-exe--main-ui-architecture.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] DRWUI-exe--thread-architecture.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] DRWUI-subsystems--assistant-grading-preview.md — DIGEST → digests/uiux-savescan-filerepair-config.md

## `re-workspace/artifacts/DiskHealth/`
- [x] **diskhealth--smart-companion.md** — FULL

## `re-workspace/artifacts/EuShare/`
- [x] eumrx-nas-architecture.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] eumrx-rdbss-internals--minirdr-dispatch.md — DIGEST → digests/uiux-savescan-filerepair-config.md

## `re-workspace/artifacts/FileRepair/`
- [x] archive-repair--7z-engine.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] deepening-batch2--pdf-office-repair.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] hwp-pathchecker--module-decode.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] image-repair--format-grammar.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] image-repair--gif-bmp-svg-tiff-walks.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] image-repair--state-machines.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] **repair-architecture.md** — FULL
- [x] x3f-repair--section-checkers-walk.md — DIGEST → digests/uiux-savescan-filerepair-config.md

## `re-workspace/artifacts/Installer/`
- [x] setupue-infoforsetup--bootstrap-pair.md — DIGEST → digests/uiux-savescan-filerepair-config.md

## `re-workspace/artifacts/Languages/`
- [x] README.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] language-packs--family-census.md — DIGEST → digests/uiux-savescan-filerepair-config.md

## `re-workspace/artifacts/Licensing/`
- [x] euinapp--ie-bootstrapper.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] **licensing-activation-chain.md** — FULL

## `re-workspace/artifacts/MOM-modules/`
- [x] CallbackOperator.md — DIGEST → digests/mom-partition-winpe.md
- [x] CheckVersion.md — DIGEST → digests/mom-partition-winpe.md
- [x] Common.md — DIGEST → digests/mom-partition-winpe.md
- [x] Config.md — DIGEST → digests/mom-partition-winpe.md
- [x] Device.md — DIGEST → digests/mom-partition-winpe.md
- [x] DeviceAdapter.md — DIGEST → digests/mom-partition-winpe.md
- [x] DeviceManager.md — DIGEST → digests/mom-partition-winpe.md
- [x] EFIBoot.md — DIGEST → digests/mom-partition-winpe.md
- [x] Error.md — DIGEST → digests/mom-partition-winpe.md
- [x] ExFatFileSystemAnalyser.md — DIGEST → digests/mom-partition-winpe.md
- [x] ExtFileSystemAnalyser.md — DIGEST → digests/mom-partition-winpe.md
- [x] FATFileSystemAnalyser.md — DIGEST → digests/mom-partition-winpe.md
- [x] FatFormat.md — DIGEST → digests/mom-partition-winpe.md
- [x] FatLib.md — DIGEST → digests/mom-partition-winpe.md
- [x] FileSystem.md — DIGEST → digests/mom-partition-winpe.md
- [x] FileSystemAnalyser.md — DIGEST → digests/mom-partition-winpe.md
- [x] FileSystemCheck.md — DIGEST → digests/mom-partition-winpe.md
- [x] Fixup.md — DIGEST → digests/mom-partition-winpe.md
- [x] LdmManager.md — DIGEST → digests/mom-partition-winpe.md
- [x] Log.md — DIGEST → digests/mom-partition-winpe.md
- [x] Mom.md — DIGEST → digests/mom-partition-winpe.md
- [x] NTFSFileSystemAnalyser.md — DIGEST → digests/mom-partition-winpe.md
- [x] NTFSLib.md — DIGEST → digests/mom-partition-winpe.md
- [x] NTFSUtil.md — DIGEST → digests/mom-partition-winpe.md
- [x] Partition.md — DIGEST → digests/mom-partition-winpe.md
- [x] PartitionRecoveryD.md — DIGEST → digests/mom-partition-winpe.md
- [x] RefsFileSystemAnalyser.md — DIGEST → digests/mom-partition-winpe.md
- [x] RegLib.md — DIGEST → digests/mom-partition-winpe.md
- [x] Thread.md — DIGEST → digests/mom-partition-winpe.md
- [x] bitlockerhelp.md — DIGEST → digests/mom-partition-winpe.md
- [x] module-internals-wave2--ldm-reglib-ctbfile.md — DIGEST → digests/mom-partition-winpe.md
- [x] mom-modules--ui-bridge-and-dependency-matrix.md — DIGEST → digests/mom-partition-winpe.md

## `re-workspace/artifacts/PartitionRecovery/`
- [x] bitlockerlib--bitlocker-implementation.md — DIGEST → digests/mom-partition-winpe.md
- [x] burn--imapi2-disc-writer.md — DIGEST → digests/mom-partition-winpe.md
- [x] epmntdrv-internals-v2--completion-engine-identity-ioctls.md — DIGEST → digests/mom-partition-winpe.md
- [x] epmntdrv-trio--byte-diff-verification.md — DIGEST → digests/mom-partition-winpe.md
- [x] epmntdrv-trio--naming-pid-handshake.md — DIGEST → digests/mom-partition-winpe.md
- [x] eprdtdrv-sys--driver-internals.md — DIGEST → digests/mom-partition-winpe.md
- [x] eudcpepm-euedkepm--hook-drivers.md — DIGEST → digests/mom-partition-winpe.md
- [x] eudcpepm-euedkepm--hook-framework-urgent-cache.md — DIGEST → digests/mom-partition-winpe.md
- [x] getdriverinfo--tb-module-decode.md — DIGEST → digests/mom-partition-winpe.md
- [x] mo-module-map.md — DIGEST → digests/mom-partition-winpe.md
- [x] mom-module-system--architecture.md — DIGEST → digests/mom-partition-winpe.md
- [x] pending-operation-engine.md — DIGEST → digests/mom-partition-winpe.md
- [x] rawfixer-fs-repair.md — DIGEST → digests/mom-partition-winpe.md
- [x] rawfixer-v2--method-inventory.md — DIGEST → digests/mom-partition-winpe.md

## `re-workspace/artifacts/SaveScan/`
- [x] ifileproperties-part1--ntfs-fat-exfat.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] **ifileproperties-part2--ext4-hfs-refs.md** — FULL
- [x] ifileproperties-part3--btrfs-zfs-raw-generic.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] **rsf-format--scan-session.md** — FULL
- [x] rsf-record4--tree-node-grammar.md — DIGEST → digests/uiux-savescan-filerepair-config.md

## `re-workspace/artifacts/SearchDLL/`
- [x] **analyser-granular-batch1--pst-rar-mkv-heic.md** — FULL
- [x] **btrfs-engine--deep-dive.md** — FULL
- [x] **engine-matrix--factory-contract-verification.md** — FULL
- [x] **exfat-engine--deep-dive.md** — FULL
- [x] **ext4-engine--deep-dive.md** — FULL
- [x] **ext4-hfs-engine-algorithms-v2.md** — FULL
- [x] **fat-engine--deep-dive.md** — FULL
- [x] **fat-exfat-engine-algorithms-v2.md** — FULL
- [x] **fat-exfat-engine-internals.md** — FULL
- [x] **hfsplus-engine--deep-dive.md** — FULL
- [x] **ntfs-engine--deep-dive.md** — FULL
- [x] **ntfs-engine-algorithms-v2--deleted-viability.md** — FULL
- [x] **ntfs-engine-internals.md** — FULL
- [x] **per-engine-function-map.md** — FULL
- [x] **raw-dual-registry--u16-magic-keys.md** — FULL
- [x] **rawsearch-carving--deep-dive.md** — FULL
- [x] **rawsearchdll--analyser-registry-census.md** — FULL
- [x] **rawsearchdll--family-brance.md** — FULL
- [x] **rawsearchdll--family-bruteforce.md** — FULL
- [x] **rawsearchdll--family-filegen.md** — FULL
- [x] **rawsearchdll--family-rawspeed.md** — FULL
- [x] **rawsearchdll--family-special.md** — FULL
- [x] **rawsearchdll--family-xjq.md** — FULL
- [x] **rawsearchdll--zip-analyser-carving.md** — FULL
- [x] **refs-btrfs-zfs-engine-algorithms-v2.md** — FULL
- [x] **refs-engine--deep-dive.md** — FULL
- [x] **remaining-engines--ext4-hfs-refs-btrfs-zfs.md** — FULL
- [x] **searchdll-family--architecture.md** — FULL
- [x] **zfs-engine--deep-dive.md** — FULL
- [x] **zfs-vdev--mftmirr--calcpartition.md** — FULL

## `re-workspace/artifacts/UIUX/`
- [x] 01-visual-inventory.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] 02-string-inventory.md — DIGEST → digests/uiux-longtail.md
- [x] 03-window-class-map.md — DIGEST → digests/uiux-longtail.md
- [x] 04-userflow-state-machine.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] 05-styling-theming-layout.md — DIGEST → digests/uiux-longtail.md
- [x] 06-dialog-telemetry-inventory.md — DIGEST → digests/uiux-longtail.md
- [x] 07-home-phase-deep-dive.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] 08-scan-phase-deep-dive.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] 09-results-phase-deep-dive.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] **10-preview-subsystem.md** — FULL
- [x] 11-recovery-phase-funnel.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] 12-repair-ui-flows.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] 13-ui-core-wiring-state.md — DIGEST → digests/uiux-longtail.md
- [x] 14-cdetailpage-slots-viewmodes.md — DIGEST → digests/uiux-longtail.md
- [x] 15-scenario-flows-nas-bitlocker-linux-pe.md — DIGEST → digests/uiux-longtail.md
- [x] 16-shell-internals-ai-layer.md — DIGEST → digests/uiux-longtail.md
- [x] 17-monetization-growth-surfaces.md — DIGEST → digests/uiux-longtail.md
- [x] 18-preview-architecture.md — DIGEST → digests/uiux-longtail.md
- [x] 19-diskhealth-quick-ui.md — DIGEST → digests/uiux-longtail.md
- [x] 20-moc-surface-sink-architecture.md — DIGEST → digests/uiux-longtail.md
- [x] 21-qss-property-census.md — DIGEST → digests/uiux-longtail.md
- [x] 22-rtti-class-census.md — DIGEST → digests/uiux-longtail.md
- [x] 23-euinapp-ie-emulation-bootstrapper.md — DIGEST → digests/uiux-longtail.md
- [x] 24-dynamic-ground-truth-ingestion.md — DIGEST → digests/uiux-longtail.md
- [x] 25-connect-wiring-graph.md — DIGEST → digests/uiux-longtail.md
- [x] 30-interaction-driver-ground-truth.md — DIGEST → digests/uiux-longtail.md
- [x] 31-filterbar-family-decode.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] 32-repair-item-rows-decode.md — DIGEST → digests/uiux-longtail.md
- [x] 33-objectname-loop-closure.md — DIGEST → digests/uiux-longtail.md
- [x] 34-navigatebar-bottomarea-chrome.md — DIGEST → digests/uiux-longtail.md
- [x] 35-redesign-reference-atlas.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] dynamic-ground-truth--landing-page-windows.md — DIGEST → digests/uiux-longtail.md
- [x] dynamic-ground-truth--scan-phase-captured.md — DIGEST → digests/uiux-longtail.md
- [x] dynamic-ground-truth--splash-captured.md — DIGEST → digests/uiux-longtail.md
- [x] dynamic-ground-truth--window-census-verification.md — DIGEST → digests/uiux-longtail.md
- [x] dynamic-ground-truth--wine-loader-run.md — DIGEST → digests/uiux-longtail.md

## `re-workspace/artifacts/WinPE/`
- [x] boot-config--winpeshl-autobat-regdrv.md — DIGEST → digests/mom-partition-winpe.md
- [x] eusendmessage--wm-copydata-messenger.md — DIGEST → digests/mom-partition-winpe.md
- [x] pe-payload-census--buildpe-easeus-x64.md — DIGEST → digests/mom-partition-winpe.md

## `re-workspace/artifacts/config-files/`
- [x] config-data-reference.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] drwcfg-bin--aes-config-cache.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] **filtering-priority--config-layer.md** — FULL
- [x] qa-logs--session-forensics.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] userdata-remoteconfig--signature-and-flags.md — DIGEST → digests/uiux-savescan-filerepair-config.md
- [x] vendor-logs--qa-session-forensics.md — DIGEST → digests/uiux-savescan-filerepair-config.md

## `re-workspace/artifacts/per-binary/`
- [x] AliyunWrap.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] AliyunWrapExe.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] AutoUpdate.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] BitLockerLib.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] Boot.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] BootDriver.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] BtrfsSearchDLL.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] Burn.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] CallbackOperator.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] CheckVersion.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] CloneModule.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] Common.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] Config.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] DRW.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] DRWReport.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] DRWService.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] DRWUI.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] Device.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] DeviceAdapter.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] DeviceManager.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] DiskHealth.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EFIBoot.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EUAdobeRepair.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EUArchiveRepair.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EUDCPEPM.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EUEDKEPM.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EUHwpFilePathCheckerModule.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EUHwpViewer.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EUImageEnhance.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EUImgRepair.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EUImgRepairFile.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EUOfficeViewer.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EURawImg.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EUUnZip.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EUVideoRepair.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EUinApp.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EXT4SearchDLL.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EaseUS_MediaGraph.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EaseUS_VideoViewer.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] Error.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EuDownloader.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EuICloudRecovery.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EuMrx.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EuOfficeRepairFile.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EuOfficeRepairWin32.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EuOfficeTools.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EuPDFRepairFile.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EuRepairHost.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EuSendMessage.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] EuUserRate.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] ExFatFileSystemAnalyser.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] ExtFileSystemAnalyser.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] FATFileSystemAnalyser.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] FATSearchDLL.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] FatFormat.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] FatLib.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] FileSystem.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] FileSystemAnalyser.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] FileSystemCheck.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] Fixup.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] GetDriverInfo.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] HFSPlusSearchDLL.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] InfoForSetup.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] Japanese.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] LdmManager.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] Log.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] Logger.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] MainModule.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] Mom.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] NTFSFileSystemAnalyser.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] NTFSLib.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] NTFSSearchDLL.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] NTFSUtil.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] PDFViewer.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] PE_Tools.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] Partition.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] PartitionRecovery.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] PartitionRecoveryD.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] PubLog.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] RAWSearchDLL.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] RawFixer.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] ReFSSearchDLL.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] RecoveryModule.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] RefsFileSystemAnalyser.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] RegLib.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] SetupUE.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] Thread.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] ToolsModule.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] UILogic.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] VideoFileRepairer.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] Win32Bcd.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] WinChkdsk.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] ZfsSearchDLL.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] archive.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] bitlockerhelp.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] cloud_session.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] cpprest141_2_10.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] deepening-batch1--japanese-winchkdsk-setupdrv-legacy.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] deepening-batch3--eight-stubs-resolved.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] ebrntdrv.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] epmdkdrv.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] eprdtdrv.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] eprdtdrv_win7older.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] exFATSearchDLL.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] firebasefetch.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] firebaseonlineconfig.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] legacy.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] libcurl.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] libssh2.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] libvmdk.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] peformat.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] send_dmp_with_aliyun.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] setupdrvx64.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] unins000.md — DIGEST → digests/per-binary-runtime-reversed.md

## `re-workspace/artifacts/runtime-families/`
- [x] d3d-graphics-shims.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] imagemagick-modules.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] media-codecs.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] misc-windows-shims.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] network-libraries.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] openssl-crypto-trio.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] qt5-core-stack.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] qt5-quick-qml-stack.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] ucrt-forwarders.md — DIGEST → digests/per-binary-runtime-reversed.md
- [x] vc-runtime-generations.md — DIGEST → digests/per-binary-runtime-reversed.md

## `re-workspace/raw-evidence/ui-screenshots/`
- [x] **README.md** — FULL

## `re-workspace/reversed/ci-multitool/`
- [x] REPORT.md — DIGEST → digests/per-binary-runtime-reversed.md

## `re-workspace/reversed/ci-multitool/core/`
- [x] REPORT.md — DIGEST → digests/per-binary-runtime-reversed.md

## `re-workspace/reversed/ci-multitool/partition/`
- [x] REPORT.md — DIGEST → digests/per-binary-runtime-reversed.md

## `re-workspace/reversed/ci-multitool/repair/`
- [x] REPORT.md — DIGEST → digests/per-binary-runtime-reversed.md

## `re-workspace/reversed/ci-multitool/searchdll/`
- [x] REPORT.md — DIGEST → digests/per-binary-runtime-reversed.md

## `re-workspace/reversed/ghidra-core/`
- [x] REPORT.md — DIGEST → digests/per-binary-runtime-reversed.md

## `re-workspace/reversed/ghidra-drivers/`
- [x] REPORT.md — DIGEST → digests/per-binary-runtime-reversed.md

## `re-workspace/reversed/ghidra-drwui/`
- [x] REPORT.md — DIGEST → digests/per-binary-runtime-reversed.md

## `re-workspace/reversed/ghidra-misc/`
- [x] REPORT.md — DIGEST → digests/per-binary-runtime-reversed.md

## `re-workspace/reversed/ghidra-partition/`
- [x] REPORT.md — DIGEST → digests/per-binary-runtime-reversed.md

## `re-workspace/reversed/ghidra-repair/`
- [x] REPORT.md — DIGEST → digests/per-binary-runtime-reversed.md

## `re-workspace/reversed/ghidra-searchdll/`
- [x] REPORT.md — DIGEST → digests/per-binary-runtime-reversed.md

## `re-workspace/reversed/ghidra-wave2/`
- [x] REPORT.md — DIGEST → digests/per-binary-runtime-reversed.md

## (repo root)
- [x] worklog.md — DIGEST → digests/(in-ledger: RE process journal, stages 0-5 skimmed by main agent; process log, not spec).md

## Ledger summary

- Total markdown files: **299**
- FULL (line-by-line, main agent): **49**
- DIGEST (sub-agent, recorded): **250**
- UNCOVERED: **0**
- Coverage: **100%** (every file FULL or DIGEST with a recorded digest)
