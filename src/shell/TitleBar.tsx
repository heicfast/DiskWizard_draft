/**
 * The custom frameless titlebar: drag region, quiet right cluster
 * (activate CTA, license chip, theme toggle) and the platform caption
 * controls (Windows caption buttons on the right). Deliberately
 * brandless — the sidebar carries the lockup (reference layout).
 */
import { useEffect, useState } from "react";
import { MoonIcon, SunIcon } from "lucide-react";
import { useTheme } from "../theme/useTheme";
import { useLicenseStore } from "../state/license";
import { useViewStore } from "../state/view";
import { IS_MACOS, IS_WINDOWS } from "../lib/platform";
import { SparkleIcon } from "./icons";

export function TitleBar() {
  const { isDark, toggle } = useTheme();
  const posture = useLicenseStore((s) => s.status?.posture ?? null);
  const openLicenseDialog = useViewStore((s) => s.openLicenseDialog);
  const isPro = posture === "pro" || posture === "grace";
  const [maximized, setMaximized] = useState<boolean | null>(null);

  useEffect(() => {
    // Native maximize state for the restore/maximize glyph (Tauri-only;
    // plain browser mode leaves the default glyph).
    let alive = true;
    import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => {
        const w = getCurrentWindow();
        void w.isMaximized().then((m) => {
          if (alive) setMaximized(m);
        });
        const unlisten = w.onResized(async () => {
          const m = await w.isMaximized().catch(() => null);
          if (alive && m !== null) setMaximized(m);
        });
        return unlisten;
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, []);

  const caption = async (verb: "min" | "max" | "close") => {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const w = getCurrentWindow();
      if (verb === "min") await w.minimize();
      else if (verb === "max") await w.toggleMaximize();
      else await w.close();
    } catch {
      /* plain browser (mock) — no-op */
    }
  };

  return (
    <header className="dw-titlebar" data-platform={IS_WINDOWS ? "windows" : IS_MACOS ? "macos" : "other"}>
      <div className="dw-titlebar-drag" data-tauri-drag-region />
      {IS_MACOS && <div className="dw-titlebar-traffic-reserve" aria-hidden="true" />}
      <div className="dw-titlebar-actions">
        {!isPro && (
          <button
            type="button"
            className="dw-titlebar-buy"
            onClick={openLicenseDialog}
            title="Activate DiskWizard"
          >
            <SparkleIcon size={13} />
            <span>Activate</span>
          </button>
        )}
        <button
          type="button"
          className={`dw-chip-license ${isPro ? "is-pro" : ""}`}
          onClick={openLicenseDialog}
          title={isPro ? "License status" : "Activate DiskWizard"}
        >
          {isPro ? "Pro" : "Free"}
        </button>
        <button
          type="button"
          className="dw-btn--icon dw-btn"
          onClick={toggle}
          title={isDark ? "Switch to light theme" : "Switch to dark theme"}
          aria-label={isDark ? "Switch to light theme" : "Switch to dark theme"}
          style={{ width: 30, height: 30 }}
        >
          {isDark ? <SunIcon size={15} /> : <MoonIcon size={15} />}
        </button>
      </div>
      {IS_WINDOWS && (
        <div className="dw-titlebar-caption">
          <button type="button" className="dw-caption-btn" onClick={() => caption("min")} aria-label="Minimize">
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
              <rect x="1" y="4.75" width="8" height="1" rx="0.5" fill="currentColor" />
            </svg>
          </button>
          <button type="button" className="dw-caption-btn" onClick={() => caption("max")} aria-label={maximized ? "Restore" : "Maximize"}>
            {maximized ? (
              <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
                <rect x="1.5" y="2.5" width="6" height="6" rx="1" fill="none" stroke="currentColor" strokeWidth="1" />
                <rect x="0.5" y="0.5" width="4" height="4" rx="1" fill="currentColor" />
              </svg>
            ) : (
              <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
                <rect x="1" y="1" width="8" height="8" rx="1.4" fill="none" stroke="currentColor" strokeWidth="1" />
              </svg>
            )}
          </button>
          <button type="button" className="dw-caption-btn dw-caption-close" onClick={() => caption("close")} aria-label="Close">
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
              <path d="M1.5 1.5l7 7M8.5 1.5l-7 7" stroke="currentColor" strokeWidth="1.1" strokeLinecap="round" fill="none" />
            </svg>
          </button>
        </div>
      )}
    </header>
  );
}
