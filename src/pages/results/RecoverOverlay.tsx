/**
 * Recovery progress overlay + completion summary: per-file counters,
 * the aggregate bar, the repaired-files stat when file repair is on,
 * and the honest failure accounting ("N files / M failed — check the
 * recovery log").
 */
import { AnimatePresence, motion } from "framer-motion";
import { useEffect } from "react";
import { CheckIcon, TriangleAlertIcon, WrenchIcon, XIcon } from "lucide-react";
import { formatBytes, formatCountFull } from "../../lib/format";
import { useResultsStore } from "../../state/results";
import { useViewStore } from "../../state/view";

export function RecoverOverlay() {
  const progress = useResultsStore((s) => s.recoverProgress);
  const summary = useResultsStore((s) => s.recoverSummary);
  const dismissSummary = useResultsStore((s) => s.dismissSummary);
  const showToast = useViewStore((s) => s.showToast);

  // Surface the completion as a toast as well (channel redundancy for
  // the case where the summary was dismissed quickly).
  useEffect(() => {
    if (summary) {
      showToast(
        summary.error ? `Recovery failed — ${summary.error}` : "Recovery completed — files saved",
        summary.error ? "info" : "check",
      );
    }
  }, [summary, showToast]);

  return (
    <>
      <AnimatePresence>
        {progress && !progress.finished && (
          <motion.div
            className="dw-dialog-veil dw-recover-veil"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
          >
            <motion.div
              className="dw-dialog dw-recover-dialog"
              role="dialog"
              aria-modal="true"
              aria-label="Recovering files"
              initial={{ scale: 0.96, opacity: 0, y: 10 }}
              animate={{ scale: 1, opacity: 1, y: 0 }}
              exit={{ scale: 0.97, opacity: 0 }}
              transition={{ duration: 0.26, ease: [0.2, 0.9, 0.3, 1.04] }}
            >
              <h2 className="dw-dialog-title dw-display">Recovering your files</h2>
              <p className="dw-dialog-sub">
                Saving to <strong>{progress.destination}</strong> — keep the drive connected.
              </p>
              <div className="dw-recover-bar">
                <div
                  className="dw-recover-bar-fill"
                  style={{ width: `${Math.min(100, (progress.doneBytes / Math.max(1, progress.totalBytes)) * 100)}%` }}
                />
              </div>
              <div className="dw-recover-stats dw-num">
                <span>Recovered {formatCountFull(progress.doneFiles)} / {formatCountFull(progress.totalFiles)} files</span>
                <span>{formatBytes(progress.doneBytes)} of {formatBytes(progress.totalBytes)}</span>
              </div>
              <div className="dw-recover-current dw-truncate" title={progress.currentFile}>
                <span className="dw-scan-strip-ticker" aria-hidden="true" />
                {progress.currentFile}
              </div>
              {progress.enableRepair && (
                <div className="dw-recover-repair">
                  <WrenchIcon size={13} />
                  <span className="dw-num">{progress.repairedFiles} repaired so far</span>
                </div>
              )}
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {summary && (
          <motion.div
            className="dw-dialog-veil"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            onClick={(e) => {
              if (e.target === e.currentTarget) dismissSummary();
            }}
          >
            <motion.div
              className="dw-dialog dw-recover-summary"
              role="dialog"
              aria-modal="true"
              aria-label="Recovery summary"
              initial={{ scale: 0.95, opacity: 0, y: 12 }}
              animate={{ scale: 1, opacity: 1, y: 0 }}
              exit={{ scale: 0.97, opacity: 0 }}
              transition={{ duration: 0.28, ease: [0.2, 0.9, 0.3, 1.05] }}
            >
              <button type="button" className="dw-dialog-x" onClick={dismissSummary} aria-label="Close">✕</button>
              <div className={`dw-recover-summary-badge${summary.error ? " is-error" : ""}`}>
                {summary.error ? <TriangleAlertIcon size={26} strokeWidth={2.4} /> : <CheckIcon size={26} strokeWidth={2.4} />}
              </div>
              <h2 className="dw-dialog-title dw-display">
                {summary.error ? "Recovery Could Not Start" : "Recovery Successful"}
              </h2>
              {summary.error && (
                <p className="dw-recover-summary-note" role="alert">
                  {summary.error}
                </p>
              )}
              <div className="dw-recover-summary-stats dw-num">
                <div className="dw-stop-stat">
                  <span className="dw-stop-stat-num">{formatCountFull(summary.totalFiles - summary.failedFiles)}</span>
                  <span className="dw-stop-stat-label">Files recovered</span>
                </div>
                <div className="dw-stop-stat">
                  <span className="dw-stop-stat-num">{formatBytes(summary.totalBytes)}</span>
                  <span className="dw-stop-stat-label">Data saved</span>
                </div>
                {summary.failedFiles > 0 && (
                  <div className="dw-stop-stat">
                    <span className="dw-stop-stat-num is-warn">{formatCountFull(summary.failedFiles)}</span>
                    <span className="dw-stop-stat-label">Failed — see recovery log</span>
                  </div>
                )}
                {summary.enableRepair && summary.repairedFiles > 0 && (
                  <div className="dw-stop-stat">
                    <span className="dw-stop-stat-num">{formatCountFull(summary.repairedFiles)}</span>
                    <span className="dw-stop-stat-label">Repaired during recovery</span>
                  </div>
                )}
              </div>
              <p className="dw-recover-summary-note">
                {summary.failedFiles > 0
                  ? `${formatCountFull(summary.failedFiles)} file(s) could not be fully read — details are in the recovery log. Damaged files can still be repaired.`
                  : "Every selected file was saved successfully. Unrecovered files risk being permanently overwritten by continued computer use — recover anything else you need now."}
              </p>
              <div className="dw-dialog-actions">
                <button type="button" className="dw-outline-button" onClick={dismissSummary}>
                  <XIcon size={13} /> Close
                </button>
                <button type="button" className="dw-ink-button" onClick={dismissSummary}>
                  Done
                </button>
              </div>
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>
    </>
  );
}
