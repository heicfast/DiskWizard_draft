/**
 * Recover Data — the landing page ("Select recovery source"), the
 * cherry-blossom reference layout item for item: eyebrow + title +
 * sub, Scan History pill, Local Drives (3-col cards), External Devices
 * (+ dashed connect card), Quick Actions (4 tiles), Recent Recovery
 * Sessions (3-col cards with status circles).
 */
import { useMemo, useState } from "react";
import { motion } from "framer-motion";
import {
  ClockIcon,
  DesktopIcon,
  FolderOpenIcon,
  ImageIcon,
  PlusIcon,
  RefreshIcon,
  Trash2Icon,
} from "../../shell/icons";
import { useDevicesStore } from "../../state/devices";
import { useScanStore } from "../../state/scan";
import { useViewStore } from "../../state/view";
import { useLicenseStore } from "../../state/license";
import type { StorageDevice } from "../../types";
import { DeviceCard } from "./components/DeviceCard";
import { QuickActionCard } from "./components/QuickActionCard";
import { SessionCard } from "./components/SessionCard";
import { BitLockerDialog } from "./dialogs/BitLockerDialog";
import { LostPartitionDialog } from "./dialogs/LostPartitionDialog";

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

  const driveProps = {
    onBitLocker: setBitLockerDevice,
    onLostPartition: setLostPartitionDevice,
  };

  return (
    <div className="dw-page dw-page-recover dw-scroll">
      <motion.header
        className="dw-page-head"
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.32, ease: [0.2, 0.8, 0.3, 1] }}
      >
        <div className="dw-page-head-copy">
          <div className="dw-eyebrow">Data Recovery</div>
          <h1 className="dw-page-title dw-display">Select recovery source</h1>
          <p className="dw-page-sub">
            Choose a drive, device, or location to scan for lost files. We&rsquo;ll guide
            you through the recovery process.
          </p>
        </div>
        <div className="dw-page-head-actions">
          <motion.button
            type="button"
            className="dw-btn dw-btn--secondary"
            onClick={() => void refresh()}
            disabled={refreshing}
            title="Re-enumerate disks and devices"
          >
            <RefreshIcon size={14} className={refreshing ? "dw-spin" : ""} />
            {refreshing ? "Refreshing…" : "Refresh"}
          </motion.button>
          <motion.button
            type="button"
            className="dw-btn dw-btn--secondary"
            onClick={() => showToast("Scan history keeps every saved session (.rsf)", "info")}
          >
            <ClockIcon size={14} />
            Scan History
          </motion.button>
        </div>
      </motion.header>

      <section className="dw-section">
        <div className="dw-section-head">
          <h2 className="dw-section-title">Local Drives ({localDisks.length})</h2>
        </div>
        <div className="dw-drive-grid">
          {localDisks.map((d, i) => (
            <motion.div
              key={d.id}
              initial={{ opacity: 0, y: 14 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.05 + i * 0.045, duration: 0.34, ease: [0.2, 0.8, 0.3, 1] }}
            >
              <DeviceCard device={d} index={i} {...driveProps} />
            </motion.div>
          ))}
        </div>
      </section>

      <section className="dw-section">
        <div className="dw-section-head">
          <h2 className="dw-section-title">External Devices ({externalDevices.length})</h2>
        </div>
        <div className="dw-drive-grid">
          {externalDevices.map((d, i) => (
            <motion.div
              key={d.id}
              initial={{ opacity: 0, y: 14 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.12 + i * 0.045, duration: 0.34, ease: [0.2, 0.8, 0.3, 1] }}
            >
              <DeviceCard device={d} index={i} {...driveProps} />
            </motion.div>
          ))}
          <motion.button
            type="button"
            className="dw-card dw-card--dashed dw-connect-card"
            initial={{ opacity: 0, y: 14 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.22, duration: 0.34, ease: [0.2, 0.8, 0.3, 1] }}
            onClick={() => void connectDevice()}
            disabled={connecting}
          >
            <span className="dw-connect-plus">
              <PlusIcon size={20} strokeWidth={2.2} />
            </span>
            <span className="dw-connect-copy">
              <span className="dw-connect-title">{connecting ? "Connecting…" : "Connect a device"}</span>
              <span className="dw-connect-sub">
                Plug in a USB drive, SD card or other device to start recovery.
              </span>
            </span>
          </motion.button>
        </div>
      </section>

      <section className="dw-section">
        <div className="dw-section-head">
          <h2 className="dw-section-title">Quick Actions</h2>
        </div>
        <div className="dw-quick-grid">
          <QuickActionCard
            index={0}
            icon={<Trash2Icon size={19} strokeWidth={1.8} />}
            title="Recycle Bin"
            sub="Scan deleted files"
            onClick={() => void startFolder("Recycle Bin")}
          />
          <QuickActionCard
            index={1}
            icon={<DesktopIcon size={19} strokeWidth={1.8} />}
            title="Desktop"
            sub="Scan desktop files"
            onClick={() => void startFolder("Desktop")}
          />
          <QuickActionCard
            index={2}
            icon={<FolderOpenIcon size={19} strokeWidth={1.8} />}
            title="Select Folder"
            sub="Choose a specific folder"
            onClick={() => void startFolder("Documents")}
          />
          <QuickActionCard
            index={3}
            icon={<ImageIcon size={19} strokeWidth={1.8} />}
            title="Disk Image"
            sub="Scan from image file"
            onClick={() => void startDiskImage("data-backup-2026.img")}
          />
        </div>
      </section>

      <section className="dw-section">
        <div className="dw-section-head">
          <h2 className="dw-section-title">Recent Recovery Sessions</h2>
          <button
            type="button"
            className="dw-link"
            onClick={() => showToast("All saved sessions live in Scan History", "info")}
          >
            View All
          </button>
        </div>
        <div className="dw-session-grid">
          {sessions.slice(0, 3).map((s, i) => (
            <motion.div
              key={s.id}
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.28 + i * 0.045, duration: 0.32, ease: [0.2, 0.8, 0.3, 1] }}
            >
              <SessionCard session={s} />
            </motion.div>
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
