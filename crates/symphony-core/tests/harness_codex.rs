//! Integration tests for the Codex harness driven via DuplexStream.
//! A scripted "server" task reads incoming JSON-RPC frames and replies
//! deterministically, mirroring what `codex app-server` would emit.

use codex_client::Client;
use serde_json::{json, Value};

fn fake_thread(id: &str) -> Value {
    json!({
        "cliVersion": "0",
        "createdAt": 0_i64,
        "cwd": "/tmp",
        "ephemeral": true,
        "id": id,
        "modelProvider": "openai",
        "preview": "",
        "sessionId": "session-1",
        "source": "appServer",
        "status": {"type": "idle"},
        "turns": [],
        "updatedAt": 0_i64
    })
}

fn fake_turn(id: &str, status: &str) -> Value {
    json!({
        "id": id,
        "items": [],
        "itemsView": "full",
        "status": status
    })
}
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use symphony_core::config::EffectiveConfig;
use symphony_core::events::broadcast::OrchestratorEventBus;
use symphony_core::events::AgentEventKind;
use symphony_core::harness::approvals::ApprovalRouter;
use symphony_core::harness::codex::run_with_client;
use symphony_core::harness::HarnessContext;
use symphony_core::policy::{PermissionMode, Policy};
use symphony_core::workflow::load_workflow;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

fn write_temp_workflow() -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("symphony_codex_test_{}", uuid::Uuid::new_v4()));
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

/// Read one JSON-RPC frame (one line) from `reader` and return the parsed Value.
async fn read_frame<R: tokio::io::AsyncRead + Unpin>(reader: &mut BufReader<R>) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    serde_json::from_str(line.trim()).expect("valid JSON-RPC frame")
}

async fn write_frame<W: tokio::io::AsyncWrite + Unpin>(writer: &mut W, v: Value) {
    let s = serde_json::to_string(&v).unwrap();
    writer.write_all(s.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();
}

/// Drive a scripted server through initialize → thread/start → turn/start.
/// Reads three requests and writes three minimal responses against the
/// supplied half-streams; returns when turn/start is acknowledged.
async fn handshake<R, W>(reader: &mut BufReader<R>, writer: &mut W)
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    // initialize
    let req = read_frame(reader).await;
    assert_eq!(req["method"], "initialize");
    write_frame(
        writer,
        json!({
            "jsonrpc": "2.0",
            "id": req["id"],
            "result": {
                "protocolVersion": "v1",
                "serverInfo": {"name": "codex-mock", "version": "0"}
            }
        }),
    )
    .await;

    // thread/start
    let req = read_frame(reader).await;
    assert_eq!(req["method"], "thread/start");
    write_frame(
        writer,
        json!({
            "jsonrpc": "2.0",
            "id": req["id"],
            "result": {
                "approvalPolicy": "never",
                "approvalsReviewer": "user",
                "cwd": "/tmp",
                "model": "gpt-5",
                "modelProvider": "openai",
                "sandbox": {"type": "dangerFullAccess"},
                "thread": fake_thread("thread-1")
            }
        }),
    )
    .await;

    // turn/start
    let req = read_frame(reader).await;
    assert_eq!(req["method"], "turn/start");
    write_frame(
        writer,
        json!({
            "jsonrpc": "2.0",
            "id": req["id"],
            "result": {"turn": fake_turn("turn-1", "inProgress")}
        }),
    )
    .await;
}

fn fake_started(review_id: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "item/autoApprovalReview/started",
        "params": {
            "action": {"type": "command", "command": "ls", "cwd": "/tmp", "source": "shell"},
            "review": {"status": "inProgress"},
            "reviewId": review_id,
            "startedAtMs": 0_i64,
            "threadId": "thread-1",
            "turnId": "turn-1"
        }
    })
}

fn fake_completed(review_id: &str, status: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "item/autoApprovalReview/completed",
        "params": {
            "action": {"type": "command", "command": "ls", "cwd": "/tmp", "source": "shell"},
            "completedAtMs": 1_i64,
            "decisionSource": "agent",
            "review": {"status": status},
            "reviewId": review_id,
            "startedAtMs": 0_i64,
            "threadId": "thread-1",
            "turnId": "turn-1"
        }
    })
}

fn fake_turn_completed() -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "turn/completed",
        "params": {
            "threadId": "thread-1",
            "turn": fake_turn("turn-1", "completed")
        }
    })
}

