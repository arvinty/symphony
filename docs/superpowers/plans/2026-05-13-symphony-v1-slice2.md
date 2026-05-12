# Symphony v1.0 Slice 2 — Codex App-Server End-to-End Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `codex_stub` with a real Codex `app-server` v2 JSON-RPC client end-to-end. New crate `crates/codex-client` (transport + dispatcher + typed protocol). New `symphony-core::harness::codex` adapter. Reuses `Policy`, `OrchestratorEventBus`, `ApprovalRouter`, `mcp_bridge`, and the VCS pipeline unchanged.

**Architecture:** Three sequential commit stages on `v1-slice2`:
1. **Types** — `codex-client` crate scaffolding, hand-rolled v1::Initialize, typify-generated v2 protocol, hand-rolled method-dispatch enums.
2. **Transport + correlation** — `StdioTransport` (newline-delimited JSON over child stdio) + `Dispatcher` (oneshot waiter map + notification channel).
3. **Client API + harness** — typed `Client` methods + `CodexHarness` in `symphony-core` replacing `codex_stub`.

**Tech Stack:** Rust (tokio 1, serde 1, thiserror, futures, tracing). Build-time: `typify` over the committed v2 schema JSON. Test transport: `tokio::io::DuplexStream`.

---

## File Structure

**Created:**
- `crates/codex-client/Cargo.toml`
- `crates/codex-client/build.rs`
- `crates/codex-client/src/lib.rs`
- `crates/codex-client/src/error.rs`
- `crates/codex-client/src/protocol/mod.rs`
- `crates/codex-client/src/protocol/v1.rs`
- `crates/codex-client/src/protocol/messages.rs`
- `crates/codex-client/src/transport.rs`
- `crates/codex-client/src/dispatcher.rs`
- `crates/codex-client/src/client.rs`
- `crates/codex-client/tests/protocol_roundtrip.rs`
- `crates/codex-client/tests/transport_tests.rs`
- `crates/codex-client/tests/dispatcher_tests.rs`
- `crates/codex-client/tests/client_tests.rs`
- `crates/symphony-core/src/harness/codex.rs`
- `crates/symphony-core/tests/harness_codex.rs`
- `crates/symphony-core/tests/slice2_smoke.rs`

**Modified:**
- `Cargo.toml` (workspace) — add `codex-client` member, add `typify` build dep.
- `crates/symphony-core/Cargo.toml` — add `codex-client` dep.
- `crates/symphony-core/src/harness/mod.rs` — `pub mod codex;`, delete `pub mod codex_stub;`, update `select_harness`.
- `crates/symphony-core/src/policy.rs` — add `translate_codex_permissions` (private to crate).
- `crates/symphony-core/src/error.rs` — add `UnknownHarness(String)` variant.

**Deleted:**
- `crates/symphony-core/src/harness/codex_stub.rs`

---

## Task 1: Bootstrap `crates/codex-client` crate

**Files:**
- Create: `crates/codex-client/Cargo.toml`
- Create: `crates/codex-client/src/lib.rs`
- Create: `crates/codex-client/src/error.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Add the workspace member**

In root `Cargo.toml`, append to `members`:

```toml
members = [
    "crates/symphony",
    "crates/symphony-core",
    "crates/linear-clone",
    "crates/codex-client",
]
```

- [ ] **Step 2: Create `crates/codex-client/Cargo.toml`**

```toml
[package]
name = "codex-client"
version.workspace = true
edition.workspace = true
license.workspace = true
build = "build.rs"

[dependencies]
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
futures.workspace = true
tracing.workspace = true
async-trait.workspace = true

[build-dependencies]
typify = "0.4"
serde_json.workspace = true
```

(Verify `typify = "0.4"` is the latest at implementation time and adjust if needed.)

- [ ] **Step 3: Scaffold `src/lib.rs`**

```rust
//! Codex `app-server` JSON-RPC v2 client.
//!
//! Stages are layered: `protocol` (types) → `transport` (framed I/O) →
//! `dispatcher` (correlation) → `client` (typed API).

pub mod error;
pub mod protocol;
pub mod transport;
pub mod dispatcher;
pub mod client;

pub use client::{Client, NotificationStream};
pub use error::{ClientError, ClientResult};
```

Modules `protocol`, `transport`, `dispatcher`, `client` will be empty placeholders this task (single `// stub` comment each) — later tasks fill them in.

- [ ] **Step 4: Implement `src/error.rs`**

```rust
use serde_json::Value;
use thiserror::Error;

pub type RequestId = Value;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("transport closed")]
    TransportClosed,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid json on {role}: {source}")]
    Decode {
        role: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("server error {code}: {message}")]
    JsonRpc { code: i64, message: String },
    #[error("request {0:?} dropped before response")]
    OneshotDropped(RequestId),
    #[error("server sent unknown method: {0}")]
    UnknownMethod(String),
}

pub type ClientResult<T> = Result<T, ClientError>;
```

- [ ] **Step 5: Verify it builds**

Run: `cargo check -p codex-client`
Expected: clean compile.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/codex-client/
git commit -m "Scaffold codex-client crate with error types"
```

---

## Task 2: Protocol stage — v1 Initialize + typify build script

**Files:**
- Create: `crates/codex-client/build.rs`
- Create: `crates/codex-client/src/protocol/mod.rs`
- Create: `crates/codex-client/src/protocol/v1.rs`

- [ ] **Step 1: Write `build.rs` that runs typify on the v2 schema**

`crates/codex-client/build.rs`:

```rust
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let schema_path = manifest_dir
        .parent().unwrap()
        .parent().unwrap()
        .join("docs/codex-protocol/codex_app_server_protocol.v2.schemas.json");

    println!("cargo:rerun-if-changed={}", schema_path.display());

    if !schema_path.exists() {
        panic!(
            "codex v2 schema not found at {}; regenerate via `codex app-server generate-json-schema --out docs/codex-protocol/`",
            schema_path.display()
        );
    }

    let schema_json = fs::read_to_string(&schema_path)
        .expect("read codex v2 schema");
    let schema: schemars::schema::RootSchema = serde_json::from_str(&schema_json)
        .expect("parse codex v2 schema");

    let mut settings = typify::TypeSpaceSettings::default();
    settings.with_derive("Clone".to_string()).with_derive("Debug".to_string());
    let mut type_space = typify::TypeSpace::new(&settings);
    type_space.add_root_schema(schema).expect("typify add_root_schema");

    let contents = format!(
        "#![allow(clippy::all, dead_code, non_snake_case)]\n\n{}",
        type_space.to_stream()
    );

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out = out_dir.join("v2.rs");
    fs::write(&out, contents).expect("write v2.rs");
}
```

(`schemars` may need to be a build-dependency depending on typify's API at the version in use; verify and add to `Cargo.toml` `[build-dependencies]` if so.)

- [ ] **Step 2: Create `src/protocol/mod.rs`**

```rust
pub mod v1;
pub mod v2 {
    include!(concat!(env!("OUT_DIR"), "/v2.rs"));
}
pub mod messages; // populated in Task 3
```

- [ ] **Step 3: Create `src/protocol/v1.rs` with hand-rolled Initialize types**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: String,
    pub client_info: ClientInfo,
    #[serde(default)]
    pub capabilities: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResponse {
    pub protocol_version: String,
    pub server_info: ServerInfo,
    #[serde(default)]
    pub capabilities: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}
```

