use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::Value;

#[derive(Clone, Default)]
pub struct TokenStore {
    inner: Arc<Mutex<HashMap<String, String>>>, // token -> issue_id
}

impl TokenStore {
    pub fn issue(&self, issue_id: &str) -> String {
        let token = format!("lct_{}", Uuid::new_v4().simple());
        self.inner.lock().unwrap().insert(token.clone(), issue_id.to_string());
        token
    }

    pub fn lookup(&self, token: &str) -> Option<String> {
        self.inner.lock().unwrap().get(token).cloned()
    }
}

#[derive(Clone, Debug)]
pub struct AuthCtx {
    pub bound_issue: Option<String>,
}

pub async fn admin_mint_token(
    State(state): State<crate::AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let want = std::env::var("LINEAR_CLONE_ADMIN_TOKEN").unwrap_or_default();
    let got = headers.get("x-admin-token").and_then(|v| v.to_str().ok()).unwrap_or_default();
    if want.is_empty() || got != want {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let issue_id = body.get("issue_id").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let token = state.token_store.issue(issue_id);
    Ok(Json(serde_json::json!({"token": token})))
}
