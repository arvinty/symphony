# Symphony v1.0 Slice 3 — Hermes Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mirror slice 1's Claude harness on Hermes — policy translation, MCP bridge wiring, tool-call bus events. Runtime changes stay concentrated in the Hermes harness, with supporting tests and docs.

**Architecture:** Three additive changes to `crates/symphony-core/src/harness/hermes.rs`. See `docs/superpowers/specs/2026-05-13-symphony-v1-slice3-design.md`.

---

## File Structure

**Created:**
- `crates/symphony-core/tests/hermes_integration.rs`
- `crates/symphony-core/tests/slice3_smoke.rs`

**Modified:**
- `crates/symphony-core/src/harness/hermes.rs` — add policy translation, MCP wiring, tool-call surfacing.
- `crates/symphony-core/Cargo.toml` — add `e2e_hermes` feature.
- `README.md` — update the Hermes harness row.

---

## Task 1: Policy translation

**Files:**
- Modify: `crates/symphony-core/src/harness/hermes.rs`

- [ ] **Step 1: Write failing tests**

```rust
// crates/symphony-core/src/harness/hermes.rs
use super::translate_hermes_policy_args;
use crate::policy::{PermissionMode, Policy};

fn with_mode(mode: PermissionMode) -> Policy {
    let mut p = Policy::default();
    p.permission_mode = mode;
    p
}

#[test]
fn accept_edits_maps_to_accept_edits_flag() {
    let p = with_mode(PermissionMode::AcceptEdits);
    let args = translate_hermes_policy_args(&p);
    assert_eq!(args, vec!["--permission-mode".to_string(), "acceptEdits".into()]);
}

#[test]
fn require_approval_maps_to_default_mode() {
    let p = with_mode(PermissionMode::RequireApproval);
    let args = translate_hermes_policy_args(&p);
    assert_eq!(args, vec!["--permission-mode".to_string(), "default".into()]);
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
```

- [ ] **Step 2: Verify failures**

Run: `cargo test -p symphony-core hermes::tests`
Expected: `translate_hermes_policy_args` not in scope.

- [ ] **Step 3: Implement**

Add to `hermes.rs`:

```rust
use crate::policy::{PermissionMode, Policy};

fn translate_hermes_policy_args(p: &Policy) -> Vec<String> {
    let mode = match p.permission_mode {
        PermissionMode::AcceptEdits => "acceptEdits",
        PermissionMode::RequireApproval => "default",
        PermissionMode::ReadOnly => "plan",
    };
    let mut args: Vec<String> = vec!["--permission-mode".into(), mode.into()];
    if !p.allowed_tools.is_empty() {
        args.push("--allowed-tools".into());
        args.push(p.allowed_tools.join(","));
    }
    args
}
```

- [ ] **Step 4: Verify pass**

Run: `cargo test -p symphony-core hermes::tests` → green.

- [ ] **Step 5: Commit**

```bash
git add crates/symphony-core/src/harness/hermes.rs
git commit -m "Add Hermes policy translation mirroring Claude's permission-mode flags"
```

---

## Task 2: MCP wiring + tool-call surfacing

**Files:**
- Modify: `crates/symphony-core/src/harness/hermes.rs`

- [ ] **Step 1: Modify `run()` to write MCP config and pass flag**

In `run()`, before `child.spawn()`:

```rust
// Policy flags.
for arg in translate_hermes_policy_args(&ctx.policy) {
    cmd.arg(arg);
}

// MCP wiring (only when Linear creds are available).
let issue_id_clone = ctx.issue_id.clone();
if let (Some(token), Some(endpoint)) = (ctx.linear_token.as_ref(), ctx.linear_endpoint.as_ref()) {
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("symphony"));
    let mcp_json = crate::harness::mcp_bridge::generate_mcp_config_json(&exe, &ctx.issue_id);
    let mcp_path = ctx.workspace.join(".symphony-mcp.json");
    std::fs::write(&mcp_path, mcp_json).ok();
    cmd.arg("--mcp-config").arg(&mcp_path);
    cmd.env("SYMPHONY_LINEAR_TOKEN", token);
    cmd.env("SYMPHONY_LINEAR_ENDPOINT", endpoint);
}
```

- [ ] **Step 2: Modify the stdout pump to surface tool_use**

Wrap the existing line-handling logic. Inside the stdout task:

