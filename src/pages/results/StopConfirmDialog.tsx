/**
 * Stop-scan confirmation (the CScanCommonPopDlg stop mode): honest
 * risk framing + the files-found / used-time stat tiles.
 */
import { motion } from "framer-motion";
import { AlertIcon } from "../../shell/icons";
import { formatBytes, formatCountFull, formatDuration } from "../../lib/format";

export function StopConfirmDialog({
  filesFound,
  bytesFound,
  elapsed,
  onConfirm,
  onCancel,
}: {
  filesFound: number;
  bytesFound?: number;
  elapsed: number;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <motion.div
      className="dw-dialog-veil"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <motion.div
        className="dw-dialog dw-stop-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-label="Stop searching for lost data?"
        initial={{ scale: 0.95, opacity: 0, y: 12 }}
        animate={{ scale: 1, opacity: 1, y: 0 }}
        transition={{ duration: 0.26, ease: [0.2, 0.9, 0.3, 1.04] }}
      >
        <div className="dw-stop-icon">
          <AlertIcon size={22} strokeWidth={1.9} />
        </div>
        <h2 className="dw-dialog-title dw-display">Stop searching for lost data?</h2>
        <p className="dw-dialog-sub">
          Remaining data can be rescanned and recovered anytime later — but continuing
          now saves you a full re-scan of what is already found.
        </p>
        <div className="dw-stop-stats">
          <div className="dw-stop-stat">
            <span className="dw-stop-stat-num dw-num">{formatCountFull(filesFound)}</span>
            <span className="dw-stop-stat-label">Files found</span>
          </div>
          {bytesFound !== undefined && (
            <div className="dw-stop-stat">
              <span className="dw-stop-stat-num dw-num">{formatBytes(bytesFound)}</span>
              <span className="dw-stop-stat-label">Data found</span>
            </div>
          )}
          <div className="dw-stop-stat">
            <span className="dw-stop-stat-num dw-num">{formatDuration(elapsed)}</span>
            <span className="dw-stop-stat-label">Used time</span>
          </div>
        </div>
        <div className="dw-dialog-actions">
          <button type="button" className="dw-outline-button dw-danger" onClick={onConfirm}>
            Stop
          </button>
          <button type="button" className="dw-ink-button" onClick={onCancel}>
            Continue
          </button>
        </div>
      </motion.div>
    </motion.div>
  );
}
