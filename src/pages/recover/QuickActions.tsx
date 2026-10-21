/**
 * Quick actions (Recycle Bin / Desktop / Select Folder / Disk Image)
 * and the recent-session rows — the two lower sections of the
 * source-selection page, extracted for clarity.
 */
import { motion } from "framer-motion";
import { CheckIcon, ChevronRightIcon, PauseIcon } from "lucide-react";
import { formatBytes, formatCountFull, formatDate } from "../../lib/format";
import { useScanStore } from "../../state/scan";
import type { RecoverySession } from "../../types";

/* ── Quick actions ───────────────────────────────────────────────────── */

export function QuickCard({
  icon: Icon,
  title,
  sub,
  onClick,
  index,
}: {
  icon: typeof CheckIcon;
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

export function SessionRow({ session }: { session: RecoverySession }) {
  // The real core's resume_session loads the .rsf and adopts the tree
  // under a fresh generation (CORE-INTERFACE-GAPS P0-3); the mocks
  // answer the same call with a simulated continuation.
  const resumeSession = useScanStore((s) => s.resumeSession);
  const completed = session.status === "completed";
  const paused = session.status === "paused";
  return (
    <button
      type="button"
      className="dw-session-row"
      onClick={() => void resumeSession(session.id)}
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
