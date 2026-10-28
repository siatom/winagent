import { cn } from "../lib/utils";

export function Logo({ size = 26, className }: { size?: number; className?: string }) {
  return (
    <div
      className={cn(
        "relative grid shrink-0 place-items-center rounded-[8px] bg-gradient-to-br from-[#7b9bff] via-[#6d8cff] to-[#a06dff]",
        "shadow-[0_4px_18px_-6px_rgba(109,140,255,0.85)]",
        className,
      )}
      style={{ width: size, height: size }}
    >
      <svg width={size * 0.62} height={size * 0.62} viewBox="0 0 24 24" fill="none">
        <path
          d="M4 7l4.5 5L4 17"
          stroke="white"
          strokeWidth="2.4"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        <path d="M12 18h7" stroke="white" strokeWidth="2.4" strokeLinecap="round" />
      </svg>
    </div>
  );
}
