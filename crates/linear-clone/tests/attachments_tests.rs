use linear_clone::{build_router_for_test, init_pool_for_test};
use serde_json::json;
use tower::ServiceExt;

async fn gql(app: axum::Router, body: serde_json::Value) -> serde_json::Value {
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn first_issue_id(pool: &sqlx::SqlitePool) -> String {
    sqlx::query_as::<_, (String,)>("SELECT id FROM issues LIMIT 1")
        .fetch_one(pool)
        .await
        .unwrap()
        .0
}

#[tokio::test]
async fn add_attachment_appears_on_issue() {
    let pool = init_pool_for_test().await;
    let issue_id = first_issue_id(&pool).await;

    let app = build_router_for_test(pool.clone());
    let resp = gql(app, json!({
        "query": "mutation($i:ID!,$u:String!,$t:String){ addAttachment(issueId:$i,url:$u,title:$t){ id url title kind } }",
        "variables": {"i": issue_id, "u": "https://github.com/o/r/pull/1", "t": "feat: x"}
    })).await;

    assert_eq!(
        resp["data"]["addAttachment"]["url"],
        "https://github.com/o/r/pull/1"
    );
    assert_eq!(resp["data"]["addAttachment"]["kind"], "pull_request");

    let app = build_router_for_test(pool);
    let q = gql(
        app,
        json!({
            "query": "query($i:ID!){ issue(id:$i){ attachments{ nodes{ url } } } }",
            "variables": {"i": issue_id}
        }),
    )
    .await;
    assert_eq!(
        q["data"]["issue"]["attachments"]["nodes"][0]["url"],
        "https://github.com/o/r/pull/1"
    );
}
