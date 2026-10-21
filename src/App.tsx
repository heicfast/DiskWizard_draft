/**
 * DiskWizard app shell: custom titlebar → body (sidebar | page), with
 * the boot sequence (splash → elevation → app), the six pages, the
 * license dialog + gate coordination, and the CI tour driver
 * (DISKWIZARD_TOUR=1 / ?tour=1 — the agent screenshot channel).
 */
import { useEffect, useRef, type ReactNode } from "react";
import { AnimatePresence } from "framer-motion";
import { TitleBar } from "./shell/TitleBar";
import { Sidebar } from "./shell/Sidebar";
import { Splash } from "./shell/Splash";
import { ElevationGate } from "./shell/ElevationGate";
import { TourDriver } from "./shell/TourDriver";
import { AppErrorBoundary } from "./shell/AppErrorBoundary";
import { LicenseDialog } from "./components/LicenseDialog";
import { Toast } from "./components/Toast";
import { RecoverPage } from "./pages/recover/RecoverPage";
import { ResultsPage } from "./pages/results/ResultsPage";
import { ToolsPage } from "./pages/tools/ToolsPage";
import { SettingsPage } from "./pages/settings/SettingsPage";
import { HelpPage } from "./pages/help/HelpPage";
import { useViewStore } from "./state/view";
import { attachLicenseEvents, useLicenseStore } from "./state/license";
import { attachScanEvents } from "./state/scan";
import { attachDeviceEvents, useDevicesStore } from "./state/devices";
import { attachRecoverEvents } from "./state/results";
import { useSettingsStore } from "./state/settings";
import { useScanStore } from "./state/scan";
import "./theme/tokens.css";
import "./styles/fonts.css";
import "./styles/base.css";
import "./styles/shell.css";
import "./styles/sidebar.css";
import "./styles/pages.css";
import "./styles/results.css";
import "./styles/dialogs.css";

/** Page-swap wrapper (CSS-driven settle-in): the entering view fades
 * 0→1 over the solid page background; the old view unmounts instantly
 * (conditional render — no exit tween, no lingering layer, so two
 * content layers are never on screen at once). */
function PageSwap({ children }: { children: ReactNode }) {
  return <div className="dw-page-swap">{children}</div>;
}

function AppShell() {
  const bootStage = useViewStore((s) => s.bootStage);
  const page = useViewStore((s) => s.page);
  const setPage = useViewStore((s) => s.setPage);
  const licenseGate = useLicenseStore((s) => s.gate);
  const dismissGate = useLicenseStore((s) => s.dismissGate);
  const openLicenseDialog = useViewStore((s) => s.openLicenseDialog);
  const loadDevices = useDevicesStore((s) => s.load);
  const loadLicense = useLicenseStore((s) => s.load);
  const loadSettings = useSettingsStore((s) => s.load);
  const progress = useScanStore((s) => s.progress);

  // Bootstrap: stores + event wiring (idempotent attachments).
  useEffect(() => {
    attachLicenseEvents();
    attachScanEvents();
    attachDeviceEvents();
    attachRecoverEvents();
    void loadLicense();
    void loadDevices();
    void loadSettings();
  }, [loadLicense, loadDevices, loadSettings]);

  // The command-gate refusal (ACTIVATION_REQUIRED) opens activation.
  useEffect(() => {
    if (licenseGate) {
      openLicenseDialog();
      dismissGate();
    }
  }, [licenseGate, openLicenseDialog, dismissGate]);

  // A NEWLY STARTED scan lands the user on the results page once (the
  // scan runs inside the results view — the CDetailPage architecture);
  // afterwards the sidebar stays free to browse (health, settings…)
  // while the scan widget keeps reporting live.
  const lastAutoSwitch = useRef(0);
  useEffect(() => {
    if (!progress) return;
    if (progress.generation !== lastAutoSwitch.current) {
      lastAutoSwitch.current = progress.generation;
      if (page !== "results") setPage("results");
    }
  }, [progress, page, setPage]);

  const booted = bootStage === "ready";

  return (
    <div className="dw-app" data-booted={booted ? "y" : "n"}>
      <TitleBar />
      <div className="dw-body">
        <Sidebar />
        <main className="dw-main" id="dw-main">
          <PageSwap>
            {page === "recover" && <RecoverPage />}
            {page === "results" && <ResultsPage />}
            {page === "tools" && <ToolsPage />}
            {page === "settings" && <SettingsPage />}
            {page === "help" && <HelpPage />}
          </PageSwap>
        </main>
      </div>

      <LicenseDialog />
      <Toast />
      <ElevationGate />
      <AnimatePresence>{bootStage === "splash" && <Splash key="splash" />}</AnimatePresence>
      <TourDriver />
    </div>
  );
}

export default function App() {
  return (
    <AppErrorBoundary>
      <AppShell />
    </AppErrorBoundary>
  );
}
