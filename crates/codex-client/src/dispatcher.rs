use crate::error::{ClientError, ClientResult, RequestId};
use crate::protocol::messages::ServerNotification;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::{mpsc, oneshot};

type WaiterMap = Arc<Mutex<HashMap<String, oneshot::Sender<ClientResult<Value>>>>>;

/// Owns the read loop on a transport. Routes incoming frames to either a
/// per-id oneshot waiter (responses) or a typed notification channel.
///
/// On EOF or fatal error: drains the waiter map (each pending oneshot
/// resolves `Err(TransportClosed)`), closes the notification channel,
/// exits. `Drop` aborts the loop task even if the transport is still open.
pub struct Dispatcher {
    waiters: WaiterMap,
    notif_rx: Option<mpsc::Receiver<ServerNotification>>,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl Dispatcher {
    pub fn spawn<R>(reader: R) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        let waiters: WaiterMap = Arc::new(Mutex::new(HashMap::new()));
        let waiters_clone = waiters.clone();
        let (notif_tx, notif_rx) = mpsc::channel::<ServerNotification>(256);

        let join = tokio::spawn(async move {
            let mut buf_reader = BufReader::new(reader);
            let mut line = String::new();
            loop {
                line.clear();
                let n = match buf_reader.read_line(&mut line).await {
                    Ok(n) => n,
                    Err(e) => {
                        tracing::debug!(error = %e, "dispatcher read error");
                        break;
                    }
                };
                if n == 0 {
                    tracing::debug!("dispatcher EOF");
                    break;
                }

                let v: Value = match serde_json::from_str(line.trim_end_matches(['\r', '\n'])) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(error = %e, line = %line.trim(), "malformed JSON frame");
                        continue;
                    }
                };

                if v.get("id").is_some() && v.get("method").is_none() {
                    // Response — route to waiter by id.
                    let id_key = serde_json::to_string(&v["id"]).unwrap_or_default();
                    let tx = waiters_clone.lock().unwrap().remove(&id_key);
                    match tx {
                        Some(tx) => {
                            let payload = if let Some(err) = v.get("error") {
                                let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
                                let message = err
                                    .get("message")
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                Err(ClientError::JsonRpc { code, message })
                            } else {
                                Ok(v.get("result").cloned().unwrap_or(Value::Null))
                            };
                            let _ = tx.send(payload);
                        }
                        None => {
                            tracing::warn!(?v, "response with no matching waiter");
                        }
                    }
                } else if v.get("method").is_some() && v.get("id").is_none() {
                    // Notification.
                    match serde_json::from_value::<ServerNotification>(v.clone()) {
                        Ok(n) => {
                            if notif_tx.send(n).await.is_err() {
                                tracing::debug!("notification consumer dropped");
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, ?v, "notification decode failed");
                        }
                    }
                } else if v.get("method").is_some() && v.get("id").is_some() {
                    // Server-to-client request — not used in slice 2.
                    tracing::warn!(?v, "ignoring server request (unsupported in slice 2)");
                } else {
                    tracing::warn!(?v, "frame matched neither response nor notification");
                }
            }
            // Cleanup: drain waiter map.
            let mut map = waiters_clone.lock().unwrap();
            for (_, tx) in map.drain() {
                let _ = tx.send(Err(ClientError::TransportClosed));
            }
        });

        Self {
            waiters,
            notif_rx: Some(notif_rx),
            join: Some(join),
        }
    }

    pub fn register(&self, id: RequestId) -> oneshot::Receiver<ClientResult<Value>> {
        let (tx, rx) = oneshot::channel();
        let key = serde_json::to_string(&id).unwrap_or_default();
        self.waiters.lock().unwrap().insert(key, tx);
        rx
    }

    /// Take ownership of the notification stream. Returns `None` if already taken.
    pub fn take_notifications(&mut self) -> Option<mpsc::Receiver<ServerNotification>> {
        self.notif_rx.take()
    }
}

impl Drop for Dispatcher {
    fn drop(&mut self) {
        if let Some(join) = self.join.take() {
            join.abort();
        }
        let mut map = self.waiters.lock().unwrap();
        for (_, tx) in map.drain() {
            let _ = tx.send(Err(ClientError::TransportClosed));
        }
    }
}
