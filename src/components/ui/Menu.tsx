/**
 * Menu — the "…" dropdown. Click-outside + Escape close, focus return,
 * ARIA menu semantics. Portal-free (renders in place) so screenshots
 * and tours stay deterministic.
 */
import {
  useEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
} from "react";

export interface MenuItem {
  id: string;
  label: string;
  icon?: ReactNode;
  danger?: boolean;
  onSelect: () => void;
}

export function Menu({
  trigger,
  items,
  label,
  align = "end",
}: {
  trigger: (open: boolean, toggle: () => void, ariaId: string) => ReactNode;
  items: MenuItem[];
  label: string;
  align?: "start" | "end";
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const id = useId();

  // Click-outside + Escape (a11y: the menu behaves like a popover).
  useEffect(() => {
    if (!open) return undefined;
    const onDown = (e: PointerEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        setOpen(false);
      }
    };
    window.addEventListener("pointerdown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="dw-menu-anchor" ref={rootRef}>
      {trigger(open, () => setOpen((v) => !v), id)}
      {open && (
        <div
          className="dw-menu"
          role="menu"
          aria-label={label}
          style={align === "start" ? { left: 0, right: "auto" } : undefined}
        >
          {items.map((item, i) => (
            <div key={item.id}>
              {i > 0 && item.id === "sep" && <div className="dw-menu__sep" />}
              {item.id !== "sep" && (
                <button
                  type="button"
                  role="menuitem"
                  className={`dw-menu__item ${item.danger ? "dw-menu__item--danger" : ""}`}
                  onClick={() => {
                    setOpen(false);
                    item.onSelect();
                  }}
                >
                  {item.icon && (
                    <span className="dw-menu__icon" aria-hidden="true">
                      {item.icon}
                    </span>
                  )}
                  {item.label}
                </button>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
