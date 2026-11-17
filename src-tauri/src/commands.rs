use std::path::PathBuf;
use std::sync::Arc;

use agent_core::{
    ChatClient, ChatMessage, EventSink, RunConfig, RunEvent, ToolCall, run_turn, system_prompt,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::State;
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::approval::TauriApprover;
use crate::db::{Db, MessageDto, Provider, Session};
use crate::secrets;
use crate::state::{AppState, RunHandle};

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ---------------------------------------------------------------- DTOs

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub elevated: bool,
    pub shell: String,
    pub data_dir: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDto {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub models: Vec<String>,
    pub default_model: String,
    pub is_default: bool,
    pub has_key: bool,
    pub needs_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiSettings {
    pub language: String,
    pub theme: String,
    pub max_concurrent_chats: u32,
    pub max_turns: u32,
    pub temperature: f32,
    pub backup_dir: String,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self { language: "zh-CN".into(), theme: "dark".into(), max_concurrent_chats: 3, max_turns: 12, temperature: 0.3, backup_dir: default_cwd().join("Documents").join("WinAgent").display().to_string() }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub app: AppInfo,
    pub providers: Vec<ProviderDto>,
    pub sessions: Vec<Session>,
    pub settings: UiSettings,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
    pub id: Option<String>,
    pub name: String,
    pub base_url: String,
    pub default_model: String,
    #[serde(default)]
    pub models: Vec<String>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub is_default: bool,
}

// ---------------------------------------------------------------- bootstrap

#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> Result<Bootstrap, String> {
    let providers = state.db.list_providers().map_err(err)?;
    Ok(Bootstrap {
        app: AppInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            elevated: state.elevated,
            shell: state.shell.pwsh_label(),
            data_dir: state.dirs.data.display().to_string(),
        },
        providers: providers.iter().map(to_provider_dto).collect(),
        sessions: state.db.list_sessions().map_err(err)?,
        settings: load_settings(&state.db),
    })
}

fn to_provider_dto(p: &Provider) -> ProviderDto {
    ProviderDto {
        id: p.id.clone(),
        name: p.name.clone(),
        base_url: p.base_url.clone(),
        models: p.models.clone(),
        default_model: p.default_model.clone(),
        is_default: p.is_default,
        has_key: secrets::get_key(&p.id).is_some(),
        needs_key: !is_local_endpoint(&p.base_url),
    }
}

/// Local OpenAI-compatible servers (Ollama, LM Studio, vLLM) usually run without auth.
pub fn is_local_endpoint(base_url: &str) -> bool {
    let lower = base_url.to_lowercase();
    lower.starts_with("http://127.0.0.1")
        || lower.starts_with("http://localhost")
        || lower.starts_with("http://[::1]")
        || lower.starts_with("http://0.0.0.0")
}

fn load_settings(db: &Db) -> UiSettings {
    db.get_setting("ui")
        .and_then(|raw| serde_json::from_str::<UiSettings>(&raw).ok())
        .unwrap_or_default()
}

// ---------------------------------------------------------------- sessions

#[tauri::command]
pub fn create_session(state: State<'_, AppState>) -> Result<Session, String> {
    let default = state.db.default_provider().map_err(err)?;
    let (pid, model) = match default {
        Some(p) => (Some(p.id), Some(p.default_model)),
        None => (None, None),
    };
    state.db.create_session("新会话", pid.as_deref(), model.as_deref()).map_err(err)
}

#[tauri::command]
pub fn rename_session(state: State<'_, AppState>, session_id: String, title: String) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("标题不能为空".into());
    }
    state.db.rename_session(&session_id, title).map_err(err)
}

#[tauri::command]
pub fn delete_session(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    state.broker.clear_session(&session_id);
    state.db.delete_session(&session_id).map_err(err)
}

#[tauri::command]
pub fn set_session_model(
    state: State<'_, AppState>,
    session_id: String,
    provider_id: Option<String>,
    model: Option<String>,
) -> Result<(), String> {
    state
        .db
        .set_session_model(&session_id, provider_id.as_deref(), model.as_deref())
        .map_err(err)
}

#[tauri::command]
pub fn get_messages(state: State<'_, AppState>, session_id: String) -> Result<Vec<MessageDto>, String> {
    state.db.list_messages(&session_id).map_err(err)
}

// ---------------------------------------------------------------- providers