(Field names match the existing `codex-cli` v1 handshake from the live probe; cross-check against `docs/codex-protocol/v1/InitializeParams.json` and `InitializeResponse.json` and adjust.)

- [ ] **Step 4: Stub `src/protocol/messages.rs`**

```rust
// Filled in Task 3.
```

- [ ] **Step 5: Verify the build generates v2**

Run: `cargo check -p codex-client -v 2>&1 | grep -E "(typify|v2.rs|error)"`
Expected: typify ran; `OUT_DIR/.../v2.rs` was written; no errors.
Run: `cargo check -p codex-client` → clean.

- [ ] **Step 6: Commit**

```bash
git add crates/codex-client/build.rs crates/codex-client/Cargo.toml crates/codex-client/src/protocol/
git commit -m "Generate v2 protocol from JSON schema; hand-roll v1 Initialize"
```

---

## Task 3: Protocol stage — JSON-RPC envelope + method-dispatch enums

**Files:**
- Modify: `crates/codex-client/src/protocol/messages.rs`
- Create: `crates/codex-client/tests/protocol_roundtrip.rs`

- [ ] **Step 1: Write failing roundtrip tests**

`crates/codex-client/tests/protocol_roundtrip.rs`:

```rust
use codex_client::protocol::messages::{ClientRequest, JsonRpcMessage, ServerNotification};
use serde_json::json;

#[test]
fn jsonrpc_message_envelope_request() {
    let raw = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {"protocolVersion": "v1", "clientInfo": {"name": "symphony", "version": "0.1.0"}}
    });
    let msg: JsonRpcMessage = serde_json::from_value(raw.clone()).unwrap();
    let JsonRpcMessage::Request { id, method, .. } = msg else { panic!("expected Request") };
    assert_eq!(method, "initialize");
    assert_eq!(id, json!(1));
}

#[test]
fn jsonrpc_message_envelope_notification() {
    let raw = json!({
        "jsonrpc": "2.0",
        "method": "turn/started",
        "params": {"turnId": "t1"}
    });
    let msg: JsonRpcMessage = serde_json::from_value(raw.clone()).unwrap();
    matches!(msg, JsonRpcMessage::Notification { .. });
}

#[test]
fn server_notification_decodes_known_methods() {
    let raw = json!({"method": "turn/completed", "params": {}});
    let n: ServerNotification = serde_json::from_value(raw).unwrap();
    matches!(n, ServerNotification::TurnCompleted(_));
}

#[test]
fn server_notification_unknown_method_falls_through() {
    let raw = json!({"method": "future/method", "params": {"some": "shape"}});
    let n: ServerNotification = serde_json::from_value(raw).unwrap();
    matches!(n, ServerNotification::Unknown { .. });
}

#[test]
fn client_request_serializes_to_jsonrpc() {
    let req = ClientRequest::TurnInterrupt(codex_client::protocol::v2::TurnInterruptParams {
        thread_id: "t1".into(),
        turn_id: "u1".into(),
    });
    let v = serde_json::to_value(&req).unwrap();
    assert_eq!(v["method"], "turn/interrupt");
    assert!(v["params"].is_object());
}
```

- [ ] **Step 2: Run tests to verify failures**

Run: `cargo test -p codex-client --test protocol_roundtrip`
Expected: compile errors — `JsonRpcMessage`, `ClientRequest`, `ServerNotification` do not exist.

- [ ] **Step 3: Implement `messages.rs`**

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::error::RequestId;
use super::{v1, v2};

