import { useTranslation } from "react-i18next";
import { useEffect, useState } from "react";
import { AlertCircle, X } from "lucide-react";
import { useApp, applyTheme } from "./store/app";
import { TitleBar } from "./components/TitleBar";
import { Sidebar } from "./components/Sidebar";
import { ChatView } from "./components/ChatView";
import { Composer } from "./components/Composer";
import { SettingsDialog } from "./components/SettingsDialog";
import { ApprovalDialog } from "./components/ApprovalDialog";
import { Logo } from "./components/Logo";
import { Button } from "./components/ui/Button";

function Splash({ error, onRetry }: { error: string | null; onRetry: () => void }) {
  const { t } = useTranslation();
  return (
    <div className="app-bg flex h-screen flex-col items-center justify-center gap-4">
      <Logo size={52} className="rounded-[14px]" />
      <div className="text-[13px] text-muted">{error ? t("启动失败") : t("正在启动 win-agent…")}</div>
      {error && (
        <div className="max-w-[520px] rounded-[10px] border border-danger/30 bg-danger/10 px-3 py-2 text-center text-[12px] text-danger">
          {error}
        </div>
      )}
      {error && (
        <Button variant="outline" size="sm" onClick={onRetry}>
          {t("重试")}
        </Button>
      )}
    </div>
  );
}

export default function App() {
  const ready = useApp((s) => s.ready);
  const error = useApp((s) => s.error);
  const theme = useApp((s) => s.settings.theme);
  const init = useApp((s) => s.init);
  const dismissError = useApp((s) => s.dismissError);
  const [settingsOpen, setSettingsOpen] = useState(false);

  useEffect(() => {
    void init();
  }, [init]);

  useEffect(() => {
    applyTheme(theme);
    if (theme !== "system") return;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => applyTheme("system");
    media.addEventListener("change", handler);
    return () => media.removeEventListener("change", handler);
  }, [theme]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.key === ",") {
        event.preventDefault();
        setSettingsOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    if (!error || !ready) return;
    const timer = window.setTimeout(() => dismissError(), 9000);
    return () => window.clearTimeout(timer);
  }, [error, ready, dismissError]);

  if (!ready) return <Splash error={error} onRetry={() => void init()} />;

  const openSettings = () => setSettingsOpen(true);

  return (
    <div className="app-bg flex h-screen flex-col overflow-hidden">
      <TitleBar onOpenSettings={openSettings} />
      <div className="flex min-h-0 flex-1">
        <Sidebar onOpenSettings={openSettings} />
        <main className="flex min-w-0 flex-1 flex-col">
          <ChatView onOpenSettings={openSettings} />
          <Composer onOpenSettings={openSettings} />
        </main>
      </div>

      <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
      <ApprovalDialog />

      {error && (
        <div className="fixed bottom-4 right-4 z-[60] flex max-w-[420px] items-start gap-2 rounded-[10px] border border-danger/30 bg-surface px-3 py-2.5 shadow-[var(--shadow-panel)] animate-fade-up">
          <AlertCircle size={14} className="mt-0.5 shrink-0 text-danger" />
          <div className="min-w-0 flex-1 break-words text-[12px] leading-relaxed text-fg">{error}</div>
          <button onClick={dismissError} className="shrink-0 text-dim hover:text-fg">
            <X size={13} />
          </button>
        </div>
      )}
    </div>
  );
}
