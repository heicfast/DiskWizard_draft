/**
 * Mock-core seed data (DEV/TEST ONLY — the browser mock backend; the
 * same dataset shape is mirrored by the Rust mock-core for packaged
 * dev builds). Every parameter is tuned to be REALISTIC against the
 * reference design and the reverse-engineered architecture:
 *
 *  - The device inventory matches the cherry-blossom reference page
 *    byte-for-byte (C: SSD 120.5/232.3 GB … USB F: 14.8/32.0 GB).
 *  - Recent sessions mirror the reference ("USB Drive (F:) —
 *    Completed · 1,248 files" …).
 *  - SMART attributes follow the CrystalDiskInfo-lineage table from
 *    the DiskHealth research (IDs 5/9/10/12/194/196/197/198 + SSD
 *    wear IDs), including one genuine Caution disk.
 */
import type {
  DeviceInventory,
  RecoverDestination,
  RecoverySession,
  SmartReport,
  StorageDevice,
} from "../types";

const KB = 1024;
export const MB = KB * 1024;
export const GB = MB * 1024;

/* ── Devices ─────────────────────────────────────────────────────────── */

export const DEVICES: DeviceInventory = {
  localDisks: [
    {
      id: "disk0-part2",
      name: "Windows (C:)",
      letter: "C:",
      kind: "ssd",
      fileSystem: "NTFS",
      used: 120.5 * GB,
      total: 232.3 * GB,
      health: "ok",
      bitLocker: "none",
      trim: "on",
      isSystem: true,
      diskIndex: 0,
    },
    {
      id: "disk1-part1",
      name: "Data (D:)",
      letter: "D:",
      kind: "hdd",
      fileSystem: "NTFS",
      used: 312.6 * GB,
      total: 931.5 * GB,
      health: "ok",
      bitLocker: "none",
      trim: null,
      isSystem: false,
      diskIndex: 1,
    },
    {
      id: "disk1-part2",
      name: "Projects (E:)",
      letter: "E:",
      kind: "hdd",
      fileSystem: "NTFS",
      used: 78.4 * GB,
      total: 500.0 * GB,
      health: "ok",
      bitLocker: "none",
      trim: null,
      isSystem: false,
      diskIndex: 1,
    },
    {
      id: "disk2-part1",
      name: "Backup (H:)",
      letter: "H:",
      kind: "hdd",
      fileSystem: "NTFS",
      used: 1_402.0 * GB,
      total: 2_000.0 * GB,
      health: "bitlocker",
      bitLocker: "locked",
      trim: null,
      isSystem: false,
      diskIndex: 2,
    },
  ],
  externalDevices: [
    {
      id: "usb-F",
      name: "USB Drive (F:)",
      letter: "F:",
      kind: "usb",
      fileSystem: "exFAT",
      used: 14.8 * GB,
      total: 32.0 * GB,
      health: "ok",
      bitLocker: "none",
      trim: null,
      isSystem: false,
      diskIndex: 3,
    },
    {
      id: "ssd-G",
      name: "Portable SSD (G:)",
      letter: "G:",
      kind: "portable-ssd",
      fileSystem: "NTFS",
      used: 256.1 * GB,
      total: 500.0 * GB,
      health: "ok",
      bitLocker: "none",
      trim: "on",
      isSystem: false,
      diskIndex: 4,
    },
  ],
  diskImages: [],
};

/** The SD-card-rescue scenario device: card that prompts to format —
 * SDR first attempts access restore, then continues with recovery
 * (the ADDED feature from the product notes). */
export const SDR_DEVICE: StorageDevice = {
  id: "sd-H",
  name: "SD Card (H:)",
  letter: "H:",
  kind: "sdcard",
  fileSystem: "RAW",
  used: 59.4 * GB,
  total: 64.0 * GB,
  health: "raw",
  bitLocker: "none",
  trim: null,
  isSystem: false,
  diskIndex: 5,
};

/** Deep-scan discovery: a lost partition surfaced on Data (D:)'s disk
 * ("Also lost partitions are displayed here" — step 1 of the flow). */
export const LOST_PARTITION: StorageDevice = {
  id: "disk1-lost0",
  name: "Lost Partition (912.4 GB)",
  letter: null,
  kind: "hdd",
  fileSystem: "NTFS",
  used: 0,
  total: 912.4 * GB,
  health: "lost",
  bitLocker: "none",
  trim: null,
  isSystem: false,
  diskIndex: 1,
};

/* ── Recent recovery sessions (reference page) ───────────────────────── */

export const RECENT_SESSIONS: RecoverySession[] = [
  {
    id: "rs-1",
    targetName: "USB Drive (F:)",
    status: "completed",
    filesFound: 1_248,
    bytesFound: 18.6 * GB,
    startedAt: daysAgo(2),
    sessionFile: "auto_USB Drive (F_)10_15_21_04_12.rsf",
  },
  {
    id: "rs-2",
    targetName: "Data (D:)",
    status: "paused",
    filesFound: 12_430,
    bytesFound: 214.8 * GB,
    startedAt: daysAgo(5),
    sessionFile: "auto_Data (D_)10_12_09_41_55.rsf",
  },
  {
    id: "rs-3",
    targetName: "Projects (E:)",
    status: "completed",
    filesFound: 856,
    bytesFound: 4.2 * GB,
    startedAt: daysAgo(9),
    sessionFile: "auto_Projects (E_)10_08_16_22_31.rsf",
  },
];

export function daysAgo(n: number): number {
  return Math.floor(Date.now() / 1000) - n * 86400 - 3600 * 4;
}

/* ── SMART reports (DiskHealth research: CrystalDiskInfo lineage) ────── */

