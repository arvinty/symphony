use linear_clone::{build_router_for_test_with_auth, init_pool_for_test};
use serde_json::json;
use tower::ServiceExt;

async fn first_two_issue_ids(pool: &sqlx::SqlitePool) -> (String, String) {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT id FROM issues ORDER BY identifier LIMIT 2")
        .fetch_all(pool)
        .await
        .unwrap();
    assert!(rows.len() >= 2, "need at least 2 seeded issues, got {}", rows.len());
    (rows[0].0.clone(), rows[1].0.clone())
}

async fn gql_with_token(
    app: axum::Router,
    token: Option<&str>,
    body: serde_json::Value,
) -> serde_json::Value {
    let mut req = axum::http::Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let req = req.body(axum::body::Body::from(body.to_string())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn token_only_mutates_bound_issue() {
    let pool = init_pool_for_test().await;
    let (issue_a, issue_b) = first_two_issue_ids(&pool).await;

    let store = linear_clone::auth::TokenStore::default();
    let token = store.issue(&issue_a);

    let app = build_router_for_test_with_auth(pool.clone(), store.clone());
    let ok = gql_with_token(app, Some(&token), json!({
        "query": "mutation($i:ID!,$u:String!){ addAttachment(issueId:$i,url:$u){ id } }",
        "variables": { "i": issue_a, "u": "https://example.com/pr/1" }
    })).await;
    assert!(ok["data"]["addAttachment"]["id"].is_string(), "expected success: {ok}");

    let app = build_router_for_test_with_auth(pool.clone(), store.clone());
    let bad = gql_with_token(app, Some(&token), json!({
        "query": "mutation($i:ID!,$u:String!){ addAttachment(issueId:$i,url:$u){ id } }",
        "variables": { "i": issue_b, "u": "https://example.com/pr/2" }
    })).await;
    let msg = bad["errors"][0]["message"].as_str().unwrap_or("");
    assert!(msg.contains("issue_token_scope"), "expected scope error, got: {bad}");
}

#[tokio::test]
async fn no_token_still_allowed_back_compat() {
    let pool = init_pool_for_test().await;
    let (issue_a, _) = first_two_issue_ids(&pool).await;
    let store = linear_clone::auth::TokenStore::default();
    let app = build_router_for_test_with_auth(pool, store);

    let ok = gql_with_token(app, None, json!({
        "query": "mutation($i:ID!,$u:String!){ addAttachment(issueId:$i,url:$u){ id } }",
        "variables": { "i": issue_a, "u": "https://example.com/pr/3" }
    })).await;
    assert!(ok["data"]["addAttachment"]["id"].is_string(), "anonymous should still work, got: {ok}");
}
