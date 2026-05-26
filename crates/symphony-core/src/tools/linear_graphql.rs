use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Clone)]
pub struct LinearGraphqlClient {
    endpoint: String,
    token: String,
    client: reqwest::Client,
}

impl LinearGraphqlClient {
    pub fn new(endpoint: String, token: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("reqwest client");
        Self {
            endpoint,
            token,
            client,
        }
    }

    pub async fn get_issue(&self, id: &str) -> Result<Value> {
        let q = "query($i:ID!){ issue(id:$i){ id identifier title description state{ name } comments{ nodes{ id body createdAt } } } }";
        self.run(q, json!({"i": id}))
            .await
            .map(|v| v["issue"].clone())
    }

    pub async fn list_comments(&self, id: &str) -> Result<Value> {
        let q = "query($i:ID!){ issue(id:$i){ comments{ nodes{ id body createdAt } } } }";
        self.run(q, json!({"i": id}))
            .await
            .map(|v| v["issue"]["comments"]["nodes"].clone())
    }

    pub async fn add_comment(&self, issue_id: &str, body: &str) -> Result<String> {
        let q = "mutation($i:ID!,$b:String!){ addComment(issueId:$i, body:$b){ id } }";
        let v = self.run(q, json!({"i": issue_id, "b": body})).await?;
        v["addComment"]["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow!("addComment.id missing"))
    }

    pub async fn link_pull_request(
        &self,
        issue_id: &str,
        url: &str,
        title: Option<&str>,
    ) -> Result<String> {
        let q = "mutation($i:ID!,$u:String!,$t:String){ addAttachment(issueId:$i,url:$u,title:$t){ id } }";
        let v = self
            .run(q, json!({"i": issue_id, "u": url, "t": title}))
            .await?;
        v["addAttachment"]["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow!("addAttachment.id missing"))
    }

    async fn run(&self, query: &str, variables: Value) -> Result<Value> {
        let body = json!({"query": query, "variables": variables});
        let mut delay_ms = 200u64;
        let mut last_err = None;
        for _ in 0..3 {
            let resp = self
                .client
                .post(&self.endpoint)
                .bearer_auth(&self.token)
                .json(&body)
                .send()
                .await;
            match resp {
                Ok(r) => {
                    let status = r.status();
                    if status.is_success() {
                        let v: Value = r.json().await?;
                        if let Some(errs) = v.get("errors") {
                            return Err(anyhow!("graphql_errors: {}", errs));
                        }
                        return Ok(v["data"].clone());
                    }
                    if status.as_u16() < 500 {
                        return Err(anyhow!(
                            "http {}: {}",
                            status,
                            r.text().await.unwrap_or_default()
                        ));
                    }
                    last_err = Some(anyhow!("http {}", status));
                }
                Err(e) => {
                    last_err = Some(anyhow!(e));
                }
            }
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            delay_ms = (delay_ms * 2).min(2_000);
        }
        Err(last_err.unwrap_or_else(|| anyhow!("retry_exhausted")))
    }
}
