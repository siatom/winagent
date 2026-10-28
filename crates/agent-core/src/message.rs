use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON string exactly as produced by the model.
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum ChatMessage {
    System { content: String },
    User { content: String },
    Assistant {
        content: Option<String>,
        reasoning: Option<String>,
        #[serde(default)]
        tool_calls: Vec<ToolCall>,
    },
    Tool {
        tool_call_id: String,
        name: String,
        content: String,
    },
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self::User { content: content.into() }
    }

    pub fn to_openai(&self) -> Value {
        match self {
            ChatMessage::System { content } => json!({ "role": "system", "content": content }),
            ChatMessage::User { content } => json!({ "role": "user", "content": content }),
            ChatMessage::Assistant { content, tool_calls, .. } => {
                let mut v = json!({
                    "role": "assistant",
                    "content": content.clone().unwrap_or_default(),
                });
                if !tool_calls.is_empty() {
                    v["tool_calls"] = Value::Array(
                        tool_calls
                            .iter()
                            .map(|c| {
                                json!({
                                    "id": c.id,
                                    "type": "function",
                                    "function": { "name": c.name, "arguments": c.arguments }
                                })
                            })
                            .collect(),
                    );
                }
                v
            }
            ChatMessage::Tool { tool_call_id, content, .. } => {
                json!({ "role": "tool", "tool_call_id": tool_call_id, "content": content })
            }
        }
    }

    pub fn is_empty_assistant(&self) -> bool {
        matches!(self, ChatMessage::Assistant { content, tool_calls, .. }
            if tool_calls.is_empty() && content.as_deref().unwrap_or("").trim().is_empty())
    }
}

/// Make a persisted history safe for the API:
/// - drop empty assistant messages
/// - drop orphan tool results
/// - synthesize missing tool results so the model never sees a dangling tool call
pub fn sanitize_history(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut out: Vec<ChatMessage> = Vec::with_capacity(messages.len());
    for msg in messages {
        if msg.is_empty_assistant() {
            continue;
        }
        match msg {
            ChatMessage::Tool { ref tool_call_id, .. } => {
                let expected = out.iter().rev().find_map(|m| {
                    if let ChatMessage::Assistant { tool_calls, .. } = m {
                        Some(tool_calls.iter().any(|c| &c.id == tool_call_id))
                    } else if matches!(m, ChatMessage::Tool { .. }) {
                        None
                    } else {
                        Some(false)
                    }
                });
                if expected == Some(true) {
                    out.push(msg);
                }
            }
            other => out.push(other),
        }
    }

    // Fill in missing tool results for the last assistant tool-call block.
    let mut result: Vec<ChatMessage> = Vec::with_capacity(out.len() + 4);
    let mut i = 0;
    while i < out.len() {
        let msg = out[i].clone();
        if let ChatMessage::Assistant { ref tool_calls, .. } = msg {
            if !tool_calls.is_empty() {
                let calls: Vec<(String, String)> =
                    tool_calls.iter().map(|c| (c.id.clone(), c.name.clone())).collect();
                let mut answered = Vec::new();
                let mut j = i + 1;
                while j < out.len() {
                    if let ChatMessage::Tool { tool_call_id, .. } = &out[j] {
                        answered.push(tool_call_id.clone());
                        j += 1;
                    } else {
                        break;
                    }
                }
                result.push(msg);
                for m in out.iter().take(j).skip(i + 1) {
                    result.push(m.clone());
                }
                for (id, name) in calls {
                    if !answered.contains(&id) {
                        result.push(ChatMessage::Tool {
                            tool_call_id: id,
                            name,
                            content: "未执行：上一轮操作被中断。".into(),
                        });
                    }
                }
                i = j;
                continue;
            }
        }
        result.push(msg);
        i += 1;
    }
    result
}

/// Drop the oldest conversation groups until the estimated size fits the budget.
pub fn trim_to_budget(messages: &mut Vec<ChatMessage>, max_chars: usize) {
    let size = |m: &ChatMessage| -> usize {
        match m {
            ChatMessage::System { content } | ChatMessage::User { content } => content.chars().count(),
            ChatMessage::Assistant { content, reasoning, tool_calls } => {
                content.as_deref().unwrap_or("").chars().count()
                    + reasoning.as_deref().unwrap_or("").chars().count()
                    + tool_calls.iter().map(|c| c.arguments.chars().count() + c.name.len()).sum::<usize>()
            }
            ChatMessage::Tool { content, .. } => content.chars().count(),
        }
    };

    while messages.len() > 4 {
        let total: usize = messages.iter().map(size).sum();
        if total <= max_chars {
            return;
        }
        // Never drop the system prompt.
        let start = usize::from(matches!(messages.first(), Some(ChatMessage::System { .. })));
        if start >= messages.len().saturating_sub(1) {
            return;
        }
        messages.remove(start);
        while start < messages.len() && matches!(messages.get(start), Some(ChatMessage::Tool { .. })) {
            messages.remove(start);
        }
        if messages.len() < 2 {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_fills_missing_tool_results() {
        let msgs = vec![
            ChatMessage::User { content: "hi".into() },
            ChatMessage::Assistant {
                content: None,
                reasoning: None,
                tool_calls: vec![ToolCall { id: "a".into(), name: "run_powershell".into(), arguments: "{}".into() }],
            },
        ];
        let out = sanitize_history(msgs);
        assert_eq!(out.len(), 3);
        assert!(matches!(out[2], ChatMessage::Tool { .. }));
    }

    #[test]
    fn sanitize_drops_orphan_tool() {
        let msgs = vec![
            ChatMessage::User { content: "hi".into() },
            ChatMessage::Tool { tool_call_id: "x".into(), name: "t".into(), content: "y".into() },
        ];
        assert_eq!(sanitize_history(msgs).len(), 1);
    }
}
