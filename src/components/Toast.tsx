/**
 * Global toast (bottom-center, auto-dismissing) — success / info /
 * warning notices (session resumed, access restored, recovery done…).
 */
import { AnimatePresence, motion } from "framer-motion";
import { useEffect } from "react";
import { AlertIcon, CheckIcon, InfoIcon, ShieldIcon } from "../shell/icons";
import { useViewStore } from "../state/view";

const DWELL = 3_600;

export function Toast() {
  const toast = useViewStore((s) => s.toast);
  const clear = useViewStore((s) => s.clearToast);

  useEffect(() => {
    if (!toast) return undefined;
    const t = window.setTimeout(clear, DWELL);
    return () => window.clearTimeout(t);
  }, [toast, clear]);

  return (
    <div className="dw-toast-layer" aria-live="polite">
      <AnimatePresence>
        {toast && (
          <motion.div
            key={toast.text}
            className={`dw-toast is-${toast.icon}`}
            initial={{ opacity: 0, y: 16, scale: 0.96 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 8, scale: 0.97 }}
            transition={{ duration: 0.26, ease: [0.2, 0.9, 0.3, 1.02] }}
          >
            <span className="dw-toast-icon">
              {toast.icon === "check" && <CheckIcon size={15} strokeWidth={2.4} />}
              {toast.icon === "shield" && <ShieldIcon size={15} strokeWidth={2.1} />}
              {toast.icon === "trash" && <AlertIcon size={15} strokeWidth={2.1} />}
              {toast.icon === "info" && <InfoIcon size={15} strokeWidth={2.1} />}
            </span>
            {toast.text}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
