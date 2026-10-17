/**
 * View store — the page router (stack-index switching, the
 * DrwMainViewManagerWizard analog), modal/dialog coordination, and the
 * boot sequence flags (splash → elevation → app).
 */
import { create } from "zustand";
import type { PageId } from "../types";

export type BootStage = "splash" | "elevation" | "ready";

interface ViewStore {
  page: PageId;
  bootStage: BootStage;
  elevationRequired: boolean;
  licenseDialogOpen: boolean;
  toast: { text: string; icon: "check" | "shield" | "trash" | "info" } | null;
  setPage: (p: PageId) => void;
  advanceBoot: () => void;
  setElevationRequired: (req: boolean) => void;
  openLicenseDialog: () => void;
  closeLicenseDialog: () => void;
  showToast: (text: string, icon?: "check" | "shield" | "trash" | "info") => void;
  clearToast: () => void;
}

export const useViewStore = create<ViewStore>((set) => ({
  page: "recover",
  bootStage: "splash",
  elevationRequired: false,
  licenseDialogOpen: false,
  toast: null,

  setPage: (p) => set({ page: p }),
  advanceBoot: () =>
    set((s) => ({
      bootStage: s.bootStage === "splash" ? "elevation" : "ready",
    })),
  setElevationRequired: (req) => set({ elevationRequired: req }),
  openLicenseDialog: () => set({ licenseDialogOpen: true }),
  closeLicenseDialog: () => set({ licenseDialogOpen: false }),
  showToast: (text, icon = "check") => set({ toast: { text, icon } }),
  clearToast: () => set({ toast: null }),
}));