/// JSON-RPC 2.0 message envelope — parsed first to discriminate request /
/// response / notification before deeper typing.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    Request {
        jsonrpc: String,
        id: RequestId,
        method: String,
        #[serde(default)]
        params: Value,
    },
    Response {
        jsonrpc: String,
        id: RequestId,
        #[serde(flatten)]
        result: JsonRpcResult,
    },
    Notification {
        jsonrpc: String,
        method: String,
        #[serde(default)]
        params: Value,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub enum JsonRpcResult {
    #[serde(rename = "result")] Ok(Value),
    #[serde(rename = "error")]  Err(JsonRpcError),
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

/// Requests the client (us) sends to the server.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "method", content = "params", rename_all = "kebab-case")]
pub enum ClientRequest {
    #[serde(rename = "initialize")]                          Initialize(v1::InitializeParams),
    #[serde(rename = "turn/start")]                          TurnStart(v2::TurnStartParams),
    #[serde(rename = "turn/interrupt")]                      TurnInterrupt(v2::TurnInterruptParams),
    #[serde(rename = "thread/approveGuardianDeniedAction")]  ThreadApproveGuardianDeniedAction(v2::ThreadApproveGuardianDeniedActionParams),
}

/// Notifications the server emits.
///
/// Only variants we handle are enumerated; everything else falls through to
/// `Unknown { method, params }` so the dispatcher can log + preserve raw
/// payload without dropping frames.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum ServerNotification {
    #[serde(rename = "turn/started")]                           TurnStarted(v2::TurnStartedNotification),
    #[serde(rename = "turn/completed")]                         TurnCompleted(v2::TurnCompletedNotification),
    #[serde(rename = "turn/diff/updated")]                      TurnDiffUpdated(v2::TurnDiffUpdatedNotification),
    #[serde(rename = "turn/plan/updated")]                      TurnPlanUpdated(v2::TurnPlanUpdatedNotification),
    #[serde(rename = "item/started")]                           ItemStarted(v2::ItemStartedNotification),
    #[serde(rename = "item/completed")]                         ItemCompleted(v2::ItemCompletedNotification),
    #[serde(rename = "item/agentMessage/delta")]                AgentMessageDelta(v2::AgentMessageDeltaNotification),
    #[serde(rename = "item/fileChange/patchUpdated")]           FileChangePatchUpdated(v2::FileChangePatchUpdatedNotification),
    #[serde(rename = "item/autoApprovalReview/started")]        AutoApprovalReviewStarted(v2::ItemGuardianApprovalReviewStartedNotification),
    #[serde(rename = "item/autoApprovalReview/completed")]      AutoApprovalReviewCompleted(v2::ItemGuardianApprovalReviewCompletedNotification),
    #[serde(rename = "thread/tokenUsage/updated")]              TokenUsageUpdated(v2::ThreadTokenUsageUpdatedNotification),
    #[serde(rename = "account/rateLimits/updated")]             RateLimitsUpdated(v2::AccountRateLimitsUpdatedNotification),
    #[serde(rename = "error")]                                  Error(v2::ErrorNotification),
    #[serde(rename = "warning")]                                Warning(v2::WarningNotification),
    #[serde(rename = "guardianWarning")]                        GuardianWarning(v2::GuardianWarningNotification),
    #[serde(rename = "deprecationNotice")]                      DeprecationNotice(v2::DeprecationNoticeNotification),
    #[serde(rename = "configWarning")]                          ConfigWarning(v2::ConfigWarningNotification),
    #[serde(rename = "mcpServer/startupStatus/updated")]        McpServerStartupStatusUpdated(v2::McpServerStatusUpdatedNotification),
    #[serde(other)]                                              Unknown,
}
```

(Verify that the typify output uses the struct names referenced above; rename in this enum to match the generated identifiers if typify renames anything.)

- [ ] **Step 4: Run tests to verify passing**

Run: `cargo test -p codex-client --test protocol_roundtrip` → all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/codex-client/src/protocol/messages.rs crates/codex-client/tests/protocol_roundtrip.rs
git commit -m "Add JSON-RPC envelope and method-dispatch enums"
```

---

## Task 4: Transport stage — `StdioTransport`

**Files:**
- Modify: `crates/codex-client/src/transport.rs`
- Create: `crates/codex-client/tests/transport_tests.rs`

- [ ] **Step 1: Write failing transport tests**

`crates/codex-client/tests/transport_tests.rs`:

```rust
use codex_client::transport::StdioTransport;
use serde_json::json;
use tokio::io::{duplex, AsyncWriteExt};

#[tokio::test]
async fn single_frame_round_trip() {
    let (server, client) = duplex(4096);
    let (server_r, server_w) = tokio::io::split(server);
    let mut transport = StdioTransport::from_halves(server_r, server_w);

    let (mut client_r, mut client_w) = tokio::io::split(client);

    transport.send(json!({"method": "ping"})).await.unwrap();

    let mut buf = String::new();
    use tokio::io::AsyncBufReadExt;
    let mut reader = tokio::io::BufReader::new(&mut client_r);
    reader.read_line(&mut buf).await.unwrap();
    assert_eq!(buf.trim(), r#"{"method":"ping"}"#);

    client_w.write_all(b"{\"method\":\"pong\"}\n").await.unwrap();
    let v = transport.recv().await.unwrap();
    assert_eq!(v["method"], "pong");
}

#[tokio::test]
async fn three_frames_preserve_order() {
    let (server, client) = duplex(4096);
    let (server_r, server_w) = tokio::io::split(server);
    let mut transport = StdioTransport::from_halves(server_r, server_w);
    let (_, mut client_w) = tokio::io::split(client);

    client_w.write_all(b"{\"n\":1}\n{\"n\":2}\n{\"n\":3}\n").await.unwrap();
    assert_eq!(transport.recv().await.unwrap()["n"], 1);
    assert_eq!(transport.recv().await.unwrap()["n"], 2);
    assert_eq!(transport.recv().await.unwrap()["n"], 3);
}

#[tokio::test]
async fn eof_returns_transport_closed() {
    let (server, client) = duplex(4096);
    let (server_r, server_w) = tokio::io::split(server);
    let mut transport = StdioTransport::from_halves(server_r, server_w);
    drop(client);
    let err = transport.recv().await.unwrap_err();
    matches!(err, codex_client::ClientError::TransportClosed);
}
```

- [ ] **Step 2: Run tests to verify failures**

Run: `cargo test -p codex-client --test transport_tests`
Expected: compile errors — `StdioTransport` does not exist.

- [ ] **Step 3: Implement `transport.rs`**

```rust
use crate::error::{ClientError, ClientResult};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// Newline-delimited JSON transport over a pair of async read/write halves.
/// Generic over reader/writer so tests can drive it with `tokio::io::DuplexStream`.
pub struct StdioTransport<R, W> {
    reader: BufReader<R>,
    writer: W,
    line_buf: String,
}

impl<R, W> StdioTransport<R, W>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    pub fn from_halves(reader: R, writer: W) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer,
            line_buf: String::new(),
        }
    }

    pub async fn send(&mut self, v: Value) -> ClientResult<()> {
        let s = serde_json::to_string(&v)
            .map_err(|e| ClientError::Decode { role: "send", source: e })?;
        self.writer.write_all(s.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;
        Ok(())
    }

    pub async fn recv(&mut self) -> ClientResult<Value> {
        self.line_buf.clear();
        let n = self.reader.read_line(&mut self.line_buf).await?;
        if n == 0 {
            return Err(ClientError::TransportClosed);
        }
        serde_json::from_str(self.line_buf.trim_end())
            .map_err(|e| ClientError::Decode { role: "recv", source: e })
    }
}
```

(If `AsyncReadExt` import is unused, drop it.)

- [ ] **Step 4: Run tests to verify passing**

Run: `cargo test -p codex-client --test transport_tests` → all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/codex-client/src/transport.rs crates/codex-client/tests/transport_tests.rs
git commit -m "Add newline-delimited JSON transport over async halves"
```

---

## Task 5: Dispatcher — response correlation + notification routing

**Files:**
- Modify: `crates/codex-client/src/dispatcher.rs`
- Create: `crates/codex-client/tests/dispatcher_tests.rs`

- [ ] **Step 1: Write failing dispatcher tests**

`crates/codex-client/tests/dispatcher_tests.rs`:

```rust
use codex_client::dispatcher::Dispatcher;
use codex_client::transport::StdioTransport;
use codex_client::ClientError;
use serde_json::json;
use tokio::io::AsyncWriteExt;

async fn pair() -> (Dispatcher<tokio::io::ReadHalf<tokio::io::DuplexStream>>, tokio::io::WriteHalf<tokio::io::DuplexStream>) {
    let (server, client) = tokio::io::duplex(4096);
    let (server_r, _server_w) = tokio::io::split(server);
    let (_client_r, client_w) = tokio::io::split(client);
    let dispatcher = Dispatcher::spawn(server_r);
    (dispatcher, client_w)
}

#[tokio::test]
async fn response_resolves_oneshot() {
    let (dispatcher, mut peer_w) = pair().await;
    let rx = dispatcher.register(json!(1));
    peer_w.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n").await.unwrap();
    let v = rx.await.unwrap().unwrap();
    assert_eq!(v["ok"], true);
}

#[tokio::test]
async fn notification_routes_to_channel() {
    let (mut dispatcher, mut peer_w) = pair().await;
    peer_w.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"turn/started\",\"params\":{\"turnId\":\"t1\"}}\n").await.unwrap();
    let n = dispatcher.notifications().recv().await.unwrap();
    // n is a Value here; later refactored to ServerNotification once messages.rs is wired
    assert_eq!(n["method"], "turn/started");
}

