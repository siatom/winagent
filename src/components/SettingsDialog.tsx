import { useEffect, useState } from "react";
import { Dialog } from "radix-ui";
import {
  Check,
  Cpu,
  Info,
  Loader2,
  Plus,
  RefreshCw,
  ShieldCheck,
  SlidersHorizontal,
  Star,
  Trash2,
  X,
} from "lucide-react";
import { useApp } from "../store/app";
import type { Provider } from "../lib/types";
import { cn } from "../lib/utils";
import { Button } from "./ui/Button";

type Tab = "models" | "general" | "about";

const inputClass =
  "h-9 w-full rounded-[8px] border border-line bg-surface-2 px-2.5 text-[13px] text-fg outline-none transition-colors placeholder:text-dim focus:border-accent/60";
const labelClass = "mb-1.5 block text-[11.5px] font-medium text-muted";

export function SettingsDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const [tab, setTab] = useState<Tab>("models");

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-black/55 backdrop-blur-[3px]" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-50 flex h-[min(680px,calc(100vh-64px))] w-[min(900px,calc(100vw-64px))] -translate-x-1/2 -translate-y-1/2 overflow-hidden rounded-[16px] border border-line-strong bg-surface shadow-[var(--shadow-panel)] outline-none animate-fade-up">
          <div className="flex w-[172px] shrink-0 flex-col border-r border-line bg-surface-2/40 p-2.5">
            <div className="px-2 pb-3 pt-1">
              <div className="text-[13px] font-semibold text-fg">设置</div>
              <div className="text-[10.5px] text-dim">Windows God</div>
            </div>
            {(
              [
                { id: "models", label: "模型与供应商", icon: Cpu },
                { id: "general", label: "通用", icon: SlidersHorizontal },
                { id: "about", label: "关于", icon: Info },
              ] as const
            ).map(({ id, label, icon: Icon }) => (
              <button
                key={id}
                onClick={() => setTab(id)}
                className={cn(
                  "mb-0.5 flex items-center gap-2 rounded-[7px] px-2.5 py-2 text-left text-[12.5px] transition-colors",
                  tab === id ? "bg-surface-3 text-fg" : "text-muted hover:bg-surface-2 hover:text-fg",
                )}
              >
                <Icon size={14} />
                {label}
              </button>
            ))}
            <div className="mt-auto px-2 pb-1 text-[10px] text-dim">快捷键 Ctrl+,</div>
          </div>

          <div className="flex min-w-0 flex-1 flex-col">
            <div className="flex h-12 shrink-0 items-center justify-between border-b border-line px-5">
              <Dialog.Title className="text-[13.5px] font-medium text-fg">
                {tab === "models" ? "模型与供应商" : tab === "general" ? "通用设置" : "关于"}
              </Dialog.Title>
              <Dialog.Close asChild>
                <button className="grid h-7 w-7 place-items-center rounded-md text-dim hover:bg-surface-3 hover:text-fg">
                  <X size={14} />
                </button>
              </Dialog.Close>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
              {tab === "models" && <ModelsTab />}
              {tab === "general" && <GeneralTab />}
              {tab === "about" && <AboutTab />}
            </div>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function ModelsTab() {
  const providers = useApp((s) => s.providers);
  const [editing, setEditing] = useState<string | null>(providers[0]?.id ?? "new");
  const current = providers.find((p) => p.id === editing) ?? null;

  useEffect(() => {
    if (editing !== "new" && !providers.some((p) => p.id === editing)) {
      setEditing(providers[0]?.id ?? "new");
    }
  }, [providers, editing]);

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap gap-1.5">
        {providers.map((p) => (
          <button
            key={p.id}
            onClick={() => setEditing(p.id)}
            className={cn(
              "flex items-center gap-1.5 rounded-[8px] border px-2.5 py-1.5 text-[12px] transition-colors",
              editing === p.id
                ? "border-accent/50 bg-accent/10 text-fg"
                : "border-line bg-surface-2 text-muted hover:text-fg",
            )}
          >
            {p.name}
            {p.isDefault && <Star size={10} className="fill-current text-accent" />}
            {!p.hasKey && p.needsKey && <span className="h-1.5 w-1.5 rounded-full bg-warn" title="未配置 Key" />}
          </button>
        ))}
        <button
          onClick={() => setEditing("new")}
          className={cn(
            "flex items-center gap-1 rounded-[8px] border border-dashed px-2.5 py-1.5 text-[12px] transition-colors",
            editing === "new" ? "border-accent/50 text-accent" : "border-line-strong text-muted hover:text-fg",
          )}
        >
          <Plus size={12} />
          添加供应商
        </button>
      </div>

      <ProviderForm key={editing ?? "new"} provider={current} isNew={editing === "new"} onSaved={(id) => setEditing(id)} />
    </div>
  );
}

