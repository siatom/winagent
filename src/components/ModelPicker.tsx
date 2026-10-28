import { useMemo, useState } from "react";
import { Popover } from "radix-ui";
import { Check, ChevronDown, Cpu, Settings2 } from "lucide-react";
import { useApp } from "../store/app";
import { cn } from "../lib/utils";

export function ModelPicker({ onOpenSettings }: { onOpenSettings?: () => void }) {
  const providers = useApp((s) => s.providers);
  const sessions = useApp((s) => s.sessions);
  const activeId = useApp((s) => s.activeSessionId);
  const setModel = useApp((s) => s.setModel);
  const streaming = useApp((s) => s.streaming);

  const session = sessions.find((s) => s.id === activeId) ?? null;
  const currentProvider =
    providers.find((p) => p.id === session?.providerId) ??
    providers.find((p) => p.isDefault) ??
    providers[0];
  const currentModel = session?.model ?? currentProvider?.defaultModel ?? "";

  const [open, setOpen] = useState(false);
  const [browseId, setBrowseId] = useState<string | null>(null);
  const [custom, setCustom] = useState("");

  const browse = providers.find((p) => p.id === (browseId ?? currentProvider?.id)) ?? currentProvider;
  const models = useMemo(() => {
    if (!browse) return [];
    return browse.models.includes(browse.defaultModel)
      ? browse.models
      : [browse.defaultModel, ...browse.models];
  }, [browse]);

  const choose = (providerId: string, model: string) => {
    if (!model.trim()) return;
    void setModel(providerId, model.trim());
    setOpen(false);
    setCustom("");
  };

  return (
    <Popover.Root
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (next) setBrowseId(currentProvider?.id ?? null);
      }}
    >
      <Popover.Trigger asChild>
        <button
          disabled={streaming}
          className="flex h-7 max-w-[300px] items-center gap-1.5 rounded-[7px] border border-transparent px-2 text-[11.5px] text-muted transition-colors hover:border-line hover:bg-surface-3 hover:text-fg disabled:opacity-50"
          title="切换模型"
        >
          <Cpu size={12} className="text-accent" />
          <span className="truncate">
            <span className="text-dim">{currentProvider?.name ?? "未配置"}</span>
            <span className="mx-1 text-line-strong">/</span>
            {currentModel || "选择模型"}
          </span>
          <ChevronDown size={11} className="shrink-0" />
        </button>
      </Popover.Trigger>

      <Popover.Portal>
        <Popover.Content
          side="top"
          align="start"
          sideOffset={8}
          className="z-50 w-[430px] overflow-hidden rounded-[12px] border border-line-strong bg-surface shadow-[var(--shadow-panel)] animate-fade-up"
        >
          {providers.length === 0 ? (
            <div className="p-4 text-[12.5px] text-muted">还没有配置供应商</div>
          ) : (
            <div className="flex">
              <div className="w-[136px] shrink-0 border-r border-line bg-surface-2/50 p-1.5">
                <div className="px-2 pb-1 pt-1 text-[10px] uppercase tracking-wider text-dim">供应商</div>
                {providers.map((p) => (
                  <button
                    key={p.id}
                    onClick={() => setBrowseId(p.id)}
                    className={cn(
                      "flex w-full items-center gap-1.5 rounded-[6px] px-2 py-1.5 text-left text-[12.5px]",
                      browse?.id === p.id ? "bg-surface-3 text-fg" : "text-muted hover:bg-surface-2 hover:text-fg",
                    )}
                  >
                    <span className="truncate">{p.name}</span>
                    {p.isDefault && <span className="ml-auto text-[9.5px] text-accent">默认</span>}
                  </button>
                ))}
              </div>
              <div className="min-w-0 flex-1 p-1.5">
                <div className="px-2 pb-1 pt-1 text-[10px] uppercase tracking-wider text-dim">模型</div>
                <div className="max-h-[220px] overflow-y-auto">
                  {models.map((model) => {
                    const active = browse?.id === currentProvider?.id && model === currentModel;
                    return (
                      <button
                        key={model}
                        onClick={() => browse && choose(browse.id, model)}
                        className={cn(
                          "flex w-full items-center gap-2 rounded-[6px] px-2 py-1.5 text-left text-[12.5px]",
                          active ? "bg-accent/12 text-fg" : "text-muted hover:bg-surface-2 hover:text-fg",
                        )}
                      >
                        <span className="truncate font-mono text-[11.5px]">{model}</span>
                        {active && <Check size={12} className="ml-auto shrink-0 text-accent" />}
                      </button>
                    );
                  })}
                </div>
                <div className="mt-1 border-t border-line/70 px-1.5 pb-0.5 pt-1.5">
                  <input
                    value={custom}
                    onChange={(e) => setCustom(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" && browse) choose(browse.id, custom);
                    }}
                    placeholder="输入自定义模型名后回车"
                    className="h-7 w-full rounded-[6px] border border-transparent bg-surface-2 px-2 font-mono text-[11.5px] outline-none placeholder:font-sans placeholder:text-dim focus:border-line-strong"
                  />
                </div>
              </div>
            </div>
          )}
          {onOpenSettings && (
            <button
              onClick={() => {
                setOpen(false);
                onOpenSettings();
              }}
              className="flex w-full items-center gap-2 border-t border-line px-3 py-2 text-[12px] text-muted hover:bg-surface-2 hover:text-fg"
            >
              <Settings2 size={13} />
              管理供应商与模型
            </button>
          )}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
