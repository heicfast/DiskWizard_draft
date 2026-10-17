/**
 * Cloud Recovery — the cloud-drive hub (research flow: sign-in →
 * "Select a Cloud Drive to Start Recovering" → scan). Provider cards
 * for iCloud / Google Drive / Dropbox / OneDrive with the trust copy
 * ("DiskWizard NEVER keeps records of your account"), a simulated
 * OAuth sign-in, and drive tiles that launch the recovery scan.
 */
import { useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import {
  ArrowRightIcon,
  CheckIcon,
  CloudIcon,
  DatabaseIcon,
  LoaderIcon,
  ShieldIcon,
} from "../../shell/icons";
import { formatBytes } from "../../lib/format";
import { invoke } from "../../lib/ipc";
import { useScanStore } from "../../state/scan";
import { useViewStore } from "../../state/view";
import type { CloudDrive, CloudProvider } from "../../types";

const PROVIDERS: { id: CloudProvider; label: string; tint: string; note: string }[] = [
  { id: "icloud", label: "iCloud Drive", tint: "dw-cloud-icloud", note: "Photos, Drive files, notes" },
  { id: "google", label: "Google Drive", tint: "dw-cloud-google", note: "Drive, Photos, shared items" },
  { id: "dropbox", label: "Dropbox", tint: "dw-cloud-dropbox", note: "Files, folders, paper docs" },
  { id: "onedrive", label: "OneDrive", tint: "dw-cloud-onedrive", note: "Personal + business" },
];

export function CloudPage() {
  const [drives, setDrives] = useState<CloudDrive[] | null>(null);
  const [signingIn, setSigningIn] = useState<CloudProvider | null>(null);
  const [signedIn, setSignedIn] = useState<Set<CloudProvider>>(new Set());
  const startCloud = useScanStore((s) => s.startCloud);
  const showToast = useViewStore((s) => s.showToast);

  // Browsable without a license (the gate fires on the scan command).
  useEffect(() => {
    void invoke<CloudDrive[]>("list_cloud_drives")
      .then(setDrives)
      .catch(() => setDrives([]));
  }, []);

  const connected = useMemo(() => drives ?? [], [drives]);

  const signIn = async (provider: CloudProvider) => {
    setSigningIn(provider);
    try {
      await invoke("cloud_sign_in", { provider });
      setSignedIn((s) => new Set(s).add(provider));
      showToast(`${PROVIDERS.find((p) => p.id === provider)?.label} connected`, "check");
    } catch {
      showToast("Could not sign in — check your connection", "info");
    } finally {
      setSigningIn(null);
    }
  };

  return (
    <div className="dw-page dw-page-cloud dw-scroll">
      <header className="dw-page-head">
        <div>
          <div className="dw-eyebrow">Cloud Recovery</div>
          <h1 className="dw-page-title dw-display">Recover from the cloud</h1>
          <p className="dw-page-sub">
            Sign in to a cloud drive and recover deleted files — DiskWizard scans your
            cloud trash and previous states without leaving the app.
          </p>
        </div>
      </header>

      <section className="dw-section">
        <div className="dw-section-head">
          <h2 className="dw-section-title">Select a Cloud Drive to Start Recovering</h2>
        </div>
        <div className="dw-cloud-grid">
          {PROVIDERS.map((p, i) => {
            const drive = connected.find((d) => d.provider === p.id);
            const isSignedIn = signedIn.has(p.id);
            const busy = signingIn === p.id;
            return (
              <motion.div
                key={p.id}
                className={`dw-card dw-cloud-card ${p.tint}`}
                initial={{ opacity: 0, y: 14 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ delay: i * 0.05, duration: 0.32, ease: [0.2, 0.8, 0.3, 1] }}
              >
                <span className="dw-cloud-art">
                  <CloudIcon size={24} strokeWidth={1.6} />
                </span>
                <div className="dw-cloud-copy">
                  <div className="dw-cloud-name">{p.label}</div>
                  <div className="dw-cloud-note">{isSignedIn && drive ? drive.account : p.note}</div>
                  {isSignedIn && drive && (
                    <div className="dw-cloud-usage dw-num">
                      {formatBytes(drive.used)} of {formatBytes(drive.total)} used
                      <span className="dw-cloud-bar" aria-hidden="true">
                        <span style={{ width: `${Math.min(100, (drive.used / drive.total) * 100)}%` }} />
                      </span>
                    </div>
                  )}
                </div>
                {isSignedIn && drive ? (
                  <button type="button" className="dw-ink-button dw-cloud-scan" onClick={() => void startCloud(drive.id)}>
                    Scan Drive
                    <ArrowRightIcon size={14} />
                  </button>
                ) : (
                  <button type="button" className="dw-outline-button" onClick={() => void signIn(p.id)} disabled={busy}>
                    {busy ? <LoaderIcon size={13} className="dw-spin" /> : isSignedIn ? <CheckIcon size={13} /> : null}
                    {busy ? "Signing in…" : "Sign In"}
                  </button>
                )}
              </motion.div>
            );
          })}
        </div>
      </section>

      <section className="dw-section">
        <div className="dw-cloud-trust dw-card">
          <span className="dw-cloud-trust-icon">
            <ShieldIcon size={18} strokeWidth={1.8} />
          </span>
          <div>
            <strong>DiskWizard NEVER keeps records of your account.</strong> Credentials
            stay on this device for the session and are used only to read the drive you
            choose. Please refer to our Privacy Policy for more information.
          </div>
        </div>
      </section>

      <section className="dw-section">
        <div className="dw-section-head">
          <h2 className="dw-section-title">How cloud recovery works</h2>
        </div>
        <div className="dw-cloud-steps">
          <Step num={1} icon={ShieldIcon} title="Sign in securely" body="OAuth sign-in runs locally; DiskWizard never stores your password." />
          <Step num={2} icon={DatabaseIcon} title="We enumerate the drive" body="It will take a few minutes to find cloud drive files. Please be patient!" />
          <Step num={3} icon={CloudIcon} title="Scan and recover" body="Deleted cloud files appear in the same familiar results view — preview, filter and recover." />
        </div>
      </section>
    </div>
  );
}

function Step({
  num,
  icon: Icon,
  title,
  body,
}: {
  num: number;
  icon: typeof CloudIcon;
  title: string;
  body: string;
}) {
  return (
    <div className="dw-cloud-step">
      <span className="dw-cloud-step-num dw-num">{num}</span>
      <span className="dw-cloud-step-icon">
        <Icon size={16} strokeWidth={1.8} />
      </span>
      <div>
        <div className="dw-cloud-step-title">{title}</div>
        <p className="dw-cloud-step-body">{body}</p>
      </div>
    </div>
  );
}
