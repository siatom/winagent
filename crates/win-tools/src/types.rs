use serde::{Deserialize, Serialize};

/// Description of a tool exposed to the model (OpenAI function-calling schema).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: serde_json::Value,
}

/// Raw result of one execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutput {
    pub ok: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub timed_out: bool,
    pub truncated: bool,
}

impl ToolOutput {
    pub fn failure(message: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            ok: false,
            exit_code: None,
            stdout: String::new(),
            stderr: message.into(),
            duration_ms,
            timed_out: false,
            truncated: false,
        }
    }

    /// Text handed back to the model as the tool result.
    pub fn model_text(&self) -> String {
        const LIMIT: usize = 24_000;
        let mut out = String::new();
        if self.timed_out {
            out.push_str("[执行超时，进程已被终止]\n");
        }
        out.push_str(&format!("exit_code: {}\n", self.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "null".into())));
        if !self.stdout.trim().is_empty() {
            out.push_str("--- stdout ---\n");
            out.push_str(&truncate(&self.stdout, LIMIT));
            out.push('\n');
        }
        if !self.stderr.trim().is_empty() {
            out.push_str("--- stderr ---\n");
            out.push_str(&truncate(&self.stderr, LIMIT / 2));
            out.push('\n');
        }
        if self.stdout.trim().is_empty() && self.stderr.trim().is_empty() {
            out.push_str("(no output)\n");
        }
        out
    }
}

fn truncate(s: &str, limit: usize) -> String {
    if s.chars().count() <= limit {
        return s.to_string();
    }
    let head: String = s.chars().take(limit).collect();
    format!("{head}\n…[输出已截断，共 {} 字符]", s.chars().count())
}
