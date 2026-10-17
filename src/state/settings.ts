/**
 * Settings store — persisted user preferences (backend get_settings /
 * set_settings; localStorage under the mock). The theme itself lives
 * in the theme system (its own persistence); these are the product
 * settings surfaced on the Settings page.
 */
import { create } from "zustand";
import { invoke } from "../lib/ipc";

export interface AppSettings {
  resultsMode: "essential" | "complete";
  confirmStop: boolean;
  enableRepairDefault: boolean;
  scanOnSdInsert: boolean;
  launchAtLogin: boolean;
}

const DEFAULTS: AppSettings = {
  resultsMode: "essential",
  confirmStop: true,
  enableRepairDefault: true,
  scanOnSdInsert: false,
  launchAtLogin: false,
};

interface SettingsState {
  settings: AppSettings;
  loaded: boolean;
  load: () => Promise<void>;
  patch: (p: Partial<AppSettings>) => Promise<void>;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: { ...DEFAULTS },
  loaded: false,

  load: async () => {
    try {
      const s = await invoke<AppSettings>("get_settings");
      set({ settings: { ...DEFAULTS, ...s }, loaded: true });
    } catch {
      set({ loaded: true });
    }
  },

  patch: async (p) => {
    const next = { ...get().settings, ...p };
    set({ settings: next });
    try {
      const s = await invoke<AppSettings>("set_settings", p);
      set({ settings: { ...DEFAULTS, ...s } });
    } catch {
      // Session-only fallback: the in-memory state already applied.
    }
  },
}));