#[tokio::test]
async fn happy_path_runs_to_completion() {
    let (workflow_path, dir) = write_temp_workflow();
    let wf = load_workflow(&workflow_path).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();

    let (server, client) = tokio::io::duplex(16384);
    let (s_r, s_w) = tokio::io::split(server);
    let (c_r, c_w) = tokio::io::split(client);

    let (codex_client, notifs) = Client::from_halves(c_r, c_w);
    let codex_client = Arc::new(codex_client);

    // Scripted server side: respond to handshake + thread/start + turn/start,
    // then push notifications culminating in turn/completed.
    let server_task = tokio::spawn(async move {
        let mut reader = BufReader::new(s_r);
        let mut writer = s_w;

        // initialize
        let req = read_frame(&mut reader).await;
        assert_eq!(req["method"], "initialize");
        write_frame(
            &mut writer,
            json!({
                "jsonrpc": "2.0",
                "id": req["id"],
                "result": {
                    "protocolVersion": "v1",
                    "serverInfo": {"name": "codex-mock", "version": "0"}
                }
            }),
        )
        .await;

        // thread/start
        let req = read_frame(&mut reader).await;
        assert_eq!(req["method"], "thread/start");
        let thread_start_response = json!({
            "approvalPolicy": "never",
            "approvalsReviewer": "user",
            "cwd": "/tmp",
            "model": "gpt-5",
            "modelProvider": "openai",
            "sandbox": {"type": "dangerFullAccess"},
            "thread": fake_thread("thread-1")
        });
        write_frame(
            &mut writer,
            json!({"jsonrpc": "2.0", "id": req["id"], "result": thread_start_response}),
        )
        .await;

        // turn/start
        let req = read_frame(&mut reader).await;
        assert_eq!(req["method"], "turn/start");
        assert_eq!(req["params"]["threadId"], "thread-1");
        let turn_start_response = json!({
            "turn": fake_turn("turn-1", "inProgress")
        });
        write_frame(
            &mut writer,
            json!({"jsonrpc": "2.0", "id": req["id"], "result": turn_start_response}),
        )
        .await;

        // Push turn/completed notification.
        tokio::time::sleep(Duration::from_millis(10)).await;
        write_frame(
            &mut writer,
            json!({
                "jsonrpc": "2.0",
                "method": "turn/completed",
                "params": {
                    "threadId": "thread-1",
                    "turn": fake_turn("turn-1", "completed")
                }
            }),
        )
        .await;
    });

    // Build the HarnessContext.
    let (tx, mut rx) = mpsc::channel(64);
    let bus = OrchestratorEventBus::new(64);
    let approval_router = ApprovalRouter::new();
    let workspace = dir.clone();
    let policy = Policy::default();

    let ctx = HarnessContext {
        workspace: &workspace,
        prompt: "say hi",
        cfg: &cfg,
        tx,
        bus: bus.clone(),
        approval_router,
        policy,
        linear_token: None,
        linear_endpoint: None,
        issue_id: "DEMO-1".into(),
    };

    let outcome = tokio::time::timeout(
        Duration::from_secs(3),
        run_with_client(codex_client, notifs, &ctx),
    )
    .await
    .expect("did not time out")
    .expect("run_with_client succeeded");

    assert!(outcome.success, "expected success, got {outcome:?}");
    assert_eq!(outcome.thread_id, "thread-1");
    assert_eq!(outcome.turn_id, "turn-1");

    // Verify the AgentEvent stream saw TurnStarted then TurnCompleted.
    let mut kinds = vec![];
    while let Ok(Some(ev)) = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await {
        kinds.push(ev.kind);
    }
    assert!(
        kinds.contains(&AgentEventKind::TurnStarted),
        "missing TurnStarted: {kinds:?}"
    );
    assert!(
        kinds.contains(&AgentEventKind::TurnCompleted),
        "missing TurnCompleted: {kinds:?}"
    );

    let _ = bus;
    server_task.await.unwrap();
}

#[tokio::test]
async fn select_harness_routes_codex_name_to_codex_harness() {
    use symphony_core::harness::select_harness;
    let h = select_harness("codex");
    assert_eq!(h.name(), "codex");
    let h = select_harness("codex-app-server");
    assert_eq!(h.name(), "codex");
}

#[tokio::test]
async fn select_harness_returns_unknown_for_other_names() {
    use symphony_core::harness::select_harness;
    let h = select_harness("definitely-not-a-harness");
    assert_eq!(h.name(), "unknown");
}

