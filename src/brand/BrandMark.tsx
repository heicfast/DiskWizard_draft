/**
 * DiskWizard brand marks — the theme-reactive logo system.
 *
 * The DiskGenie BrandMark architecture, adapted: there, the product
 * mark is a cutout sitting on a CSS-painted circle that uses
 * `var(--ink-grad)` — so the mark's BACKGROUND follows the selected
 * theme live and rides the same View-Transition crossfade as the rest
 * of the chrome.
 *
 * DiskWizard's approved identity is a rich 3D composition on a navy
 * squircle tile (brand kit: the wizard-hat + drive + restored-file
 * mark, no cutout exists and the artwork is tightly integrated with
 * its baked backdrop). The theme-reactive system therefore expresses
 * itself in two coordinated layers:
 *
 *  1. <BrandTile/> — the app-chrome mark: an --ink-grad painted
 *     rounded tile carrying the white wizard glyph (exactly the
 *     reference design's sidebar logo). Its background IS the theme
 *     gradient: switch theme → the tile repaints → the View
 *     Transition crossfades the whole app in one buttery step.
 *
 *  2. <LogoMark/> — the full approved 3D logo (splash, About): the
 *     artwork itself stays brand-true, while its AMBIENT backdrop
 *     (the --ambient-grad wash + themed glow) repaints with the
 *     theme.
 */
import logoPrimary from "../assets/logo-primary.png";

/** The wizard-hat glyph — the DiskWizard identity reduced to its
 * clean geometric core for small surfaces (white on the ink tile).
 * Drawn on a 24-grid so it scales crisply from 14px chips to the
 * 96px splash tile. */
export function WizardGlyph({ size = 16 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
      focusable="false"
    >
      {/* Cone (bent tip) */}
      <path d="M12.6 2.6c-1.7 3.1-3.4 7.4-5 11.9l3.1-1 .3 2.4c1.7-.9 3.5-1.5 5.5-1.9-1.3-4.2-2.6-8-3.9-11.4z" />
      {/* Brim */}
      <path d="M3.6 18.7c0-1.5 3.8-2.8 8.4-2.8s8.4 1.3 8.4 2.8-3.8 2.7-8.4 2.7-8.4-1.2-8.4-2.7zm1.6 0c0 .9 3 1.9 6.8 1.9s6.8-1 6.8-1.9-3-1.9-6.8-1.9-6.8 1-6.8 1.9z" />
      {/* Sparkle */}
      <path d="M15.6 5.1l.55 1.45 1.45.55-1.45.55-.55 1.45-.55-1.45-1.45-.55 1.45-.55z" />
    </svg>
  );
}

/**
 * <BrandTile/> — the theme-reactive product mark for app chrome
 * (titlebar, sidebar, dialogs, empty states). The tile background is
 * painted with var(--ink-grad) — the exact gradient every ink-filled
 * control uses — so the mark follows the selected theme live and
 * rides the same View-Transition crossfade as the rest of the chrome
 * (the token change re-paints the background; the transition wraps
 * the whole app). The glow matches the ink-button language
 * (--ink-glow), keeping the mark in symmetry with the interactive
 * ink surfaces.
 */
export function BrandTile({
  size = 36,
  radius,
  glyphScale = 1,
  title = "DiskWizard",
}: {
  size?: number;
  radius?: number;
  glyphScale?: number;
  title?: string;
}) {
  const r = radius ?? Math.round(size * 0.29);
  return (
    <span
      className="dw-brand-tile"
      style={{ width: size, height: size, borderRadius: r }}
      role="img"
      aria-label={title}
    >
      <WizardGlyph size={Math.round(size * 0.62 * glyphScale)} />
    </span>
  );
}

/**
 * <LogoMark/> — the full approved 3D logo (wizard hat + drive +
 * restored files on the navy squircle). Brand-true artwork; the
 * ambient treatment around it (halo + glow, painted with the theme's
 * ink tokens) is what changes with the theme.
 */
export function LogoMark({
  size = 96,
  halo = true,
  className,
}: {
  size?: number;
  halo?: boolean;
  className?: string;
}) {
  return (
    <span className={`dw-logo-mark ${halo ? "dw-logo-mark-halo" : ""} ${className ?? ""}`} style={{ width: size, height: size }}>
      <img src={logoPrimary} alt="DiskWizard" draggable={false} />
    </span>
  );
}

/** Sidebar lockup: BrandTile + wordmark + product descriptor (the
 * reference design's sidebar header). */
export function BrandLockup({ compact = false }: { compact?: boolean }) {
  return (
    <span className="dw-brand-lockup">
      <BrandTile size={compact ? 30 : 36} />
      {!compact && (
        <span className="dw-brand-lockup-text">
          <span className="dw-brand-lockup-name dw-display">DiskWizard</span>
          <span className="dw-brand-lockup-sub">Data Recovery</span>
        </span>
      )}
    </span>
  );
}
