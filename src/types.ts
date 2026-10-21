/**
 * DiskWizard domain types — the UI's model of the (currently
 * mock-simulated) core. Every type mirrors the reverse-engineered
 * architecture so the real Rust core can slot in without reshaping
 * the frontend:
 *
 *  - Device inventory        ↔ EnumCollectPartition / DeviceManager
 *  - ScanSession + phases    ↔ CScanMgr / CDoScanPartition strategy
 *  - FileNode + grading      ↔ CFileNode + CCheckNodeThread (the
 *                              "file recoverability indicators")
 *  - Lost-path branches      ↔ IRawDirectoryTreeBuilder carve results
 *  - SmartReport             ↔ DiskHealth (CrystalDiskInfo lineage)
 *  - LicenseStatus           ↔ licensing-activation-chain (postures)
 */

/* ── Devices ─────────────────────────────────────────────────────────── */

export type StorageKind = "ssd" | "hdd" | "usb" | "sdcard" | "portable-ssd";

export type FileSystemKind =
  | "NTFS"
  | "NTFS5"
  | "FAT32"
  | "FAT16"
  | "exFAT"
  | "ReFS"
  | "ext4"
  | "HFS+"
  | "RAW";

/** BitLocker states (icons lock/unlock; DRW type codes 2-5/9-13/17-21). */
export type BitLockerState = "none" | "locked" | "unlocked";

export interface StorageDevice {
  id: string;
  /** Display name, e.g. "Windows (C:)". */
  name: string;
  letter: string | null;
  kind: StorageKind;
  fileSystem: FileSystemKind;
  /** Bytes used. */
  used: number;
  /** Bytes total. */
  total: number;
  /** Volume health for lost partitions ("lost" = found by deep scan). */
  health: "ok" | "lost" | "raw" | "bitlocker";
  bitLocker: BitLockerState;
  /** SSD TRIM state — recovery-chance chip (TRIM ON = lower odds). */
  trim: "on" | "off" | null;
  isSystem: boolean;
  /** Physical disk index (grouping). */
  diskIndex: number;
}

export interface DeviceInventory {
  localDisks: StorageDevice[];
  externalDevices: StorageDevice[];
  /** Mounted disk-image files available as scan targets. */
  diskImages: StorageDevice[];
}

/* ── Scanning ────────────────────────────────────────────────────────── */

export type ScanPhase =
  | "idle"
  | "preparing"
  | "quick" // FS-metadata pass
  | "deep" // sector-by-sector + RAW carving
  | "organizing" // tree build / classify / check (BuildTree phase)
  | "paused"
  | "done"
  | "stopped"
  | "error";

export interface ScanStats {
  filesFound: number;
  bytesFound: number;
  /** Sectors read / total (deep scan). */
  sectorsRead: number;
  sectorsTotal: number;
}

export interface ScanProgress {
  generation: number;
  targetId: string;
  targetName: string;
  phase: ScanPhase;
  /** 0..100 (aggregate of the current phase). */
  percent: number;
  stats: ScanStats;
  /** Wall-clock seconds since start (excluding pauses). */
  elapsedSec: number;
  /** Estimated seconds to finish (deep scan). */
  remainingSec: number;
  /** Live "Searching: <path>" ticker (raw sector addresses). */
  searchingLabel: string;
}

export interface RecoverySession {
  id: string;
  targetName: string;
  status: "completed" | "paused" | "running" | "failed";
  filesFound: number;
  bytesFound: number;
  startedAt: number;
  /** ISO-ish label of the saved scan session (.rsf analog). */
  sessionFile: string | null;
}

/* ── Found files ─────────────────────────────────────────────────────── */

export type FileCategory =
  | "images"
  | "videos"
  | "audio"
  | "documents"
  | "archives"
  | "email"
  | "other";

export const CATEGORY_ORDER: FileCategory[] = [
  "images",
  "videos",
  "audio",
  "documents",
  "archives",
  "email",
  "other",
];

export const CATEGORY_LABELS: Record<FileCategory, string> = {
  images: "Images",
  videos: "Videos",
  audio: "Audio",
  documents: "Documents",
  archives: "Archives",
  email: "Email",
  other: "Others",
};

/** The recoverability grading (CCheckNodeThread analog → row chips +
 * the preview panel's "Recovery chances" gauge). */
export type Recoverability = "excellent" | "good" | "poor";

/** Where the node came from — the results tree's special branches. */
export type NodeOrigin =
  | "existing" // still on disk
  | "deleted" // FS record present, unlinked
  | "lost-path" // carved by RAW engine, path unreconstructable
  | "lost-name" // carved, name synthesized from content
  | "tags" // identified via EXIF/metadata tag mining
  | "version"; // NTFS multi-version record

