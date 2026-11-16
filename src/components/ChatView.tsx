import { useTranslation } from "react-i18next";
import { useEffect, useRef, useState } from "react";
import { ArrowUpRight, Sparkles } from "lucide-react";
import { useApp } from "../store/app";
import { Logo } from "./Logo";
import { MessageItem, StreamingMessage } from "./MessageItem";
import { ToolCard } from "./ToolCard";
import { ModelPicker } from "./ModelPicker";

export function ChatView({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { t } = useTranslation();
  const suggestions = [
    { label: t("看看这台电脑的配置"), prompt: t("查看这台电脑的系统信息，包括 CPU、内存和磁盘使用情况。") },
    { label: t("哪些进程最占内存？"), prompt: t("列出当前占用内存最多的 15 个进程。") },
    { label: t("检查开机自启动项"), prompt: t("检查当前有哪些开机自启动项，说明哪些可能是多余的。") },
    { label: t("用户 PATH 里有什么？"), prompt: t("列出当前用户环境变量 PATH 的每一项目录，并标注哪些路径不存在。") },
  ];
  const sessions = useApp((s) => s.sessions);
  const activeId = useApp((s) => s.activeSessionId);
  const messages = useApp((s) => s.messages);
  const streaming = useApp((s) => s.streaming);
  const streamText = useApp((s) => s.streamText);
  const streamReasoning = useApp((s) => s.streamReasoning);
  const pendingTools = useApp((s) => s.pendingTools);
  const send = useApp((s) => s.send);
  const providers = useApp((s) => s.providers);
  const hasKey = providers.some((p) => p.hasKey || !p.needsKey);

  const session = sessions.find((s) => s.id === activeId);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [pinned, setPinned] = useState(true);
  const pending = Object.values(pendingTools);

  useEffect(() => {
    if (!pinned) return;
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages, streamText, streamReasoning, pending.length, pinned]);

  useEffect(() => {
    setPinned(true);
  }, [activeId]);

  const empty = messages.length === 0 && !streaming;

  return (
    <section className="flex min-h-0 min-w-0 flex-1 flex-col">
      <header className="flex h-12 shrink-0 items-center gap-3 border-b border-line px-5">
        <h2 className="truncate text-[13.5px] font-medium text-fg">{session && !session.id.startsWith("draft-") ? session.title : t("新会话")}</h2>
        <span className="hidden text-[11px] text-dim md:inline">
          {t("消息数量", { count: messages.filter((m) => m.role !== "tool").length })}</span>
        <div className="ml-auto flex items-center gap-1">
          <ModelPicker onOpenSettings={onOpenSettings} />
        </div>
      </header>

      <div
        ref={scrollRef}
        onScroll={(e) => {
          const el = e.currentTarget;
          setPinned(el.scrollHeight - el.scrollTop - el.clientHeight < 90);
        }}
        className="min-h-0 flex-1 overflow-y-auto"
      >
        <div className="mx-auto w-full max-w-[820px] px-6 py-6">
          {empty ? (
            <div className="flex flex-col items-center justify-center pt-[12vh] text-center">
              <Logo size={54} className="rounded-[14px]" />
              <h1 className="mt-5 text-[22px] font-semibold tracking-tight text-fg">{t("需要我做什么？")}</h1>
              <p className="mt-2 max-w-[420px] text-[13px] leading-relaxed text-muted">
                {t("我可以直接操作这台 Windows：执行 PowerShell、读写注册表、管理环境变量、查询和结束进程。")}
              </p>
              <div className="mt-7 grid w-full max-w-[560px] grid-cols-1 gap-2 sm:grid-cols-2">
                {suggestions.map((item) => (
                  <button
                    key={item.label}
                    onClick={() => {
                      if (!hasKey) {
                        onOpenSettings();
                        return;
                      }
                      void send(item.prompt);
                    }}
                    className="group flex items-center gap-2 rounded-[10px] border border-line bg-surface-2/60 px-3.5 py-2.5 text-left text-[12.5px] text-muted transition-all hover:border-accent/40 hover:bg-surface-3 hover:text-fg"
                  >
                    <Sparkles size={13} className="shrink-0 text-dim group-hover:text-accent" />
                    <span className="truncate">{item.label}</span>
                    <ArrowUpRight size={12} className="ml-auto shrink-0 text-dim opacity-0 transition-opacity group-hover:opacity-100" />
                  </button>
                ))}
              </div>
            </div>
          ) : (
            <div className="space-y-5">
              {messages.map((message) => (
                <MessageItem key={message.id} message={message} />
              ))}
              {pending.map((tool) => (
                <div key={tool.toolCallId} className="pl-[34px]">
                  <ToolCard name={tool.name} pending={tool} />
                </div>
              ))}
              {streaming && <StreamingMessage text={streamText} reasoning={streamReasoning} />}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
