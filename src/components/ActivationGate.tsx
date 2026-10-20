/**
 * The in-page lock screen for licensed surfaces (DiskGenie
 * ActivationGate analog): lock glyph, per-page value pitch, Activate +
 * Purchase buttons. Disk Health (Disk Tools) never renders this — it
 * is the free tier's Monitor analog.
 */
import { LockIcon, ShieldCheckIcon } from "lucide-react";
import { useLicenseStore } from "../state/license";
import { useViewStore } from "../state/view";

const PITCHES: Record<string, { title: string; body: string }> = {
  recover: {
    title: "Recover Data — deep scanning on any device",
    body: "NTFS, FAT, exFAT, ReFS and ext file systems. Deleted files, lost partitions, BitLocker volumes and RAW carving — unlimited recovery.",
  },
  results: {
    title: "Scan Results — preview, filter and recover",
    body: "Browse found files with recoverability indicators, preview photos, videos and documents, then recover everything you need.",
  },
  tools: {
    title: "Rescue tools",
    body: "SD Card Rescue, lost-partition repair guidance and disk images.",
  },
};

export function ActivationGate({ area }: { area: keyof typeof PITCHES | string }) {
  const posture = useLicenseStore((s) => s.status?.posture ?? null);
  const openLicenseDialog = useViewStore((s) => s.openLicenseDialog);
  const status = useLicenseStore((s) => s.status);
  const pitch = PITCHES[area] ?? PITCHES.recover;
  const degraded = posture === "degraded";

  return (
    <div className="dw-gate">
      <div className="dw-gate-card dw-settle-in">
        <div className="dw-gate-icon">
          <LockIcon size={22} strokeWidth={1.8} />
        </div>
        <div className="dw-gate-title dw-display">{pitch.title}</div>
        <p className="dw-gate-body">{pitch.body}</p>
        {degraded && (
          <p className="dw-gate-warn">
            Your last validation could not be confirmed. Reconnect to the internet and validate to continue.
          </p>
        )}
        <div className="dw-gate-actions">
          <button type="button" className="dw-ink-button" onClick={openLicenseDialog}>
            <ShieldCheckIcon size={14} />
            Activate
          </button>
          <a
            className="dw-outline-button"
            href={status?.purchaseUrl ?? "https://diskwizard.app/pricing"}
            target="_blank"
            rel="noreferrer"
          >
            Purchase Licence
          </a>
        </div>
        <div className="dw-gate-foot">Disk health monitoring stays free — open Disk Tools anytime.</div>
      </div>
    </div>
  );
}
