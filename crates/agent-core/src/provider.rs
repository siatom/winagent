use std::collections::BTreeMap;
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{Value, json};
use win_tools::ToolSpec;

use crate::error::AgentError;
use crate::message::{ChatMessage, ToolCall};

/// Receives incremental output while a completion is streaming.
pub trait DeltaSink: Send + Sync {
    fn text(&self, delta: &str);
    fn reasoning(&self, delta: &str);
}

#[derive(Debug, Default, Clone)]
pub struct ChatResult {
    pub content: String,
    pub reasoning: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: Option<String>,
}

pub struct ChatClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl ChatClient {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Result<Self, AgentError> {
        let base_url = normalize_base(&base_url.into());
        if base_url.is_empty() {
            return Err(AgentError::Config("Base URL 不能为空".into()));
        }
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .user_agent("windows-god/0.1")
            .build()?;
        Ok(Self { http, base_url, api_key: api_key.into() })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn endpoint(&self, path: &str) -> String {
        if self.base_url.ends_with("/v1") {
            format!("{}/{}", self.base_url, path)
        } else {
            format!("{}/v1/{}", self.base_url, path)
        }
    }

    pub async fn list_models(&self) -> Result<Vec<String>, AgentError> {
        let resp = self
            .http
            .get(self.endpoint("models"))
            .bearer_auth(&self.api_key)
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AgentError::Api { status: status.as_u16(), message: extract_error(&text) });
        }
        let v: Value = serde_json::from_str(&text).map_err(|e| AgentError::Parse(e.to_string()))?;
        let mut models: Vec<String> = v
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|id| id.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        models.sort();
        Ok(models)
    }

    pub async fn stream_chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
        temperature: f32,
        max_tokens: Option<u32>,
        sink: &dyn DeltaSink,
    ) -> Result<ChatResult, AgentError> {
        let mut body = json!({
            "model": model,
            "messages": messages.iter().map(|m| m.to_openai()).collect::<Vec<_>>(),
            "stream": true,
        });
        let lowered = model.to_lowercase();
        let reasoning_only = lowered.contains("reasoner")
            || lowered.starts_with("o1")
            || lowered.starts_with("o3")
            || lowered.starts_with("o4")
            || lowered.contains("gpt-5");
        if !reasoning_only {
            body["temperature"] = json!(temperature);
        }
        if let Some(max_tokens) = max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }
        if !tools.is_empty() {
            body["tools"] = Value::Array(
                tools
                    .iter()
                    .map(|t| {
                        json!({
                            "type": "function",
                            "function": {
                                "name": t.name,
                                "description": t.description,
                                "parameters": t.parameters
                            }
                        })
                    })
                    .collect(),
            );
            body["tool_choice"] = json!("auto");
        }

        let resp = self
            .http
            .post(self.endpoint("chat/completions"))
            .bearer_auth(&self.api_key)
            .header("Accept", "text/event-stream")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AgentError::Api { status: status.as_u16(), message: extract_error(&text) });
        }

        let mut result = ChatResult::default();
        let mut calls: BTreeMap<usize, ToolCall> = BTreeMap::new();
        let mut buf: Vec<u8> = Vec::new();
        let mut stream = resp.bytes_stream();
        let mut finished = false;

        while let Some(chunk) = stream.next().await {
            let bytes = chunk?;
            buf.extend_from_slice(&bytes);
            while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                let raw: Vec<u8> = buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&raw);
                let line = line.trim_end_matches(['\r', '\n']);
                let Some(data) = line.strip_prefix("data:") else { continue };
                let data = data.trim();
                if data == "[DONE]" {
                    finished = true;
                    break;
                }
                if data.is_empty() {
                    continue;
                }
                let value: Value = match serde_json::from_str(data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if let Some(err) = value.get("error") {
                    let message = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or_else(|| data)
                        .to_string();
                    return Err(AgentError::Api { status: 200, message });
                }
                absorb_chunk(&value, &mut result, &mut calls, sink);
            }
            if finished {
                break;
            }
        }

        for (index, call) in calls {
            let mut call = call;
            if call.id.is_empty() {
                call.id = format!("call_{index}");
            }
            if call.name.is_empty() {
                continue;
            }
            result.tool_calls.push(call);
        }

        if result.content.trim().is_empty() && result.tool_calls.is_empty() {
            return Err(AgentError::Empty);
        }
        Ok(result)
    }
}

fn absorb_chunk(
    value: &Value,
    result: &mut ChatResult,
    calls: &mut BTreeMap<usize, ToolCall>,
    sink: &dyn DeltaSink,
) {
    let Some(choice) = value.get("choices").and_then(|c| c.get(0)) else { return };
    let Some(delta) = choice.get("delta") else { return };

    let reasoning = delta
        .get("reasoning_content")
        .and_then(|v| v.as_str())
        .or_else(|| delta.get("reasoning").and_then(|v| v.as_str()));
    if let Some(text) = reasoning {
        if !text.is_empty() {
            result.reasoning.push_str(text);
            sink.reasoning(text);
        }
    }

    if let Some(text) = delta.get("content").and_then(|v| v.as_str()) {
        if !text.is_empty() {
            result.content.push_str(text);
            sink.text(text);
        }
    }

    if let Some(items) = delta.get("tool_calls").and_then(|v| v.as_array()) {
        for item in items {
            let index = item.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let entry = calls.entry(index).or_insert_with(|| ToolCall {
                id: String::new(),
                name: String::new(),
                arguments: String::new(),
            });
            if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                if !id.is_empty() {
                    entry.id = id.to_string();
                }
            }
            if let Some(name) = item.get("function").and_then(|f| f.get("name")).and_then(|v| v.as_str()) {
                if !name.is_empty() {
                    if entry.name.is_empty() {
                        entry.name = name.to_string();
                    } else if !entry.name.ends_with(name) {
                        entry.name.push_str(name);
                    }
                }
            }
            if let Some(args) = item
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|v| v.as_str())
            {
                entry.arguments.push_str(args);
            }
        }
    }

    if let Some(reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
        result.finish_reason = Some(reason.to_string());
    }
}

fn normalize_base(raw: &str) -> String {
    let mut base = raw.trim().trim_end_matches('/').to_string();
    if let Some(stripped) = base.strip_suffix("/chat/completions") {
        base = stripped.trim_end_matches('/').to_string();
    }
    base
}

fn extract_error(text: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(text) {
        if let Some(msg) = v
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .or_else(|| v.get("message").and_then(|m| m.as_str()))
        {
            return msg.to_string();
        }
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        "（服务未返回错误详情）".into()
    } else {
        trimmed.chars().take(600).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_normalization() {
        assert_eq!(normalize_base("https://api.deepseek.com/"), "https://api.deepseek.com");
        assert_eq!(
            normalize_base("https://api.deepseek.com/v1/chat/completions"),
            "https://api.deepseek.com/v1"
        );
    }

    #[test]
    fn endpoint_appends_v1() {
        let c = ChatClient::new("https://api.deepseek.com", "k").unwrap();
        assert_eq!(c.endpoint("chat/completions"), "https://api.deepseek.com/v1/chat/completions");
        let c = ChatClient::new("http://localhost:11434/v1", "k").unwrap();
        assert_eq!(c.endpoint("chat/completions"), "http://localhost:11434/v1/chat/completions");
    }
}
