import type { ButtonHTMLAttributes } from "react";
import { cn } from "../../lib/utils";

type Variant = "primary" | "ghost" | "outline" | "danger" | "subtle";
type Size = "sm" | "md" | "lg";

const variants: Record<Variant, string> = {
  primary:
    "bg-accent text-white hover:brightness-110 active:brightness-95 shadow-[0_6px_20px_-8px_rgba(109,140,255,0.8)]",
  ghost: "text-muted hover:text-fg hover:bg-surface-3",
  outline: "border border-line-strong text-fg hover:bg-surface-3",
  danger: "bg-critical text-white hover:brightness-110",
  subtle: "bg-surface-3 text-fg hover:bg-line-strong",
};

const sizes: Record<Size, string> = {
  sm: "h-7 px-2.5 text-[12.5px] rounded-[7px] gap-1.5",
  md: "h-9 px-3.5 text-[13px] rounded-[9px] gap-2",
  lg: "h-10 px-5 text-[14px] rounded-[10px] gap-2",
};

export function Button({
  variant = "outline",
  size = "md",
  className,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; size?: Size }) {
  return (
    <button
      className={cn(
        "inline-flex select-none items-center justify-center font-medium transition-all duration-150",
        "disabled:cursor-not-allowed disabled:opacity-40",
        variants[variant],
        sizes[size],
        className,
      )}
      {...props}
    />
  );
}
