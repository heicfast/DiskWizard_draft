import type React from "react";
/**
 * DeviceArt — the illustrated device glyphs on the Recover Data cards
 * (the reference's drive renders, reduced to clean SVG): internal
 * HDD/SSD, USB stick, portable SSD, SD card. Each accepts a `windows`
 * flag (C: shows the Windows mark overlay in the reference).
 */
export type DeviceArtKind = "hdd" | "ssd" | "usb" | "portable-ssd" | "sdcard";

interface Props {
  kind: DeviceArtKind;
  size?: number;
  windows?: boolean;
}

const defs = (
  <defs>
    <linearGradient id="dw-dev-body" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stopColor="#ffffff" />
      <stop offset="100%" stopColor="#e8e4e6" />
    </linearGradient>
    <linearGradient id="dw-dev-body-dark" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stopColor="#c9cdd4" />
      <stop offset="100%" stopColor="#9aa0ab" />
    </linearGradient>
    <linearGradient id="dw-dev-plate" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stopColor="#f4f2f3" />
      <stop offset="100%" stopColor="#dcd8db" />
    </linearGradient>
    <linearGradient id="dw-dev-usb" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stopColor="#f2f0f1" />
      <stop offset="55%" stopColor="#d7d3d6" />
      <stop offset="100%" stopColor="#b9b4b8" />
    </linearGradient>
  </defs>
);

/** Internal 3.5" HDD (upright drive with platter circle). */
function HddArt() {
  return (
    <svg viewBox="0 0 48 48" width="48" height="48" aria-hidden="true">
      {defs}
      <rect x="8" y="6" width="32" height="36" rx="4" fill="url(#dw-dev-body)" stroke="#c9c2c6" strokeWidth="1" />
      <rect x="12" y="10" width="24" height="20" rx="2.5" fill="url(#dw-dev-plate)" stroke="#cfc8cc" strokeWidth="0.8" />
      <circle cx="24" cy="20" r="7.5" fill="#eceef2" stroke="#c5cad2" strokeWidth="1" />
      <circle cx="24" cy="20" r="2.2" fill="#d3d7de" />
      <circle cx="24" cy="20" r="0.9" fill="#9aa0ab" />
      <rect x="12" y="33" width="6" height="3" rx="1.5" fill="#d9dde3" />
      <rect x="20" y="33" width="2.6" height="3" rx="1.3" fill="#f6c34e" />
      <rect x="24.4" y="33" width="2.6" height="3" rx="1.3" fill="#8fd3a2" />
    </svg>
  );
}

/** Internal 2.5" SSD (sleek slab). */
function SsdArt() {
  return (
    <svg viewBox="0 0 48 48" width="48" height="48" aria-hidden="true">
      {defs}
      <rect x="7" y="10" width="34" height="28" rx="4" fill="url(#dw-dev-body)" stroke="#c9c2c6" strokeWidth="1" />
      <rect x="11" y="14" width="26" height="14" rx="2" fill="url(#dw-dev-plate)" stroke="#d3ccd0" strokeWidth="0.8" />
      <circle cx="17" cy="21" r="3.4" fill="#eceef2" stroke="#c5cad2" strokeWidth="0.9" />
      <circle cx="17" cy="21" r="1" fill="#9aa0ab" />
      <rect x="24" y="19.4" width="10" height="3.2" rx="1.6" fill="#e3e0e3" />
      <rect x="11" y="31.4" width="4.4" height="2.6" rx="1.3" fill="#f6c34e" />
      <rect x="17" y="31.4" width="4.4" height="2.6" rx="1.3" fill="#8fd3a2" />
    </svg>
  );
}

