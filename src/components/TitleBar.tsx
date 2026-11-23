import { useTranslation } from "react-i18next";
import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Settings2, Shield, ShieldCheck, Square, X } from "lucide-react";
import { useApp } from "../store/app";
import { cn } from "../lib/utils";
import { Logo } from "./Logo";

export function TitleBar({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { t } = useTranslation();
  const app = useApp((s) => s.app);
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();
    void win.isMaximized().then(setMaximized).catch(() => {});
  }, []);

  const controls = [
    {
      icon: Minus,
      label: t("最小化"),
      onClick: () => void getCurrentWindow().minimize(),
    },
    {
      icon: Square,
      label: maximized ? t("还原") : t("最大化"),
      onClick: () => {
        void getCurrentWindow().toggleMaximize();
        setMaximized((v) => !v);
      },
    },
    {
      icon: X,
      label: t("关闭"),
      onClick: () => void getCurrentWindow().close(),
      danger: true,
    },
  ];

  return (
    <header
      data-tauri-drag-region
      className="flex h-11 shrink-0 items-center gap-3 border-b border-line bg-surface/80 px-3 backdrop-blur"
    >
      <div className="flex items-center gap-2.5 pl-1" data-tauri-drag-region>
        <Logo size={22} />
        <span data-tauri-drag-region className="text-[12.5px] font-semibold tracking-[0.14em] text-fg">win-agent</span>
        <span data-tauri-drag-region className="rounded-full border border-line bg-surface-2 px-1.5 py-0.5 text-[10.5px] text-dim">
          v{app?.version ?? "0.1.0"}
        </span>
      </div>

      <div className="flex-1" data-tauri-drag-region />

      <div
        className={cn(
          "no-drag flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11.5px]",
          app?.elevated
            ? "border-ok/30 bg-ok/10 text-ok"
            : "border-line bg-surface-2 text-muted",
        )}
        title={app?.elevated ? t("当前以管理员权限运行") : t("当前为标准权限，需要时会对单条命令请求 UAC 提权")}
      >
        {app?.elevated ? <ShieldCheck size={13} /> : <Shield size={13} />}
        {app?.elevated ? t("管理员") : t("标准权限")}
      </div>

      <div className="no-drag flex items-center gap-0.5">
        <button
          onClick={onOpenSettings}
          className="grid h-8 w-8 place-items-center rounded-md text-muted transition-colors hover:bg-surface-3 hover:text-fg"
          title={t("设置")}
        >
          <Settings2 size={15} />
        </button>
        <span className="mx-1 h-4 w-px bg-line" />
        {controls.map(({ icon: Icon, label, onClick, danger }) => (
          <button
            key={label}
            onClick={onClick}
            title={label}
            className={cn(
              "grid h-8 w-9 place-items-center rounded-md text-muted transition-colors",
              danger ? "hover:bg-critical hover:text-white" : "hover:bg-surface-3 hover:text-fg",
            )}
          >
            <Icon size={14} />
          </button>
        ))}
      </div>
    </header>
  );
}
