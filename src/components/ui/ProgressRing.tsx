/**
 * ProgressRing — the circular gauge (sidebar scan widget, overlays).
 * Pure SVG; percent-driven with a smooth dash transition. Numbers
 * inside use the tabular-numeric utility.
 */
export function ProgressRing({
  percent,
  size = 54,
  strokeWidth = 4.5,
  paused,
  showLabel = true,
  label,
}: {
  percent: number;
  size?: number;
  strokeWidth?: number;
  paused?: boolean;
  showLabel?: boolean;
  label?: string;
}) {
  const r = (size - strokeWidth * 2) / 2;
  const c = 2 * Math.PI * r;
  const p = Math.min(100, Math.max(0, percent));
  return (
    <div
      className={`dw-scan-ring ${paused ? "is-paused" : ""}`}
      style={{ width: size, height: size }}
      role="img"
      aria-label={label ?? `Progress ${Math.round(p)} percent`}
    >
      <svg width={size} height={size} aria-hidden="true">
        <circle
          className="dw-scan-ring-track"
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          strokeWidth={strokeWidth}
        />
        <circle
          className="dw-scan-ring-fill"
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          strokeWidth={strokeWidth}
          strokeLinecap="round"
          strokeDasharray={c}
          strokeDashoffset={c * (1 - p / 100)}
          transform={`rotate(-90 ${size / 2} ${size / 2})`}
        />
      </svg>
      {showLabel && (
        <span className="dw-scan-ring-num dw-num" style={{ fontSize: Math.max(10, size * 0.235) }}>
          {Math.round(p)}%
        </span>
      )}
    </div>
  );
}
