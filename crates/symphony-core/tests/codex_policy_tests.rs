use codex_client::protocol::v2::{AskForApproval, SandboxPolicy};
use symphony_core::policy::{
    translate_codex_approval_policy, translate_codex_sandbox_policy, PermissionMode, Policy,
    SandboxProfile,
};

fn policy(mode: PermissionMode, sandbox: SandboxProfile) -> Policy {
    let mut p = Policy::default();
    p.permission_mode = mode;
    p.sandbox = sandbox;
    p
}

#[test]
fn require_approval_routes_through_guardian_via_untrusted() {
    let p = policy(
        PermissionMode::RequireApproval,
        SandboxProfile::WorkspaceWrite,
    );
    assert!(matches!(
        translate_codex_approval_policy(&p),
        AskForApproval::Untrusted
    ));
}

#[test]
fn accept_edits_never_asks() {
    let p = policy(PermissionMode::AcceptEdits, SandboxProfile::WorkspaceWrite);
    assert!(matches!(
        translate_codex_approval_policy(&p),
        AskForApproval::Never
    ));
}

#[test]
fn read_only_mode_never_asks_but_locks_sandbox() {
    let p = policy(PermissionMode::ReadOnly, SandboxProfile::WorkspaceWrite);
    assert!(matches!(
        translate_codex_approval_policy(&p),
        AskForApproval::Never
    ));
    assert!(matches!(
        translate_codex_sandbox_policy(&p),
        SandboxPolicy::ReadOnly {
            network_access: false
        }
    ));
}

#[test]
fn accept_edits_workspace_write_produces_workspace_sandbox() {
    let p = policy(PermissionMode::AcceptEdits, SandboxProfile::WorkspaceWrite);
    match translate_codex_sandbox_policy(&p) {
        SandboxPolicy::WorkspaceWrite {
            network_access,
            writable_roots,
            exclude_slash_tmp,
            exclude_tmpdir_env_var,
        } => {
            assert!(!network_access);
            assert!(writable_roots.is_empty());
            assert!(!exclude_slash_tmp);
            assert!(!exclude_tmpdir_env_var);
        }
        other => panic!("expected WorkspaceWrite, got {other:?}"),
    }
}

#[test]
fn accept_edits_unrestricted_produces_danger_full_access() {
    let p = policy(PermissionMode::AcceptEdits, SandboxProfile::Unrestricted);
    assert!(matches!(
        translate_codex_sandbox_policy(&p),
        SandboxPolicy::DangerFullAccess
    ));
}

#[test]
fn require_approval_forces_read_only_sandbox_regardless_of_profile() {
    for sandbox in [
        SandboxProfile::WorkspaceWrite,
        SandboxProfile::Unrestricted,
        SandboxProfile::ReadOnly,
    ] {
        let p = policy(PermissionMode::RequireApproval, sandbox);
        assert!(matches!(
            translate_codex_sandbox_policy(&p),
            SandboxPolicy::ReadOnly {
                network_access: false
            }
        ));
    }
}
