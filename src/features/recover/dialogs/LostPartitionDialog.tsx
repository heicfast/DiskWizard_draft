/**
 * Lost-partition guidance (the IMPROVED feature: "recommending a
 * direct repair path for eligible partitions, helping users restore
 * access without navigating complex recovery procedures"). Clicking a
 * lost partition offers Repair (recommended, when eligible) vs the
 * classic deep file scan.
 */
import { AnimatePresence, motion } from "framer-motion";
import { useEffect, useState } from "react";
import { ArrowRightIcon, HardDriveIcon, SearchIcon, WrenchIcon } from "lucide-react";
import { useScanStore } from "../../../state/scan";
import { useViewStore } from "../../../state/view";
import type { StorageDevice } from "../../../types";

export function LostPartitionDialog({ device, onClose }: { device: StorageDevice | null; onClose: () => void }) {
  const start = useScanStore((s) => s.start);
  const showToast = useViewStore((s) => s.showToast);
  const [repairing, setRepairing] = useState(false);

  useEffect(() => {
    setRepairing(false);
  }, [device]);

  if (!device) return null;

  const onRepair = () => {
    setRepairing(true);
    // The partition-table reconstruction round-trip (rawfixer), demo timescale.
    window.setTimeout(() => {
      showToast("Partition restored — access recovered without a file scan", "check");
      onClose();
    }, 2_100);
  };

  const onScan = () => {
    onClose();
    void start(device.id);
  };

  return (
    <AnimatePresence>
      <motion.div
        className="dw-dialog-veil"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        onClick={(e) => {
          if (e.target === e.currentTarget) onClose();
        }}
      >
        <motion.div
          className="dw-dialog"
          role="dialog"
          aria-modal="true"
          aria-label={`Lost partition options for ${device.name}`}
          initial={{ scale: 0.95, opacity: 0, y: 12 }}
          animate={{ scale: 1, opacity: 1, y: 0 }}
          exit={{ scale: 0.97, opacity: 0 }}
          transition={{ duration: 0.26, ease: [0.2, 0.9, 0.3, 1.04] }}
        >
          <button type="button" className="dw-dialog-x" onClick={onClose} aria-label="Close">✕</button>
          <div className="dw-stop-icon is-ink">
            <HardDriveIcon size={22} strokeWidth={1.9} />
          </div>
          <h2 className="dw-dialog-title dw-display">{device.name}</h2>
          <p className="dw-dialog-sub">
            We found this partition's signature while scanning its disk. It looks
            <strong> eligible for direct repair</strong> — restoring access rewrites the
            partition entry so the volume reappears in Windows, no file recovery needed.
          </p>

          <div className="dw-lost-options">
            <button type="button" className="dw-lost-option is-primary" onClick={onRepair} disabled={repairing}>
              <span className="dw-lost-option-icon">
                <WrenchIcon size={18} strokeWidth={1.8} />
              </span>
              <span className="dw-lost-option-copy">
                <span className="dw-lost-option-title">
                  Repair partition <span className="dw-tag is-accent">Recommended</span>
                </span>
                <span className="dw-lost-option-sub">Restore access directly — files stay where they are.</span>
              </span>
              <ArrowRightIcon size={15} />
            </button>
            <button type="button" className="dw-lost-option" onClick={onScan}>
              <span className="dw-lost-option-icon">
                <SearchIcon size={18} strokeWidth={1.8} />
              </span>
              <span className="dw-lost-option-copy">
                <span className="dw-lost-option-title">Scan for files instead</span>
                <span className="dw-lost-option-sub">Deep-scan the lost partition and recover files to another drive.</span>
              </span>
              <ArrowRightIcon size={15} />
            </button>
          </div>

          {repairing && (
            <div className="dw-repairing-row">
              <span className="dw-scan-strip-ticker" aria-hidden="true" />
              Rebuilding the partition entry — do not disconnect the disk…
            </div>
          )}

          <p className="dw-dest-warn">
            Repair writes to the partition table of the source disk. Scanning is
            read-only — when in doubt, scan first and recover, repair later.
          </p>
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
}
