import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  ApprovalRequest,
  Bootstrap,
  Message,
  Provider,
  ProviderInput,
  RunEvent,
  Session,
  UiSettings,
} from "./types";

export const api = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),

  createSession: () => invoke<Session>("create_session"),
  renameSession: (sessionId: string, title: string) => invoke<void>("rename_session", { sessionId, title }),
  deleteSession: (sessionId: string) => invoke<void>("delete_session", { sessionId }),
  setSessionModel: (sessionId: string, providerId: string | null, model: string | null) =>
    invoke<void>("set_session_model", { sessionId, providerId, model }),
  getMessages: (sessionId: string) => invoke<Message[]>("get_messages", { sessionId }),

  saveProvider: (input: ProviderInput) => invoke<Provider>("save_provider", { input }),
  deleteProvider: (providerId: string) => invoke<void>("delete_provider", { providerId }),
  setDefaultProvider: (providerId: string) => invoke<void>("set_default_provider", { providerId }),
  fetchModels: (providerId: string, apiKey?: string, baseUrl?: string) =>
    invoke<string[]>("fetch_models", { providerId, apiKey, baseUrl }),

  saveSettings: (settings: UiSettings) => invoke<void>("save_settings", { settings }),

  sendMessage: (sessionId: string, text: string, channel: Channel<RunEvent>) =>
    invoke<string>("send_message", { sessionId, text, channel }),
  sendFollowUp: (sessionId: string, text: string) =>
    invoke<void>("send_follow_up", { sessionId, text }),
  cancelRun: (runId: string) => invoke<void>("cancel_run", { runId }),
  resolveApproval: (approvalId: string, approved: boolean, remember: boolean) =>
    invoke<void>("resolve_approval", { approvalId, approved, remember }),
};

export function runChannel(onEvent: (event: RunEvent) => void): Channel<RunEvent> {
  const channel = new Channel<RunEvent>();
  channel.onmessage = onEvent;
  return channel;
}

export type { ApprovalRequest };
