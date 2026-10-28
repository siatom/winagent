use std::path::PathBuf;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use win_tools::{Engine, Prepared, RiskLevel, tool_specs};

use crate::error::AgentError;
use crate::message::{ChatMessage, ToolCall, sanitize_history, trim_to_budget};
use crate::provider::{ChatClient, DeltaSink};

const CONTEXT_BUDGET_CHARS: usize = 120_000;
const UI_OUTPUT_LIMIT: usize = 24_000;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RunEvent {
    RunStarted {
        run_id: String,
    },
    TurnStarted {
        turn: u32,
    },
    TextDelta {
        delta: String,
    },
    ReasoningDelta {
        delta: String,
    },
    AssistantMessage {
        id: String,
        content: String,
        reasoning: String,
        tool_calls: Vec<ToolCall>,
        created_at: i64,
    },
    ToolPreparing {
        tool_call_id: String,
        name: String,
        summary: String,
        command: String,
        elevated: bool,
        level: RiskLevel,
        reasons: Vec<String>,
        requires_approval: bool,
    },
    ApprovalRequired {
        approval_id: String,
        tool_call_id: String,
        tool: String,
        summary: String,
        command: String,
        elevated: bool,
        level: RiskLevel,
        reasons: Vec<String>,
    },
    ToolFinished {
        id: String,
        tool_call_id: String,
        name: String,
        summary: String,
        command: String,
        ok: bool,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
        duration_ms: u64,
        timed_out: bool,
        level: RiskLevel,
        reasons: Vec<String>,
        decision: String,
        created_at: i64,
    },
    Finished {
        reason: String,
    },
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub id: String,
    pub tool_call_id: String,
    pub tool: String,
    pub summary: String,
    pub command: String,
    pub elevated: bool,
    pub level: RiskLevel,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ApprovalOutcome {
    pub approved: bool,
    pub remembered: bool,
    pub reason: Option<String>,
}

#[async_trait]
pub trait Approver: Send + Sync {
    async fn decide(&self, request: ApprovalRequest) -> ApprovalOutcome;
}

pub trait EventSink: Send + Sync {
    fn emit(&self, event: RunEvent);
}

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub model: String,
    pub max_turns: u32,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
    pub cwd: PathBuf,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            max_turns: 12,
            temperature: 0.3,
            max_tokens: None,
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("C:\\")),
        }
    }
}

struct SinkDelta<'a> {
    sink: &'a dyn EventSink,
}

impl DeltaSink for SinkDelta<'_> {
    fn text(&self, delta: &str) {
        self.sink.emit(RunEvent::TextDelta { delta: delta.to_string() });
    }

    fn reasoning(&self, delta: &str) {
        self.sink.emit(RunEvent::ReasoningDelta { delta: delta.to_string() });
    }
}

pub struct TurnOutcome {
    pub reason: String,
    pub messages: Vec<ChatMessage>,
}

/// Run the tool-calling loop until the model stops, the turn budget is exhausted,
/// or the user cancels.
#[allow(clippy::too_many_arguments)]
pub async fn run_turn(
    run_id: &str,
    client: &ChatClient,
    engine: &Engine,
    history: Vec<ChatMessage>,
    cfg: &RunConfig,
    approver: &dyn Approver,
    sink: &dyn EventSink,
    cancel: &CancellationToken,
) -> Result<TurnOutcome, AgentError> {
    let tools = tool_specs();
    let mut messages = sanitize_history(history);
    trim_to_budget(&mut messages, CONTEXT_BUDGET_CHARS);
    let mut cwd = cfg.cwd.clone();

    sink.emit(RunEvent::RunStarted { run_id: run_id.to_string() });

    for turn in 1..=cfg.max_turns {
        if cancel.is_cancelled() {
            sink.emit(RunEvent::Finished { reason: "cancelled".into() });
            return Ok(TurnOutcome { reason: "cancelled".into(), messages });
        }
        sink.emit(RunEvent::TurnStarted { turn });

        let delta_sink = SinkDelta { sink };
        let request = client.stream_chat(
            &cfg.model,
            &messages,
            &tools,
            cfg.temperature,
            cfg.max_tokens,
            &delta_sink,
        );

        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                sink.emit(RunEvent::Finished { reason: "cancelled".into() });
                return Ok(TurnOutcome { reason: "cancelled".into(), messages });
            }
            r = request => {
                match r {
                    Ok(result) => result,
                    Err(e) => {
                        sink.emit(RunEvent::Failed { message: e.to_string() });
                        return Err(e);
                    }
                }
            }
        };

        let assistant = ChatMessage::Assistant {
            content: if result.content.trim().is_empty() { None } else { Some(result.content.clone()) },
            reasoning: if result.reasoning.trim().is_empty() { None } else { Some(result.reasoning.clone()) },
            tool_calls: result.tool_calls.clone(),
        };
        messages.push(assistant);
        sink.emit(RunEvent::AssistantMessage {
            id: Uuid::new_v4().to_string(),
            content: result.content.clone(),
            reasoning: result.reasoning.clone(),
            tool_calls: result.tool_calls.clone(),
            created_at: now_ms(),
        });

        if result.tool_calls.is_empty() {
            sink.emit(RunEvent::Finished { reason: "stop".into() });
            return Ok(TurnOutcome { reason: "stop".into(), messages });
        }

        for call in &result.tool_calls {
            if cancel.is_cancelled() {
                sink.emit(RunEvent::Finished { reason: "cancelled".into() });
                return Ok(TurnOutcome { reason: "cancelled".into(), messages });
            }
            let mut prepared = match engine.prepare(&call.name, &parse_args(&call.arguments)) {
                Ok(p) => p,
                Err(message) => {
                    messages.push(ChatMessage::Tool {
                        tool_call_id: call.id.clone(),
                        name: call.name.clone(),
                        content: format!("工具参数错误: {message}"),
                    });
                    sink.emit(RunEvent::ToolFinished {
                        id: Uuid::new_v4().to_string(),
                        tool_call_id: call.id.clone(),
                        name: call.name.clone(),
                        summary: call.name.clone(),
                        command: String::new(),
                        ok: false,
                        exit_code: None,
                        stdout: String::new(),
                        stderr: format!("工具参数错误: {message}"),
                        duration_ms: 0,
                        timed_out: false,
                        level: RiskLevel::Info,
                        reasons: Vec::new(),
                        decision: "invalid".into(),
                        created_at: now_ms(),
                    });
                    continue;
                }
            };

            if let Some(dir) = prepared.cwd.clone() {
                cwd = dir;
            } else {
                prepared.cwd = Some(cwd.clone());
            }
            execute_and_record(call, &prepared, &mut messages, engine, approver, sink, cancel).await;

            if cancel.is_cancelled() {
                sink.emit(RunEvent::Finished { reason: "cancelled".into() });
                return Ok(TurnOutcome { reason: "cancelled".into(), messages });
            }
        }
    }

    sink.emit(RunEvent::Finished { reason: "max_turns".into() });
    Ok(TurnOutcome { reason: "max_turns".into(), messages })
}