#[tokio::test]
async fn malformed_line_does_not_kill_loop() {
    let (mut dispatcher, mut peer_w) = pair().await;
    peer_w.write_all(b"not json\n{\"jsonrpc\":\"2.0\",\"method\":\"warning\",\"params\":{}}\n").await.unwrap();
    let n = dispatcher.notifications().recv().await.unwrap();
    assert_eq!(n["method"], "warning");
}

#[tokio::test]
async fn eof_closes_waiters() {
    let (dispatcher, peer_w) = pair().await;
    let rx = dispatcher.register(json!(42));
    drop(peer_w);
    let err = rx.await.unwrap().unwrap_err();
    matches!(err, ClientError::TransportClosed);
}
```

- [ ] **Step 2: Run tests to verify failures**

Run: `cargo test -p codex-client --test dispatcher_tests`
Expected: compile errors — `Dispatcher` does not exist.

- [ ] **Step 3: Implement `dispatcher.rs`**

```rust
use crate::error::{ClientError, ClientResult, RequestId};
use crate::transport::StdioTransport;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncRead;
use tokio::sync::{mpsc, oneshot};

pub struct Dispatcher<R> {
    waiters: Arc<Mutex<HashMap<String, oneshot::Sender<ClientResult<Value>>>>>,
    notif_rx: mpsc::Receiver<Value>,
    _join: tokio::task::JoinHandle<()>,
    _r: std::marker::PhantomData<R>,
}

impl<R> Dispatcher<R>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    pub fn spawn(reader: R) -> Self {
        // For simplicity dispatcher owns its own write-less transport; writes
        // go via Client directly. Construct a one-sided transport here:
        let (sink_w, _sink_r) = tokio::io::duplex(1);
        let mut transport = StdioTransport::from_halves(reader, sink_w);
        let waiters: Arc<Mutex<HashMap<String, oneshot::Sender<ClientResult<Value>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let waiters_clone = waiters.clone();
        let (notif_tx, notif_rx) = mpsc::channel::<Value>(256);

        let join = tokio::spawn(async move {
            loop {
                match transport.recv().await {
                    Ok(v) => {
                        if v.get("id").is_some() && v.get("method").is_none() {
                            // Response — route to waiter by id (stringified).
                            let id_key = serde_json::to_string(&v["id"]).unwrap_or_default();
                            let waiter = waiters_clone.lock().unwrap().remove(&id_key);
                            if let Some(tx) = waiter {
                                if let Some(err) = v.get("error") {
                                    let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
                                    let message = err.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string();
                                    let _ = tx.send(Err(ClientError::JsonRpc { code, message }));
                                } else {
                                    let _ = tx.send(Ok(v.get("result").cloned().unwrap_or(Value::Null)));
                                }
                            } else {
                                tracing::warn!(?v, "response with no matching waiter");
                            }
                        } else if v.get("method").is_some() && v.get("id").is_none() {
                            // Notification — forward.
                            if notif_tx.send(v).await.is_err() {
                                break; // consumer gone
                            }
                        } else {
                            tracing::warn!(?v, "frame matched neither response nor notification");
                        }
                    }
                    Err(ClientError::Decode { .. }) => {
                        tracing::warn!("malformed frame, continuing");
                        continue;
                    }
                    Err(e) => {
                        tracing::debug!(error = %e, "transport closed");
                        break;
                    }
                }
            }
            // Drain waiters with TransportClosed.
            let mut map = waiters_clone.lock().unwrap();
            for (_, tx) in map.drain() {
                let _ = tx.send(Err(ClientError::TransportClosed));
            }
        });

        Self { waiters, notif_rx, _join: join, _r: std::marker::PhantomData }
    }

    pub fn register(&self, id: RequestId) -> oneshot::Receiver<ClientResult<Value>> {
        let (tx, rx) = oneshot::channel();
        let key = serde_json::to_string(&id).unwrap_or_default();
        self.waiters.lock().unwrap().insert(key, tx);
        rx
    }

    pub fn notifications(&mut self) -> &mut mpsc::Receiver<Value> {
        &mut self.notif_rx
    }
}

impl<R> Drop for Dispatcher<R> {
    fn drop(&mut self) {
        self._join.abort();
        let mut map = self.waiters.lock().unwrap();
        for (_, tx) in map.drain() {
            let _ = tx.send(Err(ClientError::TransportClosed));
        }
    }
}
```

Note: the dispatcher needs the read half only — pairing it with a dummy write half via `duplex(1)` keeps `StdioTransport` reusable. Implementation may choose to factor `StdioTransport` into reader-only/writer-only halves instead; either is fine.

- [ ] **Step 4: Run tests to verify passing**

Run: `cargo test -p codex-client --test dispatcher_tests` → all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/codex-client/src/dispatcher.rs crates/codex-client/tests/dispatcher_tests.rs
git commit -m "Add dispatcher with response correlation and notification routing"
```

---

## Task 6: `Client` typed API + lifecycle

**Files:**
- Modify: `crates/codex-client/src/client.rs`
- Create: `crates/codex-client/tests/client_tests.rs`

- [ ] **Step 1: Write failing client tests**

`crates/codex-client/tests/client_tests.rs`:

```rust
use codex_client::{Client, ClientError};
use codex_client::protocol::v1::{ClientInfo, InitializeParams};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

// Helper: drive a scripted "server" against the client's stdin/stdout via DuplexStream.
async fn scripted(script: Vec<(serde_json::Value, serde_json::Value)>) -> Client {
    // (left as exercise — pseudocode)
    todo!()
}

#[tokio::test]
async fn initialize_happy_path() {
    // server-side: read first request, assert method=initialize, send canned response.
    // assert client.initialize(...) returns Ok with the canned response.
    todo!()
}

#[tokio::test]
async fn start_turn_error_response() {
    // server-side: respond with {"error":{"code":-32602,"message":"bad"}}.
    // assert client.start_turn(...) returns Err(ClientError::JsonRpc { code: -32602, .. }).
    todo!()
}

#[tokio::test]
async fn drop_client_kills_pending_awaits() {
    // start_turn against a silent server; drop the client; await resolves to Err(TransportClosed).
    todo!()
}
```

