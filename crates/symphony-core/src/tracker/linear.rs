use crate::error::{Result, SymphonyError};
use crate::model::{BlockerRef, Issue};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

const PAGE_SIZE: u32 = 50;
const NETWORK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct LinearTracker {
    endpoint: String,
    api_key: String,
    project_slug: String,
    active_states: Vec<String>,
    client: reqwest::Client,
}

impl LinearTracker {
    pub fn new(
        endpoint: String,
        api_key: String,
        project_slug: String,
        active_states: Vec<String>,
    ) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(NETWORK_TIMEOUT)
            .build()
            .map_err(|e| SymphonyError::LinearApiRequest(e.to_string()))?;
        Ok(Self {
            endpoint,
            api_key,
            project_slug,
            active_states,
            client,
        })
    }

    async fn graphql(&self, query: &str, variables: Value) -> Result<Value> {
        let body = json!({ "query": query, "variables": variables });
        let resp = self
            .client
            .post(&self.endpoint)
            .header("Authorization", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| SymphonyError::LinearApiRequest(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(SymphonyError::LinearApiStatus(status.as_u16()));
        }
        let payload: Value = resp
            .json()
            .await
            .map_err(|e| SymphonyError::LinearApiRequest(e.to_string()))?;
        if let Some(errs) = payload.get("errors") {
            return Err(SymphonyError::LinearGraphqlErrors(errs.to_string()));
        }
        Ok(payload)
    }

    fn parse_issue(node: &Value) -> Option<Issue> {
        let id = node.get("id")?.as_str()?.to_string();
        let identifier = node.get("identifier")?.as_str()?.to_string();
        let title = node.get("title")?.as_str()?.to_string();
        let description = node
            .get("description")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let priority = node
            .get("priority")
            .and_then(|v| v.as_i64())
            .map(|n| n as i32);
        let state = node
            .get("state")
            .and_then(|s| s.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let branch_name = node
            .get("branchName")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let url = node.get("url").and_then(|v| v.as_str()).map(str::to_string);
        let labels: Vec<String> = node
            .get("labels")
            .and_then(|l| l.get("nodes"))
            .and_then(|n| n.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.get("name").and_then(|n| n.as_str()))
                    .map(|s| s.to_lowercase())
                    .collect()
            })
            .unwrap_or_default();
        let blocked_by: Vec<BlockerRef> = node
            .get("inverseRelations")
            .and_then(|i| i.get("nodes"))
            .and_then(|n| n.as_array())
            .map(|a| {
                a.iter()
                    .filter(|r| r.get("type").and_then(|v| v.as_str()) == Some("blocks"))
                    .filter_map(|r| {
                        let issue = r.get("issue")?;
                        Some(BlockerRef {
                            id: issue.get("id").and_then(|v| v.as_str()).map(str::to_string),
                            identifier: issue
                                .get("identifier")
                                .and_then(|v| v.as_str())
                                .map(str::to_string),
                            state: issue
                                .get("state")
                                .and_then(|s| s.get("name"))
                                .and_then(|v| v.as_str())
                                .map(str::to_string),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let created_at = node
            .get("createdAt")
            .and_then(|v| v.as_str())
            .and_then(parse_iso);
        let updated_at = node
            .get("updatedAt")
            .and_then(|v| v.as_str())
            .and_then(parse_iso);
        Some(Issue {
            id,
            identifier,
            title,
            description,
            priority,
            state,
            branch_name,
            url,
            labels,
            blocked_by,
            created_at,
            updated_at,
        })
    }

    async fn fetch_with_states(&self, states: &[String]) -> Result<Vec<Issue>> {
        let query = r#"
        query Issues($slug: String!, $states: [String!]!, $first: Int!, $after: String) {
          issues(
            first: $first,
            after: $after,
            filter: {
              project: { slugId: { eq: $slug } },
              state: { name: { in: $states } }
            }
          ) {
            pageInfo { hasNextPage endCursor }
            nodes {
              id identifier title description priority url branchName createdAt updatedAt
              state { name }
              labels { nodes { name } }
              inverseRelations { nodes { type issue { id identifier state { name } } } }
            }
          }
        }
        "#;
        let mut after: Option<String> = None;
        let mut all: Vec<Issue> = Vec::new();
        loop {
            let vars = json!({
                "slug": self.project_slug,
                "states": states,
                "first": PAGE_SIZE,
                "after": after,
            });
            let payload = self.graphql(query, vars).await?;
            let issues = payload
                .get("data")
                .and_then(|d| d.get("issues"))
                .ok_or(SymphonyError::LinearUnknownPayload)?;
            let nodes = issues
                .get("nodes")
                .and_then(|n| n.as_array())
                .ok_or(SymphonyError::LinearUnknownPayload)?;
            for n in nodes {
                if let Some(i) = Self::parse_issue(n) {
                    all.push(i);
                }
            }
            let page = issues
                .get("pageInfo")
                .ok_or(SymphonyError::LinearUnknownPayload)?;
            let has_next = page
                .get("hasNextPage")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !has_next {
                break;
            }
            let cursor = page
                .get("endCursor")
                .and_then(|v| v.as_str())
                .ok_or(SymphonyError::LinearMissingEndCursor)?;
            after = Some(cursor.to_string());
        }
        Ok(all)
    }
}

fn parse_iso(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

#[async_trait]
impl super::Tracker for LinearTracker {
    async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>> {
        self.fetch_with_states(&self.active_states).await
    }
    async fn fetch_issues_by_states(&self, states: &[String]) -> Result<Vec<Issue>> {
        self.fetch_with_states(states).await
    }
    async fn fetch_issue_states_by_ids(&self, ids: &[String]) -> Result<HashMap<String, String>> {
        let query = r#"
        query States($ids: [ID!]!) {
          issues(filter: { id: { in: $ids } }, first: 250) {
            nodes { id state { name } }
          }
        }
        "#;
        let payload = self.graphql(query, json!({ "ids": ids })).await?;
        let nodes = payload
            .get("data")
            .and_then(|d| d.get("issues"))
            .and_then(|i| i.get("nodes"))
            .and_then(|n| n.as_array())
            .ok_or(SymphonyError::LinearUnknownPayload)?;
        let mut out = HashMap::new();
        for n in nodes {
            let id = n.get("id").and_then(|v| v.as_str());
            let state = n
                .get("state")
                .and_then(|s| s.get("name"))
                .and_then(|v| v.as_str());
            if let (Some(id), Some(state)) = (id, state) {
                out.insert(id.to_string(), state.to_string());
            }
        }
        Ok(out)
    }
}