#[tokio::test]
async fn require_approval_policy_translates_to_untrusted_on_start() {
    // Verifies that a RequireApproval policy is reflected in the approval_policy
    // we send on thread/start. Drives the server far enough to capture the request.
    let (workflow_path, dir) = write_temp_workflow();
    let wf = load_workflow(&workflow_path).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();

    let (server, client) = tokio::io::duplex(16384);
    let (s_r, s_w) = tokio::io::split(server);
    let (c_r, c_w) = tokio::io::split(client);
    let (codex_client, notifs) = Client::from_halves(c_r, c_w);
    let codex_client = Arc::new(codex_client);

    let captured_policy: Arc<tokio::sync::OnceCell<String>> = Arc::new(tokio::sync::OnceCell::new());
    let captured = captured_policy.clone();

    let server_task = tokio::spawn(async move {
        let mut reader = BufReader::new(s_r);
        let mut writer = s_w;

        let req = read_frame(&mut reader).await;
        write_frame(
            &mut writer,
            json!({
                "jsonrpc": "2.0",
                "id": req["id"],
                "result": {"protocolVersion": "v1", "serverInfo": {"name": "m", "version": "0"}}
            }),
        )
        .await;

        let req = read_frame(&mut reader).await;
        // Capture the approvalPolicy field.
        captured
            .set(
                req["params"]["approvalPolicy"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
            )
            .ok();

        // Respond minimally and close.
        write_frame(
            &mut writer,
            json!({
                "jsonrpc": "2.0",
                "id": req["id"],
                "result": {
                    "approvalPolicy": "untrusted",
                    "approvalsReviewer": "user",
                    "cwd": "/tmp",
                    "model": "gpt-5",
                    "modelProvider": "openai",
                    "sandbox": {"type": "readOnly", "networkAccess": false},
                    "thread": fake_thread("t")
                }
            }),
        )
        .await;

        // Force the harness to fail at turn/start by replying with an error.
        let req = read_frame(&mut reader).await;
        write_frame(
            &mut writer,
            json!({
                "jsonrpc": "2.0",
                "id": req["id"],
                "error": {"code": -32000, "message": "stop"}
            }),
        )
        .await;
    });

    let (tx, _rx) = mpsc::channel(64);
    let bus = OrchestratorEventBus::new(64);
    let approval_router = ApprovalRouter::new();
    let workspace = dir.clone();
    let mut policy = Policy::default();
    policy.permission_mode = PermissionMode::RequireApproval;

    let ctx = HarnessContext {
        workspace: &workspace,
        prompt: "hi",
        cfg: &cfg,
        tx,
        bus,
        approval_router,
        policy,
        linear_token: None,
        linear_endpoint: None,
        issue_id: "X".into(),
    };

    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        run_with_client(codex_client, notifs, &ctx),
    )
    .await;

    server_task.await.unwrap();

    let policy = captured_policy.get().cloned().unwrap_or_default();
    assert_eq!(policy, "untrusted", "expected untrusted approvalPolicy");
}

#[tokio::test]
async fn approved_review_emits_approval_auto_approved() {
    let (workflow_path, dir) = write_temp_workflow();
    let wf = load_workflow(&workflow_path).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();

    let (server, client) = tokio::io::duplex(16384);
    let (s_r, s_w) = tokio::io::split(server);
    let (c_r, c_w) = tokio::io::split(client);
    let (codex_client, notifs) = Client::from_halves(c_r, c_w);
    let codex_client = Arc::new(codex_client);

    let server_task = tokio::spawn(async move {
        let mut reader = BufReader::new(s_r);
        let mut writer = s_w;
        handshake(&mut reader, &mut writer).await;

        write_frame(&mut writer, fake_started("rev-1")).await;
        write_frame(&mut writer, fake_completed("rev-1", "approved")).await;
        write_frame(&mut writer, fake_turn_completed()).await;
    });

    let (tx, mut rx) = mpsc::channel(64);
    let bus = OrchestratorEventBus::new(64);
    let approval_router = ApprovalRouter::new();
    let workspace = dir.clone();
    let ctx = HarnessContext {
        workspace: &workspace,
        prompt: "hi",
        cfg: &cfg,
        tx,
        bus,
        approval_router,
        policy: Policy::default(),
        linear_token: None,
        linear_endpoint: None,
        issue_id: "X".into(),
    };

    let outcome = tokio::time::timeout(
        Duration::from_secs(3),
        run_with_client(codex_client, notifs, &ctx),
    )
    .await
    .expect("not timed out")
    .expect("ran");
    assert!(outcome.success);

    let mut saw_auto_approved = false;
    while let Ok(Some(ev)) = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await {
        if ev.kind == AgentEventKind::ApprovalAutoApproved {
            saw_auto_approved = true;
        }
    }
    assert!(
        saw_auto_approved,
        "expected AgentEventKind::ApprovalAutoApproved for an approved review"
    );

    server_task.await.unwrap();
}

