use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;

#[derive(Debug, Clone)]
pub struct Decision {
    pub allow: bool,
    pub reason: Option<String>,
}

pub struct PendingApproval {
    rx: oneshot::Receiver<Decision>,
}

impl PendingApproval {
    pub async fn wait(self, timeout: Duration) -> anyhow::Result<Decision> {
        match tokio::time::timeout(timeout, self.rx).await {
            Ok(Ok(d)) => Ok(d),
            Ok(Err(_)) => Ok(Decision { allow: false, reason: Some("router_dropped".into()) }),
            Err(_) => Ok(Decision { allow: false, reason: Some("timeout".into()) }),
        }
    }
}

#[derive(Default, Clone)]
pub struct ApprovalRouter {
    inner: Arc<Mutex<HashMap<String, oneshot::Sender<Decision>>>>,
}

impl ApprovalRouter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, id: String) -> PendingApproval {
        let (tx, rx) = oneshot::channel();
        self.inner.lock().unwrap().insert(id, tx);
        PendingApproval { rx }
    }

    pub fn resolve(&self, id: &str, allow: bool, reason: Option<String>) -> bool {
        if let Some(tx) = self.inner.lock().unwrap().remove(id) {
            tx.send(Decision { allow, reason }).is_ok()
        } else {
            false
        }
    }
}
