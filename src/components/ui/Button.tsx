/**
 * Button — the single button primitive. Variants map 1:1 onto
 * components/buttons.css.
 */
import type { ButtonHTMLAttributes, ReactNode } from "react";

export type ButtonVariant =
  | "primary"
  | "secondary"
  | "outline"
  | "ghost"
  | "danger";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: "sm" | "md" | "lg";
  block?: boolean;
  leading?: ReactNode;
  trailing?: ReactNode;
}

export function Button({
  variant = "outline",
  size = "md",
  block,
  leading,
  trailing,
  className,
  children,
  type = "button",
  ...rest
}: ButtonProps) {
  const classes = [
    "dw-btn",
    `dw-btn--${variant}`,
    size === "sm" ? "dw-btn--sm" : size === "lg" ? "dw-btn--lg" : "",
    block ? "dw-btn--block" : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <button type={type} className={classes} {...rest}>
      {leading}
      {children}
      {trailing}
    </button>
  );
}