(The scripted helper is intentionally `todo!()` in this plan — implementing it is part of Step 3. Pattern: spawn a tokio task that owns one half of a `duplex()` and reads/writes line-by-line per the script.)

- [ ] **Step 2: Run tests to verify failures**

Run: `cargo test -p codex-client --test client_tests`
Expected: compile errors — `Client::initialize` etc. do not exist.

- [ ] **Step 3: Implement `client.rs`**

```rust
use crate::dispatcher::Dispatcher;
use crate::error::{ClientError, ClientResult, RequestId};
use crate::protocol::messages::{ClientRequest, ServerNotification};
use crate::protocol::{v1, v2};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncWriteExt, WriteHalf};
use tokio::process::Child;
use tokio::sync::{mpsc, Mutex};

pub struct Client {
    next_id: Arc<std::sync::atomic::AtomicU64>,
    write_half: Arc<Mutex<tokio::process::ChildStdin>>,
    dispatcher: Arc<Mutex<Dispatcher<tokio::process::ChildStdout>>>,
    child: Arc<Mutex<Option<Child>>>,
}

pub struct NotificationStream {
    inner: mpsc::Receiver<ServerNotification>,
}

impl NotificationStream {
    pub async fn next(&mut self) -> Option<ServerNotification> {
        self.inner.recv().await
    }
}

impl Client {
    pub async fn connect(mut child: Child) -> ClientResult<(Self, NotificationStream)> {
        let stdin  = child.stdin.take().ok_or_else(|| ClientError::Io(std::io::Error::other("missing stdin")))?;
        let stdout = child.stdout.take().ok_or_else(|| ClientError::Io(std::io::Error::other("missing stdout")))?;

        let mut dispatcher = Dispatcher::spawn(stdout);

        // Spawn a translator task: raw Value notifications → typed ServerNotification.
        let (typed_tx, typed_rx) = mpsc::channel::<ServerNotification>(256);
        // Implementation detail: dispatcher exposes a raw-notification rx; translate here.
        // (For brevity in this plan, see Task 5 note; may merge typing into Dispatcher.)
        // ...

        Ok((
            Self {
                next_id: Arc::new(0u64.into()),
                write_half: Arc::new(Mutex::new(stdin)),
                dispatcher: Arc::new(Mutex::new(dispatcher)),
                child: Arc::new(Mutex::new(Some(child))),
            },
            NotificationStream { inner: typed_rx },
        ))
    }

    async fn rpc<T: serde::de::DeserializeOwned>(&self, req: ClientRequest) -> ClientResult<T> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id_value: RequestId = json!(id);

        let mut payload = serde_json::to_value(&req)
            .map_err(|e| ClientError::Decode { role: "send", source: e })?;
        payload["jsonrpc"] = json!("2.0");
        payload["id"] = id_value.clone();

        let waiter = self.dispatcher.lock().await.register(id_value.clone());
        {
            let mut w = self.write_half.lock().await;
            let s = serde_json::to_string(&payload)
                .map_err(|e| ClientError::Decode { role: "send", source: e })?;
            w.write_all(s.as_bytes()).await?;
            w.write_all(b"\n").await?;
            w.flush().await?;
        }
        let v = waiter.await.map_err(|_| ClientError::OneshotDropped(id_value))??;
        serde_json::from_value(v).map_err(|e| ClientError::Decode { role: "response", source: e })
    }

    pub async fn initialize(&self, params: v1::InitializeParams) -> ClientResult<v1::InitializeResponse> {
        self.rpc(ClientRequest::Initialize(params)).await
    }

    pub async fn start_turn(&self, params: v2::TurnStartParams) -> ClientResult<v2::TurnStartResponse> {
        self.rpc(ClientRequest::TurnStart(params)).await
    }

    pub async fn interrupt(&self, thread_id: String, turn_id: String) -> ClientResult<()> {
        let _: Value = self.rpc(ClientRequest::TurnInterrupt(v2::TurnInterruptParams { thread_id, turn_id })).await?;
        Ok(())
    }

    pub async fn thread_approve_guardian_denied_action(
        &self,
        params: v2::ThreadApproveGuardianDeniedActionParams,
    ) -> ClientResult<v2::ThreadApproveGuardianDeniedActionResponse> {
        self.rpc(ClientRequest::ThreadApproveGuardianDeniedAction(params)).await
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let child = self.child.clone();
        // Sync drop — spawn the cleanup task best-effort.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Some(mut c) = child.lock().await.take() {
                    let _ = c.start_kill();
                }
            });
        }
    }
}
```

