/**
 * The activation dialog + Pro status card (DiskGenie licensing §7,
 * adapted to DiskWizard copy): Later/Purchase when empty, Activate
 * when the key is complete (DB-XXXXX-XXXXX-XXXXX-XXXXX), typed error
 * copy from the server contract (KEY_NOT_FOUND / LICENSE_EXPIRED /
 * KEY_REVOKED / DEVICE_SLOT_TAKEN), a ~1.4s success state that
 * auto-closes, and the Pro card (Status: Active, identity, tier,
 * Active until / Lifetime, Validate now / Deactivate / Done).
 */
import { AnimatePresence, motion } from "framer-motion";
import { useEffect, useMemo, useRef, useState } from "react";
import { CheckIcon, KeyIcon, LoaderIcon, ShieldCheckIcon } from "lucide-react";
import { formatDate } from "../lib/format";
import { formatKeyInput, isKeyComplete, KEY_DISPLAY_LEN } from "../lib/licenseKey";
import { useLicenseStore } from "../state/license";
import { useViewStore } from "../state/view";
import { userFacingError } from "../lib/userFacingError";
import { BrandTile } from "../brand/BrandMark";

export function LicenseDialog() {
  const open = useViewStore((s) => s.licenseDialogOpen);
  const close = useViewStore((s) => s.closeLicenseDialog);
  const status = useLicenseStore((s) => s.status);
  const busy = useLicenseStore((s) => s.busy);
  const error = useLicenseStore((s) => s.error);
  const activated = useLicenseStore((s) => s.activated);
  const activate = useLicenseStore((s) => s.activate);
  const deactivate = useLicenseStore((s) => s.deactivate);
  const validateNow = useLicenseStore((s) => s.validateNow);
  const consumeActivated = useLicenseStore((s) => s.consumeActivated);

  const [key, setKey] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const isPro = status?.isPro ?? false;
  const complete = isKeyComplete(key);

  // Success → auto-close after the 1.4s celebration state.
  useEffect(() => {
    if (activated) {
      const t = window.setTimeout(() => {
        consumeActivated();
        setKey("");
        close();
      }, 1_400);
      return () => window.clearTimeout(t);
    }
    return undefined;
  }, [activated, close, consumeActivated]);

  // Focus the field whenever the (re)activation state opens.
  useEffect(() => {
    if (open && !isPro) window.setTimeout(() => inputRef.current?.focus(), 120);
  }, [open, isPro]);

  const errorCopy = useMemo(() => userFacingError(error), [error]);

  if (!open) return null;
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
          className="dw-dialog dw-license-dialog"
          role="dialog"
          aria-modal="true"
          aria-label={isPro ? "DiskWizard Pro" : "Activate DiskWizard"}
          initial={{ scale: 0.95, opacity: 0, y: 12 }}
          animate={{ scale: 1, opacity: 1, y: 0 }}
          exit={{ scale: 0.97, opacity: 0 }}
          transition={{ duration: 0.26, ease: [0.2, 0.9, 0.3, 1.04] }}
        >
          <button type="button" className="dw-dialog-x" onClick={close} aria-label="Close">
            ✕
          </button>

          {activated ? (
            <SuccessState />
          ) : isPro && status ? (
            <ProCard
              status={status}
              busy={busy}
              onValidate={() => void validateNow()}
              onDeactivate={() => void deactivate()}
              onDone={close}
            />
          ) : (
            <div className="dw-license-activate">
              <div className="dw-license-head">
                <BrandTile size={40} />
                <div>
                  <div className="dw-license-title dw-display">Activate DiskWizard</div>
                  <div className="dw-license-sub">
                    Enter your license code to unlock full recovery, file repair and every tool.
                  </div>
                </div>
              </div>

              <div className="dw-license-field">
                <KeyIcon size={15} className="dw-license-field-icon" aria-hidden="true" />
                <input
                  ref={inputRef}
                  value={key}
                  onChange={(e) => setKey(formatKeyInput(e.target.value))}
                  placeholder="DB-XXXXX-XXXXX-XXXXX-XXXXX"
                  spellCheck={false}
                  autoComplete="off"
                  maxLength={KEY_DISPLAY_LEN}
                  aria-label="License code"
                  disabled={busy}
                />
                {key && !busy && (
                  <button type="button" className="dw-license-clear" onClick={() => setKey("")} aria-label="Clear">
                    ✕
                  </button>
                )}
              </div>

              {errorCopy && <div className="dw-license-error" role="alert">{errorCopy}</div>}

              <div className="dw-license-note">
                No reinstallation or rescan required after activation.
              </div>

              <div className="dw-license-actions">
                <button type="button" className="dw-outline-button" onClick={close} disabled={busy}>
                  Later
                </button>
                {complete ? (
                  <button type="button" className="dw-ink-button" onClick={() => void activate(key)} disabled={busy}>
                    {busy ? <LoaderIcon size={14} className="dw-spin" /> : <ShieldCheckIcon size={14} />}
                    {busy ? "Activating…" : "Activate"}
                  </button>
                ) : (
                  <a
                    className="dw-ink-button dw-license-buy"
                    href={status?.purchaseUrl ?? "https://diskwizard.app/pricing"}
                    target="_blank"
                    rel="noreferrer"
                  >
                    Purchase Licence
                  </a>
                )}
              </div>

              <div className="dw-license-foot">
                Don't have a license?{" "}
                <a href={status?.purchaseUrl ?? "https://diskwizard.app/pricing"} target="_blank" rel="noreferrer">
                  Buy a License Now
                </a>
              </div>
            </div>
          )}
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
}

