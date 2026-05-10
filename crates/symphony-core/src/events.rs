use crate::model::UsageTokens;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Normalized event emitted by an agent harness back to the orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEvent {
    pub kind: AgentEventKind,
    pub timestamp: DateTime<Utc>,
    pub agent_pid: Option<String>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub message: Option<String>,
    pub tokens: Option<UsageTokens>,
    pub raw: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentEventKind {
    SessionStarted,
    StartupFailed,
    TurnStarted,
    TurnCompleted,
    TurnFailed,
    TurnCancelled,
    TurnEndedWithError,
    TurnInputRequired,
    ApprovalAutoApproved,
    UnsupportedToolCall,
    Notification,
    OtherMessage,
    Malformed,
    TokenUsageUpdated,
    RateLimitsUpdated,
}