(The translator from raw `Value` notifications → typed `ServerNotification` can be implemented either inside `Dispatcher` or as a sidecar task in `connect`; pick whichever keeps `Dispatcher`'s test surface clean. If you change `Dispatcher` to emit typed `ServerNotification` directly, update the Task 5 tests to use the typed variant.)

- [ ] **Step 4: Run tests to verify passing**

Run: `cargo test -p codex-client --test client_tests` → all pass.
Run: `cargo test -p codex-client` → entire crate green.

- [ ] **Step 5: Commit**

```bash
git add crates/codex-client/src/client.rs crates/codex-client/tests/client_tests.rs
git commit -m "Add Client API with initialize/start_turn/interrupt/override + Drop cleanup"
```

---

## Task 7: Wire `codex-client` into `symphony-core` deps

**Files:**
- Modify: `crates/symphony-core/Cargo.toml`
- Modify: `crates/symphony-core/src/error.rs` (add `UnknownHarness` variant)

- [ ] **Step 1: Add the dependency**

In `crates/symphony-core/Cargo.toml`:

```toml
[dependencies]
codex-client = { path = "../codex-client" }
```

- [ ] **Step 2: Add `UnknownHarness` to `Error`**

In `crates/symphony-core/src/error.rs`:

```rust
#[error("unknown harness: {0}")]
UnknownHarness(String),
```

- [ ] **Step 3: Verify**

Run: `cargo check -p symphony-core` → clean.

- [ ] **Step 4: Commit**

```bash
git add crates/symphony-core/Cargo.toml crates/symphony-core/src/error.rs
git commit -m "Add codex-client dep and UnknownHarness error variant"
```

---

## Task 8: `translate_codex_permissions`

**Files:**
- Modify: `crates/symphony-core/src/policy.rs`
- Modify: `crates/symphony-core/tests/policy_tests.rs`

- [ ] **Step 1: Add failing tests**

In `crates/symphony-core/tests/policy_tests.rs`, append:

```rust
use symphony_core::policy::translate_codex_permissions;
// Permissions struct comes from codex-client::protocol::v2 — the exact shape
// depends on what typify generates. The assertions below probe the *intent*
// (mode constants) rather than struct internals.

#[test]
fn permissions_read_only_for_read_only_mode() {
    let mut p = Policy::default();
    p.permission_mode = PermissionMode::ReadOnly;
    p.sandbox = SandboxProfile::WorkspaceWrite; // sandbox ignored when mode=ReadOnly
    let perms = translate_codex_permissions(&p);
    // Convert to JSON and check the resulting shape matches read_only convention.
    let v = serde_json::to_value(&perms).unwrap();
    // Spec: ReadOnly profile produces a permission set that disallows writes.
    assert!(v.to_string().to_lowercase().contains("read"));
}

#[test]
fn permissions_strict_for_require_approval_mode() {
    let mut p = Policy::default();
    p.permission_mode = PermissionMode::RequireApproval;
    let perms = translate_codex_permissions(&p);
    let v = serde_json::to_value(&perms).unwrap();
    // Spec: RequireApproval forces a maximally restrictive profile so the
    // guardian denies most actions and the operator gates each one.
    // (Concrete assertions filled in once typify gives us the real struct.)
    let _ = v;
}

#[test]
fn permissions_match_sandbox_for_accept_edits() {
    let cases = [
        (SandboxProfile::ReadOnly,       "read"),
        (SandboxProfile::WorkspaceWrite, "workspace"),
        (SandboxProfile::Unrestricted,   "danger"),
    ];
    for (sandbox, marker) in cases {
        let mut p = Policy::default();
        p.permission_mode = PermissionMode::AcceptEdits;
        p.sandbox = sandbox;
        let perms = translate_codex_permissions(&p);
        let v = serde_json::to_value(&perms).unwrap();
        assert!(v.to_string().to_lowercase().contains(marker), "expected {marker} in {v}");
    }
}
```

- [ ] **Step 2: Run tests, expect failures**

Run: `cargo test -p symphony-core --test policy_tests`
Expected: compile error — `translate_codex_permissions` does not exist.

- [ ] **Step 3: Implement `translate_codex_permissions`**

Append to `crates/symphony-core/src/policy.rs`:

```rust
use codex_client::protocol::v2;

pub fn translate_codex_permissions(p: &Policy) -> v2::Permissions {
    use v2::Permissions;
    match (&p.permission_mode, &p.sandbox) {
        (PermissionMode::ReadOnly, _)                                  => Permissions::read_only(),
        (PermissionMode::RequireApproval, _)                           => Permissions::strict(),
        (PermissionMode::AcceptEdits, SandboxProfile::ReadOnly)        => Permissions::read_only(),
        (PermissionMode::AcceptEdits, SandboxProfile::WorkspaceWrite)  => Permissions::workspace_write(),
        (PermissionMode::AcceptEdits, SandboxProfile::Unrestricted)    => Permissions::danger_full_access(),
    }
}
```

(If `v2::Permissions` doesn't have these convenience constructors yet, add them inside `codex-client` as a small additive module `protocol::permissions_helpers` and import from there. Concrete shape resolves at this task — adjust assertions in Step 1 to match.)

- [ ] **Step 4: Run tests to verify passing**

Run: `cargo test -p symphony-core --test policy_tests` → all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/symphony-core/src/policy.rs crates/symphony-core/tests/policy_tests.rs
git commit -m "Add translate_codex_permissions covering Policy×Sandbox matrix"
```

---

## Task 9: `CodexHarness` — spawn + handshake + event pump

**Files:**
- Create: `crates/symphony-core/src/harness/codex.rs`
- Modify: `crates/symphony-core/src/harness/mod.rs`
- Delete: `crates/symphony-core/src/harness/codex_stub.rs`
- Create: `crates/symphony-core/tests/harness_codex.rs`

- [ ] **Step 1: Write failing harness tests with a mock Client**

`crates/symphony-core/tests/harness_codex.rs`:

```rust
// Mock-Client-based integration tests. The mock implements a small trait that
// `CodexHarness` accepts via constructor injection in tests; production code
// uses `codex_client::Client` directly.

use symphony_core::harness::codex::{CodexHarness, CodexHarnessConfig};
use symphony_core::harness::Harness;
use symphony_core::events::AgentEventKind;
use symphony_core::policy::{Policy, PermissionMode, SandboxProfile};
// Test scaffolding for HarnessContext factory:
// see harness::test_support module (created in this task).

#[tokio::test]
async fn happy_path_emits_turn_started_and_completed() {
    // Build a MockCodexClient with a scripted notification sequence:
    //   turn/started → item/started(command) → item/autoApprovalReview/started
    //     → item/autoApprovalReview/completed{approved} → item/completed → turn/completed{success}
    // Run harness.run(ctx). Assert outcome.success == true.
    // Assert ctx.tx saw TurnStarted, ApprovalAutoApproved, TurnCompleted.
    // Assert ctx.bus saw ToolCall + ToolResult.
    todo!()
}

#[tokio::test]
async fn operator_override_path() {
    // Sequence ends with autoApprovalReview/completed{denied}.
    // Assert OrchestratorEvent::ApprovalRequest broadcast.
    // In a separate task, call approval_router.resolve(allow=true).
    // Assert mock observed `thread/approveGuardianDeniedAction`.
    // Assert OrchestratorEvent::ApprovalDecision broadcast.
    todo!()
}

#[tokio::test]
async fn operator_deny_path() {
    // Same as override path but resolve(allow=false). Assert no override RPC.
    todo!()
}

#[tokio::test]
async fn transport_closed_mid_turn_fails_outcome() {
    // Mock drops notification stream after TurnStarted.
    // Assert outcome.success == false, error contains "transport closed".
    todo!()
}
```

(`todo!()` slots are filled with concrete code in Step 3. The mock-Client trait + scripted-sequence helpers go in a new `harness::test_support` module exported under `#[cfg(test)]`.)

- [ ] **Step 2: Run tests, expect failures**

Run: `cargo test -p symphony-core --test harness_codex`
Expected: compile errors — `harness::codex` does not exist.

- [ ] **Step 3: Implement `harness/codex.rs`**

```rust
use super::{Harness, HarnessContext, HarnessOutcome};
use crate::error::{Error, Result};
use crate::events::{AgentEvent, AgentEventKind};
use crate::events::broadcast::OrchestratorEvent;
use crate::policy::translate_codex_permissions;
use async_trait::async_trait;
use chrono::Utc;
use codex_client::protocol::messages::ServerNotification;
use codex_client::protocol::{v1, v2};
use codex_client::Client;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::process::Command;

#[derive(Default, Clone)]
pub struct CodexHarness {}

#[async_trait]
impl Harness for CodexHarness {
    fn name(&self) -> &'static str { "codex" }

    async fn run(&self, ctx: HarnessContext<'_>) -> Result<HarnessOutcome> {
        // 1. Build MCP config for the linear bridge.
        let mcp_json = super::mcp_bridge::generate_mcp_config_json(&ctx)?;

        // 2. Spawn codex app-server.
        let mut cmd = Command::new("codex");
        cmd.arg("app-server")
           .arg("--listen").arg("stdio")
           .arg("-c").arg(format!("mcp_servers.linear={}", mcp_json))
           .current_dir(ctx.workspace)
           .stdin(std::process::Stdio::piped())
           .stdout(std::process::Stdio::piped())
           .stderr(std::process::Stdio::piped());
        if let Some(t) = ctx.linear_token.as_ref() { cmd.env("SYMPHONY_LINEAR_TOKEN", t); }
        if let Some(e) = ctx.linear_endpoint.as_ref() { cmd.env("SYMPHONY_LINEAR_ENDPOINT", e); }
        cmd.env("SYMPHONY_ISSUE_ID", &ctx.issue_id);

        let child = cmd.spawn().map_err(|e| {
            // Emit startup-failed AgentEvent so the orchestrator sees the cause.
            let _ = ctx.tx.try_send(AgentEvent {
                kind: AgentEventKind::StartupFailed,
                timestamp: Utc::now(),
                agent_pid: None,
                thread_id: None,
                turn_id: None,
                message: Some(format!("spawn codex failed: {e}")),
                tokens: None,
                raw: None,
            });
            Error::Harness { harness: "codex".into(), message: e.to_string() }
        })?;

        // 3. Connect + initialize.
        let (client, mut notifs) = Client::connect(child).await
            .map_err(|e| Error::Harness { harness: "codex".into(), message: e.to_string() })?;

        let init = client.initialize(v1::InitializeParams {
            protocol_version: "v1".into(),
            client_info: v1::ClientInfo { name: "symphony".into(), version: env!("CARGO_PKG_VERSION").into() },
            capabilities: serde_json::Value::Null,
        }).await.map_err(|e| Error::Harness { harness: "codex".into(), message: e.to_string() })?;
        let _ = init;

        // 4. Start turn.
        let permissions = translate_codex_permissions(&ctx.policy);
        let turn = client.start_turn(v2::TurnStartParams {
            thread_id: None,
            prompt: ctx.prompt.into(),
            permissions,
            // ...other required fields per typify-generated TurnStartParams
            ..Default::default()
        }).await.map_err(|e| Error::Harness { harness: "codex".into(), message: e.to_string() })?;

        let thread_id = turn.thread_id.clone();
        let turn_id = turn.turn_id.clone();

        let _ = ctx.tx.send(AgentEvent {
            kind: AgentEventKind::TurnStarted,
            timestamp: Utc::now(),
            agent_pid: None,
            thread_id: Some(thread_id.clone()),
            turn_id: Some(turn_id.clone()),
            message: None,
            tokens: None,
            raw: None,
        }).await;

        // 5. Event pump.
        let pending_reviews: Mutex<HashMap<String, v2::GuardianApprovalReviewAction>> = Mutex::new(HashMap::new());
        let mut success = false;
        let mut error_msg: Option<String> = None;

        loop {
            let Some(notif) = notifs.next().await else {
                error_msg = Some("transport closed".into());
                break;
            };
            match notif {
                ServerNotification::TurnStarted(_) => { /* already emitted */ }
                ServerNotification::TurnCompleted(n) => {
                    success = matches!(n.status, v2::TurnStatus::Success /* adjust to typify variant */);
                    let _ = ctx.tx.send(AgentEvent {
                        kind: if success { AgentEventKind::TurnCompleted } else { AgentEventKind::TurnFailed },
                        timestamp: Utc::now(),
                        agent_pid: None,
                        thread_id: Some(thread_id.clone()),
                        turn_id: Some(turn_id.clone()),
                        message: None,
                        tokens: None,
                        raw: Some(serde_json::to_value(&n).unwrap_or_default()),
                    }).await;
                    break;
                }
                ServerNotification::ItemStarted(n) => {
                    let _ = ctx.bus.send(OrchestratorEvent::ToolCall {
                        issue_id: ctx.issue_id.clone(),
                        tool: format!("codex.{}", action_discriminator(&n)),
                        input: serde_json::to_value(&n).unwrap_or_default(),
                    });
                }
                ServerNotification::ItemCompleted(n) => {
                    let _ = ctx.bus.send(OrchestratorEvent::ToolResult {
                        issue_id: ctx.issue_id.clone(),
                        tool: format!("codex.{}", action_discriminator(&n)),
                        output: serde_json::to_value(&n).unwrap_or_default(),
                        error: None,
                    });
                }
                ServerNotification::AutoApprovalReviewStarted(n) => {
                    pending_reviews.lock().unwrap().insert(n.review_id.clone(), n.action.clone());
                }
                ServerNotification::AutoApprovalReviewCompleted(n) => {
                    let action = pending_reviews.lock().unwrap().remove(&n.review_id);
                    handle_review_completed(&ctx, &client, action, n).await;
                }
                ServerNotification::TokenUsageUpdated(n) => {
                    let _ = ctx.tx.send(AgentEvent {
                        kind: AgentEventKind::TokenUsageUpdated,
                        timestamp: Utc::now(),
                        agent_pid: None,
                        thread_id: Some(thread_id.clone()),
                        turn_id: Some(turn_id.clone()),
                        message: None,
                        tokens: Some(/* convert from typify struct */ Default::default()),
                        raw: None,
                    }).await;
                }
                ServerNotification::Error(n) => {
                    error_msg = Some(n.message.unwrap_or_default());
                    let _ = ctx.tx.send(AgentEvent {
                        kind: AgentEventKind::TurnEndedWithError,
                        timestamp: Utc::now(),
                        agent_pid: None,
                        thread_id: Some(thread_id.clone()),
                        turn_id: Some(turn_id.clone()),
                        message: error_msg.clone(),
                        tokens: None,
                        raw: Some(serde_json::to_value(&n).unwrap_or_default()),
                    }).await;
                    break;
                }
                ServerNotification::Unknown => { /* dropped */ }
                other => {
                    // Map remaining handled notifs (warnings, plan/diff updates, etc.) to AgentEvent::Notification.
                    let _ = ctx.tx.send(AgentEvent {
                        kind: AgentEventKind::Notification,
                        timestamp: Utc::now(),
                        agent_pid: None,
                        thread_id: Some(thread_id.clone()),
                        turn_id: Some(turn_id.clone()),
                        message: None,
                        tokens: None,
                        raw: Some(serde_json::to_value(&other).unwrap_or_default()),
                    }).await;
                }
            }
        }

        Ok(HarnessOutcome { thread_id, turn_id, success, error: error_msg })
    }
}

// Helpers — keep signatures stable, fill in once typify shapes are known.

fn action_discriminator(_n: &impl serde::Serialize) -> &'static str { /* match on action variant */ "command" }

async fn handle_review_completed(
    _ctx: &HarnessContext<'_>,
    _client: &Client,
    _action: Option<v2::GuardianApprovalReviewAction>,
    _n: v2::ItemGuardianApprovalReviewCompletedNotification,
) {
    // Per spec §Approval round-trip — branch on permission_mode × status.
    // - AcceptEdits + denied → ApprovalRequest broadcast, register PendingApproval, spawn await task.
    // - AcceptEdits + approved → AgentEvent::ApprovalAutoApproved.
    // - RequireApproval + any denied → same as AcceptEdits + denied.
    // - ReadOnly → AgentEvent::Notification only.
}
```

(The body above is intentionally schematic — fill in concrete `match` arms once `typify` reveals exact field names. Cross-reference the spec's §Notification translation and §Approval round-trip tables.)

- [ ] **Step 4: Update `harness/mod.rs`**

```rust
pub mod codex;
// remove: pub mod codex_stub;

pub fn select_harness(name: &str) -> Box<dyn Harness + Send + Sync> {
    match name {
        "claude_code" | "claude" | "claude-code" => Box::new(claude_code::ClaudeCodeHarness::default()),
        "hermes" => Box::new(hermes::HermesHarness::default()),
        "codex" | "codex-app-server" => Box::new(codex::CodexHarness::default()),
        other => Box::new(UnknownHarness { name: other.to_string() }),
    }
}

#[derive(Clone)]
struct UnknownHarness { name: String }

#[async_trait]
impl Harness for UnknownHarness {
    fn name(&self) -> &'static str { "unknown" }
    async fn run(&self, _ctx: HarnessContext<'_>) -> Result<HarnessOutcome> {
        Err(crate::error::Error::UnknownHarness(self.name.clone()))
    }
}
```

- [ ] **Step 5: Delete `codex_stub.rs`**

```bash
git rm crates/symphony-core/src/harness/codex_stub.rs
```

- [ ] **Step 6: Run tests to verify passing**

Run: `cargo test -p symphony-core --test harness_codex` → all pass.
Run: `cargo test --workspace` → no regressions.
Run: `cargo clippy --workspace -- -D warnings` → clean.

- [ ] **Step 7: Commit**

```bash
git add crates/symphony-core/src/harness/codex.rs crates/symphony-core/src/harness/mod.rs crates/symphony-core/tests/harness_codex.rs
git rm crates/symphony-core/src/harness/codex_stub.rs
git commit -m "Replace codex_stub with real CodexHarness; route via select_harness"
```

---

## Task 10: End-to-end smoke test

**Files:**
- Create: `crates/symphony-core/tests/slice2_smoke.rs`

- [ ] **Step 1: Implement the smoke**

```rust
//! Slice 2 end-to-end smoke. Gated by `SYMPHONY_E2E=1` and presence of the
//! `codex` CLI on PATH. Not run in CI.

use std::path::PathBuf;
use std::process::Command;

#[tokio::test]
async fn codex_end_to_end_happy() {
    if std::env::var("SYMPHONY_E2E").is_err() { return; }
    if which::which("codex").is_err() { return; }

    let tmp = tempfile::tempdir().unwrap();
    let workspace = tmp.path().to_path_buf();
    // git init the workspace so the harness's vcs hooks have something to work with.
    Command::new("git").arg("init").current_dir(&workspace).output().unwrap();

    // Build a minimal HarnessContext via existing test scaffolding.
    // Call CodexHarness::default().run(ctx).await.
    // Assert outcome.success == true.
    // Assert at least one item/completed was observed via ctx.bus.
    todo!()
}

#[tokio::test]
async fn codex_end_to_end_require_approval() {
    if std::env::var("SYMPHONY_E2E").is_err() { return; }
    if which::which("codex").is_err() { return; }

    // Same as above but Policy.permission_mode = RequireApproval.
    // Spawn a task to auto-approve any ApprovalRequest broadcast within 5s.
    // Assert outcome.success == true and at least one ApprovalRequest was seen.
    todo!()
}
```

- [ ] **Step 2: Run locally before merging**

```bash
SYMPHONY_E2E=1 cargo test --test slice2_smoke -- --nocapture
```

Capture the transcript; attach to the PR.

- [ ] **Step 3: Commit**

```bash
git add crates/symphony-core/tests/slice2_smoke.rs
git commit -m "Add env-gated end-to-end smoke for Codex harness"
```

---

## Task 11: Workflow + docs

**Files:**
- Modify: `WORKFLOW.md`
- Modify: `README.md`

- [ ] **Step 1: Document the new harness name**

In `WORKFLOW.md`, add the example block:

```markdown
## Example: routing an issue to Codex

```yaml
harness: codex
policy:
  permission_mode: accept_edits   # or require_approval, read_only
  sandbox: workspace_write        # or read_only, unrestricted
```
```

- [ ] **Step 2: Update README**

Add a "Codex harness" section under the harness list pointing to this spec and the `codex-client` crate. Note: requires `codex-cli` >= 0.130 on PATH.

- [ ] **Step 3: Commit**

```bash
git add WORKFLOW.md README.md
git commit -m "Document codex harness configuration"
```

---

## Pre-merge checklist

- [ ] `cargo test -p codex-client` green.
- [ ] `cargo test -p symphony-core` green.
- [ ] `cargo test --workspace` green.
- [ ] `cargo clippy --workspace -- -D warnings` clean.
- [ ] `SYMPHONY_E2E=1 cargo test --test slice2_smoke` passed locally; transcript attached to PR.
- [ ] Manual: claim a real issue with `harness: codex`, observe dashboard approval toast on a guardian-denied action, click Approve, watch the override land.
- [ ] `docs/superpowers/brainstorms/2026-05-10-symphony-v1-slice2-state.md` deleted (slice complete).

## Risks & rollback

- **typify drift.** If a future regen of the schema renames a generated struct, `messages.rs` won't compile. Mitigation: protocol-roundtrip tests run on every change; CI catches the break.
- **`Permissions` constructor mismatch.** Final field shape may differ from the convenience constructors. Mitigation: Task 8 tests fail fast; refine constructors in `codex-client` if needed.
- **Drop-task spawn during runtime shutdown.** `tokio::runtime::Handle::try_current` may fail in some teardown paths; the child is then leaked until the OS reaps it. Acceptable for slice 2; revisit if it causes test flakiness.

Rollback: revert the `v1-slice2` branch merge. `codex_stub` is restored from history; harness selection falls back to it for the `"codex"` name.