```rust
let bus_clone = ctx.bus.clone();

while let Ok(Some(line)) = reader.next_line().await {
    if line.trim().is_empty() { continue; }
    match serde_json::from_str::<serde_json::Value>(&line) {
        Ok(v) => {
            if let Some(arr) = v.get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
            {
                for block in arr {
                    if block.get("type").and_then(|s| s.as_str()) == Some("tool_use") {
                        let name = block.get("name").and_then(|s| s.as_str()).unwrap_or("").to_string();
                        let input = block.get("input").cloned().unwrap_or(serde_json::json!({}));
                        let _ = bus_clone.send(crate::events::broadcast::OrchestratorEvent::ToolCall {
                            issue_id: issue_id_clone.clone(),
                            tool: name,
                            input,
                        });
                    }
                }
            }
            let ev = translate_hermes_event(&v, pid_clone.as_deref());
            let _ = tx_clone.send(ev).await;
        }
        Err(_) => {
            let _ = tx_clone.send(AgentEvent {
                kind: AgentEventKind::Malformed,
                timestamp: Utc::now(),
                agent_pid: pid_clone.clone(),
                thread_id: None,
                turn_id: None,
                message: Some(line),
                tokens: None,
                raw: None,
            }).await;
        }
    }
}
```

And add `translate_hermes_event`:

```rust
fn translate_hermes_event(v: &serde_json::Value, pid: Option<&str>) -> AgentEvent {
    let ty = v.get("type").and_then(|s| s.as_str()).unwrap_or("");
    let session_id = v.get("session_id").and_then(|s| s.as_str()).map(str::to_string);
    let turn_id = v.get("turn_id").and_then(|s| s.as_str()).map(str::to_string);
    let kind = match ty {
        "system" if v.get("subtype").and_then(|s| s.as_str()) == Some("init") => AgentEventKind::SessionStarted,
        "assistant" => AgentEventKind::Notification,
        "user" => AgentEventKind::OtherMessage,
        "result" => match v.get("subtype").and_then(|s| s.as_str()).unwrap_or("") {
            "success" => AgentEventKind::TurnCompleted,
            _ => AgentEventKind::TurnFailed,
        },
        _ => AgentEventKind::OtherMessage,
    };
    AgentEvent {
        kind,
        timestamp: Utc::now(),
        agent_pid: pid.map(str::to_string),
        thread_id: session_id.clone(),
        turn_id: turn_id.or(session_id),
        message: v.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()).map(str::to_string),
        tokens: None,
        raw: Some(v.clone()),
    }
}
```

- [ ] **Step 3: Verify workspace builds**

Run: `cargo check -p symphony-core` → clean.
Run: `cargo test --workspace` → no regressions.

- [ ] **Step 4: Commit**

```bash
git add crates/symphony-core/src/harness/hermes.rs
git commit -m "Wire Hermes harness to Linear MCP and surface tool_use on bus"
```

---

## Task 3: Integration test with mock-subprocess shim

**Files:**
- Create: `crates/symphony-core/tests/hermes_integration.rs`

- [ ] **Step 1: Write the test**

Pattern: build a shell-script `hermes` shim in a tempdir, prepend that tempdir to `PATH` for the test, run `HermesHarness::default().run(ctx)`, assert on `argv`-capture file + emitted events.

