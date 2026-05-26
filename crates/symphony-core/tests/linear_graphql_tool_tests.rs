use serde_json::json;
use symphony_core::tools::linear_graphql::LinearGraphqlClient;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn add_comment_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"addComment": {"id": "c1"}}
        })))
        .mount(&server)
        .await;

    let cli = LinearGraphqlClient::new(format!("{}/graphql", server.uri()), "tok".into());
    let id = cli.add_comment("iss_1", "hello").await.unwrap();
    assert_eq!(id, "c1");
}

#[tokio::test]
async fn retries_5xx_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"addComment": {"id": "c2"}}
        })))
        .mount(&server)
        .await;

    let cli = LinearGraphqlClient::new(format!("{}/graphql", server.uri()), "tok".into());
    let id = cli.add_comment("iss_1", "hi").await.unwrap();
    assert_eq!(id, "c2");
}

#[tokio::test]
async fn forbidden_is_not_retried() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server)
        .await;

    let cli = LinearGraphqlClient::new(format!("{}/graphql", server.uri()), "tok".into());
    let err = cli.add_comment("iss_1", "hi").await.err().unwrap();
    assert!(err.to_string().contains("403"));
}
