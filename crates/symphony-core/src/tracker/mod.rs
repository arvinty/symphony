use crate::error::Result;
use crate::model::Issue;
use async_trait::async_trait;
use std::collections::HashMap;

pub mod linear;
pub mod file_mock;

#[async_trait]
pub trait Tracker: Send + Sync {
    async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>>;
    async fn fetch_issues_by_states(&self, states: &[String]) -> Result<Vec<Issue>>;
    /// Returns map issue_id -> current state name. Missing IDs are absent.
    async fn fetch_issue_states_by_ids(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, String>>;
}
