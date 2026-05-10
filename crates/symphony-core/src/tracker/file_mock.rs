use crate::error::{Result, SymphonyError};
use crate::model::Issue;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Simple JSON-file backed tracker for local dev/testing.
///
/// File shape:
/// ```json
/// { "issues": [ { ...Issue... } ] }
/// ```
#[derive(Clone)]
pub struct FileMockTracker {
    path: PathBuf,
    active_states: Vec<String>,
    cache: Arc<Mutex<Option<Vec<Issue>>>>,
}

impl FileMockTracker {
    pub fn new(path: PathBuf, active_states: Vec<String>) -> Self {
        Self {
            path,
            active_states,
            cache: Arc::new(Mutex::new(None)),
        }
    }

    async fn read_all(&self) -> Result<Vec<Issue>> {
        let raw = tokio::fs::read_to_string(&self.path)
            .await
            .map_err(SymphonyError::Io)?;
        let parsed: serde_json::Value =
            serde_json::from_str(&raw).map_err(|e| SymphonyError::Other(e.to_string()))?;
        let arr = parsed
            .get("issues")
            .and_then(|v| v.as_array())
            .ok_or(SymphonyError::LinearUnknownPayload)?;
        let mut out = Vec::with_capacity(arr.len());
        for v in arr {
            let issue: Issue = serde_json::from_value(v.clone())
                .map_err(|e| SymphonyError::Other(e.to_string()))?;
            out.push(issue);
        }
        *self.cache.lock().await = Some(out.clone());
        Ok(out)
    }
}

#[async_trait]
impl super::Tracker for FileMockTracker {
    async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>> {
        let all = self.read_all().await?;
        let lower: Vec<String> = self
            .active_states
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        Ok(all
            .into_iter()
            .filter(|i| lower.contains(&i.state.to_lowercase()))
            .collect())
    }

    async fn fetch_issues_by_states(&self, states: &[String]) -> Result<Vec<Issue>> {
        let all = self.read_all().await?;
        let lower: Vec<String> = states.iter().map(|s| s.to_lowercase()).collect();
        Ok(all
            .into_iter()
            .filter(|i| lower.contains(&i.state.to_lowercase()))
            .collect())
    }

    async fn fetch_issue_states_by_ids(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, String>> {
        let all = self.read_all().await?;
        let mut map = HashMap::new();
        for i in all {
            if ids.contains(&i.id) {
                map.insert(i.id, i.state);
            }
        }
        Ok(map)
    }
}
