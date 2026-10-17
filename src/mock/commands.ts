/**
 * Mock command registry (DEV/TEST ONLY): implements the full Tauri
 * command surface against the synthetic engine so the UI runs in a
 * plain browser. Used by Playwright/agent-browser screenshot passes;
 * NEVER bundled into production (gated on isTauri()=false + DEV in
 * main.tsx).
 *
 * The surface mirrors the future production Rust core 1:1 — command
 * names, argument shapes, event names and gating semantics are the
 * contract; swapping the mock for the real engine touches nothing
 * above src/lib/ipc.ts.
 *
 * Realism notes (product-owner requirement — "realistic parameters"):
 *  - The scan ticker walks the true phase machine (quick → deep →
 *    organizing → done) with sector counters, a live searching label
 *    and progressive result filling, compressed to demo timescale.
 *  - SD cards run the SD Card Rescue (SDR) pre-phase: restore access
 *    first, then continue with file recovery.
 *  - Licensing enforces the DiskGenie posture gates: privileged
 *    commands throw ACTIVATION_REQUIRED when unlicensed/degraded;
 *    disk health (SMART) stays free (the Monitor-mode analog).
 */
import { emitMockEvent, setMockBackend } from "../lib/ipc";
import { isKeyComplete, normalizeKey } from "../lib/licenseKey";
import type {
  DevHooks,
  DeviceInventory,
  LicenseStatus,
  RecoverDestination,
  RecoverySession,
  ScanProgress,
  ScanResultsSnapshot,
  SmartReport,
  StorageDevice,
} from "../types";
import { EV_LICENSE_CHANGED, EV_SCAN_DONE, EV_SCAN_PROGRESS, EV_DEVICES_CHANGED, EV_RECOVER_PROGRESS, EV_RECOVER_DONE } from "../types";
import {
  CLOUD_DRIVES,
  DEVICES,
  LOST_PARTITION,
  RECENT_SESSIONS,
  SDR_DEVICE,
  recoverDestinations,
  smartReportFor,
} from "./data";
import { generateResults } from "./files";

/* ── License state ───────────────────────────────────────────────────── */

const PRO_SIM: LicenseStatus = {
  posture: "pro",
  isPro: true,
  tier: "lifetime",
  customerName: "Alex Morgan",
  customerEmail: "alex.morgan@outlook.com",
  licenseExpiresAt: 0,
  graceDaysLeft: 0,
  purchaseUrl: "https://diskwizard.app/pricing",
  simulated: true,
};

let license: LicenseStatus = {
  posture: "unlicensed",
  isPro: false,
  tier: "",
  customerName: "",
  customerEmail: "",
  licenseExpiresAt: 0,
  graceDaysLeft: 0,
  purchaseUrl: "https://diskwizard.app/pricing",
  simulated: false,
};

/** The Rust gate marker (require_licensed) — the ipc layer intercepts
 * it and opens the activation flow. */
const GATE_MSG = "ACTIVATION_REQUIRED. Activate DiskWizard Pro to use this.";
function licenseGate(): void {
  const p = String(license.posture);
  if (p === "pro" || p === "grace") return;
  throw GATE_MSG;
}

/* ── Device / session state ──────────────────────────────────────────── */

let inventory: DeviceInventory = {
  localDisks: [...DEVICES.localDisks],
  externalDevices: [...DEVICES.externalDevices],
  diskImages: [...DEVICES.diskImages],
};
/** Once a deep scan has completed anywhere, lost partitions surface in
 * the inventory ("Also lost partitions are displayed here"). */
let lostPartitionDiscovered = false;
/** BitLocker volumes the user unlocked this session (FVE pipe analog). */
const bitlockerUnlocked = new Set<string>();
/** The demo SD card joins after "Connect a device". */
let sdCardConnected = false;

let sessions: RecoverySession[] = [...RECENT_SESSIONS];

/* ── Scan state machine ──────────────────────────────────────────────── */

interface ScanRun {
  generation: number;
  device: StorageDevice;
  phase: ScanProgress["phase"];
  phaseStart: number; // Date.now ms at phase start
  phaseElapsedBase: number; // seconds accumulated before pause
  percent: number;
  elapsedSec: number;
  stats: ScanProgress["stats"];
  results: ScanResultsSnapshot | null;
  sdrDone: boolean;
  doneFired: boolean;
}

