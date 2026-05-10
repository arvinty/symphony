use symphony_core::policy::{PermissionMode, Policy, SandboxProfile};

#[test]
fn default_policy_preserves_v0_behavior() {
    let p = Policy::default();
    assert_eq!(p.permission_mode, PermissionMode::AcceptEdits);
    assert_eq!(p.sandbox, SandboxProfile::WorkspaceWrite);
    assert!(p.allowed_tools.is_empty());
    assert_eq!(p.approval_timeout_ms, 300_000);
}

#[test]
fn policy_parses_from_yaml() {
    let yaml = r#"
permission_mode: require_approval
sandbox: read_only
allowed_tools: ["Bash", "Edit"]
approval_timeout_ms: 60000
"#;
    let p: Policy = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(p.permission_mode, PermissionMode::RequireApproval);
    assert_eq!(p.sandbox, SandboxProfile::ReadOnly);
    assert_eq!(p.allowed_tools, vec!["Bash".to_string(), "Edit".to_string()]);
    assert_eq!(p.approval_timeout_ms, 60_000);
}

#[test]
fn policy_unknown_mode_errors() {
    let yaml = "permission_mode: lol\n";
    let r: Result<Policy, _> = serde_yaml::from_str(yaml);
    assert!(r.is_err());
}
