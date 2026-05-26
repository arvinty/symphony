//! Integration test for the Hermes harness using a shell-script shim
//! installed on PATH. Unix-only — Windows skips automatically.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use symphony_core::config::EffectiveConfig;
use symphony_core::events::broadcast::{OrchestratorEvent, OrchestratorEventBus};
use symphony_core::harness::approvals::ApprovalRouter;
use symphony_core::harness::hermes::HermesHarness;
use symphony_core::harness::{Harness, HarnessContext};
use symphony_core::policy::Policy;
use symphony_core::workflow::load_workflow;
use tokio::sync::mpsc;

struct PathGuard {
    original: String,
}

impl PathGuard {
    fn prepend(dir: &Path) -> Self {
        let original = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{}", dir.display(), original));
        Self { original }
    }
}

impl Drop for PathGuard {
    fn drop(&mut self) {
        std::env::set_var("PATH", &self.original);
    }
}

fn write_temp_workflow() -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("symphony_hermes_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let workflow = dir.join("WORKFLOW.md");
    std::fs::write(
        &workflow,
        r#"---
tracker:
  kind: file_mock
  endpoint: ./issues.json
---
prompt
"#,
    )
    .unwrap();
    (workflow, dir)
}

fn install_hermes_shim(dir: &Path, argv_log: &Path) {
    let shim = dir.join("hermes");
    let script = format!(
        r#"#!/bin/sh
for a in "$@"; do
  printf '%s\n' "$a"
done > "{argv}"
cat <<'JSON'
{{"type":"system","subtype":"init","session_id":"s1"}}
{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"linear_graphql.add_comment","input":{{"body":"hi"}}}}]}}}}
{{"type":"result","subtype":"success","session_id":"s1"}}
JSON
"#,
        argv = argv_log.display()
    );
    std::fs::write(&shim, script).unwrap();
    let mut perms = std::fs::metadata(&shim).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&shim, perms).unwrap();
}

#[tokio::test]
async fn hermes_passes_policy_flags_and_surfaces_tool_use() {
    let (workflow_path, dir) = write_temp_workflow();
    let wf = load_workflow(&workflow_path).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();

    let bin_dir =
        std::env::temp_dir().join(format!("symphony_hermes_bin_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&bin_dir).unwrap();
    let argv_log = bin_dir.join("argv.txt");
    install_hermes_shim(&bin_dir, &argv_log);

    let _path_guard = PathGuard::prepend(&bin_dir);

    let (tx, mut rx) = mpsc::channel(64);
    let bus = OrchestratorEventBus::new(64);
    let mut bus_rx = bus.subscribe();
    let approval_router = ApprovalRouter::new();
    let workspace = dir.clone();

    let ctx = HarnessContext {
        workspace: &workspace,
        prompt: "do a thing",
        cfg: &cfg,
        tx,
        bus: bus.clone(),
        approval_router,
        policy: Policy::default(),
        linear_token: Some("tok".into()),
        linear_endpoint: Some("http://localhost:4000/graphql".into()),
        issue_id: "DEMO-1".into(),
    };

    let outcome = tokio::time::timeout(Duration::from_secs(5), HermesHarness::default().run(ctx))
        .await
        .expect("did not time out")
        .expect("ran");

    assert!(outcome.success, "outcome: {outcome:?}");

    let argv = std::fs::read_to_string(&argv_log).unwrap_or_default();
    assert!(
        argv.contains("--permission-mode"),
        "argv missing --permission-mode: {argv}"
    );
    assert!(
        argv.contains("acceptEdits"),
        "argv missing acceptEdits: {argv}"
    );
    assert!(
        argv.contains("--mcp-config"),
        "argv missing --mcp-config: {argv}"
    );

    let mut saw_tool_call = false;
    while let Ok(Ok(ev)) = tokio::time::timeout(Duration::from_millis(100), bus_rx.recv()).await {
        if let OrchestratorEvent::ToolCall { tool, .. } = ev {
            if tool == "linear_graphql.add_comment" {
                saw_tool_call = true;
            }
        }
    }
    assert!(
        saw_tool_call,
        "expected ToolCall bus event for linear_graphql.add_comment"
    );

    // Drain remaining AgentEvents.
    while rx.try_recv().is_ok() {}
}
