use std::path::PathBuf;
use symphony_core::config::EffectiveConfig;
use symphony_core::workflow::load_workflow;

fn write_temp(name: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("symphony_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    p
}

#[test]
fn loads_minimal_workflow_with_defaults() {
    let p = write_temp(
        "WORKFLOW.md",
        r#"---
tracker:
  kind: file_mock
  endpoint: ./issues.json
---
Hello prompt.
"#,
    );
    let wf = load_workflow(&p).unwrap();
    assert_eq!(wf.prompt_template, "Hello prompt.");
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();
    assert_eq!(cfg.tracker.kind, "file_mock");
    assert_eq!(cfg.polling.interval_ms, 30_000);
    assert_eq!(cfg.agent.max_concurrent_agents, 10);
    assert_eq!(cfg.agent.max_turns, 20);
    assert_eq!(cfg.agent.harness, "claude_code");
}

#[test]
fn linear_defaults_endpoint_and_resolves_env() {
    std::env::set_var("TEST_LINEAR_KEY", "tok-123");
    let p = write_temp(
        "WORKFLOW.md",
        r#"---
tracker:
  kind: linear
  api_key: $TEST_LINEAR_KEY
  project_slug: foo
---
"#,
    );
    let wf = load_workflow(&p).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();
    assert_eq!(
        cfg.tracker.endpoint.as_deref(),
        Some("https://api.linear.app/graphql"),
    );
    assert_eq!(cfg.tracker.api_key.as_deref(), Some("tok-123"));
    cfg.validate_for_dispatch().expect("valid");
}

#[test]
fn linear_missing_key_fails_validation() {
    let p = write_temp(
        "WORKFLOW.md",
        r#"---
tracker:
  kind: linear
  project_slug: foo
---
"#,
    );
    let wf = load_workflow(&p).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();
    assert!(cfg.validate_for_dispatch().is_err());
}

#[test]
fn linear_missing_slug_fails_validation() {
    let p = write_temp(
        "WORKFLOW.md",
        r#"---
tracker:
  kind: linear
  api_key: tok
---
"#,
    );
    let wf = load_workflow(&p).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();
    assert!(cfg.validate_for_dispatch().is_err());
}

#[test]
fn unsupported_tracker_kind_fails_validation() {
    let p = write_temp(
        "WORKFLOW.md",
        r#"---
tracker:
  kind: jira
---
"#,
    );
    let wf = load_workflow(&p).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();
    assert!(cfg.validate_for_dispatch().is_err());
}

#[test]
fn body_used_as_prompt_with_no_front_matter() {
    let p = write_temp("WORKFLOW.md", "just a body\n");
    let wf = load_workflow(&p).unwrap();
    assert_eq!(wf.prompt_template, "just a body");
}

#[test]
fn workspace_root_relative_resolves_to_workflow_dir() {
    let p = write_temp(
        "WORKFLOW.md",
        r#"---
tracker:
  kind: file_mock
  endpoint: ./x.json
workspace:
  root: ./ws
---
"#,
    );
    let wf = load_workflow(&p).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();
    let parent = p.parent().unwrap();
    assert!(
        cfg.workspace_root.starts_with(parent),
        "{:?} should start with {:?}",
        cfg.workspace_root,
        parent
    );
}
