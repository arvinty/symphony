use std::path::PathBuf;
use symphony_core::config::EffectiveConfig;
use symphony_core::workflow::load_workflow;

fn write_temp(body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("symphony_aac_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("WORKFLOW.md");
    std::fs::write(&p, body).unwrap();
    p
}

#[test]
fn continue_after_success_defaults_false() {
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
        !cfg.agent.continue_after_success,
        "continue_after_success must default to false"
    );
}

#[test]
fn continue_after_success_honors_explicit_true() {
    let p = write_temp(
        r#"---
tracker:
  kind: file_mock
  endpoint: ./issues.json
agent:
  harness: claude_code
  continue_after_success: true
---
prompt
"#,
    );
    let wf = load_workflow(&p).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();
    assert!(cfg.agent.continue_after_success);
}

#[test]
fn continue_after_success_honors_explicit_false() {
    let p = write_temp(
        r#"---
tracker:
  kind: file_mock
  endpoint: ./issues.json
agent:
  harness: claude_code
  continue_after_success: false
---
prompt
"#,
    );
    let wf = load_workflow(&p).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();
    assert!(!cfg.agent.continue_after_success);
}
