/**
 * DiskWizard theme registry — borrowed as-is from DiskGenie (same
 * gradients, colours, palettes) with ONE product decision changed: the
 * app's shipped default is **blossom** (the cherry-blossom reference
 * design), not light.
 *
 * The two-layer attribute system:
 *
 *   data-theme="<id>"       the concrete palette (token block in
 *                           tokens.css) — what the user picks.
 *   data-scheme="light|dark"  the FAMILY the palette belongs to;
 *                           everything that only cares about
 *                           light-vs-dark reads this.
 *
 * `light` and `dark` remain the canonical pair (the topbar quick toggle
 * flips between exactly those two). Palette directions (researched
 * against premium precedent; none touch the generic violet/purple
 * AI-gradient territory, per the owner's direction):
 *   light    neutral white + coral
 *   dark     neutral graphite + coral
 *   ember    warm espresso surfaces + honey-gold ink
 *   tide     deep marine surfaces + lagoon-cyan ink
 *   blossom  blush cream surfaces + rose ink  ← DiskWizard default
 */

/** The persisted theme ids, in picker order. */
export const THEME_IDS = ["blossom", "light", "dark", "ember", "tide"] as const;

export type ThemeId = (typeof THEME_IDS)[number];

export type ThemeFamily = "light" | "dark";

/** Display metadata — the picker's labels/tooltips + the toggle hint. */
export interface ThemeMeta {
  id: ThemeId;
  label: string;
  family: ThemeFamily;
}

export const THEMES: Record<ThemeId, ThemeMeta> = {
  blossom: { id: "blossom", label: "Blossom", family: "light" },
  light: { id: "light", label: "Light", family: "light" },
  dark: { id: "dark", label: "Dark", family: "dark" },
  ember: { id: "ember", label: "Ember", family: "dark" },
  tide: { id: "tide", label: "Tide", family: "dark" },
};

/** The picker's ordered list (THEME_IDS guaranteed present). */
export const THEME_LIST: ThemeMeta[] = THEME_IDS.map((id) => THEMES[id]);

/** The app's shipped default (DiskWizard product decision). */
export const DEFAULT_THEME: ThemeId = "blossom";

/** Family lookup for the scheme attribute + dark-family branches. */
export function familyOf(id: ThemeId): ThemeFamily {
  return THEMES[id].family;
}

/** The pre-mount/apply DOM writes for one theme — BOTH attributes,
 * always together (every consumer of data-scheme depends on this
 * pairing, including the theme-crossfade View Transitions). */
export function applyThemeAttributes(id: ThemeId): void {
  const root = document.documentElement;
  root.setAttribute("data-theme", id);
  root.setAttribute("data-scheme", familyOf(id));
}

/** The native window theme the Tauri side should match (families map
 * onto the two values setTheme accepts; custom themes follow their
 * family so native menus/tooltips stay coherent). */
export function nativeThemeFor(id: ThemeId): "light" | "dark" {
  return familyOf(id);
}

const STORAGE_KEY = "diskwizard.theme";

/** Validate a stored/loaded value — anything unknown falls back to the
 * shipped default (blossom). */
export function validateThemeId(value: string | null | undefined): ThemeId {
  return value != null && (THEME_IDS as readonly string[]).includes(value)
    ? (value as ThemeId)
    : DEFAULT_THEME;
}

/** Read the persisted choice (best-effort; hardened WebView profiles
 * can throw on localStorage access). */
export function readSavedTheme(): ThemeId {
  try {
    return validateThemeId(window.localStorage.getItem(STORAGE_KEY));
  } catch {
    return DEFAULT_THEME;
  }
}

/** Persist a choice (same best-effort contract as readSavedTheme). */
export function persistTheme(id: ThemeId): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, id);
  } catch {
    // localStorage can throw in hardened WebView profiles; the theme
    // still applies for this session, it just will not persist.
  }
}

/** Read the theme the pre-mount script already applied. */
export function currentDomTheme(): ThemeId {
  const dom = document.documentElement.getAttribute("data-theme");
  return validateThemeId(dom);
}
