/**
 * Sidebar — DiskWizard's primary navigation (the reference design):
 * brand lockup, the 4 nav rows (Cloud Recovery removed — owner
 * directive; Help lives in the bottom card, not the nav), the live
 * scan widget while a scan runs, and the help card pinned at the
 * bottom.
 */
import { AnimatePresence, motion } from "framer-motion";
import {
  HardDriveIcon,
  ListIcon,
  SettingsIcon,
  SupportIcon,
} from "../icons";
import { scanActive, useScanStore } from "../../state/scan";
import { useViewStore } from "../../state/view";
import type { PageId } from "../../types";
import { BrandLockup } from "../../brand/BrandMark";
import { ScanWidget } from "./ScanWidget";
import { HelpCard } from "./HelpCard";

interface NavEntry {
  id: PageId;
  label: string;
  icon: typeof HardDriveIcon;
}

/** The 4 primary destinations (reference layout). */
const NAV: NavEntry[] = [
  { id: "recover", label: "Recover Data", icon: HardDriveIcon },
  { id: "results", label: "Scan Results", icon: ListIcon },
  { id: "tools", label: "Disk Tools", icon: SupportIcon },
  { id: "settings", label: "Settings", icon: SettingsIcon },
];

export function Sidebar() {
  const page = useViewStore((s) => s.page);
  const setPage = useViewStore((s) => s.setPage);
  const progress = useScanStore((s) => s.progress);
  const active = scanActive(progress);

  return (
    <nav className="dw-sidebar" aria-label="Primary">
      <div className="dw-brand-lockup">
        <BrandLockup />
      </div>

      <ul className="dw-nav-list">
        {NAV.map((item) => {
          const Icon = item.icon;
          const selected = page === item.id;
          return (
            <li key={item.id}>
              <button
                type="button"
                className={`dw-nav-item ${selected ? "is-active" : ""}`}
                onClick={() => setPage(item.id)}
                aria-current={selected ? "page" : undefined}
              >
                <Icon size={18} strokeWidth={1.8} className="dw-nav-icon" aria-hidden="true" />
                <span className="dw-truncate">{item.label}</span>
                {item.id === "results" && active && (
                  <span className="dw-nav-live-dot" aria-label="Scan active" />
                )}
              </button>
            </li>
          );
        })}
      </ul>

      {/* The bottom stack: the live scan widget (when a scan runs) sits
       * above the help card; both pin to the sidebar's floor. */}
      <div className="dw-sidebar-bottom">
        <AnimatePresence>
          {active && progress && (
            <motion.div
              key="scan-widget"
              initial={{ opacity: 0, y: 14, scale: 0.97 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: 10, scale: 0.97 }}
              transition={{ duration: 0.28, ease: [0.2, 0.8, 0.3, 1] }}
            >
              <ScanWidget />
            </motion.div>
          )}
        </AnimatePresence>
        <HelpCard />
      </div>
    </nav>
  );
}
