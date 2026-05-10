use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    AcceptEdits,
    RequireApproval,
    ReadOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxProfile {
    Unrestricted,
    WorkspaceWrite,
    ReadOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Policy {
    #[serde(default = "default_mode")]
    pub permission_mode: PermissionMode,
    #[serde(default = "default_sandbox")]
    pub sandbox: SandboxProfile,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// Consumed by `harness::approvals::ApprovalRouter` (Task 8) when the harness
    /// is in `RequireApproval` mode and the operator has not responded.
    #[serde(default = "default_timeout")]
    pub approval_timeout_ms: u64,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            permission_mode: default_mode(),
            sandbox: default_sandbox(),
            allowed_tools: vec![],
            approval_timeout_ms: default_timeout(),
        }
    }
}

fn default_mode() -> PermissionMode {
    PermissionMode::AcceptEdits
}
fn default_sandbox() -> SandboxProfile {
    SandboxProfile::WorkspaceWrite
}
fn default_timeout() -> u64 {
    300_000
}
