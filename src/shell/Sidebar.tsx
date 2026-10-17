/**
 * The sidebar — DiskWizard's primary navigation (the reference
 * design): brand lockup, six nav rows, the live scan-status widget
 * while a scan is active (progress ring, found counters, pause/stop),
 * and the help card pinned at the bottom.
 */
import { AnimatePresence, motion } from "framer-motion";
import {
  ArrowRightIcon,
  HardDriveIcon,
  HelpCircleIcon,
  ListIcon,
  PauseIcon,
  PlayIcon,
  StopIcon,
  SupportIcon,
} from "./icons";
import { formatBytes, formatCountFull } from "../lib/format";
import { scanActive, useScanStore } from "../state/scan";
import { useViewStore } from "../state/view";
import type { PageId } from "../types";
import { BrandLockup } from "../brand/BrandMark";

const NAV: { id: PageId; label: string; icon: typeof HardDriveIcon }[] = [
  { id: "recover", label: "Recover Data", icon: HardDriveIcon },
  { id: "results", label: "Scan Results", icon: ListIcon },
  { id: "tools", label: "Disk Tools", icon: SupportIcon },
  { id: "cloud", label: "Cloud Recovery", icon: HardDriveIcon },
  { id: "settings", label: "Settings", icon: HelpCircleIcon },
  { id: "help", label: "Help Center", icon: HelpCircleIcon },
];

const NAV_ICONS: Record<PageId, typeof HardDriveIcon> = {
  recover: HardDriveIcon,
  results: ListIcon,
  tools: SupportIcon,
  cloud: HardDriveIcon,
  settings: HelpCircleIcon,
  help: HelpCircleIcon,
};

export function Sidebar() {
  const page = useViewStore((s) => s.page);
  const setPage = useViewStore((s) => s.setPage);
  const progress = useScanStore((s) => s.progress);
  const pause = useScanStore((s) => s.pause);
  const resume = useScanStore((s) => s.resume);
  const stop = useScanStore((s) => s.stop);
  const active = scanActive(progress);
  const paused = progress?.phase === "paused";

  return (
    <nav className="dw-sidebar" aria-label="Primary">
      <div className="dw-sidebar-head">
        <BrandLockup />
      </div>

      <ul className="dw-nav-list">
        {NAV.map((item) => {
          const Icon = NAV_ICONS[item.id];
          const selected = page === item.id;
          return (
            <li key={item.id}>
              <button
                type="button"
                className={`dw-nav-item ${selected ? "is-active" : ""}`}
                onClick={() => setPage(item.id)}
                aria-current={selected ? "page" : undefined}
              >
                <Icon size={17} strokeWidth={1.8} className="dw-nav-icon" aria-hidden="true" />
                <span className="dw-truncate">{item.label}</span>
                {item.id === "results" && active && <span className="dw-nav-live-dot" aria-label="Scan active" />}
              </button>
            </li>
          );
        })}
      </ul>

      <AnimatePresence>
        {active && progress && (
          <motion.div
            className="dw-scan-widget"
            initial={{ opacity: 0, y: 14, scale: 0.97 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 10, scale: 0.97 }}
            transition={{ duration: 0.28, ease: [0.2, 0.8, 0.3, 1] }}
          >
            <ScanRing percent={progress.percent} paused={paused} />
            <div className="dw-scan-widget-body">
              <div className="dw-scan-widget-title">
                {paused ? "Scan Paused" : progress.phase === "organizing" ? "Organizing files" : progress.phase === "preparing" ? "Preparing scan" : "Deep scanning"}
                <span className="dw-num dw-scan-widget-pct">{Math.round(progress.percent)}%</span>
              </div>
              <div className="dw-scan-widget-stats dw-num">
                {formatCountFull(progress.stats.filesFound)} files found ({formatBytes(progress.stats.bytesFound)})
              </div>
              <div className="dw-scan-widget-actions">
                <button
                  type="button"
                  className="dw-ghost-button"
                  onClick={() => (paused ? void resume() : void pause())}
                >
                  {paused ? <PlayIcon size={12} /> : <PauseIcon size={12} />}
                  {paused ? "Resume" : "Pause"}
                </button>
                <button type="button" className="dw-ghost-button dw-stop" onClick={() => void stop()}>
                  <StopIcon size={11} />
                  Stop
                </button>
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <div className="dw-sidebar-foot">
        <div className="dw-help-card">
          <div className="dw-help-card-art" aria-hidden="true">
            <SupportIcon size={20} strokeWidth={1.7} />
          </div>
          <div className="dw-help-card-copy">
            <div className="dw-help-card-title">Need help?</div>
            <div className="dw-help-card-sub">Guides, FAQs and support.</div>
          </div>
          <button
            type="button"
            className="dw-help-card-btn"
            onClick={() => setPage("help")}
          >
            Open Help Center
            <ArrowRightIcon size={12} />
          </button>
        </div>
      </div>
    </nav>
  );
}

/** The 44px progress ring (the sidebar widget's gauge). */
export function ScanRing({ percent, paused, size = 44 }: { percent: number; paused?: boolean; size?: number }) {
  const r = (size - 5) / 2;
  const c = 2 * Math.PI * r;
  const p = Math.min(100, Math.max(0, percent));
  return (
    <div className={`dw-scan-ring ${paused ? "is-paused" : ""}`} style={{ width: size, height: size }}>
      <svg width={size} height={size} aria-hidden="true">
        <circle className="dw-scan-ring-track" cx={size / 2} cy={size / 2} r={r} fill="none" strokeWidth="4" />
        <circle
          className="dw-scan-ring-fill"
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          strokeWidth="4"
          strokeLinecap="round"
          strokeDasharray={c}
          strokeDashoffset={c * (1 - p / 100)}
          transform={`rotate(-90 ${size / 2} ${size / 2})`}
        />
      </svg>
      <span className="dw-scan-ring-num dw-num">{Math.round(p)}</span>
    </div>
  );
}
