/**
 * Platform detection — Windows is the primary target (Tauri 2 /
 * WebView2 / NSIS + MSIX), macOS renders the traffic-light reserve.
 * The titlebar and caption controls branch on these.
 */

export type Platform = "windows" | "macos" | "other";

export function detectPlatform(): Platform {
  if (typeof navigator === "undefined") return "other";
  const ua = navigator.userAgent;
  if (/Win/i.test(ua)) return "windows";
  if (/Mac/i.test(ua)) return "macos";
  return "other";
}

export const IS_WINDOWS = detectPlatform() === "windows";
export const IS_MACOS = detectPlatform() === "macos";