let run: ScanRun | null = null;
let ticker: number | null = null;
let paused = false;
let stopped = false;

/** Phase timings (ms, demo-compressed but proportional to the real
 * quick:deep ratio — the quick metadata pass is a fraction of the
 * sector-by-sector deep pass). */
const QUICK_MS = 7_000;
const DEEP_MS = 26_000;
const ORGANIZE_MS = 2_400;
const SDR_MS = 3_600;
const TICK_MS = 220;

function deviceById(id: string): StorageDevice | undefined {
  const all = [...inventory.localDisks, ...inventory.externalDevices, ...(sdCardConnected ? [SDR_DEVICE] : []), ...(lostPartitionDiscovered ? [LOST_PARTITION] : [])];
  const found = all.find((d) => d.id === id);
  // Apply the session's unlock state over the stored row.
  return found && bitlockerUnlocked.has(found.id)
    ? { ...found, health: "ok" as const, bitLocker: "unlocked" as const }
    : found;
}

function nowSec(): number {
  return Date.now() / 1000;
}

function progressOf(r: ScanRun): ScanProgress {
  const elapsed = r.phaseElapsedBase + (paused ? 0 : Math.max(0, nowSec() - r.phaseStart));
  return {
    generation: r.generation,
    targetId: r.device.id,
    targetName: r.device.name,
    phase: r.phase,
    percent: r.percent,
    stats: r.stats,
    elapsedSec: Math.floor(elapsed),
    remainingSec: Math.max(0, Math.round(((100 - r.percent) / 100) * (r.phase === "quick" ? QUICK_MS : DEEP_MS) / 1000)),
    searchingLabel: searchingLabelFor(r),
  };
}

function searchingLabelFor(r: ScanRun): string {
  if (r.phase === "organizing") return "Organizing found files…";
  if (r.phase === "preparing") return `Reading ${r.device.name} — boot sector, partition table…`;
  if (r.device.health === "raw" && !r.sdrDone) return "SD Card Rescue — restoring access to the card…";
  const total = r.stats.sectorsTotal || 1;
  const read = Math.min(total, Math.round((r.percent / 100) * total));
  const folder = FOLDER_TOUR[Math.floor(r.stats.filesFound / 900) % FOLDER_TOUR.length];
  return `Searching: ${folder} · sector ${read.toLocaleString("en-US")} / ${total.toLocaleString("en-US")}`;
}

const FOLDER_TOUR = [
  "Users\\Alex\\Documents",
  "Users\\Alex\\Pictures\\Camera Roll",
  "Users\\Alex\\Videos",
  "$MFT :: deleted record walk",
  "ProgramData\\Packages",
  "Users\\Alex\\Downloads",
  "Windows\\WinSxS",
  "RAW carve :: signature sweep",
  "Users\\Alex\\Music",
  "Users\\Alex\\Desktop",
];

function tick(): void {
  if (!run || paused || stopped) return;
  const r = run;
  const inPhase = (nowSec() - r.phaseStart) * 1000;

  if (r.phase === "preparing") {
    if (inPhase >= 1_200) {
      r.phase = "quick";
      r.phaseStart = nowSec();
      r.phaseElapsedBase = 0;
    }
    emitMockEvent(EV_SCAN_PROGRESS, progressOf(r));
    return;
  }

  if (r.phase === "quick") {
    // The quick metadata pass covers 0..38% of the aggregate bar; an
    // SD Card Rescue (RAW card) runs its access-restore pre-phase
    // inside the front of the same bar.
    const isSdr = r.device.health === "raw";
    const phaseMs = isSdr ? SDR_MS + QUICK_MS : QUICK_MS;
    const frac = Math.min(1, inPhase / phaseMs);
    r.percent = Math.min(38, frac * 38);
    r.sdrDone = isSdr && inPhase >= SDR_MS;
    applyQuickStats(r);
    if (frac >= 1) {
      // The quick→advanced moment: results browse while deep continues.
      r.phase = "deep";
      r.phaseStart = nowSec();
      r.phaseElapsedBase = Math.round(phaseMs / 1000);
      r.results = generateResults(r.device, r.generation, 0.38);
    }
    emitMockEvent(EV_SCAN_PROGRESS, progressOf(r));
    return;
  }

  if (r.phase === "deep") {
    const frac = Math.min(1, inPhase / DEEP_MS);
    r.percent = 38 + frac * 62;
    applyDeepStats(r, frac);
    // Progressive fill: refresh the browsable tree in slices.
    const fill = 0.38 + frac * 0.62;
    const target = Math.round(fill * 40) / 40; // 40 quantization steps
    const currentFrac = r.results ? quantizedFraction(r.results, r.device) : 0;
    if (target > currentFrac + 1e-9) {
      r.results = generateResults(r.device, r.generation, target);
    }
    if (lostPartitionDiscovered === false && frac > 0.45) {
      lostPartitionDiscovered = true;
      emitMockEvent(EV_DEVICES_CHANGED, null);
    }
    if (frac >= 1) {
      r.phase = "organizing";
      r.phaseStart = nowSec();
      r.percent = 100;
    }
    emitMockEvent(EV_SCAN_PROGRESS, progressOf(r));
    return;
  }

  if (r.phase === "organizing") {
    if (inPhase >= ORGANIZE_MS) {
      finishScan(r);
    } else {
      emitMockEvent(EV_SCAN_PROGRESS, progressOf(r));
    }
    return;
  }
}

