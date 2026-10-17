/**
 * Scan store — the scan lifecycle state machine the whole app reads:
 * phase, percent, live stats, pause/resume/stop, and completion.
 * Subscribes to the backend's scan-progress / scan-done events (mock
 * bus or real engine — same contract).
 */
import { create } from "zustand";
import { invoke, listen } from "../lib/ipc";
import type { ScanProgress } from "../types";
import { EV_SCAN_DONE, EV_SCAN_PROGRESS } from "../types";

export interface ScanDoneInfo {
  generation: number;
  targetName: string;
  stats: { filesFound: number; bytesFound: number };
  durationSec: number;
  stopped?: boolean;
}

interface ScanState {
  generation: number;
  progress: ScanProgress | null;
  /** The last completion info (drives the celebration + stats). */
  done: ScanDoneInfo | null;
  starting: boolean;
  error: string | null;
  start: (deviceId: string) => Promise<boolean>;
  startFolder: (label: string) => Promise<boolean>;
  startDiskImage: (imagePath: string) => Promise<boolean>;
  startCloud: (driveId: string) => Promise<boolean>;
  pause: () => Promise<void>;
  resume: () => Promise<void>;
  stop: () => Promise<void>;
  adoptProgress: (p: ScanProgress | null) => void;
  clearDone: () => void;
}

export const useScanStore = create<ScanState>((set) => ({
  generation: 0,
  progress: null,
  done: null,
  starting: false,
  error: null,

  start: async (deviceId) => {
    set({ starting: true, error: null, done: null });
    try {
      const r = await invoke<{ generation: number }>("start_scan", { deviceId });
      set({ generation: r.generation, starting: false });
      return true;
    } catch (e) {
      set({ starting: false, error: String(e) });
      return false;
    }
  },

  startFolder: async (label) => {
    set({ starting: true, error: null, done: null });
    try {
      const r = await invoke<{ generation: number }>("start_folder_scan", { label });
      set({ generation: r.generation, starting: false });
      return true;
    } catch (e) {
      set({ starting: false, error: String(e) });
      return false;
    }
  },

  startDiskImage: async (imagePath) => {
    set({ starting: true, error: null, done: null });
    try {
      const r = await invoke<{ generation: number }>("start_disk_image_scan", { imagePath });
      set({ generation: r.generation, starting: false });
      return true;
    } catch (e) {
      set({ starting: false, error: String(e) });
      return false;
    }
  },

  startCloud: async (driveId) => {
    set({ starting: true, error: null, done: null });
    try {
      const r = await invoke<{ generation: number }>("start_cloud_scan", { driveId });
      set({ generation: r.generation, starting: false });
      return true;
    } catch (e) {
      set({ starting: false, error: String(e) });
      return false;
    }
  },

  pause: async () => {
    await invoke("pause_scan").catch(() => undefined);
  },
  resume: async () => {
    await invoke("resume_scan").catch(() => undefined);
  },
  stop: async () => {
    await invoke("stop_scan").catch(() => undefined);
  },

  adoptProgress: (p) => set({ progress: p }),
  clearDone: () => set({ done: null }),
}));

let attached = false;
/** Attach scan-event listeners (progress / done). Idempotent. */
export function attachScanEvents(): void {
  if (attached) return;
  attached = true;
  void listen<ScanProgress>(EV_SCAN_PROGRESS, (p) => {
    useScanStore.getState().adoptProgress(p);
  }).catch(() => undefined);
  void listen<ScanDoneInfo>(EV_SCAN_DONE, (info) => {
    useScanStore.setState({ done: info, progress: null });
  }).catch(() => undefined);
}

/** Convenience: is a scan currently active (any live phase)? */
export function scanActive(p: ScanProgress | null): boolean {
  if (!p) return false;
  return p.phase !== "done" && p.phase !== "stopped" && p.phase !== "error" && p.phase !== "idle";
}
