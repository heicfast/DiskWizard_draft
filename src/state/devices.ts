/**
 * Device + session inventory store: the Recover Data page's data, the
 * SMART reports, and the recent-session list. Refreshes on the
 * devices-changed event (connect/disconnect, lost-partition discovery).
 */
import { create } from "zustand";
import { invoke, listen } from "../lib/ipc";
import type { DeviceInventory, RecoverySession, SmartReport } from "../types";

interface DevicesState {
  inventory: DeviceInventory | null;
  sessions: RecoverySession[];
  smart: Record<string, SmartReport>;
  /** Session 8: per-device SMART failure reasons — the dashboard
   * shows an honest "unavailable + why" card instead of silently
   * hiding (the owner's "switch disk → rescue tools only" symptom). */
  smartErrors: Record<string, string>;
  refreshing: boolean;
  connecting: boolean;
  load: () => Promise<void>;
  refresh: () => Promise<void>;
  connectDevice: () => Promise<string | null>;
  loadSmart: (deviceId: string) => Promise<void>;
}

export const useDevicesStore = create<DevicesState>((set, get) => ({
  inventory: null,
  sessions: [],
  smart: {},
  smartErrors: {},
  refreshing: false,
  connecting: false,

  load: async () => {
    try {
      const [inventory, sessions] = await Promise.all([
        invoke<DeviceInventory>("list_devices"),
        invoke<RecoverySession[]>("list_sessions"),
      ]);
      set({ inventory, sessions });
    } catch {
      set({ inventory: null, sessions: [] });
    }
  },

  refresh: async () => {
    set({ refreshing: true });
    try {
      await invoke("refresh_devices");
      await get().load();
    } finally {
      set({ refreshing: false });
    }
  },

  connectDevice: async () => {
    set({ connecting: true });
    try {
      const r = await invoke<{ deviceId: string }>("connect_demo_device");
      await get().load();
      return r.deviceId;
    } catch {
      return null;
    } finally {
      set({ connecting: false });
    }
  },

  loadSmart: async (deviceId) => {
    // In-flight dedup: never stack concurrent fetches for one disk.
    if (inFlightSmart.has(deviceId)) return;
    inFlightSmart.add(deviceId);
    try {
      const report = await invoke<SmartReport>("smart_report", { deviceId });
      const { [deviceId]: _err, ...restErrors } = get().smartErrors;
      void _err;
      set({ smart: { ...get().smart, [deviceId]: report }, smartErrors: restErrors });
    } catch (e) {
      // SMART unavailable for this device — record the REASON so the
      // UI can say why (transport unsupported, permissions, timing),
      // never a silent hide.
      const msg = e instanceof Error ? e.message : String(e);
      set({ smartErrors: { ...get().smartErrors, [deviceId]: msg } });
    } finally {
      inFlightSmart.delete(deviceId);
    }
  },
}));

const inFlightSmart = new Set<string>();

let attached = false;
/** Subscribe to device-list invalidations (connect/disconnect/lost). */
export function attachDeviceEvents(): void {
  if (attached) return;
  attached = true;
  void listen("devices-changed", () => {
    void useDevicesStore.getState().load();
  }).catch(() => undefined);
}
