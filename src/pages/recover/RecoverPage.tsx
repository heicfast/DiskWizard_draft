/**
 * Recover Data — the landing page ("Select recovery source"):
 * local disks, external devices, quick actions (Recycle Bin /
 * Desktop / Select Folder / Disk Image) and recent recovery sessions
 * — the cherry-blossom reference layout, item for item.
 */
import { useMemo, useState } from "react";
import { motion } from "framer-motion";
import {
  ArrowRightIcon,
  CheckIcon,
  ChevronRightIcon,
  DesktopIcon,
  HardDriveIcon,
  ImageIcon,
  LockIcon,
  PauseIcon,
  PlusIcon,
  PortableIcon,
  RefreshIcon,
  SearchIcon,
  SdCardIcon,
  Trash2Icon,
  UsbIcon,
} from "../../shell/icons";
import { formatBytes, formatCountFull, formatDate } from "../../lib/format";
import { useDevicesStore } from "../../state/devices";
import { useScanStore } from "../../state/scan";
import { useViewStore } from "../../state/view";
import { useLicenseStore } from "../../state/license";
import type { StorageDevice } from "../../types";
import { BitLockerDialog } from "./BitLockerDialog";
import { LostPartitionDialog } from "./LostPartitionDialog";

/* ── Device icons ────────────────────────────────────────────────────── */

