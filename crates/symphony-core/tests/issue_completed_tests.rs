//! Coverage for the IssueCompleted bus event variant. The release-on-success
//! behavior itself is exercised by the local smoke and verified via the absence
//! of re-claim activity in `running` after a successful turn.

use std::time::Duration;
use symphony_core::events::broadcast::{OrchestratorEvent, OrchestratorEventBus};

#[test]
fn issue_completed_serializes_with_kind_tag() {
    let v = serde_json::to_value(OrchestratorEvent::IssueCompleted {
        issue_id: "iss_demo_1".into(),
        identifier: "DEMO-1".into(),
    })
    .unwrap();
    assert_eq!(v["kind"], "issue_completed");
    assert_eq!(v["issue_id"], "iss_demo_1");
    assert_eq!(v["identifier"], "DEMO-1");
}

#[tokio::test]
async fn issue_completed_round_trips_through_bus() {
    let bus = OrchestratorEventBus::new(8);
    let mut rx = bus.subscribe();
    bus.send(OrchestratorEvent::IssueCompleted {
        issue_id: "iss_demo_1".into(),
        identifier: "DEMO-1".into(),
    })
    .unwrap();
    let ev = tokio::time::timeout(Duration::from_millis(200), rx.recv())
        .await
        .unwrap()
        .unwrap();
    match ev {
        OrchestratorEvent::IssueCompleted {
            issue_id,
            identifier,
        } => {
            assert_eq!(issue_id, "iss_demo_1");
            assert_eq!(identifier, "DEMO-1");
        }
        other => panic!("expected IssueCompleted, got {other:?}"),
    }
}
