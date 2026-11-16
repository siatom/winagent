import { useTranslation } from "react-i18next";
import { useEffect, useRef, useState } from "react";
import { ArrowUp, KeyRound, Square } from "lucide-react";
import { useApp } from "../store/app";
import { Button } from "./ui/Button";

export function Composer({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { t } = useTranslation();
  const send = useApp((s) => s.send);
  const stop = useApp((s) => s.stop);
  const streaming = useApp((s) => s.streaming);
  const ready = useApp((s) => s.ready);
  const providers = useApp((s) => s.providers);
  const activeId = useApp((s) => s.activeSessionId);
  const sessions = useApp((s) => s.sessions);

  const [value, setValue] = useState("");
  const ref = useRef<HTMLTextAreaElement>(null);

  const session = sessions.find((s) => s.id === activeId);
  const provider =
    providers.find((p) => p.id === session?.providerId) ??
    providers.find((p) => p.isDefault) ??
    providers[0];
  const hasKey = Boolean(provider && (provider.hasKey || !provider.needsKey));

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "0px";
    el.style.height = `${Math.min(el.scrollHeight, 220)}px`;
  }, [value]);

  const canSend = ready && hasKey && !streaming && value.trim().length > 0;

  const submit = () => {
    if (!canSend) return;
    const text = value.trim();
    setValue("");
    void send(text);
  };

  return (
    <div className="shrink-0 border-t border-line bg-canvas/70 px-6 py-3.5 backdrop-blur">
      <div className="mx-auto w-full max-w-[820px]">
        {!hasKey && ready && (
          <button
            onClick={onOpenSettings}
            className="mb-2.5 flex w-full items-center gap-2 rounded-[9px] border border-warn/30 bg-warn/10 px-3 py-2 text-left text-[12px] text-warn transition-colors hover:bg-warn/15"
          >
            <KeyRound size={13} />
            {t("还没有配置 {{name}} 的 API Key，点这里去设置", { name: provider?.name ?? t("模型供应商") })}
          </button>
        )}

        <div className="rounded-[14px] border border-line-strong bg-surface-2 shadow-[0_10px_40px_-24px_rgba(0,0,0,0.8)] transition-colors focus-within:border-accent/50">
          <textarea
            ref={ref}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                e.preventDefault();
                submit();
              }
            }}
            rows={1}
            placeholder={t("告诉我要做什么，例如：把用户 PATH 里的 D:\\tools 去掉")}
            className="block max-h-[220px] min-h-[54px] w-full resize-none bg-transparent px-3.5 pb-1 pt-3 text-[14px] leading-relaxed text-fg outline-none placeholder:text-dim"
          />
          <div className="flex items-center gap-2 px-2.5 pb-2.5">
            <span className="flex items-center gap-1.5 rounded-[6px] px-1.5 py-1 text-[11px] text-dim">
              <span className={`h-1.5 w-1.5 rounded-full ${hasKey ? "bg-ok" : "bg-warn"}`} />
              {provider ? `${provider.name} · ${session?.model ?? provider.defaultModel}` : t("未配置模型")}
            </span>
            <div className="ml-auto flex items-center gap-2">
              <span className="hidden text-[10.5px] text-dim sm:inline">{t("Enter 发送 · Shift+Enter 换行")}</span>
              {streaming ? (
                <Button variant="subtle" size="sm" onClick={() => void stop()} className="gap-1.5">
                  <Square size={10} className="fill-current" />
                  {t("停止")}
                </Button>
              ) : (
                <Button
                  variant="primary"
                  size="sm"
                  onClick={submit}
                  disabled={!canSend}
                  className="h-8 w-8 rounded-[8px] px-0"
                  title={t("发送")}
                >
                  <ArrowUp size={15} />
                </Button>
              )}
            </div>
          </div>
        </div>
        <div className="mt-2 flex items-center justify-center gap-1.5 text-[10.5px] text-dim">
          <span>{t("命令会在本机真实执行")}</span>
          <span className="text-line-strong">·</span>
          <span>{t("危险操作会先向你确认")}</span>
        </div>
      </div>
    </div>
  );
}
