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

// --- Codex translation ---
//
// v2's `turn/start` and `thread/start` take approval policy and sandbox policy
// as two independent fields. Slice 2 sets them per `Policy.permission_mode`
// × `Policy.sandbox`. See docs/superpowers/specs/2026-05-13-...-design.md.

use codex_client::protocol::v2::{AskForApproval, SandboxPolicy};

pub fn translate_codex_approval_policy(p: &Policy) -> AskForApproval {
    match p.permission_mode {
        // Guardian denies most things; operator overrides each denial.
        PermissionMode::RequireApproval => AskForApproval::Untrusted,
        // Guardian doesn't interrupt; we rely on the sandbox profile alone.
        PermissionMode::AcceptEdits | PermissionMode::ReadOnly => AskForApproval::Never,
    }
}

pub fn translate_codex_sandbox_policy(p: &Policy) -> SandboxPolicy {
    match (&p.permission_mode, &p.sandbox) {
        // ReadOnly mode forces the strictest sandbox regardless of `sandbox`.
        (PermissionMode::ReadOnly, _) => SandboxPolicy::ReadOnly {
            network_access: false,
        },
        // RequireApproval falls back to read-only so the guardian gates writes.
        (PermissionMode::RequireApproval, _) => SandboxPolicy::ReadOnly {
            network_access: false,
        },
        // AcceptEdits maps directly from the sandbox profile.
        (PermissionMode::AcceptEdits, SandboxProfile::ReadOnly) => SandboxPolicy::ReadOnly {
            network_access: false,
        },
        (PermissionMode::AcceptEdits, SandboxProfile::WorkspaceWrite) => {
            SandboxPolicy::WorkspaceWrite {
                exclude_slash_tmp: false,
                exclude_tmpdir_env_var: false,
                network_access: false,
                writable_roots: vec![],
            }
        }
        (PermissionMode::AcceptEdits, SandboxProfile::Unrestricted) => {
            SandboxPolicy::DangerFullAccess
        }
    }
}
