import i18n, { normalizeLanguage } from "../i18n";
import { create } from "zustand";
import type { Channel } from "@tauri-apps/api/core";
import { api, runChannel } from "../lib/ipc";
import type {
  AppInfo,
  ApprovalRequest,
  Message,
  PendingTool,
  Provider,
  ProviderInput,
  RunEvent,
  Session,
  UiSettings,
} from "../lib/types";

export function applyTheme(theme: UiSettings["theme"]) {
  const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const dark = theme === "dark" || (theme === "system" && prefersDark);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

interface SessionState {
  messages: Message[];
  streaming: boolean;
  currentRunId: string | null;
  streamText: string;
  streamReasoning: string;
  pendingTools: Record<string, PendingTool>;
  error: string | null;
}

const emptySession = (): SessionState => ({ messages: [], streaming: false, currentRunId: null, streamText: "", streamReasoning: "", pendingTools: {}, error: null });

interface AppStore {
  sessionStates: Record<string, SessionState>;
  ready: boolean;
  initializing: boolean;
  app: AppInfo | null;
  providers: Provider[];
  sessions: Session[];
  settings: UiSettings;
  activeSessionId: string | null;
  messages: Message[];
  streaming: boolean;
  currentRunId: string | null;
  streamText: string;
  streamReasoning: string;
  pendingTools: Record<string, PendingTool>;
  approvals: ApprovalRequest[];
  error: string | null;

  init: () => Promise<void>;
  refreshProviders: () => Promise<void>;
  selectSession: (id: string) => Promise<void>;
  createSession: () => Promise<void>;
  renameSession: (id: string, title: string) => Promise<void>;
  deleteSession: (id: string) => Promise<void>;
  setModel: (providerId: string, model: string) => Promise<void>;
  send: (text: string) => Promise<void>;
  stop: () => Promise<void>;
  resolveApproval: (approved: boolean, remember: boolean) => Promise<void>;
  updateSettings: (patch: Partial<UiSettings>) => Promise<void>;
  saveProvider: (input: ProviderInput) => Promise<Provider>;
  deleteProvider: (id: string) => Promise<void>;
  setDefaultProvider: (id: string) => Promise<void>;
  fetchModels: (providerId: string, apiKey?: string, baseUrl?: string) => Promise<string[]>;
  handleEvent: (runSession: string, event: RunEvent) => void;
  dismissError: () => void;
}

const channels = new Map<string, Channel<RunEvent>>();
let selectionVersion = 0;

export const isDraftSession = (id: string) => id.startsWith("draft-");

function draftSession(providers: Provider[]): Session {
  const provider = providers.find((p) => p.isDefault) ?? providers[0];
  return {
    id: `draft-${crypto.randomUUID()}`,
    title: "新会话",
    providerId: provider?.id ?? null,
    model: provider?.defaultModel ?? null,
    createdAt: Date.now(),
    updatedAt: Date.now(),
  };
}

const emptySettings: UiSettings = { language: "zh-CN", theme: "dark", maxConcurrentChats: 3, maxTurns: 12, temperature: 0.3, backupDir: "" };

export const useApp = create<AppStore>((set, get) => ({
  sessionStates: {},
  ready: false,
  initializing: false,
  app: null,
  providers: [],
  sessions: [],
  settings: emptySettings,
  activeSessionId: null,
  messages: [],
  streaming: false,
  currentRunId: null,
  streamText: "",
  streamReasoning: "",
  pendingTools: {},
  approvals: [],
  error: null,

  init: async () => {
    if (get().initializing || get().ready) return;
    set({ initializing: true, error: null });
    try {
      const boot = await api.bootstrap();
      const settings = { ...emptySettings, ...boot.settings, language: normalizeLanguage(boot.settings?.language) };
      await i18n.changeLanguage(settings.language);
      const active = draftSession(boot.providers);
      const sessions = [active, ...boot.sessions];
      const messages: Message[] = [];
      set({
        app: boot.app,
        providers: boot.providers,
        sessions,
        settings,
        activeSessionId: active.id,
        messages,
        ready: true,
        initializing: false,
      });
      applyTheme(boot.settings?.theme ?? "dark");
    } catch (e) {
      set({ error: String(e), initializing: false });
    }
  },

  refreshProviders: async () => {
    const boot = await api.bootstrap();
    set((s) => ({
      providers: boot.providers,
      app: boot.app,
      sessions: [...s.sessions.filter((item) => isDraftSession(item.id)), ...boot.sessions],
    }));
  },

  selectSession: async (id) => {
    const version = ++selectionVersion;
    if (get().activeSessionId === id) return;
    try {
      const cached = get().sessionStates[id];
      const messages = cached?.messages ?? (isDraftSession(id) ? [] : await api.getMessages(id));
      if (version !== selectionVersion || !get().sessions.some((s) => s.id === id)) return;
      const view = get().sessionStates[id] ?? { ...emptySession(), messages };
      set((s) => ({ sessionStates: { ...s.sessionStates, [id]: view }, activeSessionId: id, ...view }));
    } catch (e) { set({ error: String(e) }); }
  },

  createSession: async () => {
    ++selectionVersion;
    const existing = get().sessions.find((s) => isDraftSession(s.id) && !get().sessionStates[s.id]?.streaming);
    const session = existing ?? draftSession(get().providers);
    const view = get().sessionStates[session.id] ?? emptySession();
    set((s) => ({
      sessions: existing ? s.sessions : [session, ...s.sessions],
      sessionStates: { ...s.sessionStates, [session.id]: view },
      activeSessionId: session.id,
      ...view,
    }));
  },

  renameSession: async (id, title) => {
    if (!isDraftSession(id)) await api.renameSession(id, title);
    set((s) => ({
      sessions: s.sessions.map((item) => (item.id === id ? { ...item, title } : item)),
    }));
  },

  deleteSession: async (id) => {
    try {
      if (get().sessionStates[id]?.streaming) throw new Error(i18n.t("请先停止会话再删除"));
      if (!isDraftSession(id)) await api.deleteSession(id);
      ++selectionVersion;
      set((s) => {
        let sessions = s.sessions.filter((session) => session.id !== id);
        if (sessions.length === 0) sessions = [draftSession(s.providers)];
        const sessionStates = { ...s.sessionStates };
        delete sessionStates[id];
        if (s.activeSessionId !== id) return { sessions, sessionStates };
        // Return to a fresh draft rather than loading another chat after deletion.
        const draft = sessions.find((session) => isDraftSession(session.id)) ?? draftSession(s.providers);
        if (!sessions.some((session) => session.id === draft.id)) sessions = [draft, ...sessions];
        return { sessions, sessionStates, activeSessionId: draft.id, ...emptySession() };
      });
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  setModel: async (providerId, model) => {
    const sessionId = get().activeSessionId;
    if (!sessionId || get().sessionStates[sessionId]?.streaming) return;
    if (!isDraftSession(sessionId)) await api.setSessionModel(sessionId, providerId, model);
    set((s) => ({
      sessions: s.sessions.map((item) =>
        item.id === sessionId ? { ...item, providerId, model } : item,
      ),
    }));
  },

  send: async (text) => {
    const { activeSessionId, settings, sessionStates } = get();
    if (!activeSessionId || sessionStates[activeSessionId]?.streaming || !text.trim()) return;
    if (Object.values(sessionStates).filter((s) => s.streaming).length >= settings.maxConcurrentChats) {
      set({ error: i18n.t("已达到同时运行对话上限，请等待完成或在设置中调整。") });
      return;
    }
    let runSession = activeSessionId;
    const update = (patch: Partial<SessionState>) => set((s) => {
      const view = { ...(s.sessionStates[runSession] ?? emptySession()), ...patch };
      return { sessionStates: { ...s.sessionStates, [runSession]: view }, ...(s.activeSessionId === runSession ? view : {}) };
    });
    update({ streaming: true, error: null });
    if (isDraftSession(runSession)) {
      try {
        const draft = get().sessions.find((item) => item.id === runSession);
        const saved = await api.createSession();
        if (draft?.providerId && draft.model) {
          await api.setSessionModel(saved.id, draft.providerId, draft.model);
          saved.providerId = draft.providerId;
          saved.model = draft.model;
        }
        set((s) => ({
          sessions: [saved, ...s.sessions.filter((item) => item.id !== activeSessionId)],
          activeSessionId: s.activeSessionId === activeSessionId ? saved.id : s.activeSessionId,
          sessionStates: Object.fromEntries(Object.entries(s.sessionStates).map(([id, view]) => [id === activeSessionId ? saved.id : id, view])),
        }));
        runSession = saved.id;
      } catch (e) {
        update({ streaming: false, error: String(e) });
        return;
      }
    }
    update({
      streaming: true, streamText: "", streamReasoning: "", pendingTools: {}, error: null,
      messages: [...(get().sessionStates[runSession]?.messages ?? get().messages), {
        id: `local-${crypto.randomUUID()}`, sessionId: runSession, role: "user", content: text,
        reasoning: null, toolCalls: null, toolCallId: null, name: null, meta: null, createdAt: Date.now(),
      }],
    });
    const channel = runChannel((event) => get().handleEvent(runSession, event));
    channels.set(runSession, channel);
    try {
      const runId = await api.sendMessage(runSession, text, channel);
      if (channels.get(runSession) === channel && get().sessionStates[runSession]?.streaming) update({ currentRunId: runId });
    } catch (e) {
      channels.delete(runSession);
      update({ streaming: false, error: String(e), currentRunId: null });
    }
  },

  stop: async () => {
    const sid = get().activeSessionId;
    const runId = sid ? get().sessionStates[sid]?.currentRunId : null;
    if (!runId) return;
    try { await api.cancelRun(runId); }
    catch (e) { set({ error: String(e) }); }
  },

  resolveApproval: async (approved, remember) => {
    const current = get().approvals[0];
    if (!current) return;
    set((s) => ({ approvals: s.approvals.slice(1) }));
    try {
      await api.resolveApproval(current.approvalId, approved, remember);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  updateSettings: async (patch) => {
    const next = { ...get().settings, ...patch };
    try {
      await api.saveSettings(next);
      set({ settings: next });
      if (patch.theme) applyTheme(next.theme);
      if (patch.language) await i18n.changeLanguage(next.language);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  saveProvider: async (input) => {
    const provider = await api.saveProvider(input);
    await get().refreshProviders();
    return provider;
  },

  deleteProvider: async (id) => {
    await api.deleteProvider(id);
    await get().refreshProviders();
  },

  setDefaultProvider: async (id) => {
    await api.setDefaultProvider(id);
    await get().refreshProviders();
  },

  fetchModels: async (providerId, apiKey, baseUrl) => api.fetchModels(providerId, apiKey, baseUrl),

  handleEvent: (runSession, event) => {
    const update = (patch: Partial<AppStore> | ((s: AppStore) => Partial<AppStore>)) => set((s) => {
      const view = s.sessionStates[runSession] ?? emptySession();
      const next = typeof patch === "function" ? patch({ ...s, ...view }) : patch;
      const { approvals, sessions, ...local } = next;
      const updated = { ...view, ...local };
      return {
        sessionStates: { ...s.sessionStates, [runSession]: updated },
        ...(s.activeSessionId === runSession ? local : {}),
        ...(sessions ? { sessions } : {}),
        ...(approvals ? { approvals } : {}),
      };
    });

    switch (event.type) {
      case "runStarted":
        update({ currentRunId: event.runId });
        break;
      case "sessionTitleUpdated":
        update((s) => ({ sessions: s.sessions.map((session) => session.id === event.sessionId ? { ...session, title: event.title } : session) }));
        break;
      case "textDelta":
        update((s) => ({ streamText: s.streamText + event.delta }));
        break;
      case "reasoningDelta":
        update((s) => ({ streamReasoning: s.streamReasoning + event.delta }));
        break;
      case "assistantMessage":
        {
          update((s) => ({
            streamText: "",
            streamReasoning: "",
            messages: [
              ...s.messages,
              {
                id: event.id,
                sessionId: runSession,
                role: "assistant",
                content: event.content,
                reasoning: event.reasoning || null,
                toolCalls: event.toolCalls,
                toolCallId: null,
                name: null,
                meta: null,
                createdAt: event.createdAt,
              },
            ],
          }));
        }
        break;
      case "toolPreparing":
        {
          update((s) => ({
            pendingTools: {
              ...s.pendingTools,
              [event.toolCallId]: {
                toolCallId: event.toolCallId,
                name: event.name,
                summary: event.summary,
                command: event.command,
                elevated: event.elevated,
                level: event.level,
                reasons: event.reasons,
                requiresApproval: event.requiresApproval,
              },
            },
          }));
        }
        break;
      case "toolFinished":
        {
          update((s) => {
            const pending = { ...s.pendingTools };
            delete pending[event.toolCallId];
            const meta = {
              toolCallId: event.toolCallId,
              name: event.name,
              summary: event.summary,
              command: event.command,
              ok: event.ok,
              exitCode: event.exitCode,
              stdout: event.stdout,
              stderr: event.stderr,
              durationMs: event.durationMs,
              timedOut: event.timedOut,
              level: event.level,
              reasons: event.reasons,
              decision: event.decision,
            };
            return {
              pendingTools: pending,
              messages: [
                ...s.messages,
                {
                  id: event.id,
                  sessionId: runSession,
                  role: "tool",
                  content: `${event.stdout}\n${event.stderr}`,
                  reasoning: null,
                  toolCalls: null,
                  toolCallId: event.toolCallId,
                  name: event.name,
                  meta,
                  createdAt: event.createdAt,
                },
              ],
            };
          });
        }
        break;
      case "approvalRequired":
        update((s) => ({
          approvals: [
            ...s.approvals,
            {
              sessionId: runSession,
              approvalId: event.approvalId,
              toolCallId: event.toolCallId,
              tool: event.tool,
              summary: event.summary,
              command: event.command,
              elevated: event.elevated,
              level: event.level,
              reasons: event.reasons,
            },
          ],
        }));
        break;
      case "failed":
        channels.delete(runSession);
        update({
          streaming: false,
          currentRunId: null,
          error: event.message,
          pendingTools: {},
          streamText: "",
          streamReasoning: "",
          approvals: get().approvals.filter((a) => a.sessionId !== runSession),
        });
        break;
      case "finished": {
        channels.delete(runSession);
        update({
          streaming: false,
          currentRunId: null,
          streamText: "",
          streamReasoning: "",
          pendingTools: {},
          approvals: get().approvals.filter((a) => a.sessionId !== runSession),
        });
        const sid = runSession;
        const completed = get().sessionStates[sid];
        if (sid) {
          void api.getMessages(sid).then((messages) => {
            if (get().sessionStates[sid] === completed && get().sessions.some((s) => s.id === sid)) update({ messages });
          }).catch((e) => update({ error: String(e) }));
        }
        break;
      }
      default:
        break;
    }
  },

  dismissError: () => set({ error: null }),
}));
