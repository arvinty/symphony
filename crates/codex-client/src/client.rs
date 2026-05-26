use crate::dispatcher::Dispatcher;
use crate::error::{ClientError, ClientResult, RequestId};
use crate::protocol::messages::{ClientRequest, ServerNotification};
use crate::protocol::{v1, v2};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::process::Child;
use tokio::sync::Mutex;

pub type NotificationStream = tokio::sync::mpsc::Receiver<ServerNotification>;

/// Typed Codex `app-server` client.
///
/// `connect` consumes a `tokio::process::Child` (Codex spawned with stdio
/// piped). `Drop` kills the child synchronously. For tests, use
/// `Client::from_halves` to wire the client against in-memory pipes.
pub struct Client {
    next_id: Arc<AtomicU64>,
    writer: Arc<Mutex<Box<dyn AsyncWrite + Send + Unpin>>>,
    dispatcher: Arc<Mutex<Dispatcher>>,
    child: Arc<std::sync::Mutex<Option<Child>>>,
}

impl Client {
    /// Spawn the client from a child process with piped stdin/stdout.
    pub fn connect(mut child: Child) -> ClientResult<(Self, NotificationStream)> {
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ClientError::Io(std::io::Error::other("child stdin missing")))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ClientError::Io(std::io::Error::other("child stdout missing")))?;
        let (client, notifs) = Self::from_halves(stdout, stdin);
        *client.child.lock().unwrap() = Some(child);
        Ok((client, notifs))
    }

    /// Construct from arbitrary async halves. Used by tests; production goes
    /// through `connect`.
    pub fn from_halves<R, W>(reader: R, writer: W) -> (Self, NotificationStream)
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let mut dispatcher = Dispatcher::spawn(reader);
        let notifs = dispatcher.take_notifications().expect("fresh dispatcher");
        let client = Self {
            next_id: Arc::new(AtomicU64::new(1)),
            writer: Arc::new(Mutex::new(Box::new(writer))),
            dispatcher: Arc::new(Mutex::new(dispatcher)),
            child: Arc::new(std::sync::Mutex::new(None)),
        };
        (client, notifs)
    }

    async fn rpc<T: DeserializeOwned>(&self, req: ClientRequest) -> ClientResult<T> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let id_value: RequestId = json!(id);

        // Serialize the typed request, then overlay JSON-RPC envelope fields.
        let mut payload = serde_json::to_value(&req).map_err(|e| ClientError::Decode {
            role: "send",
            source: e,
        })?;
        payload["jsonrpc"] = json!("2.0");
        payload["id"] = id_value.clone();

        // Register waiter *before* writing, so a fast response can't race us.
        let waiter = self.dispatcher.lock().await.register(id_value.clone());

        {
            let mut w = self.writer.lock().await;
            let s = serde_json::to_string(&payload).map_err(|e| ClientError::Decode {
                role: "send",
                source: e,
            })?;
            w.write_all(s.as_bytes()).await?;
            w.write_all(b"\n").await?;
            w.flush().await?;
        }

        let v = waiter
            .await
            .map_err(|_| ClientError::OneshotDropped(id_value))??;

        serde_json::from_value(v).map_err(|e| ClientError::Decode {
            role: "response",
            source: e,
        })
    }

    pub async fn initialize(
        &self,
        params: v1::InitializeParams,
    ) -> ClientResult<v1::InitializeResponse> {
        self.rpc(ClientRequest::Initialize(params)).await
    }

    pub async fn start_thread(
        &self,
        params: v2::ThreadStartParams,
    ) -> ClientResult<v2::ThreadStartResponse> {
        self.rpc(ClientRequest::ThreadStart(params)).await
    }

    pub async fn start_turn(
        &self,
        params: v2::TurnStartParams,
    ) -> ClientResult<v2::TurnStartResponse> {
        self.rpc(ClientRequest::TurnStart(params)).await
    }

    pub async fn interrupt(&self, thread_id: String, turn_id: String) -> ClientResult<()> {
        let _v: Value = self
            .rpc(ClientRequest::TurnInterrupt(v2::TurnInterruptParams {
                thread_id,
                turn_id,
            }))
            .await?;
        Ok(())
    }

    pub async fn thread_approve_guardian_denied_action(
        &self,
        params: v2::ThreadApproveGuardianDeniedActionParams,
    ) -> ClientResult<v2::ThreadApproveGuardianDeniedActionResponse> {
        self.rpc(ClientRequest::ThreadApproveGuardianDeniedAction(params))
            .await
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(mut c) = guard.take() {
                let _ = c.start_kill();
            }
        }
    }
}
