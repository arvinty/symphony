//! Codex `app-server` harness. Spawns the CLI, talks v2 JSON-RPC over stdio,
//! translates notifications to AgentEvent / OrchestratorEvent, and routes
//! guardian-denied actions through the operator dashboard via ApprovalRouter.

use super::{Harness, HarnessContext, HarnessOutcome};
use crate::error::{Result, SymphonyError};
use crate::events::broadcast::{OrchestratorEvent, OrchestratorEventBus};
use crate::events::{AgentEvent, AgentEventKind};
use crate::harness::approvals::ApprovalRouter;
use crate::policy::{
    translate_codex_approval_policy, translate_codex_sandbox_policy, PermissionMode, Policy,
};
use async_trait::async_trait;
use chrono::Utc;
use codex_client::protocol::messages::{KnownServerNotification, ServerNotification};
use codex_client::protocol::v2::{
    GuardianApprovalReviewStatus, ItemGuardianApprovalReviewCompletedNotification,
    ItemGuardianApprovalReviewStartedNotification, ThreadApproveGuardianDeniedActionParams,
    ThreadStartParams, TurnStartParams, TurnStatus, UserInput,
};
use codex_client::protocol::{v1, v2};
use codex_client::{Client, NotificationStream};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

#[derive(Default, Clone)]
pub struct CodexHarness {}

#[async_trait]
impl Harness for CodexHarness {
    fn name(&self) -> &'static str {
        "codex"
    }

    async fn run(&self, ctx: HarnessContext<'_>) -> Result<HarnessOutcome> {
        // 1. Build MCP bridge config for Linear tooling.
        let symphony_exe = std::env::current_exe()
            .unwrap_or_else(|_| std::path::PathBuf::from("symphony"));
        let mcp_inline_toml = format!(
            "mcp_servers.linear = {{ command = \"{}\", args = [\"mcp-bridge\", \"--issue\", \"{}\"] }}",
            toml_basic_string(&symphony_exe.to_string_lossy()),
            toml_basic_string(&ctx.issue_id)
        );

        // 2. Spawn codex.
        let mut cmd = Command::new("codex");
        cmd.arg("app-server")
            .arg("--listen")
            .arg("stdio")
            .arg("-c")
            .arg(&mcp_inline_toml)
            .current_dir(ctx.workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(t) = ctx.linear_token.as_ref() {
            cmd.env("SYMPHONY_LINEAR_TOKEN", t);
        }
        if let Some(e) = ctx.linear_endpoint.as_ref() {
            cmd.env("SYMPHONY_LINEAR_ENDPOINT", e);
        }
        cmd.env("SYMPHONY_ISSUE_ID", &ctx.issue_id);

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                SymphonyError::AgentNotFound("codex".into())
            } else {
                SymphonyError::Io(e)
            }
        })?;
        let stderr = child.stderr.take().expect("stderr piped");
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::debug!(target = "codex.stderr", "{}", line);
            }
        });

        let (client, notifs) = Client::connect(child).map_err(|e| {
            SymphonyError::CodexClient(format!("connect: {e}"))
        })?;

        run_with_client(Arc::new(client), notifs, &ctx).await
    }
}

