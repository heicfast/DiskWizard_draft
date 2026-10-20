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
 *
 * TWO CORES, ONE DRIVER (QA-1): `get_dev_hooks.core` tells which
 * command surface answered. The SIM core drives the synthetic device
 * (fixed id, deterministic fills). The REAL core picks a LIVE device
 * row (the CI job seeds a VHD labeled e.g. "DWTEST" and passes
 * DISKWIZARD_TOUR_DEVICE=DWTEST), runs a REAL scan (quick + deep on
 * actual NTFS), walks the results, and drives the REAL recovery
 * funnel — select-all → destination rows → export → progress →
 * summary — so the screenshots prove the production pipeline, not a
 * simulation of it. The unlicensed gate is still captured first
 * (license_sim_set seeds Pro only AFTER the gate frame).
 */
import { useEffect } from "react";
import { useViewStore } from "../state/view";
import { useScanStore } from "../state/scan";
import { useResultsStore } from "../state/results";
import { useDevicesStore } from "../state/devices";
import { invoke, isTauri } from "../lib/ipc";
import type { DevHooks, StorageDevice } from "../types";

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

/** Fail LOUDLY (the QA harness's honesty rule): stamp the reserved
 * t99 failure frame — the harness captures it like any step — and
 * stop the tour. The workflow's fail-frame check turns this into a
 * precise CI failure naming the broken funnel stage, instead of a
 * silent no-op surfacing minutes later as RECOVERY_OUTPUT_MISSING
 * with zero diagnostics (exactly how the first tour run died). */
