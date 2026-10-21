/**
 * QuickActionCard — the reference's quick action tile: rose icon tile,
 * bold title, quiet subtitle ("Recycle Bin / Scan deleted files").
 */
import type { ReactNode } from "react";
import { motion } from "framer-motion";

export function QuickActionCard({
  icon,
  title,
  sub,
  onClick,
  index,
}: {
  icon: ReactNode;
  title: string;
  sub: string;
  onClick: () => void;
  index: number;
}) {
  return (
    <motion.button
      type="button"
      className="dw-card dw-card--hover dw-quick-card"
      initial={{ opacity: 0, y: 14 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay: 0.2 + index * 0.05, duration: 0.34, ease: [0.2, 0.8, 0.3, 1] }}
      onClick={onClick}
    >
      <span className="dw-quick-icon">{icon}</span>
      <span className="dw-quick-copy">
        <span className="dw-quick-title">{title}</span>
        <span className="dw-quick-sub">{sub}</span>
      </span>
    </motion.button>
  );
}