function SuccessState() {
  return (
    <div className="dw-license-success">
      <div className="dw-license-success-badge">
        <CheckIcon size={26} strokeWidth={2.4} />
      </div>
      <div className="dw-license-success-title dw-display">DiskWizard Pro activated</div>
      <div className="dw-license-success-sub">Thank you for purchasing — every capability is unlocked.</div>
    </div>
  );
}

function ProCard({
  status,
  busy,
  onValidate,
  onDeactivate,
  onDone,
}: {
  status: NonNullable<ReturnType<typeof useLicenseStore.getState>["status"]>;
  busy: boolean;
  onValidate: () => void;
  onDeactivate: () => void;
  onDone: () => void;
}) {
  const lifetime = status.licenseExpiresAt === 0;
  return (
    <div className="dw-license-pro">
      <div className="dw-license-head">
        <BrandTile size={40} />
        <div>
          <div className="dw-license-title dw-display">
            DiskWizard <span className="dw-chip dw-chip-license is-pro">Pro</span>
          </div>
          <div className="dw-license-sub">Thank you for purchasing DiskWizard.</div>
        </div>
      </div>
      <dl className="dw-license-rows">
        <div className="dw-license-row">
          <dt>Status</dt>
          <dd>
            <span className="dw-chip dw-chip-ok">Active</span>
          </dd>
        </div>
        <div className="dw-license-row">
          <dt>Licensee</dt>
          <dd>{status.customerName}</dd>
        </div>
        <div className="dw-license-row">
          <dt>Email</dt>
          <dd>{status.customerEmail}</dd>
        </div>
        <div className="dw-license-row">
          <dt>Tier</dt>
          <dd className="dw-text-cap">{status.tier}</dd>
        </div>
        <div className="dw-license-row">
          <dt>Active until</dt>
          <dd>{lifetime ? "Lifetime" : formatDate(status.licenseExpiresAt)}</dd>
        </div>
        {status.posture === "grace" && (
          <div className="dw-license-row">
            <dt>Offline grace</dt>
            <dd>{status.graceDaysLeft} days left — reconnect to revalidate</dd>
          </div>
        )}
      </dl>
      <div className="dw-license-actions">
        <button type="button" className="dw-outline-button" onClick={onValidate} disabled={busy}>
          {busy ? "Validating…" : "Validate now"}
        </button>
        <button type="button" className="dw-outline-button dw-danger" onClick={onDeactivate} disabled={busy}>
          Deactivate
        </button>
        <button type="button" className="dw-ink-button" onClick={onDone}>
          Done
        </button>
      </div>
    </div>
  );
}
