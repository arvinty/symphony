use std::path::PathBuf;
use symphony_core::config::EffectiveConfig;
use symphony_core::policy::{PermissionMode, SandboxProfile};
use symphony_core::workflow::load_workflow;

fn write_temp(body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("symphony_rev_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("WORKFLOW.md");
    std::fs::write(&p, body).unwrap();
    p
}

#[test]
fn default_disables_reviewer() {
    let p = write_temp(
        r#"---
tracker:
  kind: file_mock
  endpoint: ./issues.json
---
prompt
"#,
    );
    let wf = load_workflow(&p).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();
    assert!(
        !cfg.reviewer.enabled,
        "reviewer should be disabled by default"
    );
}

#[test]
fn enabled_with_defaults_applies_read_only_policy() {
    let p = write_temp(
        r#"---
tracker:
  kind: file_mock
  endpoint: ./issues.json
reviewer:
  enabled: true
---
prompt
"#,
    );
    let wf = load_workflow(&p).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();
    assert!(cfg.reviewer.enabled);
    let default_policy = cfg.reviewer.effective_policy();
    assert_eq!(default_policy.permission_mode, PermissionMode::ReadOnly);
    assert_eq!(default_policy.sandbox, SandboxProfile::ReadOnly);
}

#[test]
fn explicit_harness_override_is_respected() {
    let p = write_temp(
        r#"---
tracker:
  kind: file_mock
  endpoint: ./issues.json
reviewer:
  enabled: true
  harness: codex
---
prompt
"#,
    );
    let wf = load_workflow(&p).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();
    assert_eq!(cfg.reviewer.harness.as_deref(), Some("codex"));
}

#[test]
fn explicit_policy_overrides_default() {
    let p = write_temp(
        r#"---
tracker:
  kind: file_mock
  endpoint: ./issues.json
reviewer:
  enabled: true
  policy:
    permission_mode: accept_edits
    sandbox: workspace_write
---
prompt
"#,
    );
    let wf = load_workflow(&p).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();
    let pol = cfg.reviewer.effective_policy();
    assert_eq!(pol.permission_mode, PermissionMode::AcceptEdits);
    assert_eq!(pol.sandbox, SandboxProfile::WorkspaceWrite);
}