/// Core pump. Extracted from `Harness::run` so tests can drive it with a
/// `Client` built via `Client::from_halves(DuplexStream, DuplexStream)`.
pub async fn run_with_client(
    client: Arc<Client>,
    mut notifs: NotificationStream,
    ctx: &HarnessContext<'_>,
) -> Result<HarnessOutcome> {
    // 3. Initialize handshake.
    let _init = client
        .initialize(v1::InitializeParams {
            protocol_version: "v1".into(),
            client_info: v1::ClientInfo {
                name: "symphony".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            capabilities: serde_json::Value::Null,
        })
        .await
        .map_err(|e| SymphonyError::CodexClient(format!("initialize: {e}")))?;

    // 4. Start a fresh thread carrying the policy.
    let mut thread_params = ThreadStartParams::default();
    thread_params.approval_policy = Some(translate_codex_approval_policy(&ctx.policy));
    thread_params.cwd = Some(ctx.workspace.to_string_lossy().to_string());
    // `ThreadStartParams::sandbox` is the coarse SandboxMode; sandbox_policy
    // is overridden on the turn instead.
    let thread_resp = client
        .start_thread(thread_params)
        .await
        .map_err(|e| SymphonyError::CodexClient(format!("thread/start: {e}")))?;
    let thread_id = thread_resp.thread.id.clone();

    // 5. Start the turn with the prompt + sandbox override.
    let turn_params = TurnStartParams {
        approval_policy: Some(translate_codex_approval_policy(&ctx.policy)),
        approvals_reviewer: None,
        cwd: Some(ctx.workspace.to_string_lossy().to_string()),
        effort: None,
        input: vec![UserInput::Text {
            text: ctx.prompt.to_string(),
            text_elements: vec![],
        }],
        model: None,
        output_schema: None,
        personality: None,
        sandbox_policy: Some(translate_codex_sandbox_policy(&ctx.policy)),
        service_tier: None,
        summary: None,
        thread_id: thread_id.clone(),
    };
    let turn_resp = client
        .start_turn(turn_params)
        .await
        .map_err(|e| SymphonyError::CodexClient(format!("turn/start: {e}")))?;
    let turn_id = turn_resp.turn.id.clone();

    let _ = ctx
        .tx
        .send(AgentEvent {
            kind: AgentEventKind::TurnStarted,
            timestamp: Utc::now(),
            agent_pid: None,
            thread_id: Some(thread_id.clone()),
            turn_id: Some(turn_id.clone()),
            message: None,
            tokens: None,
            raw: None,
        })
        .await;

    // 6. Event pump.
    let pending_reviews: Arc<Mutex<HashMap<String, serde_json::Value>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let mut success = false;
    let mut error_msg: Option<String> = None;

    loop {
        let Some(notif) = notifs.recv().await else {
            error_msg = Some("transport closed".into());
            break;
        };
        match notif {
            ServerNotification::Known(known) => match known {
                KnownServerNotification::TurnStarted(_) => {
                    // already emitted on our side after turn/start
                }
                KnownServerNotification::TurnCompleted(n) => {
                    success = matches!(n.turn.status, TurnStatus::Completed);
                    let _ = ctx
                        .tx
                        .send(AgentEvent {
                            kind: if success {
                                AgentEventKind::TurnCompleted
                            } else {
                                AgentEventKind::TurnFailed
                            },
                            timestamp: Utc::now(),
                            agent_pid: None,
                            thread_id: Some(thread_id.clone()),
                            turn_id: Some(turn_id.clone()),
                            message: None,
                            tokens: None,
                            raw: serde_json::to_value(&n).ok(),
                        })
                        .await;
                    break;
                }
                KnownServerNotification::ItemStarted(n) => {
                    let _ = ctx.bus.send(OrchestratorEvent::ToolCall {
                        issue_id: ctx.issue_id.clone(),
                        tool: format!("codex.{}", item_kind_label(&n.item)),
                        input: serde_json::to_value(&n).unwrap_or_default(),
                    });
                }
                KnownServerNotification::ItemCompleted(n) => {
                    let _ = ctx.bus.send(OrchestratorEvent::ToolResult {
                        issue_id: ctx.issue_id.clone(),
                        tool: format!("codex.{}", item_kind_label(&n.item)),
                        output: serde_json::to_value(&n).unwrap_or_default(),
                        error: None,
                    });
                }
                KnownServerNotification::AutoApprovalReviewStarted(n) => {
                    pending_reviews.lock().await.insert(
                        n.review_id.clone(),
                        serde_json::to_value(&n.action).unwrap_or_default(),
                    );
                }
                KnownServerNotification::AutoApprovalReviewCompleted(n) => {
                    handle_review_completed(
                        Arc::clone(&client),
                        Arc::clone(&pending_reviews),
                        ctx.bus.clone(),
                        ctx.tx.clone(),
                        ctx.approval_router.clone(),
                        ctx.policy.clone(),
                        ctx.issue_id.clone(),
                        thread_id.clone(),
                        turn_id.clone(),
                        n,
                    )
                    .await;
                }
                KnownServerNotification::Error(n) => {
                    let msg = serde_json::to_value(&n)
                        .ok()
                        .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_string))
                        .unwrap_or_else(|| "codex error".into());
                    error_msg = Some(msg.clone());
                    let _ = ctx
                        .tx
                        .send(AgentEvent {
                            kind: AgentEventKind::TurnEndedWithError,
                            timestamp: Utc::now(),
                            agent_pid: None,
                            thread_id: Some(thread_id.clone()),
                            turn_id: Some(turn_id.clone()),
                            message: Some(msg),
                            tokens: None,
                            raw: serde_json::to_value(&n).ok(),
                        })
                        .await;
                    break;
                }
                other @ (KnownServerNotification::TurnDiffUpdated(_)
                | KnownServerNotification::TurnPlanUpdated(_)
                | KnownServerNotification::FileChangePatchUpdated(_)
                | KnownServerNotification::Warning(_)
                | KnownServerNotification::GuardianWarning(_)
                | KnownServerNotification::DeprecationNotice(_)
                | KnownServerNotification::ConfigWarning(_)
                | KnownServerNotification::McpServerStartupStatusUpdated(_)
                | KnownServerNotification::TokenUsageUpdated(_)
                | KnownServerNotification::RateLimitsUpdated(_)) => {
                    let _ = ctx
                        .tx
                        .send(AgentEvent {
                            kind: AgentEventKind::Notification,
                            timestamp: Utc::now(),
                            agent_pid: None,
                            thread_id: Some(thread_id.clone()),
                            turn_id: Some(turn_id.clone()),
                            message: None,
                            tokens: None,
                            raw: serde_json::to_value(&other).ok(),
                        })
                        .await;
                }
                // Chatty deltas — drop per spec.
                KnownServerNotification::AgentMessageDelta(_) => {}
            },
            ServerNotification::Unknown { method, params } => {
                let _ = ctx
                    .tx
                    .send(AgentEvent {
                        kind: AgentEventKind::Notification,
                        timestamp: Utc::now(),
                        agent_pid: None,
                        thread_id: Some(thread_id.clone()),
                        turn_id: Some(turn_id.clone()),
                        message: Some(method),
                        tokens: None,
                        raw: Some(params),
                    })
                    .await;
            }
        }
    }

    Ok(HarnessOutcome {
        thread_id,
        turn_id,
        success,
        error: error_msg,
    })
}