function ProviderForm({
  provider,
  isNew,
  onSaved,
}: {
  provider: Provider | null;
  isNew: boolean;
  onSaved: (id: string) => void;
}) {
  const saveProvider = useApp((s) => s.saveProvider);
  const deleteProvider = useApp((s) => s.deleteProvider);
  const setDefaultProvider = useApp((s) => s.setDefaultProvider);
  const fetchModels = useApp((s) => s.fetchModels);
  const [name, setName] = useState(provider?.name ?? "");
  const [baseUrl, setBaseUrl] = useState(provider?.baseUrl ?? "https://api.deepseek.com");
  const [defaultModel, setDefaultModel] = useState(provider?.defaultModel ?? "deepseek-flash");
  const [modelsText, setModelsText] = useState((provider?.models ?? ["deepseek-flash", "deepseek-chat"]).join("\n"));
  const [apiKey, setApiKey] = useState("");
  const [isDefault, setIsDefault] = useState(provider?.isDefault ?? false);
  const [busy, setBusy] = useState<null | "save" | "test" | "delete">(null);
  const [notice, setNotice] = useState<{ kind: "ok" | "error"; text: string } | null>(null);

  const models = modelsText
    .split(/[\n,]/)
    .map((m) => m.trim())
    .filter(Boolean);

  const save = async (): Promise<string | null> => {
    if (!name.trim() || !baseUrl.trim() || !defaultModel.trim()) {
      setNotice({ kind: "error", text: "名称、Base URL、默认模型不能为空" });
      return null;
    }
    setBusy("save");
    setNotice(null);
    try {
      const saved = await saveProvider({
        id: provider?.id,
        name: name.trim(),
        baseUrl: baseUrl.trim(),
        defaultModel: defaultModel.trim(),
        models: models.includes(defaultModel.trim()) ? models : [defaultModel.trim(), ...models],
        apiKey: apiKey.trim() || undefined,
        isDefault,
      });
      setNotice({ kind: "ok", text: "已保存" });
      setApiKey("");
      onSaved(saved.id);
      return saved.id;
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
      return null;
    } finally {
      setBusy(null);
    }
  };

  const test = async () => {
    setBusy("test");
    setNotice(null);
    try {
      let id = provider?.id;
      if (!id) {
        id = (await save()) ?? undefined;
        if (!id) return;
      }
      const list = await fetchModels(id, apiKey.trim() || undefined, baseUrl.trim() || undefined);
      if (list.length === 0) {
        setNotice({ kind: "error", text: "连接成功，但没有返回模型列表" });
      } else {
        setModelsText(list.join("\n"));
        setNotice({ kind: "ok", text: `连接成功，获取到 ${list.length} 个模型` });
      }
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const remove = async () => {
    if (!provider) return;
    if (!window.confirm(`删除供应商「${provider.name}」？已保存的 API Key 也会一并删除。`)) return;
    setBusy("delete");
    try {
      await deleteProvider(provider.id);
      onSaved("new");
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="rounded-[10px] border border-line bg-surface-2/40 p-4">
      <div className="grid grid-cols-2 gap-3">
        <div>
          <label className={labelClass}>名称</label>
          <input className={inputClass} value={name} onChange={(e) => setName(e.target.value)} placeholder="DeepSeek" />
        </div>
        <div>
          <label className={labelClass}>Base URL</label>
          <input
            className={inputClass}
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            placeholder="https://api.deepseek.com"
          />
        </div>
      </div>

      <div className="mt-3">
        <label className={labelClass}>
          API Key
          {provider?.hasKey && <span className="ml-1.5 text-ok">已保存</span>}
          {provider && !provider.needsKey && <span className="ml-1.5 text-dim">本地端点可留空</span>}
        </label>
        <input
          type="password"
          className={inputClass}
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder={provider?.hasKey ? "留空则不修改已保存的 Key" : "sk-..."}
          autoComplete="off"
        />
        <div className="mt-1 text-[10.5px] text-dim">Key 保存在 Windows 凭据管理器，不会写入数据库或日志。</div>
      </div>

      <div className="mt-3 grid grid-cols-[1fr_180px] gap-3">
        <div>
          <label className={labelClass}>可用模型（每行一个）</label>
          <textarea
            className="h-[96px] w-full resize-none rounded-[8px] border border-line bg-surface-2 px-2.5 py-2 font-mono text-[12px] text-fg outline-none focus:border-accent/60"
            value={modelsText}
            onChange={(e) => setModelsText(e.target.value)}
            spellCheck={false}
          />
        </div>
        <div>
          <label className={labelClass}>默认模型</label>
          <input
            className={cn(inputClass, "font-mono")}
            value={defaultModel}
            onChange={(e) => setDefaultModel(e.target.value)}
          />
          <label className="mt-3 flex cursor-pointer items-center gap-2 text-[12px] text-muted">
            <input
              type="checkbox"
              checked={isDefault}
              onChange={(e) => setIsDefault(e.target.checked)}
              className="h-3.5 w-3.5 accent-[var(--accent)]"
            />
            设为默认供应商
          </label>
        </div>
      </div>

      {notice && (
        <div className={cn("mt-3 rounded-[7px] px-2.5 py-1.5 text-[11.5px]", notice.kind === "ok" ? "bg-ok/10 text-ok" : "bg-danger/10 text-danger")}>
          {notice.text}
        </div>
      )}

      <div className="mt-4 flex items-center gap-2">
        <Button variant="primary" size="sm" onClick={() => void save()} disabled={busy !== null}>
          {busy === "save" ? <Loader2 size={12} className="spin" /> : <Check size={12} />}
          保存
        </Button>
        <Button variant="outline" size="sm" onClick={() => void test()} disabled={busy !== null}>
          {busy === "test" ? <Loader2 size={12} className="spin" /> : <RefreshCw size={12} />}
          测试连接并拉取模型
        </Button>
        {provider && (
          <>
            {!provider.isDefault && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => void setDefaultProvider(provider.id)}
                className="ml-auto"
              >
                <Star size={12} />
                设为默认
              </Button>
            )}
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void remove()}
              disabled={busy !== null}
              className={cn("text-dim hover:text-critical", provider.isDefault ? "ml-auto" : "")}
            >
              <Trash2 size={12} />
              删除
            </Button>
          </>
        )}
      </div>

      {isNew && (
        <div className="mt-3 text-[10.5px] text-dim">
          支持任意 OpenAI 兼容接口，Base URL 填到域名或 /v1 即可（例如 https://api.openai.com、http://localhost:11434/v1）。
        </div>
      )}
    </div>
  );
}

