use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_core::{ApprovalOutcome, ApprovalRequest, Approver, RunEvent};
use async_trait::async_trait;
use tauri::ipc::Channel;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use win_tools::RiskLevel;

struct Pending {
    tx: oneshot::Sender<ApprovalOutcome>,
    key: String,
}

#[derive(Default)]
pub struct ApprovalBroker {
    pending: Mutex<HashMap<String, Pending>>,
    remembered: Mutex<HashSet<String>>,
}

impl ApprovalBroker {
    pub fn key(session_id: &str, tool: &str, level: RiskLevel) -> String {
        format!("{session_id}|{tool}|{}", level.as_str())
    }

    pub fn is_remembered(&self, key: &str) -> bool {
        self.remembered.lock().map(|r| r.contains(key)).unwrap_or(false)
    }

    fn register(&self, id: &str, key: &str, tx: oneshot::Sender<ApprovalOutcome>) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(id.to_string(), Pending { tx, key: key.to_string() });
        }
    }

    fn forget(&self, id: &str) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(id);
        }
    }

    pub fn resolve(&self, id: &str, approved: bool, remember: bool) -> Result<(), String> {
        let entry = self
            .pending
            .lock()
            .map_err(|_| "审批状态锁定失败".to_string())?
            .remove(id)
            .ok_or_else(|| "审批请求不存在或已处理".to_string())?;
        if remember && approved {
            if let Ok(mut remembered) = self.remembered.lock() {
                remembered.insert(entry.key.clone());
            }
        }
        entry
            .tx
            .send(ApprovalOutcome { approved, remembered: remember && approved, reason: None })
            .map_err(|_| "审批等待方已结束".to_string())
    }

    pub fn clear_session(&self, session_id: &str) {
        if let Ok(mut remembered) = self.remembered.lock() {
            remembered.retain(|k| !k.starts_with(&format!("{session_id}|")));
        }
    }
}

pub struct TauriApprover {
    pub channel: Channel<RunEvent>,
    pub broker: Arc<ApprovalBroker>,
    pub session_id: String,
    pub cancel: CancellationToken,
}

#[async_trait]
impl Approver for TauriApprover {
    async fn decide(&self, req: ApprovalRequest) -> ApprovalOutcome {
        let key = ApprovalBroker::key(&self.session_id, &req.tool, req.level);
        if self.broker.is_remembered(&key) {
            return ApprovalOutcome { approved: true, remembered: true, reason: None };
        }

        let (tx, rx) = oneshot::channel();
        self.broker.register(&req.id, &key, tx);
        let _ = self.channel.send(RunEvent::ApprovalRequired {
            approval_id: req.id.clone(),
            tool_call_id: req.tool_call_id.clone(),
            tool: req.tool.clone(),
            summary: req.summary.clone(),
            command: req.command.clone(),
            elevated: req.elevated,
            level: req.level,
            reasons: req.reasons.clone(),
        });

        let outcome = tokio::select! {
            biased;
            _ = self.cancel.cancelled() => ApprovalOutcome {
                approved: false,
                remembered: false,
                reason: Some("操作已取消".into()),
            },
            _ = tokio::time::sleep(Duration::from_secs(600)) => ApprovalOutcome {
                approved: false,
                remembered: false,
                reason: Some("等待用户确认超时（10 分钟）".into()),
            },
            r = rx => r.unwrap_or(ApprovalOutcome {
                approved: false,
                remembered: false,
                reason: Some("审批通道已关闭".into()),
            }),
        };
        self.broker.forget(&req.id);
        outcome
    }
}