function tourFail(reason: string): void {
  const slug =
    reason.replace(/[^a-z0-9]+/gi, "-").replace(/^-+|-+$/g, "").toLowerCase().slice(0, 48) ||
    "unknown";
  const title = `DiskWizard · t99-tour-fail-${slug}`;
  document.title = title;
  if (isTauri()) {
    void import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => getCurrentWindow().setTitle(title))
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

/** Pick the REAL scan target: the device row matching the tour filter
 * (the CI job's VHD label), else the first external/other row with a
 * filesystem the core can scan. Returns "" when the inventory is
 * empty — the scan step then surfaces the honest failure state. */
function pickRealDevice(hooks: DevHooks): string {
  const inv = useDevicesStore.getState().inventory;
  const rows: StorageDevice[] = [
    ...(inv?.externalDevices ?? []),
    ...(inv?.localDisks ?? []),
    ...(inv?.diskImages ?? []),
  ];
  const filter = hooks.deviceFilter?.trim().toLowerCase();
  if (filter) {
    const hit = rows.find(
      (r) =>
        r.name.toLowerCase().includes(filter) ||
        r.id.toLowerCase().includes(filter),
    );
    if (hit) return hit.id;
  }
  return rows[0]?.id ?? "";
}

/** Wait for the REAL scan to finish (quick + deep on the CI VHD is
 * fast, but be generous — deep carve on a slow runner can take a
 * while; the UI keeps rendering live progress meanwhile). */
async function waitForScanDone(maxMs: number): Promise<boolean> {
  const deadline = Date.now() + maxMs;
  while (Date.now() < deadline) {
    if (useScanStore.getState().done) return true;
    await sleep(500);
  }
  return useScanStore.getState().done !== null;
}

/** Wait for the REAL recovery to complete (recover-done swaps the
 * progress panel for the summary). */
async function waitForRecoveryDone(maxMs: number): Promise<boolean> {
  const deadline = Date.now() + maxMs;
  while (Date.now() < deadline) {
    if (useResultsStore.getState().recoverSummary) return true;
    await sleep(500);
  }
  return useResultsStore.getState().recoverSummary !== null;
}

async function runTour(hooks: DevHooks): Promise<void> {
  const view = useViewStore.getState();
  const scan = useScanStore.getState();
  const devices = useDevicesStore.getState();
  await devices.load();

  const isReal = hooks.core === "real";
  const deviceId = isReal ? pickRealDevice(hooks) : "usb-F";
  // The REAL core's scan waits scale with the actual engine (256 MB
  // VHD: seconds for quick, tens for the carve pass) — give it room.
  const scanWaitMs = isReal ? 300_000 : 45_000;

  const steps: Step[] = [
    { name: "recover", apply: () => view.setPage("recover") },
    { name: "recover-hover-states", apply: () => undefined },
    // Scan start (unlicensed → the gate fires → dialog):
    { name: "scan-gate-dialog", apply: () => void scan.start(deviceId) },
    { name: "license-dialog-close", apply: () => useViewStore.getState().closeLicenseDialog(), dwell: 1.2 },
    // Seed Pro (license_sim_set — the ci-tour-gated real arm or the
    // sim core's native hook) and scan for real:
    {
      name: "license-pro-seeded",
      apply: () => void invoke("license_sim_set", { pro: true }),
      dwell: 1.4,
    },
    { name: "scan-start", apply: () => void useScanStore.getState().start(deviceId) },
    { name: "scan-live", apply: () => undefined, dwell: isReal ? 10 : 6 },
    { name: "results-scanning", apply: () => useViewStore.getState().setPage("results"), dwell: 4 },
    { name: "scan-paused", apply: () => void useScanStore.getState().pause() },
    { name: "scan-resumed", apply: () => void useScanStore.getState().resume(), dwell: 1.4 },
    // Wait for completion then walk the results states:
    {
      name: "scan-complete",
      apply: async () => {
        await waitForScanDone(scanWaitMs);
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
  ];

  if (isReal) {
    // THE REAL RECOVERY FUNNEL (QA-1's release-truth walk): select
    // everything the scan found, open the destination rows (real
    // volumes, the source flagged), pick a non-source drive, run the
    // export, and land on the summary — the workflow then
    // byte-compares what landed on disk against the seeds. Every
    // stage asserts its precondition and fails LOUDLY (t99 frame)
    // instead of silently skipping the recovery.
    steps.push(
      {
        name: "recovery-select-all",
        apply: () => {
          const snap = useResultsStore.getState().snapshot;
          const files = (snap?.nodes ?? []).filter((n) => !n.isDir);
          if (files.length === 0) {
            tourFail(snap ? "no-files-in-snapshot" : "results-snapshot-null");
            return;
          }
          useResultsStore.getState().selectAll(files.map((n) => n.id));
        },
      },
      { name: "recovery-destination-open", apply: () => void useResultsStore.getState().openDestination(), dwell: 2 },
      {
        name: "recovery-destination-chosen",
        apply: () => {
          const rs = useResultsStore.getState();
          const rows = rs.destinations;
          if (rows.length === 0) {
            tourFail("no-destination-rows");
            return;
          }
          const chosen = rows.find((d) => !d.isSource) ?? rows[0];
          rs.chooseDestination(chosen.id);
        },
        dwell: 1.4,
      },
      {
        name: "recovery-started",
        apply: async () => {
          const rs = useResultsStore.getState();
          const snap = rs.snapshot;
          const files = (snap?.nodes ?? []).filter((n) => !n.isDir);
          const ids = [...rs.selectedIds];
          const bytes = files
            .filter((n) => rs.selectedIds.has(n.id))
            .reduce((a, n) => a + n.size, 0);
          const dest = rs.destinations.find((d) => d.id === rs.chosenDestinationId);
          if (!dest) {
            tourFail("no-destination-chosen");
            return;
          }
          if (ids.length === 0) {
            tourFail("selection-empty-at-recovery");
            return;
          }
          // Drive the store's funnel so the progress/summary states
          // render exactly as a user would see them.
          rs.chooseDestination(dest.id);
          await rs.beginRecovery(ids, bytes);
        },
        dwell: 2,
      },
      {
        name: "recovery-progress",
        apply: async () => {
          const ok = await waitForRecoveryDone(180_000);
          if (!ok) {
            tourFail("recovery-no-summary-180s");
          }
        },
        dwell: 6,
      },
      {
        name: "recovery-complete",
        apply: () => {
          const summary = useResultsStore.getState().recoverSummary;
          if (!summary) {
            tourFail("recovery-summary-missing");
            return;
          }
          if (summary.error) {
            tourFail(`recovery-cmd-${summary.error}`);
            return;
          }
          if (summary.failedFiles > 0) {
            tourFail(`recovery-${summary.failedFiles}-files-failed`);
          }
        },
        dwell: 2,
      },
    );
  }

  steps.push(
    // Tools (free disk health):
    { name: "tools-disk-health", apply: () => useViewStore.getState().setPage("tools") },
    // Settings + theme sweep (each palette's crossfade gets a frame):
    { name: "settings", apply: () => useViewStore.getState().setPage("settings") },
    // Help:
    { name: "help", apply: () => useViewStore.getState().setPage("help") },
  );

  for (let i = 0; i < steps.length; i++) {
    const step = steps[i];
    tourTitle(i + 1, step.name);
    await step.apply();
    window.dispatchEvent(new CustomEvent("dw-tour-step", { detail: { index: i, name: step.name } }));
    (window as unknown as Record<string, unknown>).__DW_TOUR_STATE__ = { step: i + 1, name: step.name };
    await sleep((step.dwell ?? 1) * DWELL_MS);
    // A failed stage stamped the t99 fail frame — stop advancing so
    // the failure is the tour's last word (the end marker still lets
    // the harness break out of its capture loop immediately).
    if (document.title.includes("t99-tour-fail")) {
      break;
    }
  }

  // End marker — the harness knows the tour is over.
  tourEndTitle();
  (window as unknown as Record<string, unknown>).__DW_TOUR_STATE__ = { step: -1, name: "complete" };
}