function quantizedFraction(results: ScanResultsSnapshot, device: StorageDevice): number {
  // Recover the fraction the snapshot was generated at (files scale).
  const head = results.categories.reduce((a, c) => a + c.count, 0);
  const total = TOTALS_FOR(device.id);
  return Math.min(1, head / Math.max(1, total));
}

function TOTALS_FOR(id: string): number {
  switch (id) {
    case "usb-F": return 124_532;
    case "disk0-part2": return 1_139_275;
    case "disk1-part1": return 214_830;
    case "disk1-part2": return 41_268;
    case "ssd-G": return 189_004;
    default: return 18_420;
  }
}

function applyQuickStats(r: ScanRun): void {
  const total = TOTALS_FOR(r.device.id);
  const bytesTotal = BYTES_FOR(r.device.id);
  const frac = r.percent / 38;
  r.stats.filesFound = Math.round(total * 0.41 * frac);
  r.stats.bytesFound = Math.round(bytesTotal * 0.36 * frac);
  r.stats.sectorsRead = Math.round(r.stats.sectorsTotal * (r.percent / 100));
}

function applyDeepStats(r: ScanRun, frac: number): void {
  const total = TOTALS_FOR(r.device.id);
  const bytesTotal = BYTES_FOR(r.device.id);
  r.stats.filesFound = Math.round(total * (0.41 + 0.59 * frac));
  r.stats.bytesFound = Math.round(bytesTotal * (0.36 + 0.64 * frac));
  r.stats.sectorsRead = Math.round(r.stats.sectorsTotal * (r.percent / 100));
}

function BYTES_FOR(id: string): number {
  switch (id) {
    case "usb-F": return 18.6 * 1024 ** 3;
    case "disk0-part2": return 147.04 * 1024 ** 3;
    case "disk1-part1": return 96.2 * 1024 ** 3;
    case "disk1-part2": return 12.4 * 1024 ** 3;
    case "ssd-G": return 141.7 * 1024 ** 3;
    default: return 6.8 * 1024 ** 3;
  }
}

function finishScan(r: ScanRun): void {
  r.phase = "done";
  r.percent = 100;
  r.results = generateResults(r.device, r.generation, 1);
  if (!r.doneFired) {
    r.doneFired = true;
    // Auto-saved scan session (the .rsf analog).
    const stamp = new Date();
    const p = (n: number) => String(n).padStart(2, "0");
    const sessionFile = `auto_${r.device.name.replace(/[\\/:*?"<>|]/g, "_")}_${p(stamp.getMonth() + 1)}_${p(stamp.getDate())}_${p(stamp.getHours())}_${p(stamp.getMinutes())}_${p(stamp.getSeconds())}.rsf`;
    sessions = [
      {
        id: `rs-${Date.now()}`,
        targetName: r.device.name,
        status: "completed" as const,
        filesFound: r.stats.filesFound,
        bytesFound: r.stats.bytesFound,
        startedAt: Math.floor(Date.now() / 1000 - r.phaseElapsedBase - 40),
        sessionFile,
      },
      ...sessions.filter((s) => s.targetName !== r.device.name),
    ].slice(0, 6);
  }
  stopTicker();
  emitMockEvent(EV_SCAN_DONE, {
    generation: r.generation,
    targetName: r.device.name,
    stats: r.stats,
    durationSec: Math.round(r.phaseElapsedBase + (Date.now() / 1000 - r.phaseStart)),
  });
}

