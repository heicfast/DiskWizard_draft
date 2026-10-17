/**
 * Theme hook — the DiskGenie theming UX borrowed as-is.
 *
 * `data-theme` + `data-scheme` on the document root carry the actual
 * palette. This hook applies a theme, asks Tauri to set the native
 * window theme, and persists the choice in localStorage. First launch
 * defaults to BLOSSOM (the cherry-blossom shipped look); the pre-mount
 * inline script in index.html applies the saved/default theme before
 * React mounts so there is no flash.
 *
 * The premium crossfade between palettes is the View Transitions API
 * (Chromium 111+/WebView2; graceful instant fallback elsewhere and
 * under prefers-reduced-motion) — the same technique the theme-logo
 * backdrop rides, so the BrandTile and every --ink surface repaint in
 * one buttery step.
 */
import { useCallback, useEffect, useState } from "react";
import {
  THEME_IDS,
  type ThemeId,
  applyThemeAttributes,
  currentDomTheme,
  familyOf,
  nativeThemeFor,
  persistTheme,
} from "./themes";

/** Anything the DOM/validate layer rejects to blossom — the ids the
 * hook accepts at runtime. */
export type Theme = (typeof THEME_IDS)[number];

export {
  THEME_IDS,
  THEME_LIST,
  THEMES,
  DEFAULT_THEME,
  familyOf,
  validateThemeId,
} from "./themes";

/**
 * Read + control the app theme.
 *
 * - `theme` — the active palette id (blossom | light | dark | ember | tide).
 * - `setTheme(id)` — apply, persist, crossfade, and sync the native
 *   window theme (the picker's path).
 * - `toggle()` — the quick switch: flips between the canonical LIGHT
 *   and DARK pair. On a custom theme it returns to the opposite
 *   family's canonical member.
 * - `isDark` — the active family (icon selection, dark branches).
 */
export function useTheme(): {
  theme: Theme;
  isDark: boolean;
  setTheme: (id: Theme) => void;
  toggle: () => void;
} {
  const [theme, setThemeState] = useState<Theme>(currentDomTheme);

  // Keep React state in sync if the attributes change elsewhere
  // (the CI tour driver flips data-theme directly).
  useEffect(() => {
    const observer = new MutationObserver(() => setThemeState(currentDomTheme()));
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => observer.disconnect();
  }, []);

  /** The one apply path: crossfade + persist + native sync. */
  const setTheme = useCallback((next: ThemeId) => {
    const apply = () => {
      applyThemeAttributes(next);
      persistTheme(next);
    };
    // Premium crossfade between palettes (View Transitions API).
    const doc = document as Document & {
      startViewTransition?: (cb: () => void) => unknown;
    };
    const reduced = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    if (typeof doc.startViewTransition === "function" && !reduced) {
      doc.startViewTransition(apply);
    } else {
      apply();
    }
    setThemeState(next);
    // Native side: match window chrome (menus, tooltips) to the
    // family. Fire-and-forget (never blocks the UI; safe under Vitest
    // / plain browser where Tauri IPC is absent).
    import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => getCurrentWindow().setTheme(nativeThemeFor(next)))
      .catch(() => {
        // Running under Vitest / plain browser (no Tauri IPC) — CSS
        // still switched; nothing to report.
      });
  }, []);

  const toggle = useCallback(() => {
    // The quick control is the canonical pair flip.
    const next: ThemeId = familyOf(theme) === "dark" ? "light" : "dark";
    setTheme(next);
  }, [theme, setTheme]);

  return { theme, isDark: familyOf(theme) === "dark", setTheme, toggle };
}
