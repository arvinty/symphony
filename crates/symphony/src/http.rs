use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Router,
};
use chrono::Utc;
use futures::stream::{Stream, StreamExt};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::net::SocketAddr;
use symphony_core::events::broadcast::OrchestratorEvent;
use symphony_core::orchestrator::Orchestrator;
use tokio_stream::wrappers::BroadcastStream;

pub async fn serve(orch: Orchestrator, port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(dashboard))
        .route("/api/v1/state", get(api_state))
        .route("/api/v1/refresh", post(api_refresh))
        .route("/api/v1/events", get(api_events))
        .route("/api/v1/approvals/{approval_id}", post(api_approve))
        .route("/api/v1/{identifier}/open-pr", post(api_retry_pr))
        .route("/api/v1/{identifier}", get(api_issue))
        .with_state(orch);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!(%addr, "symphony_http_listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

async fn dashboard(State(orch): State<Orchestrator>) -> impl IntoResponse {
    let state = orch.snapshot().await;
    let body = format!(
        r#"<!doctype html><html><head><meta charset=utf-8><title>Symphony</title>
<style>
body{{font-family:ui-sans-serif,system-ui;background:#0c0d12;color:#e6e6f0;padding:24px}}
h1,h2{{font-weight:600}}
table{{border-collapse:collapse;width:100%;margin-top:8px}}
th,td{{text-align:left;padding:8px 12px;border-bottom:1px solid #23243a;font-size:13px}}
th{{color:#8e90b0;font-weight:500}}
.tag{{padding:2px 8px;border-radius:999px;background:#23243a;font-size:11px}}
</style></head><body>
<h1>Symphony</h1>
<p>Running: <b>{running}</b> · Retrying: <b>{retrying}</b> · Total tokens: <b>{total}</b> · Runtime (s): <b>{secs:.1}</b></p>
<h2>Running</h2>
<table><tr><th>Issue</th><th>State</th><th>Session</th><th>Last event</th><th>Started</th></tr>
{rows}
</table>
</body></html>"#,
        running = state.running.len(),
        retrying = state.retry_attempts.len(),
        total = state.codex_totals.total_tokens,
        secs = state.codex_totals.seconds_running_completed,
        rows = state
            .running
            .values()
            .map(|r| format!(
                "<tr><td>{}</td><td><span class=tag>{}</span></td><td>{}</td><td>{}</td><td>{}</td></tr>",
                r.issue.identifier,
                r.issue.state,
                r.session.as_ref().map(|s| s.session_id.as_str()).unwrap_or("-"),
                r.session.as_ref().and_then(|s| s.last_event.as_deref()).unwrap_or("-"),
                r.started_at.to_rfc3339(),
            ))
            .collect::<String>(),
    );
    Html(body)
}

async fn api_state(State(orch): State<Orchestrator>) -> Json<Value> {
    let s = orch.snapshot().await;
    let now = Utc::now();
    let mut active_secs = 0.0_f64;
    let running: Vec<Value> = s
        .running
        .values()
        .map(|r| {
            let dur = (now - r.started_at).num_milliseconds().max(0) as f64 / 1000.0;
            active_secs += dur;
            json!({
                "issue_id": r.issue.id,
                "issue_identifier": r.issue.identifier,
                "state": r.issue.state,
                "session_id": r.session.as_ref().map(|s| &s.session_id),
                "turn_count": r.session.as_ref().map(|s| s.turn_count).unwrap_or(0),
                "last_event": r.session.as_ref().and_then(|s| s.last_event.clone()),
                "last_message": r.session.as_ref().and_then(|s| s.last_message.clone()),
                "started_at": r.started_at,
                "last_event_at": r.session.as_ref().and_then(|s| s.last_timestamp),
                "tokens": r.session.as_ref().map(|s| &s.tokens),
            })
        })
        .collect();
    let retrying: Vec<Value> = s
        .retry_attempts
        .values()
        .map(|r| {
            json!({
                "issue_id": r.issue_id,
                "issue_identifier": r.identifier,
                "attempt": r.attempt,
                "due_at_ms": r.due_at_ms,
                "error": r.error,
            })
        })
        .collect();
    Json(json!({
        "generated_at": now,
        "counts": { "running": s.running.len(), "retrying": s.retry_attempts.len() },
        "running": running,
        "retrying": retrying,
        "codex_totals": {
            "input_tokens": s.codex_totals.input_tokens,
            "output_tokens": s.codex_totals.output_tokens,
            "total_tokens": s.codex_totals.total_tokens,
            "seconds_running": s.codex_totals.seconds_running_completed + active_secs,
        },
        "rate_limits": s.codex_rate_limits,
    }))
}

async fn api_issue(
    State(orch): State<Orchestrator>,
    Path(identifier): Path<String>,
) -> impl IntoResponse {
    let s = orch.snapshot().await;
    if let Some((_id, run)) = s.running.iter().find(|(_, r)| r.issue.identifier == identifier) {
        return Json(json!({
            "issue_identifier": run.issue.identifier,
            "issue_id": run.issue.id,
            "status": "running",
            "workspace": { "path": run.workspace_path },
            "running": {
                "session_id": run.session.as_ref().map(|s| &s.session_id),
                "turn_count": run.session.as_ref().map(|s| s.turn_count).unwrap_or(0),
                "state": run.issue.state,
                "started_at": run.started_at,
                "last_event": run.session.as_ref().and_then(|s| s.last_event.clone()),
                "last_message": run.session.as_ref().and_then(|s| s.last_message.clone()),
                "last_event_at": run.session.as_ref().and_then(|s| s.last_timestamp),
                "tokens": run.session.as_ref().map(|s| &s.tokens),
            }
        }))
        .into_response();
    }
    if let Some(retry) = s.retry_attempts.values().find(|r| r.identifier == identifier) {
        return Json(json!({
            "issue_identifier": retry.identifier,
            "issue_id": retry.issue_id,
            "status": "retrying",
            "retry": { "attempt": retry.attempt, "due_at_ms": retry.due_at_ms, "error": retry.error }
        })).into_response();
    }
    (StatusCode::NOT_FOUND, Json(json!({ "error": { "code": "issue_not_found", "message": identifier }}))).into_response()
}

async fn api_refresh(State(_orch): State<Orchestrator>) -> impl IntoResponse {
    // The poll loop already coalesces ticks; a real impl would poke a notify.
    (StatusCode::ACCEPTED, Json(json!({ "queued": true })))
}

async fn api_approve(
    State(orch): State<Orchestrator>,
    Path(approval_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let allow = body.get("allow").and_then(|v| v.as_bool()).unwrap_or(false);
    let reason = body.get("reason").and_then(|v| v.as_str()).map(str::to_string);
    let resolved = orch.approval_router().resolve(&approval_id, allow, reason);
    if resolved {
        (StatusCode::OK, Json(json!({"resolved": true}))).into_response()
    } else {
        (StatusCode::NOT_FOUND, Json(json!({"resolved": false, "reason": "unknown_or_already_resolved"}))).into_response()
    }
}

async fn api_retry_pr(
    State(orch): State<Orchestrator>,
    Path(identifier): Path<String>,
) -> impl IntoResponse {
    match orch.retry_open_pr(&identifier).await {
        Ok(url) => (StatusCode::OK, Json(json!({"url": url}))).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn api_events(
    State(orch): State<Orchestrator>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let issue_filter = q.get("issue").cloned();
    let bus = orch.event_bus();
    let rx = bus.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(move |item| {
        let f = issue_filter.clone();
        async move {
            match item {
                Ok(evt) => {
                    if let Some(want) = f.as_ref() {
                        let issue = match &evt {
                            OrchestratorEvent::AgentEvent { issue_id, .. }
                            | OrchestratorEvent::ToolCall { issue_id, .. }
                            | OrchestratorEvent::ToolResult { issue_id, .. }
                            | OrchestratorEvent::ApprovalRequest { issue_id, .. }
                            | OrchestratorEvent::ApprovalDecision { issue_id, .. }
                            | OrchestratorEvent::VcsPushed { issue_id, .. }
                            | OrchestratorEvent::PrOpened { issue_id, .. }
                            | OrchestratorEvent::VcsError { issue_id, .. } => Some(issue_id.clone()),
                            OrchestratorEvent::Resync => None,
                        };
                        if !matches!(evt, OrchestratorEvent::Resync) && issue.as_deref() != Some(want.as_str()) {
                            return None;
                        }
                    }
                    let data = serde_json::to_string(&evt).unwrap();
                    Some(Ok(Event::default().data(data)))
                }
                Err(_) => Some(Ok(Event::default().event("resync").data("{}"))),
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
