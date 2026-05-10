use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

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