#[tokio::test]
async fn denied_review_with_operator_allow_sends_override_rpc() {
    let (workflow_path, dir) = write_temp_workflow();
    let wf = load_workflow(&workflow_path).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();

    let (server, client) = tokio::io::duplex(16384);
    let (s_r, s_w) = tokio::io::split(server);
    let (c_r, c_w) = tokio::io::split(client);
    let (codex_client, notifs) = Client::from_halves(c_r, c_w);
    let codex_client = Arc::new(codex_client);

    let captured_method: Arc<tokio::sync::OnceCell<String>> =
        Arc::new(tokio::sync::OnceCell::new());
    let captured = Arc::clone(&captured_method);

    let server_task = tokio::spawn(async move {
        let mut reader = BufReader::new(s_r);
        let mut writer = s_w;
        handshake(&mut reader, &mut writer).await;

        write_frame(&mut writer, fake_started("rev-2")).await;
        write_frame(&mut writer, fake_completed("rev-2", "denied")).await;

        // Wait for the override RPC. The harness emits it after the operator
        // resolves the approval — we read one more request and capture it.
        let req = tokio::time::timeout(Duration::from_secs(2), read_frame(&mut reader))
            .await
            .expect("override RPC arrived");
        captured
            .set(req["method"].as_str().unwrap_or("").to_string())
            .ok();
        write_frame(
            &mut writer,
            json!({"jsonrpc": "2.0", "id": req["id"], "result": {}}),
        )
        .await;

        write_frame(&mut writer, fake_turn_completed()).await;
    });

    let (tx, _rx) = mpsc::channel(64);
    let bus = OrchestratorEventBus::new(64);
    let approval_router = ApprovalRouter::new();
    let resolver = approval_router.clone();
    let workspace = dir.clone();
    let ctx = HarnessContext {
        workspace: &workspace,
        prompt: "hi",
        cfg: &cfg,
        tx,
        bus,
        approval_router,
        policy: Policy::default(),
        linear_token: None,
        linear_endpoint: None,
        issue_id: "X".into(),
    };

    // Operator side: after a short delay, allow the approval.
    let operator = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        resolver.resolve("rev-2", true, None)
    });

    let outcome = tokio::time::timeout(
        Duration::from_secs(3),
        run_with_client(codex_client, notifs, &ctx),
    )
    .await
    .expect("not timed out")
    .expect("ran");
    assert!(outcome.success, "outcome failed: {:?}", outcome);
    let resolved = operator.await.unwrap();
    assert!(resolved, "approval router should have a waiter for rev-2");

    server_task.await.unwrap();
    let method = captured_method.get().cloned().unwrap_or_default();
    assert_eq!(method, "thread/approveGuardianDeniedAction");
}

#[tokio::test]
async fn denied_review_with_operator_deny_skips_override_rpc() {
    let (workflow_path, dir) = write_temp_workflow();
    let wf = load_workflow(&workflow_path).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();

    let (server, client) = tokio::io::duplex(16384);
    let (s_r, s_w) = tokio::io::split(server);
    let (c_r, c_w) = tokio::io::split(client);
    let (codex_client, notifs) = Client::from_halves(c_r, c_w);
    let codex_client = Arc::new(codex_client);

    let saw_extra_rpc: Arc<tokio::sync::Mutex<bool>> =
        Arc::new(tokio::sync::Mutex::new(false));
    let flag = Arc::clone(&saw_extra_rpc);

    let server_task = tokio::spawn(async move {
        let mut reader = BufReader::new(s_r);
        let mut writer = s_w;
        handshake(&mut reader, &mut writer).await;

        write_frame(&mut writer, fake_started("rev-3")).await;
        write_frame(&mut writer, fake_completed("rev-3", "denied")).await;
        write_frame(&mut writer, fake_turn_completed()).await;

        // Allow some time for an override RPC. If it lands, set the flag.
        let mut line = String::new();
        let read = tokio::time::timeout(
            Duration::from_millis(400),
            reader.read_line(&mut line),
        )
        .await;
        if let Ok(Ok(n)) = read {
            if n > 0 {
                if let Ok(req) = serde_json::from_str::<Value>(line.trim()) {
                    if req["method"] == "thread/approveGuardianDeniedAction" {
                        *flag.lock().await = true;
                    }
                }
            }
        }
    });

    let (tx, _rx) = mpsc::channel(64);
    let bus = OrchestratorEventBus::new(64);
    let approval_router = ApprovalRouter::new();
    let resolver = approval_router.clone();
    let workspace = dir.clone();
    let ctx = HarnessContext {
        workspace: &workspace,
        prompt: "hi",
        cfg: &cfg,
        tx,
        bus,
        approval_router,
        policy: Policy::default(),
        linear_token: None,
        linear_endpoint: None,
        issue_id: "X".into(),
    };

    // Operator denies.
    let _operator = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        resolver.resolve("rev-3", false, Some("nope".into()))
    });

    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        run_with_client(codex_client, notifs, &ctx),
    )
    .await;

    server_task.await.unwrap();
    assert!(
        !*saw_extra_rpc.lock().await,
        "operator deny should not send thread/approveGuardianDeniedAction"
    );
}