#[tauri::command]
pub fn save_provider(state: State<'_, AppState>, input: ProviderInput) -> Result<ProviderDto, String> {
    if input.name.trim().is_empty() || input.base_url.trim().is_empty() || input.default_model.trim().is_empty() {
        return Err("名称、Base URL、默认模型都不能为空".into());
    }
    let id = input.id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
    let is_first = state.db.list_providers().map_err(err)?.is_empty();
    let provider = Provider {
        id: id.clone(),
        name: input.name.trim().to_string(),
        base_url: input.base_url.trim().trim_end_matches('/').to_string(),
        models: input.models.clone(),
        default_model: input.default_model.trim().to_string(),
        is_default: input.is_default || is_first,
    };
    state.db.upsert_provider(&provider).map_err(err)?;
    if let Some(key) = input.api_key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        secrets::set_key(&id, key)?;
    }
    if provider.is_default {
        state.db.set_default_provider(&id).map_err(err)?;
    }
    let saved = state.db.get_provider(&id).map_err(err)?.ok_or("保存后读取失败")?;
    Ok(to_provider_dto(&saved))
}

#[tauri::command]
pub fn delete_provider(state: State<'_, AppState>, provider_id: String) -> Result<(), String> {
    let was_default = state
        .db
        .list_providers()
        .map_err(err)?
        .iter()
        .find(|p| p.id == provider_id)
        .map(|p| p.is_default)
        .unwrap_or(false);
    state.db.delete_provider(&provider_id).map_err(err)?;
    secrets::delete_key(&provider_id);
    if was_default {
        if let Some(next) = state.db.list_providers().map_err(err)?.into_iter().next() {
            state.db.set_default_provider(&next.id).map_err(err)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn set_default_provider(state: State<'_, AppState>, provider_id: String) -> Result<(), String> {
    state.db.set_default_provider(&provider_id).map_err(err)
}

fn resolve_key(provider_id: &str, provided: Option<String>, base_url: &str) -> Result<String, String> {
    if let Some(key) = provided.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()) {
        return Ok(key);
    }
    if let Some(key) = secrets::get_key(provider_id) {
        return Ok(key);
    }
    if is_local_endpoint(base_url) {
        return Ok(String::new());
    }
    Err("尚未配置 API Key".to_string())
}

#[tauri::command]
pub async fn fetch_models(
    state: State<'_, AppState>,
    provider_id: String,
    api_key: Option<String>,
    base_url: Option<String>,
) -> Result<Vec<String>, String> {
    let provider = state.db.get_provider(&provider_id).map_err(err)?.ok_or("供应商不存在")?;
    let base = base_url.unwrap_or(provider.base_url);
    let key = resolve_key(&provider_id, api_key, &base)?;
    let client = ChatClient::new(base, key).map_err(err)?;
    client.list_models().await.map_err(err)
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, mut settings: UiSettings) -> Result<(), String> {
    if !(1..=10).contains(&settings.max_concurrent_chats) {
        return Err("同时运行对话上限必须为 1 到 10".into());
    }
    settings.backup_dir = settings.backup_dir.trim().to_string();
    let path = PathBuf::from(&settings.backup_dir);
    if !path.is_absolute() {
        return Err("备份目录必须为绝对路径".into());
    }
    std::fs::create_dir_all(&path).map_err(|e| format!("无法创建备份目录: {e}"))?;
    let probe = path.join(format!(".winagent-write-test-{}", Uuid::new_v4()));
    let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&probe)
        .map_err(|e| format!("备份目录不可写: {e}"))?;
    drop(file);
    std::fs::remove_file(probe).map_err(err)?;
    let raw = serde_json::to_string(&settings).map_err(err)?;
    state.db.set_setting("ui", &raw).map_err(err)
}

// ---------------------------------------------------------------- chat

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    session_id: String,
    text: String,
    channel: Channel<RunEvent>,
) -> Result<String, String> {
    let st = state.inner().clone();
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("消息不能为空".into());
    }
    if let Some(active) = st.active_run_for(&session_id) {
        return Err(format!("该会话仍在执行中（{active}），请先停止"));
    }

    let session = st.db.get_session(&session_id).map_err(err)?.ok_or("会话不存在")?;
    let providers = st.db.list_providers().map_err(err)?;
    let provider = session
        .provider_id
        .as_ref()
        .and_then(|id| providers.iter().find(|p| &p.id == id).cloned())
        .or_else(|| providers.iter().find(|p| p.is_default).cloned())
        .or_else(|| providers.first().cloned())
        .ok_or("还没有配置任何模型供应商")?;
    let model = session.model.clone().filter(|m| !m.trim().is_empty()).unwrap_or_else(|| provider.default_model.clone());
    let api_key = match secrets::get_key(&provider.id) {
        Some(key) => key,
        None if is_local_endpoint(&provider.base_url) => String::new(),
        None => {
            return Err(format!(
                "供应商「{}」还没有配置 API Key，请先到设置里填写",
                provider.name
            ));
        }
    };

    let settings = load_settings(&st.db);
    let mut active_runs = st.runs.lock().unwrap();
    if active_runs.values().any(|run| run.session_id == session_id) {
        return Err("该会话仍在执行中，请先停止".into());
    }
    if active_runs.len() >= settings.max_concurrent_chats.clamp(1, 10) as usize {
        return Err("已达到同时运行对话上限，请等待完成或在设置中调整。".into());
    }
    let count = st.db.message_count(&session_id).map_err(err)?;
    if count == 0 && (session.title.trim().is_empty() || session.title == "新会话") {
        let title: String = text.chars().take(24).collect();
        if st.db.set_auto_title(&session_id, &title, false).map_err(err)? {
            let _ = channel.send(RunEvent::SessionTitleUpdated { session_id: session_id.clone(), title });
        }
    }

    let user_id = Uuid::new_v4().to_string();
    st.db
        .insert_message(&user_id, &session_id, "user", &text, None, None, None, None, None)
        .map_err(err)?;

    let mut history = Vec::with_capacity(8);
    let cwd = default_cwd();
    let prompt = format!("{}\nBackup directory (JSON-encoded path): {}. Use this directory for backups unless the user explicitly specifies another location. Create it before writing when needed. Changing this setting does not move existing backups.", system_prompt(&cwd), serde_json::to_string(&settings.backup_dir).map_err(err)?);
    history.push(ChatMessage::System { content: prompt });
    history.extend(build_history(&st.db, &session_id)?);

    let run_id = Uuid::new_v4().to_string();
    let cancel = CancellationToken::new();
    active_runs.insert(
        run_id.clone(),
        RunHandle { session_id: session_id.clone(), cancel: cancel.clone() },
    );

    drop(active_runs);

    let sink = DbSink { db: st.db.clone(), channel: channel.clone(), session_id: session_id.clone() };
    let approver = TauriApprover {
        channel: channel.clone(),
        broker: st.broker.clone(),
        session_id: session_id.clone(),
        cancel: cancel.clone(),
    };

    let engine = st.engine.clone();
    let db = st.db.clone();
    let runs = st.runs.clone();
    let run_id_task = run_id.clone();

    tauri::async_runtime::spawn(async move {
        let cfg = RunConfig {
            model,
            max_turns: settings.max_turns.clamp(1, 40),
            temperature: settings.temperature.clamp(0.0, 2.0),
            max_tokens: None,
            cwd,
        };
        let result = match ChatClient::new(&provider.base_url, &api_key) {
            Ok(client) => {
                run_turn(&run_id_task, &client, &engine, history, &cfg, &approver, &sink, &cancel).await
            }
            Err(e) => {
                sink.emit(RunEvent::Failed { message: e.to_string() });
                Err(e)
            }
        };
        db.touch_session(&session_id).ok();
        runs.lock().unwrap().remove(&run_id_task);
        match result {
            Ok(outcome) if outcome.reason == "stop" && db.needs_auto_title(&session_id).unwrap_or(false) => {
                if let Ok(client) = ChatClient::new(&provider.base_url, &api_key) {
                    if let Some(title) = crate::titles::generate(&client, &cfg.model, &outcome.messages).await {
                        // The conditional update protects manual renames and deleted sessions during the request.
                        if db.set_auto_title(&session_id, &title, true).unwrap_or(false) {
                            let _ = channel.send(RunEvent::SessionTitleUpdated { session_id, title });
                        }
                    }
                }
            }
            Err(e) => eprintln!("run {run_id_task} failed: {e}"),
            _ => {}
        }
    });

    Ok(run_id)
}

