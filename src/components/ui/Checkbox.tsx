/**
 * Checkbox — the reference's rounded-square check (deep magenta fill).
 * `mixed` renders the indeterminate dash (select-all with partial
 * selection).
 */
import type { ButtonHTMLAttributes } from "react";
import { CheckIcon } from "../../shell/icons";

export function CheckSquare({
  checked,
  mixed,
  ...rest
}: {
  checked: boolean;
  mixed?: boolean;
} & ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={mixed ? "mixed" : checked}
      className={`dw-check-square ${checked ? "is-on" : ""} ${mixed ? "is-mixed" : ""}`}
      {...rest}
    >
      {checked && !mixed && <CheckIcon size={12} strokeWidth={3.2} />}
    </button>
  );
}

/** The filter-form checkbox (native input styled via checkbox.css). */
export function FormCheckbox({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <label className="dw-check">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      {label}
    </label>
  );
}