fn toml_basic_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            ch if ch.is_control() => {
                out.push_str(&format!("\\u{:04X}", ch as u32));
            }
            ch => out.push(ch),
        }
    }
    out
}

fn item_kind_label(_item: &v2::ThreadItem) -> String {
    // ThreadItem is a `oneOf` over many shapes; we surface a coarse label
    // based on the JSON `type` tag when available, falling back to "item".
    if let Ok(v) = serde_json::to_value(_item) {
        if let Some(t) = v.get("type").and_then(|s| s.as_str()) {
            return t.to_string();
        }
        if let Some(t) = v.get("itemType").and_then(|s| s.as_str()) {
            return t.to_string();
        }
    }
    "item".into()
}

#[allow(clippy::too_many_arguments)]
async fn handle_review_completed(
    client: Arc<Client>,
    pending: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    bus: OrchestratorEventBus,
    tx: tokio::sync::mpsc::Sender<AgentEvent>,
    approval_router: ApprovalRouter,
    policy: Policy,
    issue_id: String,
    thread_id: String,
    turn_id: String,
    n: ItemGuardianApprovalReviewCompletedNotification,
) {
    let review_id = n.review_id.clone();
    let action = pending.lock().await.remove(&review_id).unwrap_or_default();
    let status = n.review.status.clone();

    match (policy.permission_mode.clone(), status) {
        (PermissionMode::ReadOnly, _) => {
            let _ = tx
                .send(AgentEvent {
                    kind: AgentEventKind::Notification,
                    timestamp: Utc::now(),
                    agent_pid: None,
                    thread_id: Some(thread_id),
                    turn_id: Some(turn_id),
                    message: Some(format!("guardian_review_completed:{review_id}")),
                    tokens: None,
                    raw: serde_json::to_value(&n).ok(),
                })
                .await;
        }
        (_, GuardianApprovalReviewStatus::Approved) => {
            let _ = tx
                .send(AgentEvent {
                    kind: AgentEventKind::ApprovalAutoApproved,
                    timestamp: Utc::now(),
                    agent_pid: None,
                    thread_id: Some(thread_id),
                    turn_id: Some(turn_id),
                    message: Some(format!("review:{review_id}")),
                    tokens: None,
                    raw: serde_json::to_value(&n).ok(),
                })
                .await;
        }
        (_, GuardianApprovalReviewStatus::Denied) => {
            let pending_approval = approval_router.register(review_id.clone());
            let _ = bus.send(OrchestratorEvent::ApprovalRequest {
                issue_id: issue_id.clone(),
                approval_id: review_id.clone(),
                tool: action
                    .get("type")
                    .and_then(|s| s.as_str())
                    .map(|t| format!("codex.{t}"))
                    .unwrap_or_else(|| "codex.action".into()),
                input: action.clone(),
            });

            let timeout = Duration::from_millis(policy.approval_timeout_ms);
            tokio::spawn(async move {
                let decision = match pending_approval.wait(timeout).await {
                    Ok(d) => d,
                    Err(_) => return,
                };
                let _ = bus.send(OrchestratorEvent::ApprovalDecision {
                    issue_id: issue_id.clone(),
                    approval_id: review_id.clone(),
                    allow: decision.allow,
                    reason: decision.reason.clone(),
                });
                if decision.allow {
                    let event = serde_json::json!({
                        "reviewId": review_id,
                        "action": action,
                    });
                    if let Err(e) = client
                        .thread_approve_guardian_denied_action(
                            ThreadApproveGuardianDeniedActionParams {
                                event,
                                thread_id: thread_id.clone(),
                            },
                        )
                        .await
                    {
                        tracing::warn!(error = %e, "thread/approveGuardianDeniedAction failed");
                    }
                }
            });
        }
        (_, GuardianApprovalReviewStatus::TimedOut | GuardianApprovalReviewStatus::Aborted | GuardianApprovalReviewStatus::InProgress) => {
            let _ = tx
                .send(AgentEvent {
                    kind: AgentEventKind::Notification,
                    timestamp: Utc::now(),
                    agent_pid: None,
                    thread_id: Some(thread_id),
                    turn_id: Some(turn_id),
                    message: Some(format!("guardian_review_terminal:{review_id}")),
                    tokens: None,
                    raw: serde_json::to_value(&n).ok(),
                })
                .await;
        }
    }
}

// Compile-time use of the started-notification type to keep it referenced
// even when only the completed flow is exercised in tests.
#[allow(dead_code)]
fn _started_used(_: ItemGuardianApprovalReviewStartedNotification) {}
