use linear_clone::{build_router_for_test, init_pool_for_test};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn graphql_lists_seeded_issues() {
    let pool = init_pool_for_test().await;
    let app = build_router_for_test(pool);

    let body = json!({
        "query": "{ issues(first: 100) { nodes { id identifier title state { name } } } }"
    });

    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body.to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let nodes = v["data"]["issues"]["nodes"].as_array().expect("nodes array");
    assert!(nodes.len() >= 4, "expected seeded issues, got {}", nodes.len());
    let ids: Vec<&str> = nodes.iter().map(|n| n["identifier"].as_str().unwrap()).collect();
    assert!(ids.contains(&"ENG-1"));
}

#[tokio::test]
async fn graphql_filters_by_project_slug_and_state() {
    let pool = init_pool_for_test().await;
    let app = build_router_for_test(pool);

    let body = json!({
        "query": "query($f: IssueFilter) { issues(first: 100, filter: $f) { nodes { identifier state { name } project { slugId } } } }",
        "variables": {
            "f": { "project": { "slugId": { "eq": "symphony" } }, "state": { "name": { "in": ["Todo"] } } }
        }
    });
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body.to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let nodes = v["data"]["issues"]["nodes"].as_array().unwrap();
    assert!(!nodes.is_empty());
    for n in nodes {
        assert_eq!(n["state"]["name"], "Todo");
        assert_eq!(n["project"]["slugId"], "symphony");
    }
}

#[tokio::test]
async fn update_issue_state_mutation() {
    let pool = init_pool_for_test().await;
    let app = build_router_for_test(pool);

    let body = json!({
        "query": "mutation($id: ID!, $sid: ID!) { updateIssueState(id: $id, stateId: $sid) }",
        "variables": { "id": "iss_1", "sid": "s_done" }
    });
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body.to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["data"]["updateIssueState"], true);
}
