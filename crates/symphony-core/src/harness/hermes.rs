use super::{Harness, HarnessOutcome};
use crate::config::EffectiveConfig;
use crate::error::{Result, SymphonyError};
use crate::events::{AgentEvent, AgentEventKind};
use async_trait::async_trait;
use chrono::Utc;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

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

    async fn run(
        &self,
        workspace: &Path,
        prompt: &str,
        _cfg: &EffectiveConfig,
        tx: mpsc::Sender<AgentEvent>,
    ) -> Result<HarnessOutcome> {
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
            .arg(prompt)
            .current_dir(workspace)
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
        let handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                let raw = serde_json::from_str::<serde_json::Value>(&line).ok();
                let ev = AgentEvent {
                    kind: AgentEventKind::Notification,
                    timestamp: Utc::now(),
                    agent_pid: pid_clone.clone(),
                    thread_id: raw
                        .as_ref()
                        .and_then(|r| r.get("session_id"))
                        .and_then(|s| s.as_str())
                        .map(str::to_string),
                    turn_id: raw
                        .as_ref()
                        .and_then(|r| r.get("turn_id"))
                        .and_then(|s| s.as_str())
                        .map(str::to_string),
                    message: raw
                        .as_ref()
                        .and_then(|r| r.get("message"))
                        .and_then(|s| s.as_str())
                        .map(str::to_string),
                    tokens: None,
                    raw,
                };
                let _ = tx_clone.send(ev).await;
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