export interface FileNode {
  id: number;
  parentId: number | null;
  name: string;
  isDir: boolean;
  category: FileCategory;
  /** Bytes (files) or aggregate (dirs). */
  size: number;
  modified: number; // unix milliseconds (session 9 precision contract)
  origin: NodeOrigin;
  recoverability: Recoverability;
  /** Full display path (F:\Photos\Vacation Photo.jpg). */
  path: string;
  /** For preview: image dims where known. */
  width?: number;
  height?: number;
  /** Days-deleted bucket for the date filter. */
  deletedAgo?: number;
  ext: string;
}

export interface CategoryCount {
  category: FileCategory;
  count: number;
  size: number;
}

export interface ScanResultsSnapshot {
  generation: number;
  targetName: string;
  /** The scan target's device id (real core) — feeds the destination
   * dialog's source flags ("same drive" rows) and the backend's own
   * source-disk exclusion. */
  deviceId?: string | null;
  /** All categories, counts, sizes. */
  categories: CategoryCount[];
  /** Complete flat node list (UI filters/folds; essential-results mode
   * hides the noise layer client-side — see resultsMode). */
  nodes: FileNode[];
  /** Wall-clock scan duration (seconds) once done. */
  durationSec: number;
}

/* ── Recovery (the export funnel) ────────────────────────────────────── */

export interface RecoverDestination {
  id: string;
  name: string;
  kind: "local";
  freeBytes: number;
  totalBytes: number;
  /** The scan's source — hard-blocked ("Cannot save to original partition"). */
  isSource?: boolean;
}

export interface RecoverProgress {
  generation: number;
  totalFiles: number;
  doneFiles: number;
  failedFiles: number;
  totalBytes: number;
  doneBytes: number;
  currentFile: string;
  finished: boolean;
}

/* ── Disk health (SMART — the free tier, DiskGenie Monitor analog) ───── */

export interface SmartAttribute {
  id: number;
  name: string;
  current: number;
  worst: number;
  threshold: number;
  rawValue: string;
  status: "ok" | "warn" | "bad";
}

export interface SmartReport {
  deviceId: string;
  deviceName: string;
  model: string;
  firmware: string;
  serial: string;
  /** Overall verdict. */
  health: "good" | "caution" | "bad";
  temperature: number;
  powerOnHours: number;
  powerCycles: number;
  /** Read/write totals (TBW for SSDs). */
  totalWrites: number;
  interface: string;
  attributes: SmartAttribute[];
}


/* ── Licensing (DiskGenie architecture, DB-prefixed) ─────────────────── */

export interface LicenseStatus {
  posture: "unlicensed" | "pro" | "grace" | "degraded";
  isPro: boolean;
  tier: string;
  /** Display identity (empty before activation). */
  customerName: string;
  customerEmail: string;
  /** License expiry (unix seconds; 0 = lifetime). */
  licenseExpiresAt: number;
  graceDaysLeft: number;
  /** The purchase URL (from the Rust layer — one source of truth). */
  purchaseUrl: string;
  /** CI simulation marker (screenshots workflow only). */
  simulated: boolean;
}

/* ── App hooks (dev/CI) ──────────────────────────────────────────────── */

export interface DevHooks {
  tour: boolean;
  tourMode: string | null;
  tourLicenseKey: string | null;
  scan: string | null;
  /** Which core answered get_dev_hooks ("real" | "sim"). The tour
   * driver branches: the real core picks a live device row + walks the
   * real recovery funnel; the sim core drives the synthetic device. */
  core?: "real" | "sim" | null;
  /** QA-1 real-core tour: substring filter selecting the device row to
   * scan (e.g. the CI VHD's label). null/empty → first eligible row. */
  deviceFilter?: string | null;
}

/* ── View state ──────────────────────────────────────────────────────── */

export type PageId = "recover" | "results" | "tools" | "settings" | "help";

export type ResultsMode = "essential" | "complete";

/* ── Events (Tauri event names; the mock bus mirrors them) ───────────── */

export const EV_SCAN_PROGRESS = "scan-progress";
export const EV_SCAN_PHASE = "scan-phase";
export const EV_SCAN_DONE = "scan-done";
export const EV_SCAN_ERROR = "scan-error";
export const EV_RECOVER_PROGRESS = "recover-progress";
export const EV_RECOVER_DONE = "recover-done";
export const EV_LICENSE_CHANGED = "license-changed";
export const EV_DEVICES_CHANGED = "devices-changed";
