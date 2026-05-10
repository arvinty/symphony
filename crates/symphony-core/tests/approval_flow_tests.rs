use std::time::Duration;
use symphony_core::harness::approvals::ApprovalRouter;

#[tokio::test]
async fn approve_resolves_decision() {
    let router = ApprovalRouter::new();
    let id = "ap_1".to_string();
    let pending = router.register(id.clone());
    assert!(router.resolve(&id, true, None));
    let decision = pending.wait(Duration::from_millis(500)).await.unwrap();
    assert!(decision.allow);
}

#[tokio::test]
async fn timeout_defaults_deny() {
    let router = ApprovalRouter::new();
    let id = "ap_2".to_string();
    let pending = router.register(id.clone());
    let decision = pending.wait(Duration::from_millis(50)).await.unwrap();
    assert!(!decision.allow);
    assert_eq!(decision.reason.as_deref(), Some("timeout"));
}

#[tokio::test]
async fn http_resolves_approval_via_router() {
    let router = ApprovalRouter::new();
    let pending = router.register("ap_x".into());
    assert!(router.resolve("ap_x", true, Some("operator".into())));
    let d = pending.wait(Duration::from_millis(200)).await.unwrap();
    assert!(d.allow);
    assert_eq!(d.reason.as_deref(), Some("operator"));
}
