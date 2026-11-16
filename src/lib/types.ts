export type RiskLevel = "info" | "caution" | "danger" | "critical";

export interface ToolCall {
  id: string;
  name: string;
  arguments: string;
}

export interface Session {
  id: string;
  title: string;
  providerId: string | null;
  model: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface Provider {
  id: string;
  name: string;
  baseUrl: string;
  models: string[];
  defaultModel: string;
  isDefault: boolean;
  hasKey: boolean;
  needsKey: boolean;
}

export interface AppInfo {
  version: string;
  elevated: boolean;
  shell: string;
  dataDir: string;
}

export interface UiSettings {
  language: "zh-CN" | "en";
  theme: "dark" | "light" | "system";
  maxTurns: number;
  temperature: number;
}

export interface Bootstrap {
  app: AppInfo;
  providers: Provider[];
  sessions: Session[];
  settings: UiSettings;
}

export interface ToolMeta {
  toolCallId: string;
  name: string;
  summary: string;
  command: string;
  ok: boolean;
  exitCode: number | null;
  stdout: string;
  stderr: string;
  durationMs: number;
  timedOut: boolean;
  level: RiskLevel;
  reasons: string[];
  decision: string;
}

export interface Message {
  id: string;
  sessionId: string;
  role: "user" | "assistant" | "tool";
  content: string;
  reasoning: string | null;
  toolCalls: ToolCall[] | null;
  toolCallId: string | null;
  name: string | null;
  meta: ToolMeta | null;
  createdAt: number;
}

export interface PendingTool {
  toolCallId: string;
  name: string;
  summary: string;
  command: string;
  elevated: boolean;
  level: RiskLevel;
  reasons: string[];
  requiresApproval: boolean;
}

export interface ApprovalRequest {
  approvalId: string;
  toolCallId: string;
  tool: string;
  summary: string;
  command: string;
  elevated: boolean;
  level: RiskLevel;
  reasons: string[];
}

export type RunEvent =
  | { type: "runStarted"; runId: string }
  | { type: "turnStarted"; turn: number }
  | { type: "textDelta"; delta: string }
  | { type: "reasoningDelta"; delta: string }
  | {
      type: "assistantMessage";
      id: string;
      content: string;
      reasoning: string;
      toolCalls: ToolCall[];
      createdAt: number;
    }
  | {
      type: "toolPreparing";
      toolCallId: string;
      name: string;
      summary: string;
      command: string;
      elevated: boolean;
      level: RiskLevel;
      reasons: string[];
      requiresApproval: boolean;
    }
  | {
      type: "approvalRequired";
      approvalId: string;
      toolCallId: string;
      tool: string;
      summary: string;
      command: string;
      elevated: boolean;
      level: RiskLevel;
      reasons: string[];
    }
  | {
      type: "toolFinished";
      id: string;
      toolCallId: string;
      name: string;
      summary: string;
      command: string;
      ok: boolean;
      exitCode: number | null;
      stdout: string;
      stderr: string;
      durationMs: number;
      timedOut: boolean;
      level: RiskLevel;
      reasons: string[];
      decision: string;
      createdAt: number;
    }
  | { type: "finished"; reason: string }
  | { type: "failed"; message: string };

export interface ProviderInput {
  id?: string;
  name: string;
  baseUrl: string;
  defaultModel: string;
  models: string[];
  apiKey?: string;
  isDefault: boolean;
}

export const TOOL_LABELS: Record<string, string> = {
  run_powershell: "PowerShell",
  process: "进程",
  env_var: "环境变量",
  registry: "注册表",
  system_info: "系统信息",
};

export const RISK_LABELS: Record<RiskLevel, string> = {
  info: "只读",
  caution: "注意",
  danger: "危险",
  critical: "高危",
};

export const DECISION_LABELS: Record<string, string> = {
  auto: "自动执行",
  approved: "已批准",
  "approved-session": "会话内自动批准",
  denied: "已拒绝",
  invalid: "参数错误",
};
