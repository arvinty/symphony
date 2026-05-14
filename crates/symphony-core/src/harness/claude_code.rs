/// NOTE: Claude Code's hosted permission UI is the source of truth for tool gating.
/// Symphony surfaces tool_use events on the bus so the dashboard can display them,
/// but cannot itself approve/deny tool calls — the `--permission-mode` flag tells
/// Claude Code which decisions to take. Slice 2 (Codex) is where Symphony fully
/// owns the approval round-trip.

use super::{Harness, HarnessContext, HarnessOutcome};
use crate::error::{Result, SymphonyError};
use crate::events::{AgentEvent, AgentEventKind};
use crate::model::UsageTokens;
use crate::policy::{Policy, PermissionMode};
use async_trait::async_trait;
use chrono::Utc;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Spawns the Claude Code CLI in headless print mode with stream-json output, and translates
/// each JSON line into a normalized AgentEvent. Uses the user's existing Claude Code subscription
/// auth — no API key required in the orchestrator process.
#[derive(Default, Clone)]
pub struct ClaudeCodeHarness {}

fn translate_policy_args(p: &Policy) -> Vec<String> {
    let mode = match p.permission_mode {
        PermissionMode::AcceptEdits => "acceptEdits",
        PermissionMode::RequireApproval => "default",
        PermissionMode::ReadOnly => "plan",
    };
    let mut args: Vec<String> = vec!["--permission-mode".into(), mode.into()];
    if !p.allowed_tools.is_empty() {
        args.push("--allowedTools".into());
        args.push(p.allowed_tools.join(","));
    }
    args
}

#[async_trait]
impl Harness for ClaudeCodeHarness {
    fn name(&self) -> &'static str {
        "claude_code"
    }

    async fn run(&self, ctx: HarnessContext<'_>) -> Result<HarnessOutcome> {
        let issue_id_clone = ctx.issue_id.clone();
        let HarnessContext { workspace, prompt, cfg: _, tx, bus, policy, linear_token, linear_endpoint, issue_id, .. } = ctx;

        let mut cmd = Command::new("claude");
        cmd.arg("-p")
            .arg(prompt)
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose");

        for arg in translate_policy_args(&policy) {
            cmd.arg(arg);
        }

        cmd.current_dir(workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        // Wire MCP config and linear env vars when credentials are available
        if let (Some(token), Some(endpoint)) = (linear_token.as_ref(), linear_endpoint.as_ref()) {
            let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("symphony"));
            let mcp_json = crate::harness::mcp_bridge::generate_mcp_config_json(&exe, &issue_id);
            let mcp_path = workspace.join(".symphony-mcp.json");
            std::fs::write(&mcp_path, mcp_json).ok();
            cmd.arg("--mcp-config").arg(&mcp_path);
            cmd.env("SYMPHONY_LINEAR_TOKEN", token);
            cmd.env("SYMPHONY_LINEAR_ENDPOINT", endpoint);
        }

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                SymphonyError::AgentNotFound("claude".into())
            } else {
                SymphonyError::Io(e)
            }
        })?;
        let pid = child.id().map(|p| p.to_string());
        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        let tx_stdout = tx.clone();
        let pid_clone = pid.clone();
        let bus_clone = bus.clone();
        let mut had_error = false;

        let stdout_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            let mut tid = String::new();
            let mut tu = String::new();
            while let Ok(Some(line)) = reader.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<serde_json::Value>(&line) {
                    Ok(v) => {
                        // Surface tool_use content blocks as OrchestratorEvent::ToolCall
                        if let Some(arr) = v.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array()) {
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

                        let ev = translate_claude_event(&v, pid_clone.as_deref());
                        if ev.kind == AgentEventKind::SessionStarted {
                            if let Some(t) = ev.thread_id.clone() {
                                tid = t;
                            }
                        }
                        if let Some(t) = &ev.turn_id {
                            tu = t.clone();
                        }
                        let _ = tx_stdout.send(ev).await;
                    }
                    Err(_) => {
                        let _ = tx_stdout
                            .send(AgentEvent {
                                kind: AgentEventKind::Malformed,
                                timestamp: Utc::now(),
                                agent_pid: pid_clone.clone(),
                                thread_id: None,
                                turn_id: None,
                                message: Some(line),
                                tokens: None,
                                raw: None,
                            })
                            .await;
                    }
                }
            }
            (tid, tu)
        });

        let stderr_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::debug!(target = "claude.stderr", "{}", line);
            }
        });

        let status = child
            .wait()
            .await
            .map_err(SymphonyError::Io)?;
        let (thread_id, last_turn_id) = stdout_handle.await.unwrap_or_default();
        let _ = stderr_handle.await;

        if !status.success() {
            had_error = true;
        }

        Ok(HarnessOutcome {
            thread_id: if thread_id.is_empty() { format!("claude-{}", uuid::Uuid::new_v4()) } else { thread_id },
            turn_id: if last_turn_id.is_empty() { format!("turn-{}", uuid::Uuid::new_v4()) } else { last_turn_id },
            success: !had_error,
            error: if had_error { Some(format!("exit_status={:?}", status.code())) } else { None },
        })
    }
}

