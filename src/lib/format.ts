/**
 * Formatting helpers — byte counts, durations, counts, dates. Pure and
 * unit-tested; the single place display strings are shaped.
 */

const KB = 1024;
const MB = KB * 1024;
const GB = MB * 1024;
const TB = GB * 1024;

/** "1.2 KB" / "18.6 GB" / "1.5 TB" — one decimal under 1000 (the
 * reference design's capacity strings: "120.5 GB / 232.3 GB"). */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < KB) return `${Math.round(bytes)} B`;
  if (bytes < MB) return trimTo(bytes / KB) + " KB";
  if (bytes < GB) return trimTo(bytes / MB) + " MB";
  if (bytes < TB) return trimTo(bytes / GB) + " GB";
  return trimTo(bytes / TB) + " TB";
}

function trimTo(v: number): string {
  return v >= 1000 ? String(Math.round(v)) : v.toFixed(1);
}

/** "42.8K" / "124,532" — counts above 10k compact (tree badges). */
export function formatCount(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "—";
  if (n < 10000) return n.toLocaleString("en-US");
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)}K`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

/** Full comma-grouped count (status bar / stats). */
export function formatCountFull(n: number): string {
  return Math.round(n).toLocaleString("en-US");
}

/** "12m 36s" (scan durations), "1h 04m" beyond the hour. */
export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "—";
  const s = Math.floor(seconds);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) {
    const rem = s % 60;
    return rem ? `${m}m ${String(rem).padStart(2, "0")}s` : `${m}m`;
  }
  const h = Math.floor(m / 60);
  return `${h}h ${String(m % 60).padStart(2, "0")}m`;
}

/** "07m43s" remaining-time form (zero-padded, research: "Remaining
 * time: 07m43s"). */
export function formatRemaining(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const m = Math.floor(s / 60);
  return `${String(m).padStart(2, "0")}m${String(s % 60).padStart(2, "0")}s`;
}

/** "1,245,632" — sector addresses, comma-grouped (the deep scan's
 * live sector ticker). */
export function formatSectors(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "—";
  return Math.round(n).toLocaleString("en-US");
}

/** Scan speed in MB/s from sectors read over elapsed time (the
 * sector size contract: 512 B — dw-core's SECTOR_SIZE). Falls back
 * to "—" while no time has elapsed. */
export function formatScanSpeed(sectorsRead: number, elapsedSec: number): string {
  if (!Number.isFinite(sectorsRead) || !Number.isFinite(elapsedSec) || elapsedSec <= 0.5) {
    return "—";
  }
  const mbps = (sectorsRead * 512) / (1024 * 1024) / elapsedSec;
  if (mbps < 1) return `${(mbps * 1024).toFixed(0)} KB/s`;
  if (mbps >= 1024) return `${(mbps / 1024).toFixed(1)} GB/s`;
  return `${mbps.toFixed(0)} MB/s`;
}

/** "2023-10-24 14:32" (file table rows — reference design format).
 * Takes unix MILLISECONDS (session 9: the wire carries the og's
 * sub-second FILETIME precision; licenses/sessions stay seconds on
 * `formatDate`). */
export function formatDateTime(unixMs: number): string {
  if (!unixMs) return "—";
  const d = new Date(unixMs);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** "Oct 24, 2023" (session cards / dialogs). */
export function formatDate(unixSeconds: number): string {
  if (!unixSeconds) return "—";
  return new Date(unixSeconds * 1000).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

/** "32°C" temperature (disk health). */
export function formatTemp(celsius: number): string {
  return `${Math.round(celsius)}°C`;
}

/** Percentage clamp for progress UIs. */
export function clampPercent(v: number): number {
  return Math.min(100, Math.max(0, v));
}
