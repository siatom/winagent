import { useTranslation } from "react-i18next";
import { useState } from "react";
import {
  Activity,
  CheckCircle2,
  ChevronDown,
  Database,
  Loader2,
  MonitorSmartphone,
  ShieldAlert,
  Terminal,
  Variable,
  XCircle,
} from "lucide-react";
import type { PendingTool, RiskLevel, ToolMeta } from "../lib/types";
import { DECISION_LABELS, RISK_LABELS, TOOL_LABELS } from "../lib/types";
import { cn, formatDuration, riskTone } from "../lib/utils";

const icons: Record<string, typeof Terminal> = {
  run_powershell: Terminal,
  process: Activity,
  env_var: Variable,
  registry: Database,
  system_info: MonitorSmartphone,
};

export function RiskBadge({ level, className }: { level: RiskLevel; className?: string }) {
  const { t } = useTranslation();
  const tone = riskTone[level];
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full border px-1.5 py-[1px] text-[10.5px] font-medium",
        tone.bg,
        tone.border,
        tone.text,
        className,
      )}
    >
      <span className={cn("h-1 w-1 rounded-full", tone.dot)} />
      {t(RISK_LABELS[level])}
    </span>
  );
}

function Section({ label, value, tone }: { label: string; value: string; tone?: "error" }) {
  return (
    <div>
      <div className="mb-1 text-[10.5px] font-medium uppercase tracking-wider text-dim">{label}</div>
      <pre
        className={cn(
          "max-h-[240px] overflow-auto whitespace-pre-wrap break-all rounded-[8px] border border-line bg-[#0b0e14] px-2.5 py-2 font-mono text-[12px] leading-relaxed",
          tone === "error" ? "text-[#ff8b8b]" : "text-[#c8d3e6]",
        )}
      >
        {value}
      </pre>
    </div>
  );
}

export function ToolCard({
  name,
  meta,
  pending,
}: {
  name: string;
  meta?: ToolMeta;
  pending?: PendingTool;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(!(meta?.ok ?? true));
  const Icon = icons[name] ?? Terminal;
  const level = meta?.level ?? pending?.level ?? "info";
  const tone = riskTone[level];
  const command = meta?.command ?? pending?.command ?? "";
  const summary = meta?.summary ?? pending?.summary ?? name;
  const reasons = meta?.reasons ?? pending?.reasons ?? [];
  const elevated = pending?.elevated ?? false;

  return (
    <div className={cn("overflow-hidden rounded-[11px] border bg-surface-2/50", tone.border)}>
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2.5 px-3 py-2 text-left transition-colors hover:bg-surface-3/50"
      >
        <span className={cn("grid h-6 w-6 shrink-0 place-items-center rounded-[6px] border", tone.bg, tone.border)}>
          <Icon size={12.5} className={tone.text} />
        </span>
        <span className="shrink-0 text-[11.5px] font-medium text-fg">{t(TOOL_LABELS[name] ?? name)}</span>
        <span className="min-w-0 flex-1 truncate text-[12px] text-muted">{summary}</span>
        <span className="flex shrink-0 items-center gap-2 text-[10.5px] text-dim">
          {elevated && (
            <span className="inline-flex items-center gap-0.5 text-warn" title={t("以管理员权限运行")}>
              <ShieldAlert size={11} /> {t("提权")}</span>
          )}
          {meta?.durationMs != null && <span className="font-mono">{formatDuration(meta.durationMs)}</span>}
          {!pending && <RiskBadge level={level} />}
          {pending ? (
            <Loader2 size={13} className="spin text-accent" />
          ) : meta?.ok ? (
            <CheckCircle2 size={13} className="text-ok" />
          ) : (
            <XCircle size={13} className="text-danger" />
          )}
          <ChevronDown size={13} className={cn("transition-transform", open && "rotate-180")} />
        </span>
      </button>

      {open && (
        <div className="space-y-2.5 border-t border-line/70 px-3 py-2.5">
          {command && <Section label={t("命令")} value={command} />}
          {meta?.stdout ? <Section label={t("输出")} value={meta.stdout} /> : null}
          {meta?.stderr ? <Section label={t("错误")} value={meta.stderr} tone="error" /> : null}
          {!meta && pending && (
            <div className="flex items-center gap-2 text-[12px] text-muted">
              <Loader2 size={12} className="spin" />
              {pending.requiresApproval ? t("等待确认…") : t("正在执行…")}
            </div>
          )}
          {meta && (
            <div className="flex flex-wrap items-center gap-1.5 text-[10.5px] text-dim">
              <span className="rounded bg-surface-3 px-1.5 py-0.5 font-mono">
                exit {meta.exitCode ?? "null"}
              </span>
              <span className="rounded bg-surface-3 px-1.5 py-0.5">
                {t(DECISION_LABELS[meta.decision] ?? meta.decision)}
              </span>
              {meta.timedOut && <span className="rounded bg-danger/15 px-1.5 py-0.5 text-danger">{t("已超时")}</span>}
            </div>
          )}
          {reasons.length > 0 && (
            <ul className="space-y-1">
              {reasons.map((reason) => (
                <li key={reason} className={cn("flex gap-1.5 text-[11.5px]", tone.text)}>
                  <span className="mt-[7px] h-1 w-1 shrink-0 rounded-full bg-current opacity-70" />
                  {reason}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}
