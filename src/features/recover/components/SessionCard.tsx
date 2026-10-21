/**
 * SessionCard — the recent-recovery-session tile (reference): status
 * circle (rose check = completed, amber pause = paused, red = failed),
 * target name, "Status · N files" line, chevron.
 */
import { CheckIcon, ChevronRightIcon, PauseIcon, WarnIcon } from "../../../shell/icons";
import { formatCountFull } from "../../../lib/format";
import { useScanStore } from "../../../state/scan";
import type { RecoverySession } from "../../../types";

const STATUS_LABEL: Record<RecoverySession["status"], string> = {
  completed: "Completed",
  paused: "Paused",
  running: "Scanning",
  failed: "Failed",
};

export function SessionCard({ session }: { session: RecoverySession }) {
  const resumeSession = useScanStore((s) => s.resumeSession);
  const status = session.status;
  const cls =
    status === "completed" ? "" : status === "paused" ? "is-paused" : status === "failed" ? "is-failed" : "";
  const statusClass =
    status === "completed"
      ? "is-status--completed"
      : status === "paused"
        ? "is-status--paused"
        : status === "failed"
          ? "is-status--failed"
          : "";

  return (
    <button
      type="button"
      className={`dw-card dw-card--hover dw-session-card ${cls}`}
      onClick={() => void resumeSession(session.id)}
      title={status === "completed" ? `Open ${session.targetName} results` : `Resume ${session.targetName} scan`}
    >
      <span className={`dw-session-status ${cls}`} aria-hidden="true">
        {status === "completed" ? (
          <CheckIcon size={15} strokeWidth={2.6} />
        ) : status === "paused" ? (
          <PauseIcon size={13} strokeWidth={2.6} />
        ) : status === "failed" ? (
          <WarnIcon size={13} strokeWidth={2.4} />
        ) : (
          <span className="dw-scan-strip-ticker" />
        )}
      </span>
      <span className="dw-session-info">
        <span className="dw-session-name dw-truncate">{session.targetName}</span>
        <span className="dw-session-meta dw-num">
          <span className={`is-status ${statusClass}`}>{STATUS_LABEL[status]}</span>
          {" · "}
          {formatCountFull(session.filesFound)} files
        </span>
      </span>
      <ChevronRightIcon size={15} className="dw-session-chevron" />
    </button>
  );
}