function DeviceGlyph({ device, size = 40 }: { device: StorageDevice; size?: number }) {
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
      style={{ width: size + 16, height: size + 16 }}
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

/* ── Drive card ──────────────────────────────────────────────────────── */

function DriveCard({
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

function kindLabel(d: StorageDevice): string {
  switch (d.kind) {
    case "ssd": return "SSD";
    case "hdd": return "HDD";
    case "usb": return "USB";
    case "sdcard": return "SD Card";
    case "portable-ssd": return "SSD";
    default: return "Drive";
  }
}

/* ── Quick actions ───────────────────────────────────────────────────── */

function QuickCard({
  icon: Icon,
  title,
  sub,
  onClick,
  index,
}: {
  icon: typeof Trash2Icon;
  title: string;
  sub: string;
  onClick: () => void;
  index: number;
}) {
  return (
    <motion.button
      type="button"
      className="dw-card dw-quick-card"
      initial={{ opacity: 0, y: 14 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay: 0.24 + index * 0.05, duration: 0.34, ease: [0.2, 0.8, 0.3, 1] }}
      onClick={onClick}
    >
      <span className="dw-quick-icon">
        <Icon size={19} strokeWidth={1.7} />
      </span>
      <span className="dw-quick-copy">
        <span className="dw-quick-title">{title}</span>
        <span className="dw-quick-sub">{sub}</span>
      </span>
    </motion.button>
  );
}

/* ── Sessions ────────────────────────────────────────────────────────── */

function SessionRow({ session }: { session: { id: string; targetName: string; status: string; filesFound: number; bytesFound: number; startedAt: number } }) {
  const resume = useScanStore((s) => s.start);
  const completed = session.status === "completed";
  const paused = session.status === "paused";
  return (
    <button
      type="button"
      className="dw-session-row"
      onClick={() => void resume(session.id)}
      title={completed ? `Open ${session.targetName} results` : `Resume ${session.targetName} scan`}
    >
      <span className={`dw-session-status ${completed ? "is-done" : paused ? "is-paused" : ""}`}>
        {completed ? <CheckIcon size={13} strokeWidth={2.6} /> : <PauseIcon size={12} strokeWidth={2.6} />}
      </span>
      <span className="dw-session-info">
        <span className="dw-session-name dw-truncate">{session.targetName}</span>
        <span className="dw-session-meta dw-num">
          {session.status === "completed" ? "Completed" : "Paused"} · {formatCountFull(session.filesFound)} files · {formatBytes(session.bytesFound)}
        </span>
      </span>
      <span className="dw-session-date">{formatDate(session.startedAt)}</span>
      <ChevronRightIcon size={15} className="dw-session-chevron" />
    </button>
  );
}

/* ── Page ────────────────────────────────────────────────────────────── */

export function RecoverPage() {
  const inventory = useDevicesStore((s) => s.inventory);
  const sessions = useDevicesStore((s) => s.sessions);
  const refresh = useDevicesStore((s) => s.refresh);
  const refreshing = useDevicesStore((s) => s.refreshing);
  const connectDevice = useDevicesStore((s) => s.connectDevice);
  const connecting = useDevicesStore((s) => s.connecting);
  const startFolder = useScanStore((s) => s.startFolder);
  const startDiskImage = useScanStore((s) => s.startDiskImage);
  const showToast = useViewStore((s) => s.showToast);
  const licensePosture = useLicenseStore((s) => s.status?.posture ?? null);
  const [bitLockerDevice, setBitLockerDevice] = useState<StorageDevice | null>(null);
  const [lostPartitionDevice, setLostPartitionDevice] = useState<StorageDevice | null>(null);

  const localDisks = useMemo(() => inventory?.localDisks ?? [], [inventory]);
  const externalDevices = useMemo(() => inventory?.externalDevices ?? [], [inventory]);

  return (
    <div className="dw-page dw-page-recover dw-scroll">
      <header className="dw-page-head">
        <div>
          <div className="dw-eyebrow">Data Recovery</div>
          <h1 className="dw-page-title dw-display">Select recovery source</h1>
          <p className="dw-page-sub">
            Pick where your data was lost — DiskWizard scans any disk, partition or device.
            Lost partitions surface here as the scan digs deeper.
          </p>
        </div>
        <div className="dw-page-head-actions">
          <button type="button" className="dw-outline-button" onClick={() => void refresh()} disabled={refreshing}>
            <RefreshIcon size={14} className={refreshing ? "dw-spin" : ""} />
            {refreshing ? "Refreshing…" : "Refresh"}
          </button>
          <button
            type="button"
            className="dw-outline-button"
            onClick={() => showToast("Scan history keeps every saved session (.rsf)", "info")}
          >
            <SearchIcon size={14} />
            Scan History
          </button>
        </div>
      </header>

      <section className="dw-section">
        <div className="dw-section-head">
          <h2 className="dw-section-title">Local Disks <span className="dw-count-chip dw-num">{localDisks.length}</span></h2>
        </div>
        <div className="dw-drive-grid">
          {localDisks.map((d, i) => (
            <DriveCard
              key={d.id}
              device={d}
              index={i}
              onBitLocker={setBitLockerDevice}
              onLostPartition={setLostPartitionDevice}
            />
          ))}
        </div>
      </section>

      <section className="dw-section">
        <div className="dw-section-head">
          <h2 className="dw-section-title">External Devices <span className="dw-count-chip dw-num">{externalDevices.length}</span></h2>
        </div>
        <div className="dw-drive-grid">
          {externalDevices.map((d, i) => (
            <DriveCard
              key={d.id}
              device={d}
              index={i}
              onBitLocker={setBitLockerDevice}
              onLostPartition={setLostPartitionDevice}
            />
          ))}
          <motion.button
            type="button"
            className="dw-card dw-connect-card"
            initial={{ opacity: 0, y: 14 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.24, duration: 0.34, ease: [0.2, 0.8, 0.3, 1] }}
            onClick={() => void connectDevice()}
            disabled={connecting}
          >
            <span className="dw-connect-plus">
              <PlusIcon size={20} strokeWidth={2} />
            </span>
            <span className="dw-connect-copy">
              <span className="dw-connect-title">{connecting ? "Connecting…" : "Connect a device"}</span>
              <span className="dw-connect-sub">Plug in a USB, SD card or camera</span>
            </span>
          </motion.button>
        </div>
      </section>

      <section className="dw-section">
        <div className="dw-section-head">
          <h2 className="dw-section-title">Quick Actions</h2>
        </div>
        <div className="dw-quick-grid">
          <QuickCard
            index={0}
            icon={Trash2Icon}
            title="Recycle Bin"
            sub="Restore deleted files"
            onClick={() => void startFolder("Recycle Bin")}
          />
          <QuickCard
            index={1}
            icon={DesktopIcon}
            title="Desktop"
            sub="Scan desktop files"
            onClick={() => void startFolder("Desktop")}
          />
          <QuickCard
            index={2}
            icon={SearchIcon}
            title="Select Folder"
            sub="Choose a specific folder"
            onClick={() => void startFolder("Documents")}
          />
          <QuickCard
            index={3}
            icon={ImageIcon}
            title="Disk Image"
            sub="Scan a saved image"
            onClick={() => void startDiskImage("data-backup-2026.img")}
          />
        </div>
      </section>

      <section className="dw-section dw-section-sessions">
        <div className="dw-section-head">
          <h2 className="dw-section-title">Recent Recovery Sessions</h2>
          <button type="button" className="dw-link" onClick={() => showToast("All saved sessions live in Scan History", "info")}>
            View All
          </button>
        </div>
        <div className="dw-session-list">
          {sessions.slice(0, 3).map((s) => (
            <SessionRow key={s.id} session={s} />
          ))}
        </div>
      </section>

      {licensePosture === "unlicensed" && (
        <p className="dw-recover-licensenote">
          Browsing is free — starting a scan requires a DiskWizard license.{" "}
          <span className="dw-link-inline">Disk health monitoring stays free in Disk Tools.</span>
        </p>
      )}

      <BitLockerDialog device={bitLockerDevice} onClose={() => setBitLockerDevice(null)} />
      <LostPartitionDialog device={lostPartitionDevice} onClose={() => setLostPartitionDevice(null)} />
    </div>
  );
}
