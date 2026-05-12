# Symphony v1.0 — Slice 3: Hermes Harness with Linear MCP + Policy

**Date:** 2026-05-13
**Status:** Approved (design); pending implementation plan
**Scope:** Slice 3 of phase 1. Builds on slice 1 (merged) and slice 2 (`v1-slice2`, PR pending). Lands on `v1-slice3` off the eventual `main` once slice 2 merges; currently branched off `v1-slice2` so it stacks.

## Goal

Bring the Hermes harness to parity with slice 1's Claude harness: Linear MCP bridge wired, policy translated to native Hermes CLI flags, tool calls surfaced on the orchestrator bus, MCP token + endpoint plumbed through. Reuses every cross-cutting piece from slice 1 — there are no new shared modules.

## Non-Goals

- Symphony-side approval interception for Hermes (its hosted `--permission-mode` UI handles user gating, identical positioning to Claude in slice 1).
- Pairing emitted `OrchestratorEvent::ToolResult` events with `ToolCall`s — same as slice 1's Claude case; the stream doesn't reliably pair them.
- Programmatic CI smoke against a real Hermes binary (env-gated manual smoke only).

## Architecture

A single file edit. `crates/symphony-core/src/harness/hermes.rs` grows from 101 LOC to ~180 LOC; no new modules, no workspace dependency changes.

Three additive changes:

1. **`translate_hermes_policy_args(&Policy) -> Vec<String>`.** Maps `Policy.permission_mode` to `--permission-mode <value>` and `Policy.allowed_tools` to `--allowed-tools <comma-joined>`. Mirrors the existing `translate_policy_args` in `claude_code.rs`. Assumes Hermes accepts the same enum names Claude does (`acceptEdits` / `default` / `plan`) — the function is a single touchpoint to retune if the actual enum differs.
2. **MCP bridge wiring.** Reuses `mcp_bridge::generate_mcp_config_json` unchanged. Writes `<workspace>/.symphony-mcp.json`, passes `--mcp-config <path>`, sets `SYMPHONY_LINEAR_TOKEN` and `SYMPHONY_LINEAR_ENDPOINT` env vars. Lifted verbatim from `claude_code.rs`.
3. **Tool-call surfacing on the stdout pump.** Replace the current "every line becomes `AgentEventKind::Notification`" loop with a typed translator parallel to `translate_claude_event`: parse the JSON line, detect `tool_use` content blocks, emit `OrchestratorEvent::ToolCall` on the bus, then emit a typed `AgentEvent` to `ctx.tx`.

## Components

### `harness::hermes::translate_hermes_policy_args` (~20 LOC, additive)

```rust
fn translate_hermes_policy_args(p: &Policy) -> Vec<String> {
    let mode = match p.permission_mode {
        PermissionMode::AcceptEdits => "acceptEdits",
        PermissionMode::RequireApproval => "default",
        PermissionMode::ReadOnly => "plan",
    };
    let mut args = vec!["--permission-mode".into(), mode.into()];
    if !p.allowed_tools.is_empty() {
        args.push("--allowed-tools".into());
        args.push(p.allowed_tools.join(","));
    }
    args
}
```

If Hermes' actual mode names differ from Claude's, only the `match` arms change.

### `harness::hermes::HermesHarness::run` — modifications (~60 LOC delta)

Lifted from `claude_code.rs`:

