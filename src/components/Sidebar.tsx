import { useTranslation } from "react-i18next";
import { useState } from "react";
import { Check, Pencil, Plus, Search, Settings2, Trash2, X } from "lucide-react";
import { useApp, isDraftSession } from "../store/app";
import { cn, formatTime } from "../lib/utils";
import { Logo } from "./Logo";

export function Sidebar({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { t } = useTranslation();
  const sessions = useApp((s) => s.sessions);
  const activeId = useApp((s) => s.activeSessionId);
  const streaming = useApp((s) => s.streaming);
  const runSessionId = useApp((s) => s.runSessionId);
  const createSession = useApp((s) => s.createSession);
  const selectSession = useApp((s) => s.selectSession);
  const renameSession = useApp((s) => s.renameSession);
  const deleteSession = useApp((s) => s.deleteSession);

  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [confirming, setConfirming] = useState<string | null>(null);

  const filtered = sessions.filter((s) => !isDraftSession(s.id) && s.title.toLowerCase().includes(query.trim().toLowerCase()));

  const commitRename = async (id: string) => {
    const title = draft.trim();
    setEditing(null);
    if (title) await renameSession(id, title);
  };

  return (
    <aside className="flex w-[268px] shrink-0 flex-col border-r border-line bg-surface/60">
      <div className="flex items-center gap-2 px-3 pb-2 pt-3">
        <button
          onClick={() => void createSession()}
          className="flex h-9 flex-1 items-center justify-center gap-2 rounded-[9px] border border-line-strong bg-surface-2 text-[13px] font-medium text-fg transition-colors hover:border-accent/50 hover:bg-surface-3"
        >
          <Plus size={15} className="text-accent" />
          {t("新建会话")}
        </button>
      </div>

      <div className="px-3 pb-2">
        <div className="relative">
          <Search size={13} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-dim" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("搜索会话")}
            className="h-8 w-full rounded-[7px] border border-transparent bg-surface-2/70 pl-8 pr-2.5 text-[12.5px] text-fg placeholder:text-dim outline-none transition-colors focus:border-line-strong focus:bg-surface-2"
          />
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
        <div className="px-2 pb-1.5 pt-1 text-[10.5px] font-medium uppercase tracking-wider text-dim">
          {t("会话")}{filtered.length > 0 && `· ${filtered.length}`}
        </div>
        {filtered.map((session) => {
          const active = session.id === activeId;
          const busy = streaming && session.id === runSessionId;
          return (
            <div
              key={session.id}
              onClick={() => void selectSession(session.id)}
              className={cn(
                "group relative mb-0.5 flex cursor-pointer items-center gap-2 rounded-[8px] px-2.5 py-2 transition-colors",
                active ? "bg-surface-3 text-fg" : "text-muted hover:bg-surface-2 hover:text-fg",
              )}
            >
              {active && <span className="absolute left-0 top-1/2 h-4 w-[2px] -translate-y-1/2 rounded-full bg-accent" />}
              <div className="min-w-0 flex-1">
                {editing === session.id ? (
                  <div className="flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
                    <input
                      autoFocus
                      value={draft}
                      onChange={(e) => setDraft(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") void commitRename(session.id);
                        if (e.key === "Escape") setEditing(null);
                      }}
                      className="h-6 w-full rounded border border-accent/60 bg-surface px-1.5 text-[12.5px] outline-none"
                    />
                    <button className="text-ok" onClick={() => void commitRename(session.id)}>
                      <Check size={13} />
                    </button>
                    <button className="text-dim hover:text-fg" onClick={() => setEditing(null)}>
                      <X size={13} />
                    </button>
                  </div>
                ) : (
                  <>
                    <div className="truncate text-[13px] leading-5">
                      {busy && <span className="pulse-dot mr-1.5 inline-block h-1.5 w-1.5 rounded-full bg-accent align-middle" />}
                      {session.title}
                    </div>
                    <div className="truncate text-[10.5px] text-dim">{formatTime(session.updatedAt)}</div>
                  </>
                )}
              </div>
              {editing !== session.id && (
                <div className="hidden shrink-0 items-center gap-0.5 group-hover:flex">
                  <button
                    className="grid h-6 w-6 place-items-center rounded text-dim hover:bg-surface-3 hover:text-fg"
                    title={t("重命名")}
                    onClick={(e) => {
                      e.stopPropagation();
                      setEditing(session.id);
                      setDraft(session.title);
                    }}
                  >
                    <Pencil size={12} />
                  </button>
                  <button
                    className={cn(
                      "grid h-6 w-6 place-items-center rounded hover:bg-surface-3",
                      confirming === session.id ? "text-critical" : "text-dim hover:text-critical",
                    )}
                    title={confirming === session.id ? t("再次点击确认删除") : t("删除")}
                    onClick={(e) => {
                      e.stopPropagation();
                      if (confirming === session.id) {
                        void deleteSession(session.id);
                        setConfirming(null);
                      } else {
                        setConfirming(session.id);
                        setTimeout(() => setConfirming((v) => (v === session.id ? null : v)), 2500);
                      }
                    }}
                  >
                    <Trash2 size={12} />
                  </button>
                </div>
              )}
            </div>
          );
        })}
      </div>

      <div className="border-t border-line p-2.5">
        <button
          onClick={onOpenSettings}
          className="flex w-full items-center gap-2.5 rounded-[8px] px-2.5 py-2 text-[13px] text-muted transition-colors hover:bg-surface-2 hover:text-fg"
        >
          <Settings2 size={15} />
          {t("设置")}
          <span className="ml-auto text-[10.5px] text-dim">Ctrl+,</span>
        </button>
        <div className="mt-2 flex items-center gap-2 px-2.5 pb-1">
          <Logo size={16} className="opacity-80" />
          <span className="text-[10.5px] text-dim">{t("本机 Windows 超级管理员")}</span>
        </div>
      </div>
    </aside>
  );
}
