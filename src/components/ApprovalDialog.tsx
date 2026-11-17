import { useTranslation } from "react-i18next";
import { useState } from "react";
import { Dialog } from "radix-ui";
import { AlertTriangle, ShieldAlert, Terminal } from "lucide-react";
import { useApp } from "../store/app";
import { RISK_LABELS, TOOL_LABELS } from "../lib/types";
import { cn, riskTone } from "../lib/utils";
import { Button } from "./ui/Button";

export function ApprovalDialog() {
  const { t } = useTranslation();
  const approvals = useApp((s) => s.approvals);
  const resolve = useApp((s) => s.resolveApproval);
  const [remember, setRemember] = useState(false);
  const current = approvals[0];
  const sessionTitle = useApp((s) => s.sessions.find((session) => session.id === current?.sessionId)?.title);
  const tone = current ? riskTone[current.level] : riskTone.info;

  return (
    <Dialog.Root
      open={Boolean(current)}
      onOpenChange={(open) => {
        if (!open && current) {
          setRemember(false);
          void resolve(false, false);
        }
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-black/55 backdrop-blur-[3px] data-[state=open]:animate-[fade-up_0.15s_ease-out]" />
        <Dialog.Content
          className="fixed left-1/2 top-1/2 z-50 w-[min(640px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 overflow-hidden rounded-[16px] border border-line-strong bg-surface shadow-[var(--shadow-panel)] outline-none animate-fade-up"
          onEscapeKeyDown={() => {
            setRemember(false);
            void resolve(false, false);
          }}
        >
          {current && (
            <>
              <div className={cn("h-[3px] w-full", current.level === "critical" ? "bg-critical" : "bg-danger")} />
              <div className="px-5 pb-4 pt-4">
                <div className="flex items-start gap-3">
                  <div className={cn("grid h-9 w-9 shrink-0 place-items-center rounded-[10px] border", tone.bg, tone.border)}>
                    <ShieldAlert size={17} className={tone.text} />
                  </div>
                  <div className="min-w-0 flex-1">
                    <Dialog.Title className="flex items-center gap-2 text-[15px] font-semibold text-fg">
                      {t("需要确认")}
                      <span className={cn("rounded-full border px-1.5 py-[1px] text-[10.5px]", tone.bg, tone.border, tone.text)}>
                        {t(RISK_LABELS[current.level])}
                      </span>
                    </Dialog.Title>
                    <Dialog.Description className="mt-0.5 text-[12.5px] text-muted">
                      {sessionTitle && <span className="block truncate text-accent">{sessionTitle}</span>}
                      {t(TOOL_LABELS[current.tool] ?? current.tool)} · {current.summary}
                    </Dialog.Description>
                  </div>
                </div>

                <div className="mt-3.5">
                  <div className="mb-1 flex items-center gap-1.5 text-[10.5px] uppercase tracking-wider text-dim">
                    <Terminal size={11} />
                    {t("即将执行的命令")}
                  </div>
                  <pre className="max-h-[240px] overflow-auto whitespace-pre-wrap break-all rounded-[10px] border border-line bg-[#0b0e14] px-3 py-2.5 font-mono text-[12px] leading-relaxed text-[#c8d3e6]">
                    {current.command}
                  </pre>
                </div>

                {current.reasons.length > 0 && (
                  <ul className="mt-3 space-y-1.5">
                    {current.reasons.map((reason) => (
                      <li key={reason} className={cn("flex gap-2 text-[12px]", tone.text)}>
                        <AlertTriangle size={12} className="mt-[3px] shrink-0" />
                        {reason}
                      </li>
                    ))}
                  </ul>
                )}

                {current.elevated && (
                  <div className="mt-3 rounded-[8px] border border-warn/30 bg-warn/10 px-3 py-2 text-[12px] text-warn">
                    {t("这条命令需要管理员权限，确认后会弹出 Windows UAC 提权窗口。")}
                  </div>
                )}

                <label className="mt-3.5 flex cursor-pointer items-center gap-2 text-[12px] text-muted">
                  <input
                    type="checkbox"
                    checked={remember}
                    onChange={(e) => setRemember(e.target.checked)}
                    className="h-3.5 w-3.5 accent-[var(--accent)]"
                  />
                  {t("本次会话内，同类操作不再询问")}
                </label>
              </div>

              <div className="flex items-center justify-end gap-2 border-t border-line bg-surface-2/50 px-5 py-3">
                <Button
                  variant="ghost"
                  onClick={() => {
                    setRemember(false);
                    void resolve(false, false);
                  }}
                >
                  {t("取消")}
                </Button>
                <Button
                  variant={current.level === "critical" ? "danger" : "primary"}
                  onClick={() => {
                    const rememberNow = remember;
                    setRemember(false);
                    void resolve(true, rememberNow);
                  }}
                >
                  {t("执行")}
                </Button>
              </div>
            </>
          )}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