```rust
// Inside run(), after extracting HarnessContext fields:
let issue_id_clone = ctx.issue_id.clone();
let bus_clone = ctx.bus.clone();

// Policy flags.
for arg in translate_hermes_policy_args(&ctx.policy) {
    cmd.arg(arg);
}

// MCP bridge (only if Linear creds are available).
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

In the stdout-pump loop, replace the current `kind: Notification` blanket with:

```rust
match serde_json::from_str::<serde_json::Value>(&line) {
    Ok(v) => {
        if let Some(arr) = v.get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        {
            for block in arr {
                if block.get("type").and_then(|s| s.as_str()) == Some("tool_use") {
                    let name = block.get("name").and_then(|s| s.as_str()).unwrap_or("").to_string();
                    let input = block.get("input").cloned().unwrap_or(json!({}));
                    let _ = bus_clone.send(OrchestratorEvent::ToolCall {
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
    Err(_) => { /* AgentEventKind::Malformed */ }
}
```

### `harness::hermes::translate_hermes_event` (~30 LOC, additive)

Parallel to `translate_claude_event`. Maps `type` strings to `AgentEventKind`. Best-effort — Hermes' stream-json schema isn't documented in this repo; the function defaults to `Notification` for unknown types and is the touchpoint to retune.

## Data Flow

Identical to slice 1's Claude flow. Diagrammed in section 3 of the slice 3 brainstorm.

Notable: no approval round-trip path. Hermes' hosted `--permission-mode` UI handles user gating, just like Claude. Symphony's bus reports tool calls for the dashboard but does not gate.

## Error Handling

Inherited from slice 1's Claude harness verbatim:

| Failure | Behavior |
|---|---|
| `hermes` not on PATH | `SymphonyError::AgentNotFound("hermes")` |
| Child crash | `HarnessOutcome { success: false, error: Some("exit_status=…") }` |
| Malformed JSON line | `AgentEventKind::Malformed` with raw line, continue parsing |
| MCP bridge crash inside Hermes | Surfaced as error event on Hermes' stream → `AgentEventKind::Notification`; final outcome reflects overall turn |

No new error types.

## Testing

### Unit: `translate_hermes_policy_args` (`crates/symphony-core/tests/hermes_policy_tests.rs`)

Parametrize the 9 `(PermissionMode, SandboxProfile)` combinations, assert flag vectors. ~40 LOC.

### Integration: mock-subprocess (`crates/symphony-core/tests/hermes_integration.rs`)

Build a small shell-script `hermes` shim into a tempdir, prepend tempdir to `PATH` for the test process, run `HermesHarness::default().run(ctx)`. The shim:

- Records its `argv` to a file the test reads back (verifies `--permission-mode`, `--mcp-config`, `--allowed-tools` arrived correctly).
- Emits a scripted JSON sequence: assistant message with a `tool_use` block, then a final `result` event.

The test then asserts:
- `tx` received the expected `AgentEvent` sequence.
- `bus` saw a `ToolCall` with `tool: "linear_graphql.add_comment"` and matching input.
- `HarnessOutcome.success == true`.

~100 LOC including the shim build helper.

### End-to-end smoke (`slice3_smoke.rs`)

Gated by `e2e_hermes` cargo feature + `HERMES_E2E=1` env var. Mirrors slice 1 and 2's smokes: documented manual verification, no programmatic assertions. Requires a real Hermes CLI on PATH.

## Pre-merge checklist

- [ ] `cargo test -p symphony-core --test hermes_policy_tests` green.
- [ ] `cargo test -p symphony-core --test hermes_integration` green.
- [ ] `cargo test --workspace` no regressions.
- [ ] `cargo clippy --workspace -- -D warnings` clean.
- [ ] Manual: claim an issue with `harness: hermes` in `WORKFLOW.md`, confirm:
  - The agent receives the `linear_graphql.*` tools from the MCP bridge.
  - At least one `OrchestratorEvent::ToolCall` shows on the dashboard.
  - `--permission-mode` is respected (the agent prompts on writes if mode is `default`).

## Open Questions

Two deferrals to implementation, both single-touchpoint fixups:

- **OQ-1: Hermes flag names.** If Hermes uses `--tools-config` instead of `--mcp-config`, or `--tools-allowed` instead of `--allowed-tools`, edit the spawn block and `translate_hermes_policy_args` accordingly.
- **OQ-2: Permission mode enum values.** If Hermes uses different strings than Claude's `acceptEdits` / `default` / `plan`, edit the `match` arms in `translate_hermes_policy_args`.

Both unblock at the env-gated smoke step.
