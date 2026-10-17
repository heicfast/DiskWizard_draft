/**
 * The elevation gate — DiskWizard requires administrator privileges
 * (raw-disk access for scanning; the packaged app carries a
 * requireAdministrator manifest so Windows raises the real UAC prompt
 * at launch).
 *
 * In the packaged Tauri build the OS already elevated the process
 * before the window existed, so this gate is SKIPPED (isTauri()).
 * In browser-dev / mock mode the equivalent moment is simulated with
 * a faithful Windows-UAC-style dialog so the flow is exercisable and
 * screenshot-able — exactly the "second mock" the owner asked for.
 */
import { AnimatePresence, motion } from "framer-motion";
import { useEffect, useState } from "react";
import { isTauri } from "../lib/ipc";
import { useViewStore } from "../state/view";
import { BrandTile, LogoMark } from "../brand/BrandMark";
import { ShieldIcon } from "./icons";

export function ElevationGate() {
  const bootStage = useViewStore((s) => s.bootStage);
  const advanceBoot = useViewStore((s) => s.advanceBoot);
  const [declined, setDeclined] = useState(false);

  // Packaged build: the process launched elevated (UAC manifest) —
  // sail straight through.
  useEffect(() => {
    if (bootStage === "elevation" && isTauri()) {
      advanceBoot();
    }
  }, [bootStage, advanceBoot]);

  if (bootStage !== "elevation") return null;

  return (
    <AnimatePresence>
      <motion.div
        className="dw-elevation-veil"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        transition={{ duration: 0.3 }}
      >
        {declined ? (
          <motion.div
            className="dw-elevation-card"
            initial={{ scale: 0.96, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            transition={{ duration: 0.3, ease: [0.2, 0.9, 0.3, 1.05] }}
          >
            <div className="dw-elevation-card-head">
              <BrandTile size={28} />
              <span className="dw-elevation-title">DiskWizard</span>
            </div>
            <p className="dw-elevation-copy">
              DiskWizard needs administrator privileges to read your disks directly.
              Raw-device access is required for scanning and recovery — the app cannot
              continue without it.
            </p>
            <div className="dw-elevation-actions">
              <button type="button" className="dw-ink-button" onClick={() => setDeclined(false)}>
                Retry elevation
              </button>
            </div>
          </motion.div>
        ) : (
          <motion.div
            className="dw-uac"
            role="dialog"
            aria-modal="true"
            aria-label="User Account Control"
            initial={{ scale: 0.94, opacity: 0, y: 14 }}
            animate={{ scale: 1, opacity: 1, y: 0 }}
            transition={{ duration: 0.34, ease: [0.2, 0.9, 0.3, 1.06] }}
          >
            <div className="dw-uac-head">
              <span className="dw-uac-head-title">User Account Control</span>
              <span className="dw-uac-head-sub">
                {`This app is running in simulation mode — the packaged build elevates at launch.`}
              </span>
            </div>
            <div className="dw-uac-body">
              <div className="dw-uac-shield">
                <ShieldIcon size={26} strokeWidth={1.8} />
              </div>
              <div className="dw-uac-copy">
                <div className="dw-uac-line">Do you want to allow this app to make changes to your device?</div>
                <div className="dw-uac-app">
                  <LogoMark size={34} halo={false} />
                  <div>
                    <div className="dw-uac-app-name">DiskWizard</div>
                    <div className="dw-uac-app-pub">Verified publisher: DiskWizard Software</div>
                  </div>
                </div>
              </div>
            </div>
            <div className="dw-uac-actions">
              <button type="button" className="dw-uac-btn is-yes" onClick={advanceBoot}>
                Yes
              </button>
              <button type="button" className="dw-uac-btn" onClick={() => setDeclined(true)}>
                No
              </button>
            </div>
          </motion.div>
        )}
      </motion.div>
    </AnimatePresence>
  );
}
