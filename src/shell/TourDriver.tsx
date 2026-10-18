/**
 * The CI/agent tour driver (the DiskGenie agent-SDK analog):
 * DISKWIZARD_TOUR=1 (or ?tour=1 in browser-dev) auto-cycles through
 * every page, phase and dialog state with a per-step dwell so
 * screenshot passes capture every state of the REAL app without
 * interactive automation. It stamps `DiskWizard · tNN-name` into the
 * title — document.title in the browser, the NATIVE window title in
 * the packaged build (Tauri 2 does not auto-sync document.title, so
 * the driver calls the window API itself) — the harness polls the
 * native title and shoots exactly one named frame per step.
 */
import { useEffect } from "react";
import { useViewStore } from "../state/view";
import { useScanStore } from "../state/scan";
import { useResultsStore } from "../state/results";
import { useDevicesStore } from "../state/devices";
import { invoke, isTauri } from "../lib/ipc";
import type { DevHooks } from "../types";

const DWELL_MS = 2_800;

interface Step {
  name: string;
  apply: () => void | Promise<void>;
  dwell?: number;
}

/** Stamp the sync signal into BOTH title channels: document.title
 * (browser-dev, where agent-browser polls it) and the native window
 * title via the Tauri window API (packaged builds — WebView2 does not
 * propagate document.title on its own; the donor's own artifacts show
 * its harness silently falling back to cadence capture for exactly
 * this reason). Fire-and-forget: the harness polls, it never blocks
 * the tour. */
function tourTitle(i: number, name: string) {
  const slug = name.replace(/[^a-z0-9]+/gi, "-").replace(/^-+|-+$/g, "").toLowerCase();
  const title = `DiskWizard · t${String(i).padStart(2, "0")}-${slug}`;
  document.title = title;
  if (isTauri()) {
    void import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => getCurrentWindow().setTitle(title))
      .catch(() => undefined);
  }
}

/** The end marker goes through the same dual channel. */
function tourEndTitle() {
  document.title = "DiskWizard · tour-complete";
  if (isTauri()) {
    void import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => getCurrentWindow().setTitle("DiskWizard · tour-complete"))
      .catch(() => undefined);
  }
}

export function TourDriver() {
  useEffect(() => {
    let disposed = false;
    void (async () => {
      const hooks = await invoke<DevHooks>("get_dev_hooks").catch(
        () => ({ tour: false, tourMode: null, tourLicenseKey: null, scan: null }) as DevHooks,
      );
      if (!hooks?.tour || disposed) return;
      await runTour(hooks);
    })();
    return () => {
      disposed = true;
    };
  }, []);
  return null;
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => window.setTimeout(r, ms));
}

async function runTour(_hooks: DevHooks): Promise<void> {
  const view = useViewStore.getState();
  const scan = useScanStore.getState();
  const devices = useDevicesStore.getState();
  await devices.load();

  const steps: Step[] = [
    { name: "recover", apply: () => view.setPage("recover") },
    { name: "recover-hover-states", apply: () => undefined },
    // Scan start (unlicensed → the gate fires → dialog):
    { name: "scan-gate-dialog", apply: () => void scan.start("usb-F") },
    { name: "license-dialog-close", apply: () => useViewStore.getState().closeLicenseDialog(), dwell: 1.2 },
    // Seed Pro (license_sim_set — the ci-license-sim analog) and scan for real:
    {
      name: "license-pro-seeded",
      apply: () => void invoke("license_sim_set", { pro: true }),
      dwell: 1.4,
    },
    { name: "scan-start", apply: () => void scan.start("usb-F") },
    { name: "scan-live", apply: () => undefined, dwell: 6 },
    { name: "results-scanning", apply: () => useViewStore.getState().setPage("results"), dwell: 4 },
    { name: "scan-paused", apply: () => void useScanStore.getState().pause() },
    { name: "scan-resumed", apply: () => void useScanStore.getState().resume(), dwell: 1.4 },
    // Wait for completion then walk the results states:
    {
      name: "scan-complete",
      apply: async () => {
        for (let i = 0; i < 90; i++) {
          if (useScanStore.getState().done) return;
          await sleep(500);
        }
      },
      dwell: 8,
    },
    { name: "results-all", apply: () => useResultsStore.getState().setCategory("all") },
    { name: "results-images", apply: () => useResultsStore.getState().setCategory("images") },
    { name: "results-videos", apply: () => useResultsStore.getState().setCategory("videos") },
    { name: "results-documents", apply: () => useResultsStore.getState().setCategory("documents") },
    { name: "results-tree-types", apply: () => useResultsStore.getState().setTreeMode("types") },
    { name: "results-tree-folders", apply: () => useResultsStore.getState().setTreeMode("folders") },
    { name: "results-search", apply: () => useResultsStore.getState().setSearch("photo") },
    { name: "results-search-clear", apply: () => useResultsStore.getState().setSearch("") },
    { name: "results-filter-open", apply: () => useResultsStore.getState().setFilterOpen(true) },
    {
      name: "results-filter-applied",
      apply: () => useResultsStore.getState().patchFilter({ sizeMin: 1024 * 1024 }),
    },
    {
      name: "results-filter-reset",
      apply: () => {
        useResultsStore.getState().resetFilter();
        useResultsStore.getState().setFilterOpen(false);
      },
    },
    { name: "results-complete-mode", apply: () => useResultsStore.getState().setResultsMode("complete") },
    { name: "results-essential-mode", apply: () => useResultsStore.getState().setResultsMode("essential") },
    // Tools (free disk health):
    { name: "tools-disk-health", apply: () => useViewStore.getState().setPage("tools") },
    // Cloud:
    { name: "cloud", apply: () => useViewStore.getState().setPage("cloud"), dwell: 2.2 },
    // Settings + theme sweep (each palette's crossfade gets a frame):
    { name: "settings", apply: () => useViewStore.getState().setPage("settings") },
    // Help:
    { name: "help", apply: () => useViewStore.getState().setPage("help") },
  ];

  for (let i = 0; i < steps.length; i++) {
    const step = steps[i];
    tourTitle(i + 1, step.name);
    await step.apply();
    window.dispatchEvent(new CustomEvent("dw-tour-step", { detail: { index: i, name: step.name } }));
    (window as unknown as Record<string, unknown>).__DW_TOUR_STATE__ = { step: i + 1, name: step.name };
    await sleep((step.dwell ?? 1) * DWELL_MS);
  }

  // End marker — the harness knows the tour is over.
  tourEndTitle();
  (window as unknown as Record<string, unknown>).__DW_TOUR_STATE__ = { step: -1, name: "complete" };
}
