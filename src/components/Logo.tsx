import { cn } from "../lib/utils";
import appIcon from "../../src-tauri/app-icon.svg";

export function Logo({ size = 26, className }: { size?: number; className?: string }) {
  return (
    <img
      src={appIcon}
      alt=""
      aria-hidden="true"
      width={size}
      height={size}
      draggable={false}
      className={cn("shrink-0 select-none", className)}
      style={{ width: size, height: size }}
    />
  );
}
