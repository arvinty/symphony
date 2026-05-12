//! Reviewer dispatch behavior. The dispatch arm lives inside
//! `process_run_request` and runs across the orchestrator's full state
//! machine, which already has no integration-level test in this repo.
//! Coverage here:
//!   1. Building blocks (ReviewerConfig::effective_policy, render_reviewer_prompt,
//!      DEFAULT_REVIEWER_PROMPT) — see reviewer_config_tests.rs +
//!      reviewer_prompt_tests.rs.
//!   2. The new ReviewerStarted / ReviewerCompleted bus event variants
//!      serialize cleanly with the `kind` discriminator the SSE consumer
//!      depends on (this file).
//!   3. The new variants round-trip through the broadcast bus to a live
//!      subscriber (this file).

use std::time::Duration;
use symphony_core::events::broadcast::{OrchestratorEvent, OrchestratorEventBus};

#[test]
fn reviewer_started_serializes_with_kind_tag() {
    let v = serde_json::to_value(OrchestratorEvent::ReviewerStarted {
        issue_id: "DEMO-1".into(),
        pr_url: "https://github.com/o/r/pull/77".into(),
    })
    .unwrap();
    assert_eq!(v["kind"], "reviewer_started");
    assert_eq!(v["issue_id"], "DEMO-1");
    assert_eq!(v["pr_url"], "https://github.com/o/r/pull/77");
}

#[test]
fn reviewer_completed_success_serializes() {
    let v = serde_json::to_value(OrchestratorEvent::ReviewerCompleted {
        issue_id: "DEMO-1".into(),
        success: true,
        error: None,
    })
    .unwrap();
    assert_eq!(v["kind"], "reviewer_completed");
    assert_eq!(v["success"], true);
    assert!(v["error"].is_null());
}

#[test]
fn reviewer_completed_failure_carries_error() {
    let v = serde_json::to_value(OrchestratorEvent::ReviewerCompleted {
        issue_id: "DEMO-1".into(),
        success: false,
        error: Some("prompt_render_failed: oops".into()),
    })
    .unwrap();
    assert_eq!(v["success"], false);
    assert_eq!(v["error"], "prompt_render_failed: oops");
}

#[tokio::test]
async fn reviewer_events_round_trip_through_bus() {
    let bus = OrchestratorEventBus::new(16);
    let mut rx = bus.subscribe();

    bus.send(OrchestratorEvent::ReviewerStarted {
        issue_id: "DEMO-1".into(),
        pr_url: "https://x".into(),
    })
    .unwrap();
    bus.send(OrchestratorEvent::ReviewerCompleted {
        issue_id: "DEMO-1".into(),
        success: true,
        error: None,
    })
    .unwrap();

    let ev1 = tokio::time::timeout(Duration::from_millis(200), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let ev2 = tokio::time::timeout(Duration::from_millis(200), rx.recv())
        .await
        .unwrap()
        .unwrap();

    match ev1 {
        OrchestratorEvent::ReviewerStarted { issue_id, pr_url } => {
            assert_eq!(issue_id, "DEMO-1");
            assert_eq!(pr_url, "https://x");
        }
        other => panic!("expected ReviewerStarted, got {other:?}"),
    }
    match ev2 {
        OrchestratorEvent::ReviewerCompleted {
            issue_id,
            success,
            error,
        } => {
            assert_eq!(issue_id, "DEMO-1");
            assert!(success);
            assert!(error.is_none());
        }
        other => panic!("expected ReviewerCompleted, got {other:?}"),
    }
}
