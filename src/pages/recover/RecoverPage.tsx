/**
 * Recover Data — the landing page ("Select recovery source"):
 * local disks, external devices, quick actions (Recycle Bin /
 * Desktop / Select Folder / Disk Image) and recent recovery sessions
 * — the cherry-blossom reference layout, item for item.
 */
import { useMemo, useState } from "react";
import { motion } from "framer-motion";
import {
  ClockIcon,
  DesktopIcon,
  FolderIcon,
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
import { BitLockerDialog } from "./BitLockerDialog";
import { LostPartitionDialog } from "./LostPartitionDialog";
import { DriveCard } from "./DriveCard";
import { QuickCard, SessionRow } from "./QuickActions";

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
          <button
            type="button"
            className="dw-icon-button dw-quiet"
            onClick={() => void refresh()}
            disabled={refreshing}
            aria-label="Refresh device list"
            title={refreshing ? "Refreshing…" : "Refresh device list"}
          >
            <RefreshIcon size={14} className={refreshing ? "dw-spin" : ""} />
          </button>
          <button
            type="button"
            className="dw-outline-button dw-pill-button"
            onClick={() => showToast("Scan history keeps every saved session (.rsf)", "info")}
          >
            <ClockIcon size={14} />
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
            icon={FolderIcon}
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
