use super::{Harness, HarnessOutcome};
use crate::config::EffectiveConfig;
use crate::error::{Result, SymphonyError};
use crate::events::{AgentEvent, AgentEventKind};
use crate::model::UsageTokens;
use async_trait::async_trait;
use chrono::Utc;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Spawns the Claude Code CLI in headless print mode with stream-json output, and translates
/// each JSON line into a normalized AgentEvent. Uses the user's existing Claude Code subscription
/// auth — no API key required in the orchestrator process.
#[derive(Default, Clone)]
pub struct ClaudeCodeHarness {}

#[async_trait]
impl Harness for ClaudeCodeHarness {
    fn name(&self) -> &'static str {
        "claude_code"
    }

    async fn run(
        &self,
        workspace: &Path,
        prompt: &str,
        _cfg: &EffectiveConfig,
        tx: mpsc::Sender<AgentEvent>,
    ) -> Result<HarnessOutcome> {
        let mut cmd = Command::new("claude");
        cmd.arg("-p")
            .arg(prompt)
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose")
            .arg("--permission-mode")
            .arg("acceptEdits")
            .current_dir(workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

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

    let tokens = v.get("usage").and_then(|u| {
        let input = u.get("input_tokens").and_then(|n| n.as_u64()).unwrap_or(0);
        let output = u.get("output_tokens").and_then(|n| n.as_u64()).unwrap_or(0);
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