function stopTicker(): void {
  if (ticker !== null) {
    window.clearInterval(ticker);
    ticker = null;
  }
}

function startScan(device: StorageDevice): { generation: number } {
  stopTicker();
  stopped = false;
  paused = false;
  const generation = (run?.generation ?? 0) + 1;
  run = {
    generation,
    device,
    phase: "preparing",
    phaseStart: nowSec(),
    phaseElapsedBase: 0,
    percent: 0,
    elapsedSec: 0,
    stats: {
      filesFound: 0,
      bytesFound: 0,
      sectorsRead: 0,
      sectorsTotal: Math.round(device.total / 512),
    },
    results: null,
    sdrDone: false,
    doneFired: false,
  };
  ticker = window.setInterval(tick, TICK_MS);
  return { generation };
}

/* ── Recovery state machine ──────────────────────────────────────────── */

interface RecoverRun {
  generation: number;
  totalFiles: number;
  doneFiles: number;
  failedFiles: number;
  totalBytes: number;
  doneBytes: number;
  currentFile: string;
  destination: string;
  enableRepair: boolean;
  repairedFiles: number;
}

let recoverRun: RecoverRun | null = null;
let recoverTicker: number | null = null;

function recoverTick(): void {
  if (!recoverRun) return;
  const r = recoverRun;
  r.doneFiles = Math.min(r.totalFiles, r.doneFiles + Math.max(1, Math.round(r.totalFiles / 90)));
  r.doneBytes = Math.min(r.totalBytes, Math.round((r.doneFiles / r.totalFiles) * r.totalBytes));
  r.currentFile = r.doneFiles >= r.totalFiles ? "Finalizing…" : CURRENT_FILE_POOL[(r.doneFiles * 7) % CURRENT_FILE_POOL.length];
  if (r.enableRepair && r.doneFiles % 23 === 0) r.repairedFiles += 1;
  const finished = r.doneFiles >= r.totalFiles;
  emitMockEvent(EV_RECOVER_PROGRESS, { ...r, finished });
  if (finished) {
    // A realistic 0.3% failure rate at scale (rounded, min 0 for tiny sets).
    r.failedFiles = r.totalFiles > 900 ? Math.round(r.totalFiles * 0.003) : 0;
    if (recoverTicker !== null) window.clearInterval(recoverTicker);
    recoverTicker = null;
    emitMockEvent(EV_RECOVER_DONE, { ...r });
  }
}

const CURRENT_FILE_POOL = [
  "Vacation Photo.jpg", "Project Proposal.docx", "holiday-4k.mp4", "Family.png",
  "Invoice 2026-03.pdf", "Podcast Episode 12.mp3", "backup-2026.zip", "IMG_4812.jpg",
  "Thesis Chapter 3.docx", "Drone Flight 04.mov", "Demo Track.wav", "Resume.pdf",
];

/* ── Settings (localStorage-persisted) ───────────────────────────────── */

export interface MockSettings {
  resultsMode: "essential" | "complete";
  confirmStop: boolean;
  enableRepairDefault: boolean;
  scanOnSdInsert: boolean;
  launchAtLogin: boolean;
}

const SETTINGS_KEY = "diskwizard.settings";
const DEFAULT_SETTINGS: MockSettings = {
  resultsMode: "essential",
  confirmStop: true,
  enableRepairDefault: true,
  scanOnSdInsert: false,
  launchAtLogin: false,
};