fn translate_claude_event(v: &serde_json::Value, pid: Option<&str>) -> AgentEvent {
    // Claude Code stream-json uses { "type": "...", ... }
    let ty = v.get("type").and_then(|s| s.as_str()).unwrap_or("");
    let session_id = v.get("session_id").and_then(|s| s.as_str()).map(str::to_string);
    let kind = match ty {
        "system" if v.get("subtype").and_then(|s| s.as_str()) == Some("init") => {
            AgentEventKind::SessionStarted
        }
        "assistant" => AgentEventKind::Notification,
        "user" => AgentEventKind::OtherMessage,
        "result" => {
            let subtype = v.get("subtype").and_then(|s| s.as_str()).unwrap_or("");
            match subtype {
                "success" => AgentEventKind::TurnCompleted,
                "error_max_turns" | "error_during_execution" => AgentEventKind::TurnFailed,
                _ => AgentEventKind::TurnCompleted,
            }
        }
        _ => AgentEventKind::OtherMessage,
    };

    // Claude Code's stream-json puts cumulative session usage on the top-level
    // `usage` of the `result` event (assistant events nest per-message usage
    // under `message.usage`, which we deliberately skip to avoid double-count).
    // The bare `input_tokens` is only the *uncached* new input — the bulk of a
    // real turn's token volume lives in `cache_creation_input_tokens` and
    // `cache_read_input_tokens`. Fold those into the input count so the
    // reported total reflects what the model actually processed.
    let tokens = v.get("usage").and_then(|u| {
        let get = |k: &str| u.get(k).and_then(|n| n.as_u64()).unwrap_or(0);
        let input =
            get("input_tokens") + get("cache_creation_input_tokens") + get("cache_read_input_tokens");
        let output = get("output_tokens");
        Some(UsageTokens {
            input_tokens: input,
            output_tokens: output,
            total_tokens: input + output,
        })
    });

    let message = v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| {
            if c.is_string() {
                c.as_str().map(str::to_string)
            } else if c.is_array() {
                c.as_array().and_then(|arr| {
                    arr.iter()
                        .filter_map(|x| x.get("text").and_then(|t| t.as_str()))
                        .next()
                        .map(str::to_string)
                })
            } else {
                None
            }
        })
        .or_else(|| v.get("result").and_then(|r| r.as_str()).map(str::to_string));

    AgentEvent {
        kind,
        timestamp: Utc::now(),
        agent_pid: pid.map(str::to_string),
        thread_id: session_id.clone(),
        turn_id: session_id,
        message,
        tokens,
        raw: Some(v.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn result_event_token_count_includes_cache() {
        // Shape mirrors a real `result` event from `claude -p --output-format
        // stream-json`: the bare input_tokens is tiny, cache fields carry the
        // bulk of the volume.
        let ev = json!({
            "type": "result",
            "subtype": "success",
            "session_id": "s1",
            "usage": {
                "input_tokens": 1,
                "cache_creation_input_tokens": 4036,
                "cache_read_input_tokens": 9536,
                "output_tokens": 5
            }
        });
        let translated = translate_claude_event(&ev, None);
        let tokens = translated.tokens.expect("result event should carry tokens");
        assert_eq!(tokens.input_tokens, 1 + 4036 + 9536);
        assert_eq!(tokens.output_tokens, 5);
        assert_eq!(tokens.total_tokens, 1 + 4036 + 9536 + 5);
    }

    #[test]
    fn assistant_event_carries_no_top_level_tokens() {
        // Assistant events nest usage under message.usage; translate_claude_event
        // reads only top-level `usage`, so it must not emit tokens here (else
        // we'd double-count against the cumulative `result` event).
        let ev = json!({
            "type": "assistant",
            "session_id": "s1",
            "message": {
                "content": [{"type": "text", "text": "hi"}],
                "usage": {"input_tokens": 1, "output_tokens": 5}
            }
        });
        let translated = translate_claude_event(&ev, None);
        assert!(translated.tokens.is_none(), "assistant events must not emit tokens");
    }

    #[test]
    fn missing_cache_fields_default_to_zero() {
        let ev = json!({
            "type": "result",
            "subtype": "success",
            "usage": {"input_tokens": 100, "output_tokens": 50}
        });
        let tokens = translate_claude_event(&ev, None).tokens.unwrap();
        assert_eq!(tokens.input_tokens, 100);
        assert_eq!(tokens.total_tokens, 150);
    }
}
