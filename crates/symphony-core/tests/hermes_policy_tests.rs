use symphony_core::harness::hermes::translate_hermes_policy_args;
use symphony_core::policy::{PermissionMode, Policy};

fn with_mode(mode: PermissionMode) -> Policy {
    let mut p = Policy::default();
    p.permission_mode = mode;
    p
}

#[test]
fn accept_edits_maps_to_accept_edits_flag() {
    let p = with_mode(PermissionMode::AcceptEdits);
    let args = translate_hermes_policy_args(&p);
    assert_eq!(
        args,
        vec!["--permission-mode".to_string(), "acceptEdits".into()]
    );
}

#[test]
fn require_approval_maps_to_default_mode() {
    let p = with_mode(PermissionMode::RequireApproval);
    let args = translate_hermes_policy_args(&p);
    assert_eq!(
        args,
        vec!["--permission-mode".to_string(), "default".into()]
    );
}

#[test]
fn read_only_maps_to_plan_mode() {
    let p = with_mode(PermissionMode::ReadOnly);
    let args = translate_hermes_policy_args(&p);
    assert_eq!(args, vec!["--permission-mode".to_string(), "plan".into()]);
}

#[test]
fn allowed_tools_render_as_comma_joined() {
    let mut p = Policy::default();
    p.allowed_tools = vec!["Bash".into(), "Edit".into()];
    let args = translate_hermes_policy_args(&p);
    assert!(args.contains(&"--allowed-tools".to_string()));
    let idx = args.iter().position(|s| s == "--allowed-tools").unwrap();
    assert_eq!(args[idx + 1], "Bash,Edit");
}

#[test]
fn no_allowed_tools_omits_the_flag() {
    let p = Policy::default();
    let args = translate_hermes_policy_args(&p);
    assert!(!args.contains(&"--allowed-tools".to_string()));
}
