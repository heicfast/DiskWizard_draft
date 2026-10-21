/**
 * Segmented — the two-mode tab switch. `underline` is the tree-panel
 * header style (Folders | File Reconstruction); `pill` is the compact
 * toolbar toggle.
 */
export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
  ariaLabel?: string;
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
  variant = "underline",
  ariaLabel,
}: {
  value: T;
  options: SegmentedOption<T>[];
  onChange: (v: T) => void;
  variant?: "underline" | "pill";
  ariaLabel: string;
}) {
  return (
    <div
      className={`dw-segmented ${variant === "pill" ? "dw-segmented--pill" : ""}`}
      role="tablist"
      aria-label={ariaLabel}
    >
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="tab"
          aria-selected={value === o.value}
          className={`dw-segmented__tab ${value === o.value ? "is-on" : ""}`}
          onClick={() => onChange(o.value)}
          aria-label={o.ariaLabel ?? o.label}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