#[allow(clippy::too_many_arguments)]
async fn execute_and_record(
    call: &ToolCall,
    prepared: &Prepared,
    messages: &mut Vec<ChatMessage>,
    engine: &Engine,
    approver: &dyn Approver,
    sink: &dyn EventSink,
    cancel: &CancellationToken,
) {
    let reasons: Vec<String> = prepared.risk.reasons.iter().map(|r| r.reason.clone()).collect();
    let level = prepared.risk.level;

    sink.emit(RunEvent::ToolPreparing {
        tool_call_id: call.id.clone(),
        name: prepared.tool.clone(),
        summary: prepared.summary.clone(),
        command: prepared.script.clone(),
        elevated: prepared.elevated,
        level,
        reasons: reasons.clone(),
        requires_approval: prepared.risk.requires_approval,
    });

    let mut decision = "auto".to_string();
    if prepared.risk.requires_approval {
        let outcome = approver
            .decide(ApprovalRequest {
                id: Uuid::new_v4().to_string(),
                tool_call_id: call.id.clone(),
                tool: prepared.tool.clone(),
                summary: prepared.summary.clone(),
                command: prepared.script.clone(),
                elevated: prepared.elevated,
                level,
                reasons: reasons.clone(),
            })
            .await;
        if !outcome.approved {
            let reason = outcome.reason.unwrap_or_else(|| "用户拒绝了这次操作".into());
            messages.push(ChatMessage::Tool {
                tool_call_id: call.id.clone(),
                name: prepared.tool.clone(),
                content: format!("未执行：{reason}"),
            });
            sink.emit(RunEvent::ToolFinished {
                id: Uuid::new_v4().to_string(),
                tool_call_id: call.id.clone(),
                name: prepared.tool.clone(),
                summary: prepared.summary.clone(),
                command: prepared.script.clone(),
                ok: false,
                exit_code: None,
                stdout: String::new(),
                stderr: reason,
                duration_ms: 0,
                timed_out: false,
                level,
                reasons,
                decision: "denied".into(),
                created_at: now_ms(),
            });
            return;
        }
        decision = if outcome.remembered { "approved-session".into() } else { "approved".into() };
    }

    let output = engine.execute(prepared, cancel).await;
    let model_text = output.model_text();
    messages.push(ChatMessage::Tool {
        tool_call_id: call.id.clone(),
        name: prepared.tool.clone(),
        content: model_text,
    });

    sink.emit(RunEvent::ToolFinished {
        id: Uuid::new_v4().to_string(),
        tool_call_id: call.id.clone(),
        name: prepared.tool.clone(),
        summary: prepared.summary.clone(),
        command: prepared.script.clone(),
        ok: output.ok,
        exit_code: output.exit_code,
        stdout: truncate(&output.stdout, UI_OUTPUT_LIMIT),
        stderr: truncate(&output.stderr, UI_OUTPUT_LIMIT / 2),
        duration_ms: output.duration_ms,
        timed_out: output.timed_out,
        level,
        reasons,
        decision,
        created_at: now_ms(),
    });
}

fn parse_args(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Value::Object(Default::default());
    }
    serde_json::from_str(trimmed).unwrap_or(Value::Object(Default::default()))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn truncate(s: &str, limit: usize) -> String {
    if s.chars().count() <= limit {
        return s.to_string();
    }
    let head: String = s.chars().take(limit).collect();
    format!("{head}\n…[已截断]")
}