function GeneralTab() {
  const settings = useApp((s) => s.settings);
  const update = useApp((s) => s.updateSettings);

  return (
    <div className="space-y-6">
      <div>
        <div className={labelClass}>外观</div>
        <div className="flex gap-1.5">
          {(
            [
              { id: "dark", label: "深色" },
              { id: "light", label: "浅色" },
              { id: "system", label: "跟随系统" },
            ] as const
          ).map(({ id, label }) => (
            <button
              key={id}
              onClick={() => void update({ theme: id })}
              className={cn(
                "rounded-[7px] border px-3 py-1.5 text-[12px] transition-colors",
                settings.theme === id
                  ? "border-accent/50 bg-accent/10 text-fg"
                  : "border-line bg-surface-2 text-muted hover:text-fg",
              )}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      <div>
        <div className="flex items-baseline justify-between">
          <div className={labelClass}>单轮最多工具调用轮数</div>
          <span className="text-[12px] font-mono text-accent">{settings.maxTurns}</span>
        </div>
        <input
          type="range"
          min={1}
          max={30}
          value={settings.maxTurns}
          onChange={(e) => void update({ maxTurns: Number(e.target.value) })}
          className="w-full accent-[var(--accent)]"
        />
        <div className="mt-1 text-[10.5px] text-dim">达到上限后会停下，等待你继续发消息。</div>
      </div>

      <div>
        <div className="flex items-baseline justify-between">
          <div className={labelClass}>Temperature</div>
          <span className="text-[12px] font-mono text-accent">{settings.temperature.toFixed(1)}</span>
        </div>
        <input
          type="range"
          min={0}
          max={1.5}
          step={0.1}
          value={settings.temperature}
          onChange={(e) => void update({ temperature: Number(e.target.value) })}
          className="w-full accent-[var(--accent)]"
        />
        <div className="mt-1 text-[10.5px] text-dim">越低越稳定；系统管理场景建议 0.2 - 0.4。</div>
      </div>

      <div className="rounded-[10px] border border-line bg-surface-2/40 p-3">
        <div className="mb-1 flex items-center gap-1.5 text-[12px] font-medium text-fg">
          <ShieldCheck size={13} className="text-ok" />
          安全模型
        </div>
        <p className="text-[11.5px] leading-relaxed text-muted">
          当前为「提醒模式」：只读和低风险命令直接执行；危险与高危操作会暂停并等待你确认。没有硬性黑名单，
          确认后的命令会以你的权限真实执行。
        </p>
      </div>
    </div>
  );
}

function AboutTab() {
  const app = useApp((s) => s.app);
  return (
    <div className="space-y-4">
      <div className="rounded-[10px] border border-line bg-surface-2/40 p-4">
        <div className="text-[13px] font-medium text-fg">Windows God</div>
        <div className="mt-1 text-[11.5px] text-muted">
          本机 Windows 超级管理员智能体。Rust + Tauri 内核，命令与系统接口全部真实执行。
        </div>
      </div>
      <dl className="grid grid-cols-[110px_1fr] gap-y-2 text-[12px]">
        <dt className="text-dim">版本</dt>
        <dd className="font-mono text-fg">v{app?.version ?? "0.1.0"}</dd>
        <dt className="text-dim">Shell</dt>
        <dd className="font-mono text-fg">{app?.shell ?? "-"}</dd>
        <dt className="text-dim">当前权限</dt>
        <dd className={app?.elevated ? "text-ok" : "text-muted"}>{app?.elevated ? "管理员" : "标准权限（按需提权）"}</dd>
        <dt className="text-dim">数据目录</dt>
        <dd className="break-all font-mono text-[11px] text-muted">{app?.dataDir ?? "-"}</dd>
      </dl>
      <p className="text-[11px] leading-relaxed text-dim">
        所有执行记录保存在本地 SQLite 的 tool_runs 表中；API Key 保存在 Windows 凭据管理器。
      </p>
    </div>
  );
}
