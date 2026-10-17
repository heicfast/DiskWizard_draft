/**
 * Disk Tools — the disk-health dashboard (the FREE tier, DiskGenie's
 * Monitor analog): device selector with live health verdicts, the
 * S.M.A.R.T. attribute table (CrystalDiskInfo-lineage columns), and
 * the rescue-tools grid (SD Card Rescue, lost-partition guidance,
 * BitLocker unlock, disk images — licensed tools).
 */
import { useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import {
  ActivityIcon,
  ArrowRightIcon,
  CpuIcon,
  HardDriveIcon,
  ImageIcon,
  LockIcon,
  ShieldIcon,
  SdCardIcon,
  TempIcon,
  WrenchIcon,
  ZapIcon,
} from "../../shell/icons";
import { formatBytes, formatCountFull, formatTemp } from "../../lib/format";
import { useDevicesStore } from "../../state/devices";
import { useLicenseStore } from "../../state/license";
import { useScanStore } from "../../state/scan";
import { useViewStore } from "../../state/view";
import type { SmartReport, StorageDevice } from "../../types";

export function ToolsPage() {
  const inventory = useDevicesStore((s) => s.inventory);
  const smart = useDevicesStore((s) => s.smart);
  const loadSmart = useDevicesStore((s) => s.loadSmart);
  const [deviceId, setDeviceId] = useState<string | null>(null);

  const devices: StorageDevice[] = useMemo(
    () => [...(inventory?.localDisks ?? []), ...(inventory?.externalDevices ?? [])],
    [inventory],
  );

  // Auto-select the first device; lazily pull SMART reports per device.
  useEffect(() => {
    if (!deviceId && devices.length > 0) setDeviceId(devices[0].id);
  }, [deviceId, devices]);
  useEffect(() => {
    if (deviceId) void loadSmart(deviceId);
  }, [deviceId, loadSmart]);
  // Prefetch all reports (small payloads — the selector flips instantly).
  useEffect(() => {
    for (const d of devices) void loadSmart(d.id);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per inventory
  }, [inventory]);

  const selected = devices.find((d) => d.id === deviceId) ?? devices[0] ?? null;
  const report: SmartReport | null = selected ? smart[selected.id] ?? null : null;

  return (
    <div className="dw-page dw-page-tools dw-scroll">
      <header className="dw-page-head">
        <div>
          <div className="dw-eyebrow">Disk Health &amp; Rescue</div>
          <h1 className="dw-page-title dw-display">Disk Tools</h1>
          <p className="dw-page-sub">
            S.M.A.R.T. monitoring, disk-health risks and rescue tools. Health monitoring is
            free forever — rescue tools unlock with DiskWizard Pro.
          </p>
        </div>
      </header>

      {/* Device selector */}
      <section className="dw-section">
        <div className="dw-section-head">
          <h2 className="dw-section-title">Which disk do you care about?</h2>
        </div>
        <div className="dw-tool-disk-row">
          {devices.map((d, i) => {
            const r = smart[d.id];
            return (
              <motion.button
                key={d.id}
                type="button"
                className={`dw-card dw-tool-disk ${selected?.id === d.id ? "is-active" : ""} ${r?.health === "caution" ? "is-caution" : ""}`}
                initial={{ opacity: 0, y: 12 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ delay: i * 0.04, duration: 0.3, ease: [0.2, 0.8, 0.3, 1] }}
                onClick={() => setDeviceId(d.id)}
              >
                <span className="dw-tool-disk-head" title={`${d.name} · ${d.fileSystem}`}>
                  <HardDriveIcon size={17} strokeWidth={1.7} />
                  <span className="dw-truncate">{d.name}</span>
                </span>
                <span className={`dw-health-chip ${r ? `is-${r.health}` : ""}`}>
                  <span className="dw-health-dot" aria-hidden="true" />
                  {r ? (r.health === "good" ? "Good" : r.health === "caution" ? "Caution!" : "Bad") : "…"}
                </span>
                <span className="dw-tool-disk-meta dw-num">
                  {r ? `${formatTemp(r.temperature)} · ${formatCountFull(r.powerOnHours)} h` : formatBytes(d.total)}
                </span>
              </motion.button>
            );
          })}
        </div>
      </section>

      {/* Health dashboard for the selected disk */}
      {report && selected && (
        <section className="dw-section dw-smart-section" aria-label={`Health of ${selected.name}`}>
          <div className="dw-section-head">
            <h2 className="dw-section-title">
              {report.model}
              <span className="dw-section-sub dw-num">
                {report.interface} · SN {report.serial} · FW {report.firmware}
              </span>
            </h2>
            <span className={`dw-health-chip dw-health-lg ${`is-${report.health}`}`}>
              <span className="dw-health-dot" aria-hidden="true" />
              {report.health === "good" ? "Good" : report.health === "caution" ? "Caution!" : "Bad"}
            </span>
          </div>

          {report.health !== "good" && (
            <div className="dw-caution-card" role="status">
              <ShieldIcon size={18} />
              <div>
                <strong>Your disk may be at risk.</strong> S.M.A.R.T. reports warning
                thresholds on this device — back it up and recover anything important
                <em> before</em> it fails. DiskWizard is guarding this disk and will keep
                alerting on changes.
              </div>
            </div>
          )}

          <div className="dw-smart-topstats">
            <TopStat icon={TempIcon} label="Temperature" value={formatTemp(report.temperature)} warn={report.temperature >= 45} />
            <TopStat icon={ZapIcon} label="Power-on hours" value={`${formatCountFull(report.powerOnHours)} h`} />
            <TopStat icon={ActivityIcon} label="Power cycles" value={formatCountFull(report.powerCycles)} />
            <TopStat icon={CpuIcon} label="Total written" value={`${report.totalWrites} TB`} />
          </div>

          {/* S.M.A.R.T. attribute table */}
          <div className="dw-smart-table-wrap dw-card">
            <table className="dw-smart-table">
              <thead>
                <tr>
                  <th>ID</th>
                  <th className="dw-smart-attr">Attribute</th>
                  <th>Current</th>
                  <th>Worst</th>
                  <th>Threshold</th>
                  <th>Raw Value</th>
                  <th>Status</th>
                </tr>
              </thead>
              <tbody>
                {report.attributes.map((a) => (
                  <tr key={a.id} className={a.status !== "ok" ? "is-warn" : ""}>
                    <td className="dw-num">{a.id}</td>
                    <td>{a.name}</td>
                    <td className="dw-num">{a.current}</td>
                    <td className="dw-num">{a.worst}</td>
                    <td className="dw-num">{a.threshold}</td>
                    <td className="dw-num">{a.rawValue}</td>
                    <td>
                      <span className={`dw-chip ${a.status === "ok" ? "dw-chip-ok" : "dw-chip-warn"}`}>
                        {a.status === "ok" ? "OK" : "Warning"}
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      )}

      {/* Rescue tools */}
      <section className="dw-section">
        <div className="dw-section-head">
          <h2 className="dw-section-title">Rescue Tools</h2>
        </div>
        <div className="dw-tool-grid">
          <ToolCard
            icon={SdCardIcon}
            title="SD Card Rescue"
            body="Cards that are inaccessible, not recognized or prompt to format — restore access first, then recover the files."
            cta="Start Rescue"
            area="tools"
            onClick={async () => {
              // Connect the rescue scenario card, then start its scan —
              // the SDR access-restore pre-phase runs inside the flow
              // ("first attempts to restore access to the card and
              // continues with file recovery").
              const view = useViewStore.getState();
              view.showToast("Connecting the SD card…", "info");
              const id = await useDevicesStore.getState().connectDevice();
              if (id) {
                view.setPage("results");
                void useScanStore.getState().start(id);
              } else {
                view.showToast("Could not read the card — reseat it and retry", "info");
              }
            }}
          />
          <ToolCard
            icon={WrenchIcon}
            title="Lost Partition Repair"
            body="Direct repair guidance for eligible lost partitions — restore access without navigating complex recovery procedures."
            cta="View Partitions"
            area="tools"
            onClick={() => useViewStore.getState().setPage("recover")}
          />
          <ToolCard
            icon={LockIcon}
            title="BitLocker Unlock"
            body="Unlock BitLocker-locked volumes with password or recovery key, then scan the decrypted content."
            cta="Unlock Drive"
            area="tools"
            onClick={() => useViewStore.getState().showToast("Pick the locked drive on Recover Data to unlock", "info")}
          />
          <ToolCard
            icon={ImageIcon}
            title="Disk Image"
            body="Create a byte-exact image of a failing disk and scan the image instead — safer for fragile media."
            cta="Create Image"
            area="tools"
            onClick={() => void useScanStore.getState().startDiskImage("data-backup-2026.img")}
          />
        </div>
      </section>
    </div>
  );
}

function TopStat({
  icon: Icon,
  label,
  value,
  warn,
}: {
  icon: typeof TempIcon;
  label: string;
  value: string;
  warn?: boolean;
}) {
  return (
    <div className={`dw-topstat dw-card ${warn ? "is-warn" : ""}`}>
      <span className="dw-topstat-icon">
        <Icon size={16} strokeWidth={1.8} />
      </span>
      <span className="dw-topstat-copy">
        <span className="dw-topstat-label">{label}</span>
        <span className="dw-topstat-value dw-num">{value}</span>
      </span>
    </div>
  );
}

function ToolCard({
  icon: Icon,
  title,
  body,
  cta,
  onClick,
  area,
}: {
  icon: typeof WrenchIcon;
  title: string;
  body: string;
  cta: string;
  onClick: () => void;
  area: string;
}) {
  const posture = useLicenseStore((s) => s.status?.posture ?? null);
  const openLicenseDialog = useViewStore((s) => s.openLicenseDialog);
  const isPro = posture === "pro" || posture === "grace";
  return (
    <motion.div
      className="dw-card dw-tool-card"
      initial={{ opacity: 0, y: 14 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.32, ease: [0.2, 0.8, 0.3, 1] }}
    >
      <span className="dw-tool-icon">
        <Icon size={19} strokeWidth={1.7} />
      </span>
      <div className="dw-tool-copy">
        <div className="dw-tool-title">{title}</div>
        <p className="dw-tool-body">{body}</p>
      </div>
      <button
        type="button"
        className="dw-outline-button dw-tool-cta"
        onClick={() => (isPro ? onClick() : openLicenseDialog())}
      >
        {isPro ? cta : "Activate to use"}
        <ArrowRightIcon size={13} />
        {!isPro && <span className="sr-only">{area}</span>}
      </button>
    </motion.div>
  );
}
