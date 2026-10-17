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
    try {
      const report = await invoke<SmartReport>("smart_report", { deviceId });
      set({ smart: { ...get().smart, [deviceId]: report } });
    } catch {
      // SMART unavailable for this device — leave absent; the UI
      // shows the unavailable state honestly (no fallback guessing).
    }
  },
}));

let attached = false;
/** Subscribe to device-list invalidations (connect/disconnect/lost). */
export function attachDeviceEvents(): void {
  if (attached) return;
  attached = true;
  void listen("devices-changed", () => {
    void useDevicesStore.getState().load();
  }).catch(() => undefined);
}
