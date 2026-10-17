/**
 * The opening splash — short, smooth, logo-led (the owner's brief:
 * "use the logo for opening animation, not very lengthy"). The full
 * approved 3D mark scales in over the theme's ambient wash (the
 * cherry-blossom gradient in the shipped Blossom theme), the wordmark
 * + tagline settle beneath, then the whole veil crossfades into the
 * app. ~2.1s wall clock; honors prefers-reduced-motion.
 */
import { motion } from "framer-motion";
import { useEffect } from "react";
import { useViewStore } from "../state/view";
import { LogoMark } from "../brand/BrandMark";

const HOLD_MS = 2_150;

export function Splash() {
  const advanceBoot = useViewStore((s) => s.advanceBoot);

  useEffect(() => {
    const t = window.setTimeout(advanceBoot, HOLD_MS);
    return () => window.clearTimeout(t);
  }, [advanceBoot]);

  return (
    <motion.div
      className="dw-splash"
      initial={{ opacity: 1 }}
      exit={{ opacity: 0, transition: { duration: 0.42, ease: [0.4, 0, 0.2, 1] } }}
      aria-label="DiskWizard"
    >
      <div className="dw-splash-bloom" aria-hidden="true" />
      <motion.div
        className="dw-splash-mark"
        initial={{ scale: 0.72, opacity: 0, y: 10 }}
        animate={{ scale: 1, opacity: 1, y: 0 }}
        transition={{ duration: 0.72, ease: [0.2, 0.9, 0.3, 1.12] }}
      >
        <LogoMark size={116} />
      </motion.div>
      <motion.div
        className="dw-splash-wording"
        initial={{ opacity: 0, y: 12 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.34, duration: 0.5, ease: [0.2, 0.8, 0.3, 1] }}
      >
        <div className="dw-splash-name dw-display">DiskWizard</div>
        <div className="dw-splash-tagline">Bring your files back.</div>
      </motion.div>
      <motion.div
        className="dw-splash-status"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ delay: 0.8, duration: 0.4 }}
      >
        <span className="dw-splash-spinner" aria-hidden="true" />
        Reading the disk information, please wait…
      </motion.div>
    </motion.div>
  );
}
