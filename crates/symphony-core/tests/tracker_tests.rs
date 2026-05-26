use serde_json::json;
use symphony_core::tracker::file_mock::FileMockTracker;
use symphony_core::tracker::Tracker;

fn write_mock(issues: serde_json::Value) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("symphony_tracker_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("issues.json");
    std::fs::write(&p, serde_json::to_vec_pretty(&issues).unwrap()).unwrap();
    p
}

fn payload() -> serde_json::Value {
    json!({
        "issues": [
            { "id": "a", "identifier": "A-1", "title": "one", "state": "Todo", "priority": 1, "labels": [], "blocked_by": [] },
            { "id": "b", "identifier": "A-2", "title": "two", "state": "In Progress", "priority": 2, "labels": [], "blocked_by": [] },
            { "id": "c", "identifier": "A-3", "title": "three", "state": "Done", "priority": null, "labels": [], "blocked_by": [] }
        ]
    })
}

#[tokio::test]
async fn fetches_active_candidates_only() {
    let p = write_mock(payload());
    let t = FileMockTracker::new(p, vec!["Todo".into(), "In Progress".into()]);
    let cands = t.fetch_candidate_issues().await.unwrap();
    assert_eq!(cands.len(), 2);
    let ids: Vec<_> = cands.into_iter().map(|i| i.id).collect();
    assert!(ids.contains(&"a".to_string()));
    assert!(ids.contains(&"b".to_string()));
}

#[tokio::test]
async fn fetches_terminal_state_filter() {
    let p = write_mock(payload());
    let t = FileMockTracker::new(p, vec!["Todo".into()]);
    let done = t.fetch_issues_by_states(&["Done".into()]).await.unwrap();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].id, "c");
}

#[tokio::test]
async fn fetches_states_by_id() {
    let p = write_mock(payload());
    let t = FileMockTracker::new(p, vec!["Todo".into()]);
    let m = t
        .fetch_issue_states_by_ids(&["a".into(), "c".into()])
        .await
        .unwrap();
    assert_eq!(m.get("a").unwrap(), "Todo");
    assert_eq!(m.get("c").unwrap(), "Done");
    assert!(!m.contains_key("b"));
}
