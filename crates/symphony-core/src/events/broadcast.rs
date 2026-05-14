use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OrchestratorEvent {
    AgentEvent {
        issue_id: String,
        event: super::AgentEvent,
    },
    ToolCall {
        issue_id: String,
        tool: String,
        input: serde_json::Value,
    },
    ToolResult {
        issue_id: String,
        tool: String,
        output: serde_json::Value,
        error: Option<String>,
    },
    ApprovalRequest {
        issue_id: String,
        approval_id: String,
        tool: String,
        input: serde_json::Value,
    },
    ApprovalDecision {
        issue_id: String,
        approval_id: String,
        allow: bool,
        reason: Option<String>,
    },
    AutoCommitted {
        issue_id: String,
        sha: String,
    },
    VcsPushed {
        issue_id: String,
        branch: String,
    },
    PrOpened {
        issue_id: String,
        url: String,
    },
    VcsError {
        issue_id: String,
        stage: String,
        message: String,
    },
    ReviewerStarted {
        issue_id: String,
        pr_url: String,
    },
    ReviewerCompleted {
        issue_id: String,
        success: bool,
        error: Option<String>,
    },
    IssueCompleted {
        issue_id: String,
        identifier: String,
    },
    Resync,
}

#[derive(Clone)]
pub struct OrchestratorEventBus {
    tx: broadcast::Sender<OrchestratorEvent>,
}

impl OrchestratorEventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }
    pub fn send(&self, e: OrchestratorEvent) -> Result<usize, broadcast::error::SendError<OrchestratorEvent>> {
        self.tx.send(e)
    }
    pub fn subscribe(&self) -> broadcast::Receiver<OrchestratorEvent> {
        self.tx.subscribe()
    }
}
