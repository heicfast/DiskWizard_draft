/**
 * The drive card — one row of the source-selection grid: device glyph
 * (kind + lock/lost/raw states), name + capacity bar + tags, and the
 * trailing scan affordance. Clicking starts the scan (or opens the
 * BitLocker / lost-partition dialog for those states).
 */
import { motion } from "framer-motion";
import {
  ArrowRightIcon,
  HardDriveIcon,
  LockIcon,
  PortableIcon,
  SdCardIcon,
  UsbIcon,
} from "../../shell/icons";
import { formatBytes } from "../../lib/format";
import { useScanStore } from "../../state/scan";
import { useViewStore } from "../../state/view";
import type { StorageDevice } from "../../types";

/* ── Device icons ────────────────────────────────────────────────────── */

export function DeviceGlyph({ device, size = 40 }: { device: StorageDevice; size?: number }) {
  const Icon =
    device.kind === "usb"
      ? UsbIcon
      : device.kind === "sdcard"
        ? SdCardIcon
        : device.kind === "portable-ssd"
          ? PortableIcon
          : HardDriveIcon;
  const locked = device.bitLocker === "locked";
  const lost = device.health === "lost";
  const raw = device.health === "raw";
  return (
    <span
      className={`dw-device-glyph ${device.kind} ${locked ? "is-locked" : ""} ${lost ? "is-lost" : ""} ${raw ? "is-raw" : ""}`}
      style={{ width: size + 12, height: size + 12 }}
      aria-hidden="true"
    >
      <Icon size={size * 0.52} strokeWidth={1.6} />
      {locked && (
        <span className="dw-device-glyph-badge">
          <LockIcon size={10} strokeWidth={2.4} />
        </span>
      )}
      {raw && (
        <span className="dw-device-glyph-badge is-warn">
          <SdCardIcon size={10} strokeWidth={2.4} />
        </span>
      )}
    </span>
  );
}

export function kindLabel(d: StorageDevice): string {
  switch (d.kind) {
    case "ssd": return "SSD";
    case "hdd": return "HDD";
    case "usb": return "USB";
    case "sdcard": return "SD Card";
    case "portable-ssd": return "SSD";
    default: return "Drive";
  }
}

/* ── Drive card ──────────────────────────────────────────────────────── */

export function DriveCard({
  device,
  index,
  onBitLocker,
  onLostPartition,
}: {
  device: StorageDevice;
  index: number;
  onBitLocker: (d: StorageDevice) => void;
  onLostPartition: (d: StorageDevice) => void;
}) {
  const start = useScanStore((s) => s.start);
  const starting = useScanStore((s) => s.starting);
  const showToast = useViewStore((s) => s.showToast);
  const pct = device.total > 0 ? Math.min(100, (device.used / device.total) * 100) : 0;
  const locked = device.bitLocker === "locked";
  const lost = device.health === "lost";
  const raw = device.health === "raw";

  const onScan = () => {
    if (locked) {
      onBitLocker(device);
      return;
    }
    if (lost) {
      // Direct-repair guidance for eligible lost partitions (the
      // IMPROVED restoration path from the product notes).
      onLostPartition(device);
      return;
    }
    if (raw) {
      // SD Card Rescue: restore access first, then continue with
      // recovery — the rescue pre-phase runs inside the scan flow.
      showToast("SD Card Rescue will restore access, then continue with recovery", "shield");
    }
    void start(device.id);
  };

  return (
    <motion.button
      type="button"
      className={`dw-card dw-drive-card ${locked ? "is-locked" : ""} ${lost ? "is-lost" : ""} ${raw ? "is-raw" : ""}`}
      initial={{ opacity: 0, y: 14 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay: 0.05 + index * 0.045, duration: 0.34, ease: [0.2, 0.8, 0.3, 1] }}
      onClick={onScan}
      disabled={starting}
      title={locked ? "BitLocker-locked — unlock to scan" : `Scan ${device.name}`}
    >
      <DeviceGlyph device={device} />
      <div className="dw-drive-info">
        <div className="dw-drive-name-row">
          <span className="dw-drive-name dw-truncate">{device.name}</span>
          <span className="dw-drive-kind">{kindLabel(device)}</span>
        </div>
        <div className="dw-drive-cap-bar" role="img" aria-label={`${formatBytes(device.used)} of ${formatBytes(device.total)} used`}>
          <div className="dw-drive-cap-fill" style={{ width: `${pct}%` }} />
        </div>
        <div className="dw-drive-meta">
          <span className="dw-num">{formatBytes(device.used)} / {formatBytes(device.total)}</span>
          <span className="dw-drive-tags">
            <span className="dw-tag">{device.fileSystem}</span>
            {device.isSystem && <span className="dw-tag is-accent">System</span>}
            {device.trim === "on" && <span className="dw-tag is-soft">TRIM ON</span>}
            {lost && <span className="dw-tag is-warn">Lost</span>}
            {raw && <span className="dw-tag is-warn">Needs Rescue</span>}
            {locked && <span className="dw-tag is-warn">BitLocker</span>}
          </span>
        </div>
      </div>
      <span className="dw-drive-cta">
        <ArrowRightIcon size={16} strokeWidth={2} />
      </span>
    </motion.button>
  );
}