#[tauri::command]
pub fn cancel_run(state: State<'_, AppState>, run_id: String) -> Result<(), String> {
    if let Some(handle) = state.runs.lock().unwrap().get(&run_id) {
        handle.cancel.cancel();
        Ok(())
    } else {
        Err("运行已结束".into())
    }
}

#[tauri::command]
pub fn resolve_approval(
    state: State<'_, AppState>,
    approval_id: String,
    approved: bool,
    remember: bool,
) -> Result<(), String> {
    state.broker.resolve(&approval_id, approved, remember)
}

fn default_cwd() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("C:\\"))
}

fn build_history(db: &Db, session_id: &str) -> Result<Vec<ChatMessage>, String> {
    let mut out = Vec::new();
    for message in db.list_messages(session_id).map_err(err)? {
        match message.role.as_str() {
            "user" => out.push(ChatMessage::User { content: message.content }),
            "assistant" => {
                let calls: Vec<ToolCall> = message
                    .tool_calls
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_default();
                out.push(ChatMessage::Assistant {
                    content: if message.content.trim().is_empty() { None } else { Some(message.content) },
                    reasoning: message.reasoning,
                    tool_calls: calls,
                });
            }
            "tool" => out.push(ChatMessage::Tool {
                tool_call_id: message.tool_call_id.unwrap_or_default(),
                name: message.name.unwrap_or_default(),
                content: message.content,
            }),
            _ => {}
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- persistence sink

struct DbSink {
    db: Arc<Db>,
    channel: Channel<RunEvent>,
    session_id: String,
}

impl EventSink for DbSink {
    fn emit(&self, event: RunEvent) {
        match &event {
            RunEvent::AssistantMessage { id, content, reasoning, tool_calls, .. } => {
                let calls = if tool_calls.is_empty() {
                    None
                } else {
                    Some(serde_json::to_string(tool_calls).unwrap_or_else(|_| "[]".into()))
                };
                let reasoning = reasoning.trim();
                if let Err(e) = self.db.insert_message(
                    id,
                    &self.session_id,
                    "assistant",
                    content,
                    if reasoning.is_empty() { None } else { Some(reasoning) },
                    calls.as_deref(),
                    None,
                    None,
                    None,
                ) {
                    eprintln!("persist assistant message failed: {e}");
                }
            }
            RunEvent::ToolFinished {
                id,
                tool_call_id,
                name,
                summary,
                command,
                ok,
                exit_code,
                stdout,
                stderr,
                duration_ms,
                timed_out,
                level,
                reasons,
                decision,
                ..
            } => {
                let meta: Value = json!({
                    "toolCallId": tool_call_id,
                    "name": name,
                    "summary": summary,
                    "command": command,
                    "ok": ok,
                    "exitCode": exit_code,
                    "stdout": stdout,
                    "stderr": stderr,
                    "durationMs": duration_ms,
                    "timedOut": timed_out,
                    "level": level,
                    "reasons": reasons,
                    "decision": decision,
                });
                let content = format!(
                    "exit_code: {}\n{}{}",
                    exit_code.map(|c| c.to_string()).unwrap_or_else(|| "null".into()),
                    stdout,
                    stderr
                );
                if let Err(e) = self.db.insert_message(
                    id,
                    &self.session_id,
                    "tool",
                    &content,
                    None,
                    None,
                    Some(tool_call_id),
                    Some(name),
                    Some(&meta.to_string()),
                ) {
                    eprintln!("persist tool message failed: {e}");
                }
                if let Err(e) = self.db.record_tool_run(
                    &self.session_id,
                    name,
                    command,
                    level.as_str(),
                    decision,
                    *exit_code,
                    *ok,
                    *duration_ms,
                    &format!("{stdout}\n{stderr}"),
                ) {
                    eprintln!("persist audit record failed: {e}");
                }
            }
            _ => {}
        }

        let _ = self.channel.send(event);
    }
}

#[cfg(test)]
mod settings_tests {
    use super::UiSettings;
    use std::path::PathBuf;

    #[test]
    fn custom_backup_directory_survives_serialization() {
        let settings = UiSettings { backup_dir: r"D:\备份 files\WinAgent".into(), ..UiSettings::default() };
        let saved = serde_json::to_string(&settings).unwrap();
        let restored: UiSettings = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.backup_dir, settings.backup_dir);
    }

    #[test]
    fn legacy_settings_keep_existing_values_and_default_to_chinese() {
        let settings: UiSettings = serde_json::from_str(
            r#"{"theme":"light","maxTurns":23,"temperature":0.7}"#,
        ).unwrap();
        assert_eq!(settings.language, "zh-CN");
        assert_eq!(settings.theme, "light");
        assert_eq!(settings.max_turns, 23);
        assert_eq!(settings.max_concurrent_chats, 3);
        assert_eq!(settings.temperature, 0.7);
        assert_eq!(PathBuf::from(settings.backup_dir), super::default_cwd().join("Documents").join("WinAgent"));
    }

    #[test]
    fn concurrent_limit_survives_settings_serialization() {
        let settings = UiSettings { max_concurrent_chats: 7, ..UiSettings::default() };
        let saved = serde_json::to_string(&settings).unwrap();
        assert!(saved.contains("\"maxConcurrentChats\":7"));
        let restored: UiSettings = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.max_concurrent_chats, 7);
    }

    #[test]
    fn language_survives_settings_serialization() {
        for language in ["zh-CN", "en"] {
            let settings = UiSettings { language: language.into(), ..UiSettings::default() };
            let saved = serde_json::to_string(&settings).unwrap();
            let restored: UiSettings = serde_json::from_str(&saved).unwrap();
            assert_eq!(restored.language, language);
        }
    }
}