```rust
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use symphony_core::config::EffectiveConfig;
use symphony_core::events::broadcast::{OrchestratorEvent, OrchestratorEventBus};
use symphony_core::harness::approvals::ApprovalRouter;
use symphony_core::harness::hermes::HermesHarness;
use symphony_core::harness::{Harness, HarnessContext};
use symphony_core::policy::Policy;
use symphony_core::workflow::load_workflow;
use tokio::sync::mpsc;

fn write_temp_workflow() -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("symphony_hermes_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let workflow = dir.join("WORKFLOW.md");
    std::fs::write(&workflow, r#"---
tracker:
  kind: file_mock
  endpoint: ./issues.json
---
prompt
"#).unwrap();
    (workflow, dir)
}

fn install_hermes_shim(dir: &std::path::Path, argv_log: &std::path::Path) -> std::path::PathBuf {
    let shim = dir.join("hermes");
    let script = format!(r#"#!/bin/sh
printf "%s\n" "$@" > "{}"
cat <<'JSON'
{{"type":"system","subtype":"init","session_id":"s1"}}
{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"linear_graphql.add_comment","input":{{"body":"hi"}}}}]}}}}
{{"type":"result","subtype":"success","session_id":"s1"}}
JSON
"#, argv_log.display());
    std::fs::write(&shim, script).unwrap();
    let mut perms = std::fs::metadata(&shim).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&shim, perms).unwrap();
    shim
}

#[tokio::test]
async fn hermes_passes_policy_flags_and_surfaces_tool_use() {
    let (workflow_path, dir) = write_temp_workflow();
    let wf = load_workflow(&workflow_path).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();

    let bin_dir = std::env::temp_dir().join(format!("symphony_hermes_bin_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&bin_dir).unwrap();
    let argv_log = bin_dir.join("argv.txt");
    install_hermes_shim(&bin_dir, &argv_log);

    let orig_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var(
        "PATH",
        format!("{}:{}", bin_dir.display(), orig_path),
    );

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

    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        HermesHarness::default().run(ctx),
    )
    .await
    .expect("did not time out")
    .expect("ran");

    std::env::set_var("PATH", orig_path);

    assert!(outcome.success);

    let argv = std::fs::read_to_string(&argv_log).unwrap();
    assert!(argv.contains("--permission-mode"));
    assert!(argv.contains("acceptEdits"));
    assert!(argv.contains("--mcp-config"));

    let mut saw_tool_call = false;
    while let Ok(ev) = tokio::time::timeout(Duration::from_millis(50), bus_rx.recv()).await {
        if let Ok(OrchestratorEvent::ToolCall { tool, .. }) = ev {
            if tool == "linear_graphql.add_comment" {
                saw_tool_call = true;
            }
        }
    }
    assert!(saw_tool_call, "expected ToolCall bus event for linear_graphql.add_comment");

    // Drain remaining AgentEvents to avoid warnings.
    while rx.try_recv().is_ok() {}
}
```

This test is Unix-only because it uses a shell-script shim. Mark it `#[cfg(unix)]` so Windows runs skip it.

- [ ] **Step 2: Run**

Run: `cargo test -p symphony-core --test hermes_integration` → green.

- [ ] **Step 3: Commit**

```bash
git add crates/symphony-core/tests/hermes_integration.rs
git commit -m "Add hermes_integration test verifying flags and tool_use surfacing"
```

---

## Task 4: env-gated smoke + feature flag

**Files:**
- Modify: `crates/symphony-core/Cargo.toml` — add `e2e_hermes` feature.
- Create: `crates/symphony-core/tests/slice3_smoke.rs`

- [ ] **Step 1: Add feature**

In `crates/symphony-core/Cargo.toml`:

```toml
[features]
e2e_claude_code = []
e2e_codex = []
e2e_hermes = []
```

- [ ] **Step 2: Write the smoke**

Mirror `slice2_smoke.rs`'s manual-verification pattern. Gated by `e2e_hermes` + `HERMES_E2E=1`.

- [ ] **Step 3: Commit**

```bash
git add crates/symphony-core/Cargo.toml crates/symphony-core/tests/slice3_smoke.rs
git commit -m "Add env-gated slice 3 smoke for Hermes harness"
```

---

## Task 5: README update

- [ ] **Step 1: Update the harness table row**

Hermes row should call out that the harness now wires Linear MCP and translates policy flags, mirroring Claude. Note `e2e_hermes` feature + `HERMES_E2E=1` for the smoke.

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "Document Hermes harness slice 3 capabilities"
```

---

## Pre-merge checklist

- [ ] `cargo test -p symphony-core hermes::tests` green.
- [ ] `cargo test -p symphony-core --test hermes_integration` green.
- [ ] `cargo test --workspace` no regressions.
- [ ] `cargo clippy --workspace -- -D warnings` clean.
- [ ] Manual: install Hermes, claim an issue with `harness: hermes`, observe MCP-served Linear tool call on the dashboard, confirm `--permission-mode` is respected on writes.
- [ ] `docs/superpowers/brainstorms/2026-05-13-symphony-v1-slice3-state.md` reconciled with the final design decision.

## Risks & rollback

- **Hermes flag names differ.** Single touchpoints: `translate_hermes_policy_args` (modes) and the spawn block (MCP flag). Each is a 1-line edit.
- **JSON stream schema differs.** `translate_hermes_event` is single-source. Fix in place after observing real stream output during smoke.

Rollback: revert the v1-slice3 branch. Hermes returns to its slice-1-era minimal shape with no Linear MCP wiring.
