use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockerRef {
    pub id: Option<String>,
    pub identifier: Option<String>,
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<i32>,
    pub state: String,
    pub branch_name: Option<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<BlockerRef>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl Issue {
    pub fn workspace_key(&self) -> String {
        sanitize_workspace_key(&self.identifier)
    }
}

pub fn sanitize_workspace_key(identifier: &str) -> String {
    identifier
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageTokens {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveSession {
    pub session_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub agent_pid: Option<String>,
    pub last_event: Option<String>,
    pub last_timestamp: Option<DateTime<Utc>>,
    pub last_message: Option<String>,
    pub tokens: UsageTokens,
    pub last_reported_tokens: UsageTokens,
    pub turn_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryEntry {
    pub issue_id: String,
    pub identifier: String,
    pub attempt: u32,
    pub due_at_ms: i64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningEntry {
    pub issue: Issue,
    pub started_at: DateTime<Utc>,
    pub workspace_path: String,
    pub session: Option<LiveSession>,
}