function loadSettings(): MockSettings {
  try {
    const raw = window.localStorage.getItem(SETTINGS_KEY);
    return raw ? { ...DEFAULT_SETTINGS, ...JSON.parse(raw) } : { ...DEFAULT_SETTINGS };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

function saveSettings(s: MockSettings): void {
  try {
    window.localStorage.setItem(SETTINGS_KEY, JSON.stringify(s));
  } catch {
    /* hardened WebView profiles — session-only is acceptable */
  }
}

/* ── Dev hooks (tour) ────────────────────────────────────────────────── */

function devHooks(): DevHooks {
  const q = new URLSearchParams(window.location.search);
  return {
    tour: window.location.hostname === "localhost" && q.get("tour") === "1",
    tourMode: q.get("tourMode"),
    tourLicenseKey: q.get("tourKey"),
    scan: q.get("scan"),
  };
}

/* ── The command surface ─────────────────────────────────────────────── */

type Args = Record<string, unknown> | undefined;

async function handle(cmd: string, args: Args): Promise<unknown> {
  switch (cmd) {
    /* ── Devices & health (SMART = free tier) ───────────────────── */
    case "list_devices": {
      const applyUnlock = (d: StorageDevice): StorageDevice =>
        bitlockerUnlocked.has(d.id)
          ? { ...d, health: "ok" as const, bitLocker: "unlocked" as const }
          : d;
      const out: DeviceInventory = {
        localDisks: [...inventory.localDisks, ...(lostPartitionDiscovered ? [LOST_PARTITION] : [])].map(applyUnlock),
        externalDevices: [...inventory.externalDevices, ...(sdCardConnected ? [SDR_DEVICE] : [])].map(applyUnlock),
        diskImages: [...inventory.diskImages],
      };
      return out;
    }
    case "refresh_devices":
      await delay(1_400);
      return { ok: true };
    case "connect_demo_device":
      // "Connect a device" — the demo SD card (RAW, prompts to format)
      // joins the inventory; SDR flow becomes available.
      await delay(900);
      sdCardConnected = true;
      emitMockEvent(EV_DEVICES_CHANGED, null);
      return { ok: true, deviceId: SDR_DEVICE.id };
    case "smart_report": {
      const id = String(args?.deviceId ?? "");
      const dev = deviceById(id);
      if (!dev) throw `UNKNOWN_DEVICE. ${id}`;
      const report: SmartReport = smartReportFor(dev);
      return report;
    }
    case "unlock_bitlocker": {
      // The FVE-service unlock round-trip (password / recovery key
      // verified through the pipe; the volume mounts decrypted).
      await delay(1_700);
      const id = String(args?.deviceId ?? "");
      const dev = deviceById(id);
      if (!dev) throw `UNKNOWN_DEVICE. ${id}`;
      if (dev.health !== "bitlocker") throw "NOT_LOCKED. This drive is not BitLocker-locked.";
      bitlockerUnlocked.add(id);
      emitMockEvent(EV_DEVICES_CHANGED, null);
      return { ok: true, unlocked: true };
    }
    case "sdr_diagnose": {
      // SD Card Rescue: restore access first, then recovery continues.
      licenseGate();
      await delay(2_800);
      const id = String(args?.deviceId ?? SDR_DEVICE.id);
      const dev = deviceById(id) ?? SDR_DEVICE;
      return {
        deviceId: id,
        restoredAccess: true,
        message: `Access to ${dev.name} restored. Continuing with file recovery.`,
        fileSystemRecovered: "exFAT",
      };
    }

    /* ── Scan lifecycle (licensed) ──────────────────────────────── */
    case "start_scan": {
      licenseGate();
      const id = String(args?.deviceId ?? "");
      const dev = deviceById(id);
      if (!dev) throw `UNKNOWN_DEVICE. ${id}`;
      if (dev.health === "bitlocker") throw "DEVICE_LOCKED. Unlock BitLocker before scanning this drive.";
      return startScan(dev);
    }
    case "start_folder_scan": {
      licenseGate();
      // Desktop / Recycle Bin / custom folder quick entries.
      const label = String(args?.label ?? "Folder");
      const pseudo: StorageDevice = {
        id: `folder-${label}`,
        name: label,
        letter: null,
        kind: "hdd",
        fileSystem: "NTFS",
        used: 0,
        total: 0,
        health: "ok",
        bitLocker: "none",
        trim: null,
        isSystem: false,
        diskIndex: 0,
      };
      return startScan(pseudo);
    }
    case "start_disk_image_scan": {
      licenseGate();
      const label = String(args?.imagePath ?? "Disk Image");
      const pseudo: StorageDevice = {
        id: `image-${label}`,
        name: label,
        letter: null,
        kind: "hdd",
        fileSystem: "NTFS",
        used: 0,
        total: 128 * 1024 ** 3,
        health: "ok",
        bitLocker: "none",
        trim: null,
        isSystem: false,
        diskIndex: 0,
      };
      return startScan(pseudo);
    }
    case "pause_scan":
      if (run && !paused) {
        paused = true;
        run.phaseElapsedBase += Math.max(0, nowSec() - run.phaseStart);
        run.phase = run.phase === "preparing" ? "preparing" : "paused";
        emitMockEvent(EV_SCAN_PROGRESS, progressOf(run));
      }
      return { ok: true };
    case "resume_scan":
      if (run && paused) {
        paused = false;
        run.phase = run.stats.sectorsRead === 0 ? "preparing" : run.percent < 38 ? "quick" : run.percent < 100 ? "deep" : "organizing";
        run.phaseStart = nowSec();
        emitMockEvent(EV_SCAN_PROGRESS, progressOf(run));
      }
      return { ok: true };
    case "stop_scan":
      stopped = true;
      stopTicker();
      if (run) {
        run.phase = "stopped";
        emitMockEvent(EV_SCAN_DONE, {
          generation: run.generation,
          targetName: run.device.name,
          stats: run.stats,
          stopped: true,
          durationSec: Math.round(run.phaseElapsedBase),
        });
      }
      return { ok: true };
    case "get_scan_progress":
      return run ? progressOf(run) : null;
    case "get_scan_results": {
      const gen = Number(args?.generation ?? 0);
      if (run && run.results && run.generation === gen) return run.results;
      if (run?.results) return run.results;
      return run?.results ?? null;
    }

    /* ── Sessions ───────────────────────────────────────────────── */
    case "list_sessions":
      return sessions;
    case "resume_session": {
      licenseGate();
      const id = String(args?.sessionId ?? "");
      const s = sessions.find((x) => x.id === id);
      if (!s) throw `UNKNOWN_SESSION. ${id}`;
      // Resuming a paused .rsf session: jump straight into a deep-scan
      // continuation from its recorded state.
      const dev =
        [...inventory.localDisks, ...inventory.externalDevices].find((d) => d.name === s.targetName) ??
        inventory.externalDevices[0];
      stopTicker();
      stopped = false;
      paused = false;
      const generation = (run?.generation ?? 0) + 1;
      run = {
        generation,
        device: dev,
        phase: "deep",
        phaseStart: nowSec(),
        phaseElapsedBase: 240,
        percent: 46,
        elapsedSec: 240,
        stats: {
          filesFound: Math.round(s.filesFound * 0.82),
          bytesFound: Math.round(s.bytesFound * 0.82),
          sectorsRead: 0,
          sectorsTotal: Math.round(dev.total / 512),
        },
        results: generateResults(dev, generation, 0.46),
        sdrDone: true,
        doneFired: false,
      };
      ticker = window.setInterval(tick, TICK_MS);
      sessions = sessions.map((x) => (x.id === id ? { ...x, status: "running" as const } : x));
      return { generation, deviceName: dev.name };
    }

    /* ── Recovery funnel (licensed) ─────────────────────────────── */
    case "list_recover_destinations": {
      const sourceId = (args?.sourceDeviceId as string | null) ?? null;
      const list: RecoverDestination[] = recoverDestinations(sourceId).map((d) => ({ ...d }));
      for (const c of CLOUD_DRIVES) {
        list.push({
          id: c.id,
          name: c.label,
          kind: "cloud",
          freeBytes: c.total - c.used,
          totalBytes: c.total,
        });
      }
      return list;
    }
    case "start_recovery": {
      licenseGate();
      const totalFiles = Math.max(1, Number(args?.fileCount ?? 1));
      const totalBytes = Math.max(1024, Number(args?.bytes ?? 1024));
      const destination = String(args?.destinationName ?? "Windows (C:)");
      recoverRun = {
        generation: Number(args?.generation ?? 0),
        totalFiles,
        doneFiles: 0,
        failedFiles: 0,
        totalBytes,
        doneBytes: 0,
        currentFile: CURRENT_FILE_POOL[0],
        destination,
        enableRepair: Boolean(args?.enableRepair),
        repairedFiles: 0,
      };
      if (recoverTicker !== null) window.clearInterval(recoverTicker);
      recoverTicker = window.setInterval(recoverTick, 180);
      return { ok: true };
    }

    /* ── Cloud (licensed scan; browsing free) ────────────────────── */
    case "list_cloud_drives":
      return CLOUD_DRIVES;
    case "cloud_sign_in": {
      await delay(1_800); // OAuth loopback handshake
      return { ok: true, account: "alex.morgan@outlook.com" };
    }
    case "start_cloud_scan": {
      licenseGate();
      const id = String(args?.driveId ?? CLOUD_DRIVES[0].id);
      const drive = CLOUD_DRIVES.find((c) => c.id === id) ?? CLOUD_DRIVES[0];
      const pseudo: StorageDevice = {
        id: drive.id,
        name: drive.label,
        letter: null,
        kind: "hdd",
        fileSystem: "NTFS",
        used: drive.used,
        total: drive.total,
        health: "ok",
        bitLocker: "none",
        trim: null,
        isSystem: false,
        diskIndex: 0,
      };
      return startScan(pseudo);
    }

    /* ── Licensing (DiskGenie architecture, DW keys) ────────────── */
    case "license_status":
      return { ...license };
    case "activate_license": {
      const key = String(args?.key ?? "");
      if (!isKeyComplete(key)) throw "BAD_REQUEST. The license key format is invalid.";
      const n = normalizeKey(key);
      await delay(1_600); // server round-trip
      if (n.includes("BADKEY")) throw "KEY_NOT_FOUND. Invalid license key.";
      if (n.includes("EXPIRE")) throw "LICENSE_EXPIRED. Your yearly license has expired. Renew to continue recovering.";
      if (n.includes("REVOK")) throw "KEY_REVOKED. This license has been revoked. Contact support.";
      if (n.includes("SLOTFU")) throw "DEVICE_SLOT_TAKEN. This license is already active on another device. Contact support to move it.";
      const lifetime = n.includes("LIFE") || n.includes("TEST");
      license = {
        ...PRO_SIM,
        simulated: false,
        tier: lifetime ? "lifetime" : "yearly",
        licenseExpiresAt: lifetime ? 0 : Math.floor(Date.now() / 1000) + 365 * 86400,
      };
      emitMockEvent(EV_LICENSE_CHANGED, { ...license });
      return { ...license };
    }
    case "deactivate_license":
      await delay(700);
      license = {
        posture: "unlicensed",
        isPro: false,
        tier: "",
        customerName: "",
        customerEmail: "",
        licenseExpiresAt: 0,
        graceDaysLeft: 0,
        purchaseUrl: license.purchaseUrl,
        simulated: false,
      };
      emitMockEvent(EV_LICENSE_CHANGED, { ...license });
      return { ...license };
    case "validate_now":
      await delay(1_100);
      return { ...license };
    case "license_sim_set": {
      // CI tour hook (ci-license-sim analog): flips pro ↔ unlicensed.
      const on = Boolean(args?.pro);
      license = on
        ? { ...PRO_SIM }
        : { ...license, posture: "unlicensed", isPro: false, tier: "", customerName: "", customerEmail: "" };
      emitMockEvent(EV_LICENSE_CHANGED, { ...license });
      return { ...license };
    }

    /* ── Settings ───────────────────────────────────────────────── */
    case "get_settings":
      return loadSettings();
    case "set_settings":
      saveSettings({ ...loadSettings(), ...(args as Partial<MockSettings>) });
      return loadSettings();

    /* ── Dev hooks (tour / CI) ───────────────────────────────────── */
    case "get_dev_hooks":
      return devHooks();

    default:
      throw `UNKNOWN_COMMAND. ${cmd}`;
  }
}

function delay(ms: number): Promise<void> {
  return new Promise((r) => window.setTimeout(r, ms));
}

/** Install the mock backend (browser-dev/test only; see main.tsx). */
export function installMock(): void {
  setMockBackend((cmd, args) => handle(cmd, args));
  // Expose a tiny debug handle for agent-driven verification (never in
  // production builds — this module is DEV-gated at the import site).
  (window as unknown as Record<string, unknown>).__DW_MOCK__ = {
    seedPro: () => {
      license = { ...PRO_SIM };
      emitMockEvent(EV_LICENSE_CHANGED, { ...license });
    },
    state: () => ({ run, license, sessions }),
  };
}
