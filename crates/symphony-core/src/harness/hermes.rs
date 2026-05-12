use super::{Harness, HarnessContext, HarnessOutcome};
use crate::error::{Result, SymphonyError};
use crate::events::{AgentEvent, AgentEventKind};
use crate::policy::{PermissionMode, Policy};
use async_trait::async_trait;
use chrono::Utc;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub fn translate_hermes_policy_args(p: &Policy) -> Vec<String> {
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

/// Hermes (Nous Research) agent harness. Spawns `hermes run` (or equivalent CLI form)
/// pointed at Claude as the model provider.
///
/// The exact CLI flags depend on the installed hermes version. We invoke:
///   hermes run --provider anthropic --model claude-opus-4-7 --json --workdir <ws> -- <prompt>
/// and read line-delimited JSON events from stdout.
#[derive(Default, Clone)]
pub struct HermesHarness {}

#[async_trait]
impl Harness for HermesHarness {
    fn name(&self) -> &'static str {
        "hermes"
    }

    async fn run(&self, ctx: HarnessContext<'_>) -> Result<HarnessOutcome> {
        let issue_id_clone = ctx.issue_id.clone();
        let HarnessContext {
            workspace,
            prompt,
            tx,
            bus,
            policy,
            linear_token,
            linear_endpoint,
            issue_id,
            ..
        } = ctx;
        let mut cmd = Command::new("hermes");
        cmd.arg("run")
            .arg("--provider")
            .arg("anthropic")
            .arg("--model")
            .arg(std::env::var("SYMPHONY_CLAUDE_MODEL").unwrap_or_else(|_| "claude-opus-4-7".into()))
            .arg("--json")
            .arg("--workdir")
            .arg(workspace)
            .arg("--prompt")
            .arg(prompt);

        for arg in translate_hermes_policy_args(&policy) {
            cmd.arg(arg);
        }

        if let (Some(token), Some(endpoint)) = (linear_token.as_ref(), linear_endpoint.as_ref()) {
            let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("symphony"));
            let mcp_json = crate::harness::mcp_bridge::generate_mcp_config_json(&exe, &issue_id);
            let mcp_path = workspace.join(".symphony-mcp.json");
            std::fs::write(&mcp_path, mcp_json).ok();
            cmd.arg("--mcp-config").arg(&mcp_path);
            cmd.env("SYMPHONY_LINEAR_TOKEN", token);
            cmd.env("SYMPHONY_LINEAR_ENDPOINT", endpoint);
        }

        cmd.current_dir(workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                SymphonyError::AgentNotFound("hermes".into())
            } else {
                SymphonyError::Io(e)
            }
        })?;
        let pid = child.id().map(|p| p.to_string());
        let stdout = child.stdout.take().expect("stdout piped");

        let tx_clone = tx.clone();
        let pid_clone = pid.clone();
        let bus_clone = bus.clone();
        let handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<serde_json::Value>(&line) {
                    Ok(v) => {
                        if let Some(arr) = v
                            .get("message")
                            .and_then(|m| m.get("content"))
                            .and_then(|c| c.as_array())
                        {
                            for block in arr {
                                if block.get("type").and_then(|s| s.as_str()) == Some("tool_use") {
                                    let name = block
                                        .get("name")
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let input = block.get("input").cloned().unwrap_or(serde_json::json!({}));
                                    let _ = bus_clone.send(
                                        crate::events::broadcast::OrchestratorEvent::ToolCall {
                                            issue_id: issue_id_clone.clone(),
                                            tool: name,
                                            input,
                                        },
                                    );
                                }
                            }
                        }
                        let ev = translate_hermes_event(&v, pid_clone.as_deref());
                        let _ = tx_clone.send(ev).await;
                    }
                    Err(_) => {
                        let _ = tx_clone
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
        });

        let status = child.wait().await.map_err(SymphonyError::Io)?;
        let _ = handle.await;
        Ok(HarnessOutcome {
            thread_id: format!("hermes-{}", uuid::Uuid::new_v4()),
            turn_id: format!("turn-{}", uuid::Uuid::new_v4()),
            success: status.success(),
            error: if status.success() {
                None
            } else {
                Some(format!("exit_status={:?}", status.code()))
            },
        })
    }
}

fn translate_hermes_event(v: &serde_json::Value, pid: Option<&str>) -> AgentEvent {
    let ty = v.get("type").and_then(|s| s.as_str()).unwrap_or("");
    let session_id = v.get("session_id").and_then(|s| s.as_str()).map(str::to_string);
    let turn_id = v.get("turn_id").and_then(|s| s.as_str()).map(str::to_string);
    let kind = match ty {
        "system" if v.get("subtype").and_then(|s| s.as_str()) == Some("init") => {
            AgentEventKind::SessionStarted
        }
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
        message: v
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .map(str::to_string),
        tokens: None,
        raw: Some(v.clone()),
    }
}
