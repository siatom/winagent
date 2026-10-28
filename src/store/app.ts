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

interface AppStore {
  ready: boolean;
  initializing: boolean;
  app: AppInfo | null;
  providers: Provider[];
  sessions: Session[];
  settings: UiSettings;
  activeSessionId: string | null;
  messages: Message[];
  streaming: boolean;
  runSessionId: string | null;
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

let channelRef: Channel<RunEvent> | null = null;

const emptySettings: UiSettings = { theme: "dark", maxTurns: 12, temperature: 0.3 };

export const useApp = create<AppStore>((set, get) => ({
  ready: false,
  initializing: false,
  app: null,
  providers: [],
  sessions: [],
  settings: emptySettings,
  activeSessionId: null,
  messages: [],
  streaming: false,
  runSessionId: null,
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
      let sessions = boot.sessions;
      if (sessions.length === 0) {
        const created = await api.createSession();
        sessions = [created];
      }
      const active = sessions[0];
      const messages = await api.getMessages(active.id);
      set({
        app: boot.app,
        providers: boot.providers,
        sessions,
        settings: boot.settings ?? emptySettings,
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
    set({ providers: boot.providers, sessions: boot.sessions, app: boot.app });
  },

  selectSession: async (id) => {
    if (get().activeSessionId === id) return;
    const messages = await api.getMessages(id);
    set({
      activeSessionId: id,
      messages,
      streamText: "",
      streamReasoning: "",
      pendingTools: {},
      error: null,
    });
  },

  createSession: async () => {
    const session = await api.createSession();
    const messages = await api.getMessages(session.id);
    set((s) => ({
      sessions: [session, ...s.sessions],
      activeSessionId: session.id,
      messages,
      streamText: "",
      streamReasoning: "",
      pendingTools: {},
    }));
  },

  renameSession: async (id, title) => {
    await api.renameSession(id, title);
    set((s) => ({
      sessions: s.sessions.map((item) => (item.id === id ? { ...item, title } : item)),
    }));
  },

  deleteSession: async (id) => {
    await api.deleteSession(id);
    let sessions = get().sessions.filter((s) => s.id !== id);
    if (sessions.length === 0) {
      const created = await api.createSession();
      sessions = [created];
    }
    const activeSessionId = get().activeSessionId === id ? sessions[0].id : get().activeSessionId;
    const messages = activeSessionId ? await api.getMessages(activeSessionId) : [];
    set({ sessions, activeSessionId, messages, streamText: "", streamReasoning: "", pendingTools: {} });
  },

  setModel: async (providerId, model) => {
    const sessionId = get().activeSessionId;
    if (!sessionId) return;
    await api.setSessionModel(sessionId, providerId, model);
    set((s) => ({
      sessions: s.sessions.map((item) =>
        item.id === sessionId ? { ...item, providerId, model } : item,
      ),
    }));
  },

  send: async (text) => {
    const { activeSessionId, streaming } = get();
    if (!activeSessionId || streaming || !text.trim()) return;
    const runSession = activeSessionId;
    set((s) => ({
      streaming: true,
      runSessionId: runSession,
      streamText: "",
      streamReasoning: "",
      pendingTools: {},
      error: null,
      messages: [
        ...s.messages,
        {
          id: `local-${Date.now()}`,
          sessionId: runSession,
          role: "user",
          content: text,
          reasoning: null,
          toolCalls: null,
          toolCallId: null,
          name: null,
          meta: null,
          createdAt: Date.now(),
        },
      ],
    }));

    channelRef = runChannel((event) => get().handleEvent(runSession, event));
    try {
      const runId = await api.sendMessage(runSession, text, channelRef);
      set({ currentRunId: runId });
    } catch (e) {
      channelRef = null;
      set({ streaming: false, error: String(e), currentRunId: null });
    }
  },

  stop: async () => {
    const runId = get().currentRunId;
    if (!runId) return;
    try {
      await api.cancelRun(runId);
    } catch (e) {
      set({ error: String(e) });
    }
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
    set({ settings: next });
    if (patch.theme) applyTheme(next.theme);
    try {
      await api.saveSettings(next);
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
    const isActive = get().activeSessionId === runSession;

    switch (event.type) {
      case "textDelta":
        if (isActive) set((s) => ({ streamText: s.streamText + event.delta }));
        break;
      case "reasoningDelta":
        if (isActive) set((s) => ({ streamReasoning: s.streamReasoning + event.delta }));
        break;
      case "assistantMessage":
        if (isActive) {
          set((s) => ({
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
        if (isActive) {
          set((s) => ({
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
        if (isActive) {
          set((s) => {
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
        set((s) => ({
          approvals: [
            ...s.approvals,
            {
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
        set({
          streaming: false,
          currentRunId: null,
          error: event.message,
          streamText: "",
          streamReasoning: "",
          approvals: [],
        });
        break;
      case "finished": {
        channelRef = null;
        set({
          streaming: false,
          currentRunId: null,
          runSessionId: null,
          streamText: "",
          streamReasoning: "",
          pendingTools: {},
          approvals: [],
        });
        const sid = get().activeSessionId;
        if (sid) {
          void api.getMessages(sid).then((messages) => {
            if (get().activeSessionId === sid) set({ messages });
          });
        }
        break;
      }
      default:
        break;
    }
  },

  dismissError: () => set({ error: null }),
}));