function attr(
  id: number,
  name: string,
  current: number,
  worst: number,
  threshold: number,
  raw: string,
  status: SmartReport["attributes"][number]["status"] = "ok",
): SmartReport["attributes"][number] {
  return { id, name, current, worst, threshold, rawValue: raw, status };
}

export const SMART_REPORTS: Record<string, Omit<SmartReport, "deviceId" | "deviceName">> = {
  "disk0-part2": {
    model: "Samsung SSD 990 PRO 2TB",
    firmware: "3B2QJXA7",
    serial: "S6Z1NJ0T812345K",
    health: "good",
    temperature: 38,
    powerOnHours: 4_812,
    powerCycles: 1_204,
    totalWrites: 96.4,
    interface: "PCIe 4.0 x4 NVMe",
    attributes: [
      attr(5, "Reallocated Sectors Count", 100, 100, 10, "0"),
      attr(9, "Power-On Hours", 96, 96, 0, "4,812"),
      attr(12, "Power Cycle Count", 99, 99, 0, "1,204"),
      attr(23, "SSD Life Left (Wear)", 97, 97, 10, "97 %"),
      attr(241, "Total LBAs Written", 100, 100, 0, "96.4 TB"),
      attr(242, "Total LBAs Read", 100, 100, 0, "212.7 TB"),
    ],
  },
  "disk1-part1": {
    model: "WDC WD40EZFX-68BKR10",
    firmware: "81.00A81",
    serial: "WD-WX32A7R61294KL",
    health: "good",
    temperature: 33,
    powerOnHours: 18_446,
    powerCycles: 987,
    totalWrites: 148.2,
    interface: "SATA III",
    attributes: [
      attr(5, "Reallocated Sectors Count", 100, 100, 36, "0"),
      attr(9, "Power-On Hours", 89, 89, 0, "18,446"),
      attr(10, "Spin Retry Count", 100, 100, 97, "0"),
      attr(12, "Power Cycle Count", 100, 100, 0, "987"),
      attr(194, "Temperature", 72, 60, 45, "33 °C"),
      attr(196, "Reallocation Event Count", 100, 100, 0, "0"),
      attr(197, "Current Pending Sector Count", 100, 100, 0, "0"),
      attr(198, "Uncorrectable Sector Count", 100, 100, 0, "0"),
    ],
  },
  "usb-F": {
    model: "SanDisk Ultra Fit 3.2",
    firmware: "1.03",
    serial: "060A1F2B3C4D5E6F",
    health: "good",
    temperature: 41,
    powerOnHours: 1_095,
    powerCycles: 620,
    totalWrites: 11.8,
    interface: "USB 3.2 Gen1",
    attributes: [
      attr(5, "Reallocated Sectors Count", 100, 100, 10, "0"),
      attr(9, "Power-On Hours", 98, 98, 0, "1,095"),
      attr(12, "Power Cycle Count", 99, 99, 0, "620"),
      attr(194, "Temperature", 78, 71, 45, "41 °C"),
    ],
  },
  "ssd-G": {
    model: "SanDisk Extreme V2 2TB",
    firmware: "1.00.04",
    serial: "AA2303F987654W",
    health: "caution",
    temperature: 46,
    powerOnHours: 3_211,
    powerCycles: 415,
    totalWrites: 842.9,
    interface: "USB 3.2 Gen2 NVMe",
    attributes: [
      attr(5, "Reallocated Sectors Count", 100, 100, 10, "0"),
      attr(9, "Power-On Hours", 91, 91, 0, "3,211"),
      attr(12, "Power Cycle Count", 99, 99, 0, "415"),
      attr(194, "Temperature", 64, 49, 45, "46 °C", "warn"),
      attr(23, "SSD Life Left (Wear)", 37, 34, 10, "37 %", "warn"),
      attr(241, "Total LBAs Written", 100, 100, 0, "842.9 TB"),
    ],
  },
};

/** Fallback report builder for devices without a curated entry. */
export function smartReportFor(
  device: StorageDevice,
): SmartReport {
  const curated = SMART_REPORTS[device.id];
  if (curated) return { ...curated, deviceId: device.id, deviceName: device.name };
  return {
    deviceId: device.id,
    deviceName: device.name,
    model: device.kind === "ssd" || device.kind === "portable-ssd" ? "Generic NVMe SSD" : "Generic ATA Disk",
    firmware: "0A0A",
    serial: "GEN0000000000",
    health: "good",
    temperature: 36,
    powerOnHours: 2_140,
    powerCycles: 330,
    totalWrites: 24.2,
    interface: device.kind === "usb" || device.kind === "sdcard" ? "USB 3.0" : "SATA III",
    attributes: [
      attr(5, "Reallocated Sectors Count", 100, 100, 10, "0"),
      attr(9, "Power-On Hours", 94, 94, 0, "2,140"),
      attr(12, "Power Cycle Count", 100, 100, 0, "330"),
      attr(194, "Temperature", 70, 65, 45, "36 °C"),
    ],
  };
}

/* ── Recovery destinations ───────────────────────────────────────────── */

export function recoverDestinations(sourceId: string | null): RecoverDestination[] {
  const list: RecoverDestination[] = [];
  for (const d of [...DEVICES.localDisks, ...DEVICES.externalDevices]) {
    if (d.id === sourceId || d.health === "bitlocker") continue;
    list.push({
      id: d.id,
      name: d.name,
      kind: "local",
      freeBytes: d.total - d.used,
      totalBytes: d.total,
      isSource: d.id === sourceId,
    });
  }
  return list;
}

/* ─── deterministic PRNG (stable screenshots across sessions) ────────── */

/** Mulberry32 — deterministic, fast; the mock's only randomness source
 * (agent screenshots and CI tours must not flicker between runs). */
export function makeRng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