/** USB stick (angled connector up). */
function UsbArt() {
  return (
    <svg viewBox="0 0 48 48" width="48" height="48" aria-hidden="true">
      {defs}
      <rect x="19" y="4" width="10" height="12" rx="1.5" fill="url(#dw-dev-body-dark)" stroke="#878d98" strokeWidth="0.9" />
      <rect x="21.4" y="6.6" width="2.2" height="2.6" rx="0.5" fill="#f2f0f1" />
      <rect x="24.6" y="6.6" width="2.2" height="2.6" rx="0.5" fill="#f2f0f1" />
      <path d="M15 14 h18 a4 4 0 0 1 4 4 v18 a6 6 0 0 1 -6 6 h-14 a6 6 0 0 1 -6 -6 v-18 a4 4 0 0 1 4 -4 z" fill="url(#dw-dev-usb)" stroke="#a89fa4" strokeWidth="1" />
      <circle cx="24" cy="28" r="4.6" fill="#ffffff" opacity="0.55" />
      <circle cx="24" cy="28" r="2" fill="#c9c2c6" />
    </svg>
  );
}

/** Portable SSD (pocket drive, dark face). */
function PortableSsdArt() {
  return (
    <svg viewBox="0 0 48 48" width="48" height="48" aria-hidden="true">
      {defs}
      <rect x="9" y="6" width="30" height="36" rx="6" fill="url(#dw-dev-body-dark)" stroke="#828898" strokeWidth="1" />
      <rect x="13" y="10" width="22" height="24" rx="3.5" fill="#39404d" opacity="0.92" />
      <circle cx="24" cy="22" r="6.4" fill="#4a525f" stroke="#5d6673" strokeWidth="1" />
      <circle cx="24" cy="22" r="1.8" fill="#737d8b" />
      <rect x="16" y="37" width="3" height="2.2" rx="1.1" fill="#f6c34e" />
      <rect x="21" y="37" width="3" height="2.2" rx="1.1" fill="#7f8794" />
    </svg>
  );
}

/** SD card. */
function SdCardArt() {
  return (
    <svg viewBox="0 0 48 48" width="48" height="48" aria-hidden="true">
      {defs}
      <path d="M14 8 h16 l6 6 v26 a3 3 0 0 1 -3 3 h-19 a3 3 0 0 1 -3 -3 v-29 a3 3 0 0 1 3 -3 z" fill="url(#dw-dev-usb)" stroke="#a89fa4" strokeWidth="1" />
      <rect x="15" y="10" width="2.4" height="5" rx="1" fill="#f2f0f1" stroke="#c5bdc2" strokeWidth="0.5" />
      <rect x="19" y="10" width="2.4" height="5" rx="1" fill="#f2f0f1" stroke="#c5bdc2" strokeWidth="0.5" />
      <rect x="23" y="10" width="2.4" height="5" rx="1" fill="#f2f0f1" stroke="#c5bdc2" strokeWidth="0.5" />
      <rect x="27" y="10" width="2.4" height="5" rx="1" fill="#f2f0f1" stroke="#c5bdc2" strokeWidth="0.5" />
      <rect x="16" y="22" width="16" height="12" rx="2" fill="#ffffff" opacity="0.5" />
      <text x="24" y="30.5" textAnchor="middle" fontSize="7" fontWeight="700" fill="#a89fa4" fontFamily="Inter, sans-serif">SD</text>
    </svg>
  );
}

const ART: Record<DeviceArtKind, () => React.ReactElement> = {
  hdd: HddArt,
  ssd: SsdArt,
  usb: UsbArt,
  "portable-ssd": PortableSsdArt,
  sdcard: SdCardArt,
};

export function DeviceArt({ kind, size = 56, windows }: Props) {
  const Art = ART[kind] ?? HddArt;
  return (
    <span
      className="dw-device-art"
      style={{ width: size, height: size }}
      aria-hidden="true"
    >
      <Art />
      {windows && (
        <span className="dw-device-art-win" title="Windows system volume">
          <svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true">
            <rect x="1" y="1" width="6.4" height="6.4" rx="0.8" fill="#0078d4" />
            <rect x="8.6" y="1" width="6.4" height="6.4" rx="0.8" fill="#0078d4" />
            <rect x="1" y="8.6" width="6.4" height="6.4" rx="0.8" fill="#0078d4" />
            <rect x="8.6" y="8.6" width="6.4" height="6.4" rx="0.8" fill="#0078d4" />
          </svg>
        </span>
      )}
    </span>
  );
}
