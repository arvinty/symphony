use symphony_core::events::broadcast::{OrchestratorEvent, OrchestratorEventBus};

#[tokio::test]
async fn approval_request_round_trips_through_bus() {
    let bus = OrchestratorEventBus::new(64);
    let mut sub = bus.subscribe();

    let evt = OrchestratorEvent::ApprovalRequest {
        issue_id: "iss_1".into(),
        approval_id: "ap_1".into(),
        tool: "linear_graphql.add_comment".into(),
        input: serde_json::json!({"body":"hi"}),
    };
    bus.send(evt.clone()).unwrap();

    let got = sub.recv().await.unwrap();
    assert_eq!(
        serde_json::to_value(&got).unwrap(),
        serde_json::to_value(&evt).unwrap()
    );
}
