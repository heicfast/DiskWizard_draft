/**
 * BitLocker unlock (research: CBitlockerStartWnd →
 * BitlockerUnlockDialog — password or recovery key, "Unlock and
 * Recover"): clicking a locked drive opens this dialog; a successful
 * unlock flips the drive to the unlocked state and continues straight
 * into the scan.
 */
import { AnimatePresence, motion } from "framer-motion";
import { useEffect, useState } from "react";
import { KeyIcon, LockIcon, ShieldIcon, UnlockIcon } from "lucide-react";
import { invoke } from "../../lib/ipc";
import { useDevicesStore } from "../../state/devices";
import { useScanStore } from "../../state/scan";
import { useViewStore } from "../../state/view";
import type { StorageDevice } from "../../types";

export function BitLockerDialog({ device, onClose }: { device: StorageDevice | null; onClose: () => void }) {
  const [mode, setMode] = useState<"password" | "key">("password");
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const start = useScanStore((s) => s.start);
  const showToast = useViewStore((s) => s.showToast);
  const refresh = useDevicesStore((s) => s.refresh);

  useEffect(() => {
    setMode("password");
    setValue("");
    setError(null);
    setBusy(false);
  }, [device]);

  if (!device) return null;

  const onUnlock = () => {
    if (!value.trim()) {
      setError(mode === "password" ? "Enter your BitLocker password." : "Enter the 48-digit recovery key.");
      return;
    }
    if (mode === "key" && value.replace(/[^0-9]/g, "").length < 48) {
      setError("The recovery key is 48 digits — check the printout or your Microsoft account.");
      return;
    }
    setBusy(true);
    setError(null);
    // The DRWService FVE pipe round-trip, simulated at demo timescale.
    void (async () => {
      try {
        await invoke("unlock_bitlocker", { deviceId: device.id, mode, value });
        setBusy(false);
        showToast(`${device.name} unlocked — BitLocker volume mounted`, "shield");
        onClose();
        await refresh().catch(() => undefined);
        void start(device.id);
      } catch (e) {
        setBusy(false);
        setError(
          String(e).startsWith("NOT_LOCKED")
            ? "This drive is no longer locked — refresh the list and scan it."
            : "The password or recovery key was not accepted. Try again.",
        );
      }
    })();
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
          className="dw-dialog dw-bitlocker-dialog"
          role="dialog"
          aria-modal="true"
          aria-label={`Unlock ${device.name}`}
          initial={{ scale: 0.95, opacity: 0, y: 12 }}
          animate={{ scale: 1, opacity: 1, y: 0 }}
          exit={{ scale: 0.97, opacity: 0 }}
          transition={{ duration: 0.26, ease: [0.2, 0.9, 0.3, 1.04] }}
        >
          <button type="button" className="dw-dialog-x" onClick={onClose} aria-label="Close">✕</button>
          <div className="dw-stop-icon is-ink">
            <LockIcon size={22} strokeWidth={1.9} />
          </div>
          <h2 className="dw-dialog-title dw-display">Unlock {device.name}</h2>
          <p className="dw-dialog-sub">
            This drive is protected by BitLocker. Enter your password or the recovery key
            to unlock and recover — keys are read through the FVE service and never stored.
          </p>

          <div className="dw-segmented dw-bitlocker-modes" role="tablist" aria-label="Unlock method">
            <button
              type="button"
              role="tab"
              aria-selected={mode === "password"}
              className={mode === "password" ? "is-on" : ""}
              onClick={() => { setMode("password"); setError(null); }}
            >
              <KeyIcon size={13} /> Password
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={mode === "key"}
              className={mode === "key" ? "is-on" : ""}
              onClick={() => { setMode("key"); setError(null); }}
            >
              <ShieldIcon size={13} /> Recovery Key
            </button>
          </div>

          <div className="dw-license-field">
            {mode === "password" ? <KeyIcon size={15} className="dw-license-field-icon" /> : <ShieldIcon size={15} className="dw-license-field-icon" />}
            <input
              value={value}
              onChange={(e) => setValue(e.target.value)}
              placeholder={mode === "password" ? "BitLocker password" : "XXXXXX-XXXXXX-XXXXXX-XXXXXX-XXXXXX-XXXXXX-XXXXXX-XXXXXX"}
              type={mode === "password" ? "password" : "text"}
              spellCheck={false}
              autoComplete="off"
              aria-label={mode === "password" ? "BitLocker password" : "Recovery key"}
              disabled={busy}
              onKeyDown={(e) => {
                if (e.key === "Enter") onUnlock();
              }}
            />
          </div>

          {error && <div className="dw-license-error" role="alert">{error}</div>}

          <p className="dw-dest-warn">
            Lost the key? The recovery key lives in your Microsoft account
            (aka.ms/myrecoverykey) or on the printout saved when the drive was encrypted.
          </p>

          <div className="dw-dialog-actions">
            <button type="button" className="dw-outline-button" onClick={onClose} disabled={busy}>Cancel</button>
            <button type="button" className="dw-ink-button" onClick={onUnlock} disabled={busy}>
              <UnlockIcon size={14} />
              {busy ? "Unlocking…" : "Unlock and Recover"}
            </button>
          </div>
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
}
