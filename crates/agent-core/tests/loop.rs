use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use agent_core::{
    ApprovalOutcome, ApprovalRequest, Approver, ChatClient, ChatMessage, EventSink, RunConfig, RunEvent,
    run_turn,
};
use async_trait::async_trait;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use win_tools::{Engine, Shell};

#[derive(Default)]
struct TestSink {
    events: Mutex<Vec<RunEvent>>,
}

impl EventSink for TestSink {
    fn emit(&self, event: RunEvent) {
        self.events.lock().unwrap().push(event);
    }
}

struct AutoApprove;

#[async_trait]
impl Approver for AutoApprove {
    async fn decide(&self, _request: ApprovalRequest) -> ApprovalOutcome {
        ApprovalOutcome { approved: true, remembered: false, reason: None }
    }
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    loop {
        let n = stream.read(&mut tmp).await.unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
            let len = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if buf.len() >= pos + 4 + len {
                break;
            }
        }
        if buf.len() > 2_000_000 {
            break;
        }
    }
    buf
}

/// Minimal OpenAI-compatible SSE server: first call asks for a tool, second answers.
async fn spawn_mock() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let calls = Arc::new(AtomicUsize::new(0));

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { break };
            let calls = calls.clone();
            tokio::spawn(async move {
                let _ = read_request(&mut stream).await;
                let n = calls.fetch_add(1, Ordering::SeqCst);
                let mut body = String::new();
                if n == 0 {
                    body.push_str(&format!(
                        "data: {}\n\n",
                        json!({"choices":[{"delta":{"content":"我先看一下。"}}]})
                    ));
                    body.push_str(&format!(
                        "data: {}\n\n",
                        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"run_powershell","arguments":"{\"script\":\"Write-Output hi\"}"}}]}}]})
                    ));
                    body.push_str(&format!(
                        "data: {}\n\n",
                        json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}]})
                    ));
                } else {
                    body.push_str(&format!("data: {}\n\n", json!({"choices":[{"delta":{"content":"完成。"}}]})));
                    body.push_str(&format!(
                        "data: {}\n\n",
                        json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
                    ));
                }
                body.push_str("data: [DONE]\n\n");

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    format!("http://127.0.0.1:{port}")
}

#[tokio::test]
async fn full_tool_loop() {
    let base = spawn_mock().await;
    let client = ChatClient::new(base, "test-key").unwrap();
    let shell = Arc::new(Shell::detect(std::env::temp_dir().join("winagent-agent-test")));
    let engine = Engine::new(shell);
    let sink = Arc::new(TestSink::default());
    let history = vec![
        ChatMessage::System { content: "你是测试助手".into() },
        ChatMessage::User { content: "跑个命令".into() },
    ];
    let cfg = RunConfig {
        model: "deepseek-flash".into(),
        max_turns: 4,
        temperature: 0.3,
        max_tokens: None,
        cwd: std::env::temp_dir(),
    };

    let outcome = run_turn(
        "run-test",
        &client,
        &engine,
        history,
        &cfg,
        &AutoApprove,
        sink.as_ref(),
        &CancellationToken::new(),
    )
    .await
    .expect("loop should succeed");

    assert_eq!(outcome.reason, "stop");
    let events = sink.events.lock().unwrap();
    let tool_finished = events
        .iter()
        .find_map(|e| match e {
            RunEvent::ToolFinished { ok, stdout, .. } => Some((*ok, stdout.clone())),
            _ => None,
        })
        .expect("tool result event");
    assert!(tool_finished.0, "tool should succeed");
    assert!(tool_finished.1.contains("hi"), "stdout: {}", tool_finished.1);

    let assistant_texts: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::AssistantMessage { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(assistant_texts.len(), 2);
    assert_eq!(assistant_texts[1], "完成。");

    let tool_message = outcome
        .messages
        .iter()
        .find_map(|m| match m {
            ChatMessage::Tool { content, .. } => Some(content.clone()),
            _ => None,
        })
        .expect("tool message in history");
    assert!(tool_message.contains("hi"));
}

#[tokio::test]
async fn api_error_is_surfaced() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = read_request(&mut stream).await;
        let body = json!({"error":{"message":"Authentication Fails"}}).to_string();
        let response = format!(
            "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes()).await;
    });

    let client = ChatClient::new(format!("http://127.0.0.1:{port}"), "bad").unwrap();
    let err = client
        .stream_chat(
            "deepseek-flash",
            &[ChatMessage::User { content: "hi".into() }],
            &[],
            0.3,
            None,
            &NoopDelta,
        )
        .await
        .expect_err("should fail");
    assert!(err.to_string().contains("Authentication Fails"), "{err}");
}

struct NoopDelta;

impl agent_core::DeltaSink for NoopDelta {
    fn text(&self, _delta: &str) {}
    fn reasoning(&self, _delta: &str) {}
}
