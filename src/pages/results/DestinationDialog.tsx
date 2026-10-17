/**
 * The destination picker (ChooseDestinationPathDialog analog): Local /
 * Cloud tabs, destination rows with free-space checks, the "Enable
 * file repair" toggle (ON by default), the overwrite-safety coaching
 * and the insufficient-space gate.
 */
import { useEffect, useMemo, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import {
  AlertIcon,
  ArchiveIcon,
  CheckIcon,
  CloudIcon,
  HardDriveIcon,
  WrenchIcon,
} from "../../shell/icons";
import { formatBytes } from "../../lib/format";
import { useResultsStore } from "../../state/results";

export function DestinationDialog() {
  const open = useResultsStore((s) => s.destinationOpen);
  const close = useResultsStore((s) => s.closeDestination);
  const destinations = useResultsStore((s) => s.destinations);
  const loading = useResultsStore((s) => s.destinationsLoading);
  const chosen = useResultsStore((s) => s.chosenDestinationId);
  const choose = useResultsStore((s) => s.chooseDestination);
  const enableRepair = useResultsStore((s) => s.enableRepair);
  const setEnableRepair = useResultsStore((s) => s.setEnableRepair);
  const selectedIds = useResultsStore((s) => s.selectedIds);
  const snapshot = useResultsStore((s) => s.snapshot);
  const beginRecovery = useResultsStore((s) => s.beginRecovery);

  const [tab, setTab] = useState<"local" | "cloud">("local");
  const [spaceGate, setSpaceGate] = useState<string | null>(null);

  const selectedBytes = useMemo(() => {
    if (!snapshot) return 0;
    let b = 0;
    for (const id of selectedIds) {
      const n = snapshot.nodes[id];
      if (n) b += n.size;
    }
    return b;
  }, [snapshot, selectedIds]);

  const rows = destinations.filter((d) => (tab === "local" ? d.kind === "local" : d.kind === "cloud"));
  const chosenDest = destinations.find((d) => d.id === chosen) ?? null;

  // Reset the space gate when the selection changes.
  useEffect(() => {
    setSpaceGate(null);
  }, [chosen, selectedBytes]);

  if (!open) return null;

  const onChoose = (id: string) => {
    choose(id);
    setSpaceGate(null);
  };

  const onSave = () => {
    if (!chosenDest) return;
    if (chosenDest.freeBytes < selectedBytes) {
      setSpaceGate(
        `Insufficient Disk Space — need an additional ${formatBytes(selectedBytes - chosenDest.freeBytes)}. Free up space or select another drive.`,
      );
      return;
    }
    void beginRecovery([...selectedIds], selectedBytes);
  };

  return (
    <AnimatePresence>
      <motion.div
        className="dw-dialog-veil"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        transition={{ duration: 0.2 }}
        onClick={(e) => {
          if (e.target === e.currentTarget) close();
        }}
      >
        <motion.div
          className="dw-dialog dw-dest-dialog"
          role="dialog"
          aria-modal="true"
          aria-label="Choose a destination path"
          initial={{ scale: 0.95, opacity: 0, y: 12 }}
          animate={{ scale: 1, opacity: 1, y: 0 }}
          exit={{ scale: 0.97, opacity: 0 }}
          transition={{ duration: 0.26, ease: [0.2, 0.9, 0.3, 1.04] }}
        >
          <button type="button" className="dw-dialog-x" onClick={close} aria-label="Close">✕</button>
          <h2 className="dw-dialog-title dw-display">Choose a destination path to save the file:</h2>
          <p className="dw-dialog-sub dw-num">
            You have {selectedIds.size} file{selectedIds.size === 1 ? "" : "s"} ({formatBytes(selectedBytes)}) to recover.
          </p>

          <div className="dw-segmented dw-dest-tabs" role="tablist" aria-label="Destination kind">
            <button type="button" role="tab" aria-selected={tab === "local"} className={tab === "local" ? "is-on" : ""} onClick={() => setTab("local")}>
              <HardDriveIcon size={13} /> Local
            </button>
            <button type="button" role="tab" aria-selected={tab === "cloud"} className={tab === "cloud" ? "is-on" : ""} onClick={() => setTab("cloud")}>
              <CloudIcon size={13} /> Cloud
            </button>
          </div>

          <div className="dw-dest-list dw-scroll" role="list">
            {loading && (
              <div className="dw-dest-loading">
                <span className="dw-scan-strip-ticker" aria-hidden="true" />
                Reading drives…
              </div>
            )}
            {!loading &&
              rows.map((d) => (
                <button
                  key={d.id}
                  type="button"
                  role="listitem"
                  className={`dw-dest-row ${chosen === d.id ? "is-chosen" : ""}`}
                  onClick={() => onChoose(d.id)}
                >
                  <span className="dw-dest-check" aria-hidden="true">
                    {chosen === d.id && <CheckIcon size={12} strokeWidth={3} />}
                  </span>
                  <span className="dw-dest-icon">
                    {d.kind === "cloud" ? <CloudIcon size={16} strokeWidth={1.8} /> : <HardDriveIcon size={16} strokeWidth={1.8} />}
                  </span>
                  <span className="dw-dest-info">
                    <span className="dw-dest-name">{d.name}</span>
                    <span className="dw-dest-space dw-num">
                      {formatBytes(d.freeBytes)} free of {formatBytes(d.totalBytes)}
                    </span>
                  </span>
                </button>
              ))}
          </div>

          <label className="dw-dest-repair" title="Repair corrupted photos, videos and documents as they recover">
            <span className="dw-toggle">
              <input
                type="checkbox"
                checked={enableRepair}
                onChange={(e) => setEnableRepair(e.target.checked)}
              />
              <span className="dw-toggle-track" aria-hidden="true" />
            </span>
            <span className="dw-dest-repair-copy">
              <WrenchIcon size={13} />
              Enable file repair
              <span className="dw-dest-repair-sub">Fix damaged photos, videos and documents during recovery</span>
            </span>
          </label>

          {spaceGate && (
            <div className="dw-dest-gate" role="alert">
              <AlertIcon size={15} />
              <span>{spaceGate}</span>
            </div>
          )}

          <p className="dw-dest-warn">
            Data might be <strong>overwritten and lost permanently</strong> if you save files
            to the original drive (where you lost data). We highly recommend a different drive.
          </p>

          <div className="dw-dialog-actions">
            <button type="button" className="dw-outline-button" onClick={close}>Cancel</button>
            <button type="button" className="dw-ink-button" disabled={!chosenDest} onClick={onSave}>
              <ArchiveIcon size={14} />
              Save
            </button>
          </div>
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
}
