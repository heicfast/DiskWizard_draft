/**
 * DeviceCard — the Recover Data drive card (reference anatomy): device
 * art top-left, "…" menu top-right, name, capacity bar, "used / total"
 * line, tag row. Whole card is a safe click target that starts the
 * scan; the menu carries the secondary verbs.
 */
import { MoreHorizontal, Lock } from "lucide-react";
import { formatBytes } from "../../../lib/format";
import { useScanStore } from "../../../state/scan";
import { useViewStore } from "../../../state/view";
import { Menu } from "../../../components/ui";
import type { StorageDevice } from "../../../types";
import { DeviceArt, type DeviceArtKind } from "./DeviceArt";

function kindOf(d: StorageDevice): DeviceArtKind {
  switch (d.kind) {
    case "ssd":
      return "ssd";
    case "hdd":
      return "hdd";
    case "usb":
      return "usb";
    case "sdcard":
      return "sdcard";
    case "portable-ssd":
      return "portable-ssd";
    default:
      return "hdd";
  }
}

export function DeviceCard({
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
      // Direct-repair guidance for eligible lost partitions.
      onLostPartition(device);
      return;
    }
    if (raw) {
      // SD Card Rescue: restore access first, then recovery continues.
      showToast("SD Card Rescue will restore access, then continue with recovery", "shield");
    }
    void start(device.id);
  };

  return (
    <div
      className="dw-card dw-card--hover dw-drive-card"
      role="button"
      tabIndex={0}
      aria-label={`Scan ${device.name}`}
      onClick={onScan}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onScan();
        }
      }}
      title={locked ? "BitLocker-locked — unlock to scan" : `Scan ${device.name}`}
      style={{ animationDelay: `${0.04 + index * 0.05}s` }}
    >
      <div className="dw-drive-card-top">
        <DeviceArt
          kind={kindOf(device)}
          windows={device.isSystem}
        />
        <div onClick={(e) => e.stopPropagation()}>
          <Menu
            label={`${device.name} actions`}
            trigger={(open, toggle, id) => (
              <button
                type="button"
                className={`dw-row-more ${open ? "is-open" : ""}`}
                aria-haspopup="menu"
                aria-expanded={open}
                aria-controls={id}
                aria-label={`${device.name} actions`}
                onClick={toggle}
              >
                <MoreHorizontal size={16} />
              </button>
            )}
            items={[
              {
                id: "scan",
                label: locked ? "Unlock to scan" : "Start scan",
                onSelect: onScan,
              },
              ...(locked
                ? [
                    {
                      id: "unlock",
                      label: "Unlock BitLocker",
                      icon: <Lock size={14} />,
                      onSelect: () => onBitLocker(device),
                    },
                  ]
                : []),
              ...(lost
                ? [
                    {
                      id: "repair",
                      label: "Partition repair",
                      onSelect: () => onLostPartition(device),
                    },
                  ]
                : []),
              {
                id: "details",
                label: `${device.fileSystem}${device.trim === "on" ? " · TRIM ON" : ""} · ${formatBytes(device.total)}`,
                onSelect: () => undefined,
              },
            ]}
          />
        </div>
      </div>

      <span className="dw-drive-name dw-truncate">{device.name}</span>

      <div className="dw-drive-cap">
        <div
          className={`dw-progress ${raw || lost ? "dw-progress--warn" : ""}`}
          role="img"
          aria-label={`${formatBytes(device.used)} of ${formatBytes(device.total)} used`}
        >
          <div className="dw-progress__fill" style={{ width: `${pct}%` }} />
        </div>
        <span className="dw-drive-cap-text dw-num">
          {formatBytes(device.used)} / {formatBytes(device.total)}
        </span>
      </div>

      <div className="dw-drive-tags">
        {/* Reference tag row: the kind chip + System on the OS volume;
         * state chips (BitLocker / Lost / Needs Rescue) only when they
         * apply. TRIM lives in the menu details to keep the row quiet. */}
        <span className="dw-tag dw-tag--accent">{kindTag(device)}</span>
        {device.isSystem && <span className="dw-tag dw-tag--accent">System</span>}
        {locked && <span className="dw-tag dw-tag--warn">BitLocker</span>}
        {lost && <span className="dw-tag dw-tag--warn">Lost</span>}
        {raw && <span className="dw-tag dw-tag--warn">Needs Rescue</span>}
      </div>
      {starting && <span className="dw-sr-only">Starting scan…</span>}
    </div>
  );
}

/** The kind chip label (reference: SSD / HDD / USB / SD). */
function kindTag(d: StorageDevice): string {
  switch (d.kind) {
    case "ssd":
      return "SSD";
    case "hdd":
      return "HDD";
    case "usb":
      return "USB";
    case "sdcard":
      return "SD";
    case "portable-ssd":
      return "USB";
    default:
      return "Drive";
  }
}
