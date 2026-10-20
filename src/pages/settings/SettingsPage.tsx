/**
 * Settings — general preferences, scan behavior, appearance (the
 * five-palette theme picker), and the license panel.
 */
import { useEffect } from "react";
import { motion } from "framer-motion";
import { CheckIcon, KeyIcon, SparkleIcon } from "lucide-react";
import { formatDate } from "../../lib/format";
import { THEME_LIST, useTheme } from "../../theme/useTheme";
import type { ThemeId } from "../../theme/themes";
import { useLicenseStore } from "../../state/license";
import { useSettingsStore } from "../../state/settings";
import { useViewStore } from "../../state/view";
import { useResultsStore } from "../../state/results";

export function SettingsPage() {
  const { theme, setTheme } = useTheme();
  const settings = useSettingsStore((s) => s.settings);
  const patch = useSettingsStore((s) => s.patch);
  const license = useLicenseStore((s) => s.status);
  const busy = useLicenseStore((s) => s.busy);
  const deactivate = useLicenseStore((s) => s.deactivate);
  const openLicenseDialog = useViewStore((s) => s.openLicenseDialog);
  const setResultsMode = useResultsStore((s) => s.setResultsMode);

  // Keep the results-mode store in sync when changed from Settings.
  useEffect(() => {
    setResultsMode(settings.resultsMode);
  }, [settings.resultsMode, setResultsMode]);

  const isPro = license?.isPro ?? false;

  return (
    <div className="dw-page dw-page-settings dw-scroll">
      <header className="dw-page-head">
        <div>
          <div className="dw-eyebrow">Settings</div>
          <h1 className="dw-page-title dw-display">Preferences</h1>
          <p className="dw-page-sub">Tune how DiskWizard scans, looks and stays licensed.</p>
        </div>
      </header>

      {/* License panel */}
      <section className="dw-section">
        <div className="dw-section-head">
          <h2 className="dw-section-title">License</h2>
          <span className={`dw-chip dw-chip-license ${isPro ? "is-pro" : ""}`}>{isPro ? "Pro" : "Free"}</span>
        </div>
        <div className="dw-card dw-license-panel">
          {isPro && license ? (
            <>
              <div className="dw-license-panel-id">
                <strong>{license.customerName}</strong>
                <span className="dw-num">{license.customerEmail}</span>
              </div>
              <dl className="dw-license-rows">
                <div className="dw-license-row">
                  <dt>Tier</dt>
                  <dd className="dw-text-cap">{license.tier}</dd>
                </div>
                <div className="dw-license-row">
                  <dt>Active until</dt>
                  <dd>{license.licenseExpiresAt === 0 ? "Lifetime" : formatDate(license.licenseExpiresAt)}</dd>
                </div>
              </dl>
              <div className="dw-license-actions">
                <button type="button" className="dw-outline-button dw-danger" onClick={() => void deactivate()} disabled={busy}>
                  Deactivate this device
                </button>
              </div>
            </>
          ) : (
            <>
              <div className="dw-license-panel-copy">
                <strong>DiskWizard Pro unlocks every recovery capability.</strong>
                Unlimited recovery, file repair, SD Card Rescue and lost-partition
                recovery. Disk health monitoring stays free.
              </div>
              <div className="dw-license-actions">
                <button type="button" className="dw-ink-button" onClick={openLicenseDialog}>
                  <KeyIcon size={14} />
                  Activate
                </button>
                <a className="dw-outline-button" href={license?.purchaseUrl ?? "https://diskwizard.app/pricing"} target="_blank" rel="noreferrer">
                  Purchase Licence
                </a>
              </div>
            </>
          )}
        </div>
      </section>

      {/* Appearance */}
      <section className="dw-section">
        <div className="dw-section-head">
          <h2 className="dw-section-title">Appearance</h2>
        </div>
        <div className="dw-theme-picker" role="radiogroup" aria-label="Theme">
          {THEME_LIST.map((t) => {
            const active = theme === t.id;
            return (
              <button
                key={t.id}
                type="button"
                role="radio"
                aria-checked={active}
                className={`dw-theme-swatch ${active ? "is-active" : ""}`}
                onClick={() => setTheme(t.id as ThemeId)}
                title={`${t.label} theme`}
              >
                <ThemeSwatchPreview id={t.id} />
                <span className="dw-theme-swatch-label">{t.label}</span>
                {active && (
                  <span className="dw-theme-swatch-check">
                    <CheckIcon size={10} strokeWidth={3.2} />
                  </span>
                )}
              </button>
            );
          })}
        </div>
        <p className="dw-setting-note">Blossom is DiskWizard's signature look — palettes crossfade app-wide, logo included.</p>
      </section>

      {/* Scanning */}
      <section className="dw-section">
        <div className="dw-section-head">
          <h2 className="dw-section-title">Scanning</h2>
        </div>
        <div className="dw-settings-list">
          <ToggleRow
            label="Essential results by default"
            hint="Filter duplicates and nonessential files (~65% fewer to review). You can switch to complete results anytime."
            checked={settings.resultsMode === "essential"}
            onChange={(on) => void patch({ resultsMode: on ? "essential" : "complete" })}
          />
          <ToggleRow
            label="Confirm before stopping a scan"
            hint="Ask before interrupting a deep scan."
            checked={settings.confirmStop}
            onChange={(on) => void patch({ confirmStop: on })}
          />
          <ToggleRow
            label="Repair files during recovery"
            hint="Automatically repair corrupted photos, videos and documents as they are recovered."
            checked={settings.enableRepairDefault}
            onChange={(on) => void patch({ enableRepairDefault: on })}
          />
          <ToggleRow
            label="Scan SD cards on connect"
            hint="Start SD Card Rescue automatically when a card is detected."
            checked={settings.scanOnSdInsert}
            onChange={(on) => void patch({ scanOnSdInsert: on })}
          />
        </div>
      </section>

      {/* General */}
      <section className="dw-section">
        <div className="dw-section-head">
          <h2 className="dw-section-title">General</h2>
        </div>
        <div className="dw-settings-list">
          <ToggleRow
            label="Launch at login"
            hint="Keep DiskWizard's disk guarding always on."
            checked={settings.launchAtLogin}
            onChange={(on) => void patch({ launchAtLogin: on })}
          />
        </div>
        <p className="dw-setting-note">
          <SparkleIcon size={12} /> Version {__APP_VERSION__} — simulated core build (UI preview).
        </p>
      </section>
    </div>
  );
}

function ToggleRow({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint: string;
  checked: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <label className="dw-setting-row">
      <span className="dw-setting-copy">
        <span className="dw-setting-label">{label}</span>
        <span className="dw-setting-hint">{hint}</span>
      </span>
      <span className="dw-toggle">
        <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
        <span className="dw-toggle-track" aria-hidden="true" />
      </span>
    </label>
  );
}

/** The mini-palette preview chip for each theme. */
function ThemeSwatchPreview({ id }: { id: ThemeId }) {
  const palettes: Record<ThemeId, [string, string, string]> = {
    blossom: ["#fbf1ee", "#f6e9e5", "#d8506b"],
    light: ["#ffffff", "#f5f5f7", "#ff6b4a"],
    dark: ["#1e1e20", "#28282b", "#ff7a5c"],
    ember: ["#201a15", "#292119", "#f2b04e"],
    tide: ["#101722", "#18202e", "#3fc5f2"],
  };
  const [bg, panel, ink] = palettes[id];
  return (
    <motion.span
      className="dw-theme-prev"
      style={{ background: bg, borderColor: panel }}
      layout
      whileTap={{ scale: 0.96 }}
    >
      <span style={{ background: panel }} />
      <span style={{ background: ink }} />
    </motion.span>
  );
}
