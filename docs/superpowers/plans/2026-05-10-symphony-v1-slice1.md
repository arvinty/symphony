# Symphony v1.0 Slice 1 — Claude Code End-to-End Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the Claude Code harness end-to-end with the four `linear_graphql` tools, workflow-level + interactive policy, GitHub PR creation, and an SSE-driven dashboard for live events and approvals. Build all the cross-cutting pieces (policy, events, tools, vcs, attachments) that slices 2 and 3 will reuse.

**Architecture:** New shared modules `policy`, `events::broadcast`, `tools::linear_graphql`, `vcs` in `symphony-core`. New SSE endpoint and approvals POST in `symphony::http`. New `harness/mcp_bridge` runs an in-process MCP server feeding tools to `claude`. linear-clone gets an `attachments` table, `addAttachment` / `removeAttachment` mutations, and an issue-scoped bearer-token auth layer. Web UI gains an SSE hook, an approval toast, and attachment chips.

**Tech Stack:** Rust (axum 0.8, tokio, async-graphql 7, sqlx/sqlite, reqwest, async-trait), TypeScript/React (Vite, Tailwind), `gh` CLI for PR creation, `wiremock` for client tests.

---

## File Structure

**Created:**
- `crates/symphony-core/src/policy.rs` — `Policy`, `PermissionMode`, `SandboxProfile`, parser.
- `crates/symphony-core/src/tools/mod.rs` — re-exports.
- `crates/symphony-core/src/tools/linear_graphql.rs` — `LinearGraphqlClient`, JSON schemas, retry/backoff.
- `crates/symphony-core/src/vcs.rs` — `push_branch`, `open_pr`.
- `crates/symphony-core/src/events/broadcast.rs` — `OrchestratorEvent`, `tokio::sync::broadcast` channel.
- `crates/symphony-core/src/harness/mcp_bridge.rs` — in-process MCP server.
- `crates/symphony-core/src/harness/approvals.rs` — `ApprovalRouter` (oneshot map + timeout).
- `crates/linear-clone/migrations/0003_attachments.sql`
- `crates/linear-clone/src/auth.rs` — issue-scoped bearer-token middleware + token store.
- `web/src/hooks/useEventStream.ts`
- `web/src/components/ApprovalToast.tsx`
- `web/src/components/AttachmentChip.tsx`
- `crates/symphony-core/tests/policy_tests.rs`
- `crates/symphony-core/tests/vcs_tests.rs`
- `crates/symphony-core/tests/linear_graphql_tool_tests.rs`
- `crates/symphony-core/tests/approval_flow_tests.rs`
- `crates/linear-clone/tests/auth_tests.rs`
- `crates/linear-clone/tests/attachments_tests.rs`

**Modified:**
- `crates/symphony-core/src/lib.rs` — add `pub mod policy; pub mod tools; pub mod vcs;`.
- `crates/symphony-core/src/events.rs` — split into module; keep `AgentEvent` here, add `events/broadcast.rs`.
- `crates/symphony-core/src/config.rs` — add `policy:` block, `vcs:` block.
- `crates/symphony-core/src/orchestrator.rs` — token minting, broadcast wiring, post-turn VCS pipeline, follow-up turn injection, captured policy.
- `crates/symphony-core/src/harness/mod.rs` — `Harness::run` gains `HarnessContext` (broadcast tx, policy, mcp_socket, approval_router).
- `crates/symphony-core/src/harness/claude_code.rs` — flag translation, MCP wiring, approval interception.
- `crates/symphony/src/http.rs` — `/api/v1/events` (SSE), `/api/v1/approvals/{id}` (POST), `/api/v1/{id}/open-pr` (POST).
- `crates/linear-clone/src/schema.rs` — `Attachment` type, `addAttachment` / `removeAttachment` mutations.
- `crates/linear-clone/src/lib.rs` — wire auth middleware.
- `web/src/App.tsx` and `web/src/views/*.tsx` — render attachment chips, mount toast, subscribe to events.
- `WORKFLOW.md` — example `policy:` and `vcs:` blocks.
- `README.md` — document new behavior.

---

## Task 1: Policy struct and config parsing

**Files:**
- Create: `crates/symphony-core/src/policy.rs`
- Create: `crates/symphony-core/tests/policy_tests.rs`
- Modify: `crates/symphony-core/src/config.rs` (add `PolicyConfig`, parse it into `EffectiveConfig`)
- Modify: `crates/symphony-core/src/lib.rs` (`pub mod policy;`)

- [ ] **Step 1: Write the failing tests**

`crates/symphony-core/tests/policy_tests.rs`:

```rust
use symphony_core::policy::{PermissionMode, Policy, SandboxProfile};

#[test]
fn default_policy_preserves_v0_behavior() {
    let p = Policy::default();
    assert_eq!(p.permission_mode, PermissionMode::AcceptEdits);
    assert_eq!(p.sandbox, SandboxProfile::WorkspaceWrite);
    assert!(p.allowed_tools.is_empty());
    assert_eq!(p.approval_timeout_ms, 300_000);
}

#[test]
fn policy_parses_from_yaml() {
    let yaml = r#"
permission_mode: require_approval
sandbox: read_only
allowed_tools: ["Bash", "Edit"]
approval_timeout_ms: 60000
"#;
    let p: Policy = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(p.permission_mode, PermissionMode::RequireApproval);
    assert_eq!(p.sandbox, SandboxProfile::ReadOnly);
    assert_eq!(p.allowed_tools, vec!["Bash".to_string(), "Edit".to_string()]);
    assert_eq!(p.approval_timeout_ms, 60_000);
}

#[test]
fn policy_unknown_mode_errors() {
    let yaml = "permission_mode: lol\n";
    let r: Result<Policy, _> = serde_yaml::from_str(yaml);
    assert!(r.is_err());
}
```

- [ ] **Step 2: Run tests to verify failures**

Run: `cargo test -p symphony-core --test policy_tests`
Expected: compile errors — `policy::Policy` does not exist.

- [ ] **Step 3: Implement `policy.rs`**

`crates/symphony-core/src/policy.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    AcceptEdits,
    RequireApproval,
    ReadOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxProfile {
    None,
    WorkspaceWrite,
    ReadOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Policy {
    #[serde(default = "default_mode")]
    pub permission_mode: PermissionMode,
    #[serde(default = "default_sandbox")]
    pub sandbox: SandboxProfile,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default = "default_timeout")]
    pub approval_timeout_ms: u64,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            permission_mode: default_mode(),
            sandbox: default_sandbox(),
            allowed_tools: vec![],
            approval_timeout_ms: default_timeout(),
        }
    }
}

fn default_mode() -> PermissionMode { PermissionMode::AcceptEdits }
fn default_sandbox() -> SandboxProfile { SandboxProfile::WorkspaceWrite }
fn default_timeout() -> u64 { 300_000 }
```

Add `pub mod policy;` to `crates/symphony-core/src/lib.rs`.

- [ ] **Step 4: Wire `Policy` into `EffectiveConfig`**

In `crates/symphony-core/src/config.rs`, add to `ServiceConfig`:

```rust
#[serde(default)]
pub policy: Option<crate::policy::Policy>,
```

Add to `EffectiveConfig`:

```rust
pub policy: crate::policy::Policy,
```

In `EffectiveConfig::from_workflow` set `policy: cfg.policy.unwrap_or_default(),`.

- [ ] **Step 5: Run tests to verify passing**

Run: `cargo test -p symphony-core --test policy_tests` → all pass.
Run: `cargo check -p symphony-core` → clean.

- [ ] **Step 6: Commit**

```bash
git add crates/symphony-core/src/policy.rs crates/symphony-core/src/lib.rs crates/symphony-core/src/config.rs crates/symphony-core/tests/policy_tests.rs
git commit -m "Add Policy config with permission mode, sandbox profile, allowlist"
```

---

## Task 2: Orchestrator broadcast channel and event variants

**Files:**
- Create: `crates/symphony-core/src/events/broadcast.rs`
- Modify: `crates/symphony-core/src/events.rs` → convert to `events/mod.rs`
- Create: `crates/symphony-core/tests/broadcast_tests.rs`

- [ ] **Step 1: Write the failing test**

`crates/symphony-core/tests/broadcast_tests.rs`:

```rust
use symphony_core::events::broadcast::{OrchestratorEvent, OrchestratorEventBus};

#[tokio::test]
async fn approval_request_round_trips_through_bus() {
    let bus = OrchestratorEventBus::new(64);
    let mut sub = bus.subscribe();

    let evt = OrchestratorEvent::ApprovalRequest {
        issue_id: "iss_1".into(),
        approval_id: "ap_1".into(),
        tool: "linear_graphql.add_comment".into(),
        input: serde_json::json!({"body":"hi"}),
    };
    bus.send(evt.clone()).unwrap();

    let got = sub.recv().await.unwrap();
    assert_eq!(serde_json::to_value(&got).unwrap(),
               serde_json::to_value(&evt).unwrap());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p symphony-core --test broadcast_tests`
Expected: compile error — module does not exist.

- [ ] **Step 3: Convert `events.rs` to `events/mod.rs` and add `broadcast.rs`**

Move the contents of `crates/symphony-core/src/events.rs` into `crates/symphony-core/src/events/mod.rs`. Add to the bottom of that mod:

```rust
pub mod broadcast;
```

Create `crates/symphony-core/src/events/broadcast.rs`:

```rust
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OrchestratorEvent {
    AgentEvent {
        issue_id: String,
        event: super::AgentEvent,
    },
    ToolCall {
        issue_id: String,
        tool: String,
        input: serde_json::Value,
    },
    ToolResult {
        issue_id: String,
        tool: String,
        output: serde_json::Value,
        error: Option<String>,
    },
    ApprovalRequest {
        issue_id: String,
        approval_id: String,
        tool: String,
        input: serde_json::Value,
    },
    ApprovalDecision {
        issue_id: String,
        approval_id: String,
        allow: bool,
        reason: Option<String>,
    },
    VcsPushed {
        issue_id: String,
        branch: String,
    },
    PrOpened {
        issue_id: String,
        url: String,
    },
    VcsError {
        issue_id: String,
        stage: String,
        message: String,
    },
    Resync,
}

#[derive(Clone)]
pub struct OrchestratorEventBus {
    tx: broadcast::Sender<OrchestratorEvent>,
}

impl OrchestratorEventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }
    pub fn send(&self, e: OrchestratorEvent) -> Result<usize, broadcast::error::SendError<OrchestratorEvent>> {
        self.tx.send(e)
    }
    pub fn subscribe(&self) -> broadcast::Receiver<OrchestratorEvent> {
        self.tx.subscribe()
    }
}
```

- [ ] **Step 4: Run tests to verify passing**

Run: `cargo test -p symphony-core --test broadcast_tests` → pass.
Run: `cargo check -p symphony-core` → clean.

- [ ] **Step 5: Commit**

```bash
git add crates/symphony-core/src/events crates/symphony-core/tests/broadcast_tests.rs
git rm crates/symphony-core/src/events.rs 2>/dev/null || true
git commit -m "Add OrchestratorEventBus with typed approval/tool/vcs variants"
```

---

## Task 3: linear-clone attachments migration + GraphQL

**Files:**
- Create: `crates/linear-clone/migrations/0003_attachments.sql`
- Modify: `crates/linear-clone/src/schema.rs`
- Create: `crates/linear-clone/tests/attachments_tests.rs`

- [ ] **Step 1: Write the failing test**

`crates/linear-clone/tests/attachments_tests.rs`:

```rust
use linear_clone::test_support::TestApp;
use serde_json::json;

#[tokio::test]
async fn add_attachment_appears_on_issue() {
    let app = TestApp::start().await;
    let issue_id = app.first_issue_id().await;

    let resp = app.gql(json!({
        "query": "mutation($i:ID!,$u:String!,$t:String){ addAttachment(issueId:$i,url:$u,title:$t){ id url title kind } }",
        "variables": {"i": issue_id, "u": "https://github.com/o/r/pull/1", "t": "feat: x"}
    })).await;

    let att = &resp["data"]["addAttachment"];
    assert_eq!(att["url"], "https://github.com/o/r/pull/1");
    assert_eq!(att["kind"], "pull_request");

    let issue = app.gql(json!({
        "query": "query($i:ID!){ issue(id:$i){ attachments{ nodes{ url } } } }",
        "variables": {"i": issue_id}
    })).await;
    assert_eq!(issue["data"]["issue"]["attachments"]["nodes"][0]["url"], "https://github.com/o/r/pull/1");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p linear-clone --test attachments_tests`
Expected: compile error — `test_support` does not exist; `addAttachment` undefined.

- [ ] **Step 3: Add `test_support` (if missing) and migration**

Confirm `linear_clone::test_support::TestApp` exists in `crates/linear-clone/src/lib.rs` from prior work; if not, expose a tiny harness:

```rust
pub mod test_support {
    use crate::{build_router, db::init_pool};
    use axum::Router;
    use serde_json::Value;
    use sqlx::SqlitePool;
    use tower::ServiceExt;

    pub struct TestApp { router: Router, pool: SqlitePool }
    impl TestApp {
        pub async fn start() -> Self {
            let pool = init_pool("sqlite::memory:").await.unwrap();
            sqlx::migrate!("./migrations").run(&pool).await.unwrap();
            let router = build_router(pool.clone());
            Self { router, pool }
        }
        pub async fn first_issue_id(&self) -> String {
            let row: (String,) = sqlx::query_as("SELECT id FROM issues LIMIT 1")
                .fetch_one(&self.pool).await.unwrap();
            row.0
        }
        pub async fn gql(&self, body: Value) -> Value {
            use axum::body::Body;
            use axum::http::{Request, header};
            let req = Request::builder()
                .method("POST")
                .uri("/graphql")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
            let resp = self.router.clone().oneshot(req).await.unwrap();
            let bytes = axum::body::to_bytes(resp.into_body(), 1<<20).await.unwrap();
            serde_json::from_slice(&bytes).unwrap()
        }
    }
}
```

Create `crates/linear-clone/migrations/0003_attachments.sql`:

```sql
CREATE TABLE attachments (
    id TEXT PRIMARY KEY,
    issue_id TEXT NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    url TEXT NOT NULL,
    title TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_attachments_issue ON attachments(issue_id);
```

- [ ] **Step 4: Add `Attachment` type and mutations**

Add to `crates/linear-clone/src/schema.rs`:

```rust
#[derive(SimpleObject, Clone)]
pub struct Attachment {
    pub id: ID,
    pub kind: String,
    pub url: String,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(SimpleObject, Clone)]
pub struct AttachmentConnection { pub nodes: Vec<Attachment> }
```

Add a resolver field on `Issue` for `attachments` that selects rows by `issue_id`. In `MutationRoot` add:

```rust
async fn add_attachment(
    &self,
    ctx: &Context<'_>,
    issue_id: ID,
    url: String,
    title: Option<String>,
) -> Result<Attachment> {
    let pool = ctx.data_unchecked::<SqlitePool>();
    let id = format!("att_{}", Uuid::new_v4());
    let now = Utc::now();
    sqlx::query("INSERT INTO attachments(id, issue_id, kind, url, title, created_at) VALUES (?,?,?,?,?,?)")
        .bind(&id).bind(issue_id.as_str()).bind("pull_request")
        .bind(&url).bind(&title).bind(now.to_rfc3339())
        .execute(pool).await?;
    Ok(Attachment { id: id.into(), kind: "pull_request".into(), url, title, created_at: now })
}

async fn remove_attachment(&self, ctx: &Context<'_>, id: ID) -> Result<bool> {
    let pool = ctx.data_unchecked::<SqlitePool>();
    let n = sqlx::query("DELETE FROM attachments WHERE id = ?")
        .bind(id.as_str()).execute(pool).await?.rows_affected();
    Ok(n > 0)
}
```

- [ ] **Step 5: Run test to verify passing**

Run: `cargo test -p linear-clone --test attachments_tests` → pass.

- [ ] **Step 6: Commit**

```bash
git add crates/linear-clone/migrations/0003_attachments.sql crates/linear-clone/src/schema.rs crates/linear-clone/src/lib.rs crates/linear-clone/tests/attachments_tests.rs
git commit -m "Add attachments table + addAttachment/removeAttachment mutations"
```

---

## Task 4: linear-clone issue-scoped bearer-token auth

**Files:**
- Create: `crates/linear-clone/src/auth.rs`
- Modify: `crates/linear-clone/src/lib.rs` (mount middleware, expose token store)
- Create: `crates/linear-clone/tests/auth_tests.rs`

- [ ] **Step 1: Write the failing test**

`crates/linear-clone/tests/auth_tests.rs`:

```rust
use linear_clone::test_support::TestApp;
use serde_json::json;

#[tokio::test]
async fn token_only_mutates_bound_issue() {
    let app = TestApp::start().await;
    let issue_id = app.first_issue_id().await;
    let other = app.second_issue_id().await;

    let token = app.mint_token(&issue_id).await;

    // Mutating bound issue: ok.
    let ok = app.gql_with_token(&token, json!({
        "query":"mutation($i:ID!,$b:String!){ addComment(issueId:$i, body:$b){ id } }",
        "variables":{"i":issue_id,"b":"hi"}
    })).await;
    assert!(ok["data"]["addComment"]["id"].is_string(), "{ok}");

    // Mutating other issue: rejected.
    let bad = app.gql_with_token(&token, json!({
        "query":"mutation($i:ID!,$b:String!){ addComment(issueId:$i, body:$b){ id } }",
        "variables":{"i":other,"b":"nope"}
    })).await;
    assert!(bad["errors"][0]["message"].as_str().unwrap_or("").contains("issue_token_scope"));
}
```

Add `second_issue_id` and `mint_token` / `gql_with_token` helpers to `test_support` (mirror of existing helpers; `mint_token` calls into a new `auth::TokenStore::issue(issue_id) -> String`).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p linear-clone --test auth_tests` → fails, types missing.

- [ ] **Step 3: Implement `auth.rs`**

```rust
use axum::{extract::State, http::{Request, StatusCode, header}, middleware::Next, response::Response};
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

#[derive(Clone)]
pub struct AuthCtx { pub bound_issue: Option<String> }

pub async fn auth_layer<B>(
    State(store): State<TokenStore>,
    mut req: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    let bound = req.headers().get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .and_then(|t| store.lookup(t.trim()));
    req.extensions_mut().insert(AuthCtx { bound_issue: bound });
    Ok(next.run(req).await)
}
```

In the `add_comment` and `add_attachment` resolvers, before mutating, fetch `AuthCtx` from `ctx.data::<AuthCtx>()`. If `bound_issue` is `Some(other)` and `other != issue_id`, return `Err(async_graphql::Error::new("issue_token_scope mismatch"))`. If `bound_issue` is `None`, allow (back-compat for human-driven UI calls — phase 3 will tighten this).

Wire `TokenStore` and `auth_layer` into `build_router(...)` so the layer runs on `/graphql`. Also propagate `AuthCtx` into the GraphQL context via `axum::middleware::map_request` or by inserting into the schema data via a per-request `Schema::execute(req.data(ctx))`.

- [ ] **Step 4: Run tests to verify passing**

Run: `cargo test -p linear-clone --test auth_tests` → pass.
Run: `cargo test -p linear-clone` → all tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/linear-clone/src/auth.rs crates/linear-clone/src/lib.rs crates/linear-clone/src/schema.rs crates/linear-clone/tests/auth_tests.rs
git commit -m "Add issue-scoped bearer token auth on linear-clone mutations"
```

---

## Task 5: `LinearGraphqlClient` with retry/backoff

**Files:**
- Create: `crates/symphony-core/src/tools/mod.rs`
- Create: `crates/symphony-core/src/tools/linear_graphql.rs`
- Create: `crates/symphony-core/tests/linear_graphql_tool_tests.rs`
- Modify: `crates/symphony-core/Cargo.toml` (add `wiremock = "0.6"` dev-dep)
- Modify: `crates/symphony-core/src/lib.rs` (`pub mod tools;`)

- [ ] **Step 1: Write the failing tests**

`crates/symphony-core/tests/linear_graphql_tool_tests.rs`:

```rust
use serde_json::json;
use symphony_core::tools::linear_graphql::LinearGraphqlClient;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn add_comment_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data":{"addComment":{"id":"c1"}}
        })))
        .mount(&server).await;

    let cli = LinearGraphqlClient::new(format!("{}/graphql", server.uri()), "tok".into());
    let id = cli.add_comment("iss_1", "hello").await.unwrap();
    assert_eq!(id, "c1");
}

#[tokio::test]
async fn retries_5xx_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/graphql"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2).mount(&server).await;
    Mock::given(method("POST")).and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data":{"addComment":{"id":"c2"}}
        }))).mount(&server).await;

    let cli = LinearGraphqlClient::new(format!("{}/graphql", server.uri()), "tok".into());
    let id = cli.add_comment("iss_1","hi").await.unwrap();
    assert_eq!(id, "c2");
}

#[tokio::test]
async fn forbidden_is_not_retried() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/graphql"))
        .respond_with(ResponseTemplate::new(403))
        .expect(1).mount(&server).await;

    let cli = LinearGraphqlClient::new(format!("{}/graphql", server.uri()), "tok".into());
    let err = cli.add_comment("iss_1","hi").await.err().unwrap();
    assert!(err.to_string().contains("403"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p symphony-core --test linear_graphql_tool_tests` → compile error.

- [ ] **Step 3: Implement `tools::linear_graphql`**

`crates/symphony-core/src/tools/mod.rs`:

```rust
pub mod linear_graphql;
```

`crates/symphony-core/src/tools/linear_graphql.rs`:

```rust
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
        Self { endpoint, token, client }
    }

    pub async fn get_issue(&self, id: &str) -> Result<Value> {
        let q = "query($i:ID!){ issue(id:$i){ id identifier title description state{ name } comments{ nodes{ id body createdAt } } } }";
        self.run(q, json!({"i": id})).await.map(|v| v["issue"].clone())
    }

    pub async fn list_comments(&self, id: &str) -> Result<Value> {
        let q = "query($i:ID!){ issue(id:$i){ comments{ nodes{ id body createdAt } } } }";
        self.run(q, json!({"i": id})).await.map(|v| v["issue"]["comments"]["nodes"].clone())
    }

    pub async fn add_comment(&self, issue_id: &str, body: &str) -> Result<String> {
        let q = "mutation($i:ID!,$b:String!){ addComment(issueId:$i, body:$b){ id } }";
        let v = self.run(q, json!({"i": issue_id, "b": body})).await?;
        v["addComment"]["id"].as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow!("addComment.id missing"))
    }

    pub async fn link_pull_request(&self, issue_id: &str, url: &str, title: Option<&str>) -> Result<String> {
        let q = "mutation($i:ID!,$u:String!,$t:String){ addAttachment(issueId:$i,url:$u,title:$t){ id } }";
        let v = self.run(q, json!({"i": issue_id, "u": url, "t": title})).await?;
        v["addAttachment"]["id"].as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow!("addAttachment.id missing"))
    }

    async fn run(&self, query: &str, variables: Value) -> Result<Value> {
        let body = json!({"query": query, "variables": variables});
        let mut delay_ms = 200u64;
        let mut last_err = None;
        for _ in 0..3 {
            let resp = self.client.post(&self.endpoint)
                .bearer_auth(&self.token)
                .json(&body)
                .send().await;
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
                        return Err(anyhow!("http {}: {}", status, r.text().await.unwrap_or_default()));
                    }
                    last_err = Some(anyhow!("http {}", status));
                }
                Err(e) => { last_err = Some(anyhow!(e)); }
            }
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            delay_ms = (delay_ms * 2).min(2_000);
        }
        Err(last_err.unwrap_or_else(|| anyhow!("retry_exhausted")))
    }
}
```

- [ ] **Step 4: Run tests to verify passing**

Run: `cargo test -p symphony-core --test linear_graphql_tool_tests` → all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/symphony-core/src/tools crates/symphony-core/src/lib.rs crates/symphony-core/Cargo.toml crates/symphony-core/tests/linear_graphql_tool_tests.rs
git commit -m "Add LinearGraphqlClient with retry/backoff and 4xx fast-fail"
```

---

## Task 6: VCS module (`push_branch` + `open_pr`)

**Files:**
- Create: `crates/symphony-core/src/vcs.rs`
- Create: `crates/symphony-core/tests/vcs_tests.rs`
- Modify: `crates/symphony-core/src/lib.rs` (`pub mod vcs;`)

- [ ] **Step 1: Write the failing test**

`crates/symphony-core/tests/vcs_tests.rs`:

```rust
use std::path::PathBuf;
use std::process::Command;
use symphony_core::vcs::{push_branch, open_pr};
use tempfile::TempDir;

fn init_workspace_with_remote() -> (TempDir, TempDir) {
    let remote = TempDir::new().unwrap();
    Command::new("git").args(["init","--bare","-q"]).current_dir(remote.path()).status().unwrap();
    let work = TempDir::new().unwrap();
    Command::new("git").args(["init","-q","-b","main"]).current_dir(work.path()).status().unwrap();
    Command::new("git").args(["config","user.email","t@t"]).current_dir(work.path()).status().unwrap();
    Command::new("git").args(["config","user.name","t"]).current_dir(work.path()).status().unwrap();
    std::fs::write(work.path().join("a.txt"),"hi").unwrap();
    Command::new("git").args(["add","-A"]).current_dir(work.path()).status().unwrap();
    Command::new("git").args(["commit","-qm","init"]).current_dir(work.path()).status().unwrap();
    Command::new("git").args(["remote","add","origin", &remote.path().to_string_lossy()]).current_dir(work.path()).status().unwrap();
    (work, remote)
}

#[tokio::test]
async fn push_branch_creates_ref_on_remote() {
    let (work, remote) = init_workspace_with_remote();
    push_branch(work.path(), "origin", "symphony/DEMO-1").await.unwrap();
    let out = Command::new("git").args(["--git-dir", &remote.path().to_string_lossy(), "show-ref"]).output().unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("refs/heads/symphony/DEMO-1"), "ref missing in remote: {s}");
}

#[tokio::test]
async fn open_pr_uses_gh_shim_and_returns_url() {
    let (work, _r) = init_workspace_with_remote();
    let shim_dir = TempDir::new().unwrap();
    let shim = shim_dir.path().join(if cfg!(windows) {"gh.cmd"} else {"gh"});
    let body = if cfg!(windows) {
        "@echo {\"url\":\"https://github.com/o/r/pull/42\"}\r\n"
    } else {
        "#!/usr/bin/env bash\necho '{\"url\":\"https://github.com/o/r/pull/42\"}'\n"
    };
    std::fs::write(&shim, body).unwrap();
    #[cfg(unix)]
    { use std::os::unix::fs::PermissionsExt; let mut p = std::fs::metadata(&shim).unwrap().permissions(); p.set_mode(0o755); std::fs::set_permissions(&shim, p).unwrap(); }

    let path = format!("{}{}{}", shim_dir.path().display(),
        if cfg!(windows) {";"} else {":"},
        std::env::var("PATH").unwrap_or_default());
    std::env::set_var("PATH", path);

    let url = open_pr(work.path(), "feat: x", "body").await.unwrap();
    assert_eq!(url, "https://github.com/o/r/pull/42");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p symphony-core --test vcs_tests` → compile error.

- [ ] **Step 3: Implement `vcs.rs`**

```rust
use anyhow::{anyhow, Context, Result};
use std::path::Path;
use tokio::process::Command;

pub async fn push_branch(workspace: &Path, remote: &str, branch: &str) -> Result<()> {
    let out = Command::new("git")
        .args(["push","-u",remote, &format!("HEAD:refs/heads/{branch}")])
        .current_dir(workspace)
        .output().await.context("spawning git push")?;
    if !out.status.success() {
        return Err(anyhow!("git push failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(())
}

pub async fn open_pr(workspace: &Path, title: &str, body: &str) -> Result<String> {
    let out = Command::new("gh")
        .args(["pr","create","--title",title,"--body",body,"--json","url","-q",".url"])
        .current_dir(workspace)
        .output().await.context("spawning gh")?;
    if !out.status.success() {
        return Err(anyhow!("gh pr create failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    // Some shims emit JSON; tolerate either.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
        if let Some(u) = v["url"].as_str() { return Ok(u.to_string()); }
    }
    if s.starts_with("http") { return Ok(s); }
    Err(anyhow!("could not parse gh output: {s}"))
}
```

Add `pub mod vcs;` to `crates/symphony-core/src/lib.rs`. Add `tempfile` as dev-dep if not present.

- [ ] **Step 4: Run tests to verify passing**

Run: `cargo test -p symphony-core --test vcs_tests` → both pass.

- [ ] **Step 5: Commit**

```bash
git add crates/symphony-core/src/vcs.rs crates/symphony-core/src/lib.rs crates/symphony-core/tests/vcs_tests.rs crates/symphony-core/Cargo.toml
git commit -m "Add vcs::push_branch and vcs::open_pr with bare-repo + gh-shim tests"
```

---

## Task 7: SSE endpoint `/api/v1/events`

**Files:**
- Modify: `crates/symphony-core/src/orchestrator.rs` (expose `event_bus()` accessor on `Orchestrator`)
- Modify: `crates/symphony/src/http.rs` (add SSE route)
- Modify: `crates/symphony/Cargo.toml` (add `tokio-stream = { version="0.1", features=["sync"] }`)

- [ ] **Step 1: Write the failing test**

Add to `crates/symphony-core/tests/orchestrator_tests.rs`:

```rust
#[tokio::test]
async fn orchestrator_exposes_event_bus() {
    let orch = symphony_core::orchestrator::Orchestrator::test_default().await;
    let _ = orch.event_bus().subscribe();
    orch.event_bus().send(symphony_core::events::broadcast::OrchestratorEvent::Resync).unwrap();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p symphony-core --test orchestrator_tests` → method missing.

- [ ] **Step 3: Add `event_bus()` accessor**

In `Orchestrator`, store `event_bus: OrchestratorEventBus` (initialized with capacity 256 in the constructor). Add:

```rust
pub fn event_bus(&self) -> &crate::events::broadcast::OrchestratorEventBus {
    &self.event_bus
}
```

- [ ] **Step 4: Add SSE route in `symphony::http`**

```rust
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{Stream, StreamExt};
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;

async fn api_events(
    State(orch): State<Orchestrator>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String,String>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let issue_filter = q.get("issue").cloned();
    let rx = orch.event_bus().subscribe();
    let stream = BroadcastStream::new(rx).filter_map(move |item| {
        let f = issue_filter.clone();
        async move {
            match item {
                Ok(evt) => {
                    if let Some(want) = f.as_ref() {
                        let issue = match &evt {
                            symphony_core::events::broadcast::OrchestratorEvent::AgentEvent { issue_id, .. } => Some(issue_id.clone()),
                            symphony_core::events::broadcast::OrchestratorEvent::ToolCall { issue_id, .. } => Some(issue_id.clone()),
                            symphony_core::events::broadcast::OrchestratorEvent::ToolResult { issue_id, .. } => Some(issue_id.clone()),
                            symphony_core::events::broadcast::OrchestratorEvent::ApprovalRequest { issue_id, .. } => Some(issue_id.clone()),
                            symphony_core::events::broadcast::OrchestratorEvent::ApprovalDecision { issue_id, .. } => Some(issue_id.clone()),
                            symphony_core::events::broadcast::OrchestratorEvent::VcsPushed { issue_id, .. } => Some(issue_id.clone()),
                            symphony_core::events::broadcast::OrchestratorEvent::PrOpened { issue_id, .. } => Some(issue_id.clone()),
                            symphony_core::events::broadcast::OrchestratorEvent::VcsError { issue_id, .. } => Some(issue_id.clone()),
                            symphony_core::events::broadcast::OrchestratorEvent::Resync => None,
                        };
                        if issue != Some(want.clone()) && !matches!(evt, symphony_core::events::broadcast::OrchestratorEvent::Resync) {
                            return None;
                        }
                    }
                    let data = serde_json::to_string(&evt).unwrap();
                    Some(Ok(Event::default().data(data)))
                }
                Err(_) => Some(Ok(Event::default().event("resync").data("{}"))),
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
```

Mount: `.route("/api/v1/events", get(api_events))` on the router.

- [ ] **Step 5: Manually smoke**

Build, run `cargo run -p symphony -- --workflow WORKFLOW.md`, then in PowerShell:

```pwsh
curl.exe -N http://127.0.0.1:8080/api/v1/events
```

Expected: connection holds open with periodic keep-alives. Send a test event from another shell (or wait for an agent to dispatch).

- [ ] **Step 6: Commit**

```bash
git add crates/symphony-core/src/orchestrator.rs crates/symphony-core/tests/orchestrator_tests.rs crates/symphony/src/http.rs crates/symphony/Cargo.toml
git commit -m "Add SSE /api/v1/events with optional ?issue= filter"
```

---

## Task 8: ApprovalRouter (oneshot map + timeout)

**Files:**
- Create: `crates/symphony-core/src/harness/approvals.rs`
- Modify: `crates/symphony-core/src/harness/mod.rs` (re-export)
- Create: `crates/symphony-core/tests/approval_flow_tests.rs`

- [ ] **Step 1: Write the failing test**

```rust
use std::time::Duration;
use symphony_core::harness::approvals::ApprovalRouter;

#[tokio::test]
async fn approve_resolves_decision() {
    let router = ApprovalRouter::new();
    let id = "ap_1".to_string();
    let pending = router.register(id.clone());
    assert!(router.resolve(&id, true, None));
    let decision = pending.wait(Duration::from_millis(500)).await.unwrap();
    assert!(decision.allow);
}

#[tokio::test]
async fn timeout_defaults_deny() {
    let router = ApprovalRouter::new();
    let id = "ap_2".to_string();
    let pending = router.register(id.clone());
    let decision = pending.wait(Duration::from_millis(50)).await.unwrap();
    assert!(!decision.allow);
    assert_eq!(decision.reason.as_deref(), Some("timeout"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p symphony-core --test approval_flow_tests` → compile error.

- [ ] **Step 3: Implement `approvals.rs`**

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;

#[derive(Debug, Clone)]
pub struct Decision { pub allow: bool, pub reason: Option<String> }

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
    pub fn new() -> Self { Self::default() }

    pub fn register(&self, id: String) -> PendingApproval {
        let (tx, rx) = oneshot::channel();
        self.inner.lock().unwrap().insert(id, tx);
        PendingApproval { rx }
    }

    pub fn resolve(&self, id: &str, allow: bool, reason: Option<String>) -> bool {
        if let Some(tx) = self.inner.lock().unwrap().remove(id) {
            tx.send(Decision { allow, reason }).is_ok()
        } else { false }
    }
}
```

Add `pub mod approvals;` to `crates/symphony-core/src/harness/mod.rs`.

- [ ] **Step 4: Run tests to verify passing**

Run: `cargo test -p symphony-core --test approval_flow_tests` → both pass.

- [ ] **Step 5: Commit**

```bash
git add crates/symphony-core/src/harness/approvals.rs crates/symphony-core/src/harness/mod.rs crates/symphony-core/tests/approval_flow_tests.rs
git commit -m "Add ApprovalRouter (oneshot map + timeout-deny)"
```

---

## Task 9: HTTP `POST /api/v1/approvals/{id}` and `POST /api/v1/{id}/open-pr`

**Files:**
- Modify: `crates/symphony-core/src/orchestrator.rs` (expose `approval_router()`, `retry_open_pr(issue_identifier)`)
- Modify: `crates/symphony/src/http.rs`

- [ ] **Step 1: Add handlers**

```rust
async fn api_approve(
    State(orch): State<Orchestrator>,
    Path(approval_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let allow = body.get("allow").and_then(|v| v.as_bool()).unwrap_or(false);
    let reason = body.get("reason").and_then(|v| v.as_str()).map(str::to_string);
    let resolved = orch.approval_router().resolve(&approval_id, allow, reason);
    if resolved {
        (StatusCode::OK, Json(json!({"resolved": true})))
    } else {
        (StatusCode::NOT_FOUND, Json(json!({"resolved": false, "reason":"unknown_or_already_resolved"})))
    }
}

async fn api_retry_pr(
    State(orch): State<Orchestrator>,
    Path(identifier): Path<String>,
) -> impl IntoResponse {
    match orch.retry_open_pr(&identifier).await {
        Ok(url) => (StatusCode::OK, Json(json!({"url": url}))).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({"error": e.to_string()}))).into_response(),
    }
}
```

Mount routes:

```rust
.route("/api/v1/approvals/{approval_id}", post(api_approve))
.route("/api/v1/{identifier}/open-pr", post(api_retry_pr))
```

- [ ] **Step 2: Stub `Orchestrator::retry_open_pr`**

For now: return `Err(anyhow!("not_implemented"))`. Task 14 will fill it in once the VCS pipeline exists.

- [ ] **Step 3: Add an integration test**

Append to `crates/symphony-core/tests/approval_flow_tests.rs`:

```rust
#[tokio::test]
async fn http_resolves_approval_via_router() {
    use symphony_core::harness::approvals::ApprovalRouter;
    let router = ApprovalRouter::new();
    let pending = router.register("ap_x".into());
    // Simulate the HTTP handler call:
    assert!(router.resolve("ap_x", true, Some("operator".into())));
    let d = pending.wait(std::time::Duration::from_millis(200)).await.unwrap();
    assert!(d.allow);
    assert_eq!(d.reason.as_deref(), Some("operator"));
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p symphony-core --test approval_flow_tests
cargo check -p symphony
git add crates/symphony-core/src/orchestrator.rs crates/symphony/src/http.rs crates/symphony-core/tests/approval_flow_tests.rs
git commit -m "Add /api/v1/approvals and /api/v1/<id>/open-pr endpoints"
```

---

## Task 10: HarnessContext — wire bus + policy + approval router into harness API

**Files:**
- Modify: `crates/symphony-core/src/harness/mod.rs` (introduce `HarnessContext`, change `Harness::run` signature)
- Modify: `crates/symphony-core/src/harness/{claude_code,hermes,codex_stub}.rs` (accept new signature, ignore new fields for now)
- Modify: `crates/symphony-core/src/orchestrator.rs` (build and pass `HarnessContext`)

- [ ] **Step 1: Define `HarnessContext`**

```rust
pub struct HarnessContext<'a> {
    pub workspace: &'a std::path::Path,
    pub prompt: &'a str,
    pub cfg: &'a EffectiveConfig,
    pub tx: tokio::sync::mpsc::Sender<AgentEvent>,
    pub bus: crate::events::broadcast::OrchestratorEventBus,
    pub approval_router: crate::harness::approvals::ApprovalRouter,
    pub policy: crate::policy::Policy,
    pub linear_token: Option<String>,
    pub linear_endpoint: Option<String>,
    pub issue_id: String,
}
```

Replace the `Harness::run(...)` trait with:

```rust
#[async_trait]
pub trait Harness: Send + Sync {
    fn name(&self) -> &'static str;
    async fn run(&self, ctx: HarnessContext<'_>) -> Result<HarnessOutcome>;
}
```

Update each harness impl to take `ctx` and use `ctx.workspace`, `ctx.prompt`, `ctx.tx` for the existing behavior. New fields stay unused in `hermes` and `codex_stub` for slice 1.

- [ ] **Step 2: Build the context in the orchestrator**

Where the orchestrator calls `harness.run(...)`, build a `HarnessContext` with real values. `linear_token` is `None` for now (filled in Task 13). `policy` from `cfg.policy.clone()`. `issue_id` from the active claim.

- [ ] **Step 3: Run + commit**

```bash
cargo test --workspace
git add crates/symphony-core/src/harness crates/symphony-core/src/orchestrator.rs
git commit -m "Introduce HarnessContext threading bus/policy/approval into harnesses"
```

---

## Task 11: MCP bridge — in-process MCP server exposing `linear_graphql.*`

**Files:**
- Create: `crates/symphony-core/src/harness/mcp_bridge.rs`
- Modify: `crates/symphony-core/Cargo.toml` (add `rmcp = "0.2"` or chosen MCP crate; if no suitable crate, see Step 3 alt)
- Create: `crates/symphony-core/tests/mcp_bridge_tests.rs`

- [ ] **Step 1: Decide on transport**

Claude Code accepts MCP servers via `--mcp-config <path-to-json>`. The JSON entry can be either `{"command":"...","args":[...]}` (Claude spawns it) or `{"url":"http://..."}` (HTTP). For in-process control we run a tiny **stdio MCP server as a child process** of the orchestrator and pass its `command`+`args` through the JSON config: orchestrator writes a temp config like:

```json
{"mcpServers":{"linear":{"command":"<this binary>","args":["mcp-bridge","--issue","DEMO-1","--port-token","<random>"]}}}
```

Symphony's binary gains a hidden subcommand `mcp-bridge` that runs the MCP server reading `SYMPHONY_LINEAR_TOKEN` and `SYMPHONY_LINEAR_ENDPOINT` from env. This avoids reimplementing MCP framing — a small `rmcp::server` + `rmcp::tool!` macro keeps it compact.

- [ ] **Step 2: Write the failing test**

```rust
use symphony_core::harness::mcp_bridge::generate_mcp_config_json;

#[test]
fn config_json_lists_linear_server() {
    let exe = std::path::PathBuf::from("/usr/bin/symphony");
    let s = generate_mcp_config_json(&exe, "DEMO-1");
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert!(v["mcpServers"]["linear"]["command"].is_string());
    let args = v["mcpServers"]["linear"]["args"].as_array().unwrap();
    assert!(args.iter().any(|a| a == "mcp-bridge"));
    assert!(args.iter().any(|a| a == "DEMO-1"));
}
```

- [ ] **Step 3: Implement the bridge module**

```rust
use serde_json::json;
use std::path::Path;

pub fn generate_mcp_config_json(symphony_exe: &Path, issue_identifier: &str) -> String {
    json!({
        "mcpServers": {
            "linear": {
                "command": symphony_exe.to_string_lossy(),
                "args": ["mcp-bridge", "--issue", issue_identifier]
            }
        }
    }).to_string()
}

/// Run the stdio MCP server. Reads SYMPHONY_LINEAR_TOKEN + SYMPHONY_LINEAR_ENDPOINT.
/// Exposes 4 tools: get_issue, list_comments, add_comment, link_pull_request.
pub async fn run_mcp_server(issue_id: String) -> anyhow::Result<()> {
    use crate::tools::linear_graphql::LinearGraphqlClient;
    let token = std::env::var("SYMPHONY_LINEAR_TOKEN")?;
    let endpoint = std::env::var("SYMPHONY_LINEAR_ENDPOINT")?;
    let cli = LinearGraphqlClient::new(endpoint, token);

    // Use rmcp (or fall back to a hand-rolled JSON-RPC stdio loop).
    // Pseudocode: register 4 tools wrapping cli.{get_issue, list_comments, add_comment, link_pull_request}.
    // For each tool that mutates (add_comment, link_pull_request), unconditionally enforce issue_id matches input (defense-in-depth).
    crate::harness::mcp_bridge_impl::serve_stdio(cli, issue_id).await
}
```

If `rmcp` is unavailable, hand-roll a minimal stdio JSON-RPC 2.0 server in `mcp_bridge_impl.rs` that handles `initialize`, `tools/list`, `tools/call`. Keep it small (~150 LOC).

Add a hidden CLI subcommand to `crates/symphony/src/main.rs`:

```rust
if std::env::args().nth(1).as_deref() == Some("mcp-bridge") {
    // parse --issue
    let issue = parse_arg("--issue").expect("--issue required");
    return tokio::runtime::Runtime::new()?.block_on(symphony_core::harness::mcp_bridge::run_mcp_server(issue));
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p symphony-core --test mcp_bridge_tests
git add crates/symphony-core/src/harness/mcp_bridge.rs crates/symphony-core/Cargo.toml crates/symphony/src/main.rs crates/symphony-core/tests/mcp_bridge_tests.rs
git commit -m "Add MCP bridge subcommand and config-JSON helper for linear_graphql tools"
```

---

## Task 12: Claude Code policy translation + MCP wiring + approval interception

**Files:**
- Modify: `crates/symphony-core/src/harness/claude_code.rs`

- [ ] **Step 1: Add policy translation**

```rust
use crate::policy::{Policy, PermissionMode, SandboxProfile};

fn translate_policy(p: &Policy) -> Vec<String> {
    let mode = match p.permission_mode {
        PermissionMode::AcceptEdits => "acceptEdits",
        PermissionMode::RequireApproval => "default",
        PermissionMode::ReadOnly => "plan",
    };
    let mut args = vec!["--permission-mode".into(), mode.into()];
    if !p.allowed_tools.is_empty() {
        args.push("--allowedTools".into());
        args.push(p.allowed_tools.join(","));
    }
    args
}
```

- [ ] **Step 2: Write MCP config and pass `--mcp-config`**

In `ClaudeCodeHarness::run`, before spawning `claude`:

1. Build `mcp_config_json` via `generate_mcp_config_json(&std::env::current_exe()?, &ctx.issue_id)`.
2. Write it to `<workspace>/.symphony-mcp.json`.
3. Add CLI flags: `--mcp-config <that path>`.
4. Set env on the spawned process: `SYMPHONY_LINEAR_TOKEN`, `SYMPHONY_LINEAR_ENDPOINT` (from `ctx.linear_token` / `ctx.linear_endpoint`; if `None`, skip MCP wiring entirely).

Replace the hard-coded `--permission-mode acceptEdits` with output of `translate_policy(&ctx.policy)`.

- [ ] **Step 3: Intercept tool_use stream-json events**

When parsing stdout, in addition to the existing `translate_claude_event`, peek for `type=="assistant"` messages whose content array contains a `tool_use` block (Claude Code emits these inline). For each:

- Publish `OrchestratorEvent::ToolCall { issue_id, tool, input }` on `ctx.bus`.

When the policy is `RequireApproval`, Claude Code's permission system will trigger an approval request via its own permission flow; capture this from the stream and translate to `OrchestratorEvent::ApprovalRequest`. **For slice 1, document a known limitation:** Claude Code's hosted permission UI is the source of truth — Symphony's dashboard surfaces requests but cannot itself approve them via stdin, so under `RequireApproval` Symphony forwards the request to the dashboard, the operator clicks Approve, and the orchestrator just emits `ApprovalDecision` for transcript logging. The actual gating happens inside Claude Code based on the `--permission-mode` flag. In practice, for v1 phase 1 we recommend running `acceptEdits` so the tool calls flow without prompts; the approval channel becomes load-bearing in slice 2 (Codex), where we own the approval round-trip.

Add a comment in code explaining this so it's clear why the channel exists but the gate is one-way for Claude Code.

- [ ] **Step 4: Manual smoke**

```pwsh
cargo run -p symphony -- --workflow WORKFLOW.md
# In another terminal: trigger an issue and watch the dashboard SSE feed.
curl.exe -N "http://127.0.0.1:8080/api/v1/events?issue=DEMO-1"
```

Expected: see `tool_call` events when the agent calls `linear_graphql.add_comment`.

- [ ] **Step 5: Commit**

```bash
git add crates/symphony-core/src/harness/claude_code.rs
git commit -m "Wire policy flags, MCP config, and tool_use surfacing into Claude Code harness"
```

---

## Task 13: Orchestrator — token minting and HarnessContext linear_* fields

**Files:**
- Modify: `crates/symphony-core/src/orchestrator.rs`
- Modify: `crates/linear-clone/src/lib.rs` (expose `TokenStore` from `build_router`-adjacent API; orchestrator drives a separate linear-clone instance via HTTP, so token minting needs an HTTP route)
- Modify: `crates/linear-clone/src/auth.rs` (add `POST /admin/tokens` route minting against the in-process store; gated by an admin secret env var `LINEAR_CLONE_ADMIN_TOKEN`)

- [ ] **Step 1: Mint endpoint on linear-clone**

```rust
// in linear-clone
async fn admin_mint_token(
    State(store): State<TokenStore>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let want = std::env::var("LINEAR_CLONE_ADMIN_TOKEN").unwrap_or_default();
    let got = headers.get("x-admin-token").and_then(|v| v.to_str().ok()).unwrap_or_default();
    if want.is_empty() || got != want { return Err(StatusCode::UNAUTHORIZED); }
    let issue_id = body.get("issue_id").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let token = store.issue(issue_id);
    Ok(Json(serde_json::json!({"token": token})))
}
```

Mount: `.route("/admin/tokens", post(admin_mint_token))`.

- [ ] **Step 2: Orchestrator mints a token before each dispatch**

When the harness is `claude_code` (or any harness in slice 1 that uses the tool), before building `HarnessContext`:

```rust
async fn mint_linear_token(cfg: &EffectiveConfig, issue_id: &str) -> Option<String> {
    let endpoint = cfg.tracker.endpoint.as_ref()?;
    if !cfg.tracker.kind.eq_ignore_ascii_case("linear") { return None; }
    let admin = std::env::var("LINEAR_CLONE_ADMIN_TOKEN").ok()?;
    let cli = reqwest::Client::new();
    let url = endpoint.trim_end_matches("/graphql").to_string() + "/admin/tokens";
    let resp = cli.post(&url)
        .header("x-admin-token", admin)
        .json(&serde_json::json!({"issue_id": issue_id}))
        .send().await.ok()?;
    if !resp.status().is_success() { return None; }
    let v: serde_json::Value = resp.json().await.ok()?;
    v["token"].as_str().map(str::to_string)
}
```

Set `ctx.linear_token = Some(token)` and `ctx.linear_endpoint = cfg.tracker.endpoint.clone()`.

- [ ] **Step 3: Test**

Add an orchestrator integration test that spins up a real linear-clone, sets `LINEAR_CLONE_ADMIN_TOKEN`, calls `mint_linear_token` (expose it as `pub` for testing), and asserts a non-empty token comes back and the token validates issue scope.

- [ ] **Step 4: Commit**

```bash
cargo test --workspace
git add crates/linear-clone/src/auth.rs crates/linear-clone/src/lib.rs crates/symphony-core/src/orchestrator.rs crates/symphony-core/tests/orchestrator_tests.rs
git commit -m "Mint per-issue tokens via linear-clone /admin/tokens before dispatch"
```

---

## Task 14: VCS pipeline + follow-up turn injection

**Files:**
- Modify: `crates/symphony-core/src/orchestrator.rs`
- Modify: `crates/symphony-core/src/config.rs` (add `vcs:` block)
- Modify: `WORKFLOW.md` (document `vcs:` block)

- [ ] **Step 1: Add `VcsConfig`**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VcsConfig {
    #[serde(default)]
    pub remote: Option<String>,             // e.g., "origin"
    #[serde(default)]
    pub branch_prefix: Option<String>,       // default "symphony/"
    #[serde(default)]
    pub auto_open_pr: bool,
}
```

Wire into `EffectiveConfig`. Default `remote=None` disables push.

- [ ] **Step 2: After turn_complete success, run pipeline**

In the orchestrator's post-turn handler (where `HarnessOutcome.success == true`):

```rust
async fn post_turn_vcs(
    cfg: &EffectiveConfig,
    workspace: &Path,
    issue_id: &str,
    issue_identifier: &str,
    issue_title: &str,
    bus: &OrchestratorEventBus,
) -> Option<String> {
    let remote = cfg.vcs.remote.as_ref()?;
    let prefix = cfg.vcs.branch_prefix.as_deref().unwrap_or("symphony/");
    let branch = format!("{prefix}{issue_identifier}");

    if let Err(e) = crate::vcs::push_branch(workspace, remote, &branch).await {
        let _ = bus.send(OrchestratorEvent::VcsError {
            issue_id: issue_id.into(), stage: "push".into(), message: e.to_string(),
        });
        return None;
    }
    let _ = bus.send(OrchestratorEvent::VcsPushed { issue_id: issue_id.into(), branch: branch.clone() });

    if !cfg.vcs.auto_open_pr { return None; }

    let title = format!("{issue_identifier}: {issue_title}");
    let body = format!("Authored by Symphony for {issue_identifier}.");
    match crate::vcs::open_pr(workspace, &title, &body).await {
        Ok(url) => {
            let _ = bus.send(OrchestratorEvent::PrOpened { issue_id: issue_id.into(), url: url.clone() });
            Some(url)
        }
        Err(e) => {
            let _ = bus.send(OrchestratorEvent::VcsError {
                issue_id: issue_id.into(), stage: "open_pr".into(), message: e.to_string(),
            });
            None
        }
    }
}
```

- [ ] **Step 3: Inject follow-up turn for `linkPullRequest`**

If `post_turn_vcs` returned `Some(url)`, schedule a follow-up turn for the same issue with prompt:

> "PR opened at {url}. Call `linear_graphql.link_pull_request` with that URL and a short title to attach it to issue {identifier}, then end the turn."

The orchestrator already supports a continuation cycle — reuse it: enqueue a `Continuation` claim with `turn_count += 1` and `prompt_override = Some(prompt)`. Cap at 1 follow-up to avoid loops.

- [ ] **Step 4: Implement `Orchestrator::retry_open_pr`**

```rust
pub async fn retry_open_pr(&self, identifier: &str) -> anyhow::Result<String> {
    let snap = self.snapshot().await;
    let run = snap.running.values().find(|r| r.issue.identifier == identifier)
        .ok_or_else(|| anyhow!("issue not currently running"))?;
    let url = post_turn_vcs(/* ... */).await
        .ok_or_else(|| anyhow!("vcs pipeline produced no url"))?;
    Ok(url)
}
```

- [ ] **Step 5: Run + commit**

```bash
cargo test --workspace
git add crates/symphony-core/src/orchestrator.rs crates/symphony-core/src/config.rs WORKFLOW.md
git commit -m "Add post-turn VCS pipeline, follow-up linkPullRequest turn, retry_open_pr"
```

---

## Task 15: Web UI — `useEventStream` hook

**Files:**
- Create: `web/src/hooks/useEventStream.ts`

- [ ] **Step 1: Write the hook**

```ts
import { useEffect, useState } from "react";

export type OrchestratorEvent =
  | { kind: "tool_call"; issue_id: string; tool: string; input: unknown }
  | { kind: "tool_result"; issue_id: string; tool: string; output: unknown; error?: string }
  | { kind: "approval_request"; issue_id: string; approval_id: string; tool: string; input: unknown }
  | { kind: "approval_decision"; issue_id: string; approval_id: string; allow: boolean; reason?: string }
  | { kind: "vcs_pushed"; issue_id: string; branch: string }
  | { kind: "pr_opened"; issue_id: string; url: string }
  | { kind: "vcs_error"; issue_id: string; stage: string; message: string }
  | { kind: "agent_event"; issue_id: string; event: unknown }
  | { kind: "resync" };

export function useEventStream(issueId?: string) {
  const [events, setEvents] = useState<OrchestratorEvent[]>([]);

  useEffect(() => {
    const url = issueId ? `/api/v1/events?issue=${encodeURIComponent(issueId)}` : "/api/v1/events";
    const es = new EventSource(url);
    es.onmessage = (e) => {
      try { setEvents((prev) => [...prev.slice(-499), JSON.parse(e.data)]); }
      catch { /* ignore */ }
    };
    es.onerror = () => {
      es.close();
      // EventSource auto-reconnects, but we recreate it on next render.
      // The orchestrator emits a `resync` event on lag, which clients use
      // to refetch /api/v1/state and resume.
    };
    return () => es.close();
  }, [issueId]);

  return events;
}
```

- [ ] **Step 2: Smoke**

`cd web && npm run dev`, open <http://localhost:5173>, use the hook in a temporary debug component, watch events arrive.

- [ ] **Step 3: Commit**

```bash
git add web/src/hooks/useEventStream.ts
git commit -m "Add useEventStream hook for /api/v1/events SSE"
```

---

## Task 16: Web UI — `<ApprovalToast>` component

**Files:**
- Create: `web/src/components/ApprovalToast.tsx`
- Modify: `web/src/App.tsx` (mount globally)

- [ ] **Step 1: Component**

```tsx
import { useState, useEffect } from "react";
import { useEventStream } from "../hooks/useEventStream";

type Pending = { approval_id: string; issue_id: string; tool: string; input: unknown };

export function ApprovalToast() {
  const [pending, setPending] = useState<Pending[]>([]);
  const events = useEventStream();

  useEffect(() => {
    for (const e of events) {
      if (e.kind === "approval_request") {
        setPending((p) => [...p, { approval_id: e.approval_id, issue_id: e.issue_id, tool: e.tool, input: e.input }]);
      } else if (e.kind === "approval_decision") {
        setPending((p) => p.filter((x) => x.approval_id !== e.approval_id));
      }
    }
  }, [events]);

  async function decide(id: string, allow: boolean) {
    await fetch(`/api/v1/approvals/${id}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ allow, reason: allow ? "operator" : "denied" }),
    });
    setPending((p) => p.filter((x) => x.approval_id !== id));
  }

  if (pending.length === 0) return null;
  return (
    <div className="fixed bottom-4 right-4 flex flex-col gap-2 z-50">
      {pending.map((p) => (
        <div key={p.approval_id} className="bg-zinc-900 border border-zinc-700 rounded-lg p-3 w-80 shadow-lg">
          <div className="text-xs text-zinc-400">{p.issue_id}</div>
          <div className="text-sm font-medium">{p.tool}</div>
          <pre className="text-xs text-zinc-300 max-h-24 overflow-auto bg-zinc-950 p-2 rounded">{JSON.stringify(p.input, null, 2)}</pre>
          <div className="flex gap-2 mt-2">
            <button onClick={() => decide(p.approval_id, true)} className="flex-1 bg-emerald-600 hover:bg-emerald-500 text-white rounded px-2 py-1 text-sm">Approve</button>
            <button onClick={() => decide(p.approval_id, false)} className="flex-1 bg-rose-600 hover:bg-rose-500 text-white rounded px-2 py-1 text-sm">Deny</button>
          </div>
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Step 2: Mount in `App.tsx`**

Render `<ApprovalToast />` once at the root.

- [ ] **Step 3: Commit**

```bash
git add web/src/components/ApprovalToast.tsx web/src/App.tsx
git commit -m "Add ApprovalToast component fed by SSE event stream"
```

---

## Task 17: Web UI — issue panel live event feed + attachment chips

**Files:**
- Create: `web/src/components/AttachmentChip.tsx`
- Modify: `web/src/views/IssuePanel.tsx` (or equivalent — the existing issue detail view)
- Modify: `web/src/graphql.ts` (add `attachments` to issue query)

- [ ] **Step 1: GraphQL — request attachments**

In the issue detail query add:

```graphql
attachments { nodes { id url title kind createdAt } }
```

- [ ] **Step 2: Render `AttachmentChip`**

```tsx
export function AttachmentChip({ url, title }: { url: string; title?: string }) {
  return (
    <a href={url} target="_blank" rel="noreferrer"
       className="inline-flex items-center gap-1 text-xs px-2 py-1 rounded-full bg-zinc-800 hover:bg-zinc-700 border border-zinc-700">
      <span className="text-emerald-400">PR</span>
      <span className="truncate max-w-[16rem]">{title ?? url}</span>
    </a>
  );
}
```

- [ ] **Step 3: Live event feed in panel**

Use `useEventStream(issue.id)` and render the last ~20 events as a small list — kind, tool/branch/url, timestamp. When a `pr_opened` event arrives, refetch the issue to pick up the new attachment.

- [ ] **Step 4: Commit**

```bash
git add web/src/components/AttachmentChip.tsx web/src/views web/src/graphql.ts
git commit -m "Render attachment chips and live event feed on issue panel"
```

---

## Task 18: Captured policy — workflow reload doesn't flap mid-flight

**Files:**
- Modify: `crates/symphony-core/src/orchestrator.rs`

- [ ] **Step 1: Capture policy at claim time**

In the per-claim runtime struct, store `policy: Policy` (cloned at the moment of claim). Use this captured policy when building `HarnessContext`, *not* the current `cfg.policy`. This way, a workflow reload that changes `policy:` only affects new claims.

- [ ] **Step 2: Add a unit test**

Mock a workflow reload: claim issue with policy A, reload with policy B, dispatch — assert the harness sees policy A in `HarnessContext`. Use a stub harness that records the received policy.

- [ ] **Step 3: Commit**

```bash
cargo test --workspace
git add crates/symphony-core/src/orchestrator.rs crates/symphony-core/tests/
git commit -m "Capture policy at claim time so workflow reload doesn't flap mid-flight"
```

---

## Task 19: MCP heartbeat + restart-once on crash

**Files:**
- Modify: `crates/symphony-core/src/harness/claude_code.rs` (track child status)

- [ ] **Step 1: Add a watchdog**

When Claude Code spawns the MCP bridge (it's a child of `claude`, not symphony, so we don't actually own the process here), Claude Code itself restarts MCP servers on crash; our `mcp_bridge` subcommand needs to be idempotent and crash-safe. Spec error handling expected the orchestrator to own the MCP server, but with `claude --mcp-config` the subprocess is owned by Claude Code. **Adjust:** drop the heartbeat-from-orchestrator design; instead, ensure `mcp_bridge` itself logs to stderr aggressively so Claude Code's restart logging is useful, and add a `panic::set_hook` to log structured panic info before exit.

- [ ] **Step 2: Add panic hook in `mcp_bridge::run_mcp_server`**

```rust
std::panic::set_hook(Box::new(|info| {
    eprintln!("MCP_BRIDGE_PANIC: {info}");
}));
```

- [ ] **Step 3: Commit**

```bash
git add crates/symphony-core/src/harness/mcp_bridge.rs
git commit -m "Add structured panic logging to MCP bridge subcommand"
```

---

## Task 20: End-to-end smoke (env-gated)

**Files:**
- Create: `crates/symphony-core/tests/slice1_smoke.rs`

- [ ] **Step 1: Write the smoke**

```rust
#![cfg(feature = "e2e_claude_code")]

#[tokio::test(flavor = "multi_thread")]
async fn slice1_smoke_claude_code_uses_linear_graphql() {
    if std::env::var("CLAUDE_CODE_E2E").is_err() {
        eprintln!("skipping: set CLAUDE_CODE_E2E=1 to run");
        return;
    }
    // 1. Start linear-clone in-process with a known issue + LINEAR_CLONE_ADMIN_TOKEN.
    // 2. Start orchestrator with WORKFLOW.md prompting agent: "comment 'hello' on this issue".
    // 3. Subscribe to bus.
    // 4. Wait up to 5min for: ToolCall{tool=="linear_graphql.add_comment"}.
    // 5. Assert linear-clone has the comment.
}
```

Add a feature `e2e_claude_code` to `crates/symphony-core/Cargo.toml`.

- [ ] **Step 2: Document running it in `README.md`**

```pwsh
$env:CLAUDE_CODE_E2E=1
$env:LINEAR_CLONE_ADMIN_TOKEN="dev-admin"
cargo test -p symphony-core --features e2e_claude_code --test slice1_smoke -- --nocapture
```

- [ ] **Step 3: Commit**

```bash
git add crates/symphony-core/tests/slice1_smoke.rs crates/symphony-core/Cargo.toml README.md
git commit -m "Add env-gated end-to-end smoke for Claude Code + linear_graphql"
```

---

## Task 21: WORKFLOW.md and README updates

**Files:**
- Modify: `WORKFLOW.md`
- Modify: `README.md`

- [ ] **Step 1: Update `WORKFLOW.md` with new blocks**

```yaml
policy:
  permission_mode: accept_edits   # accept_edits | require_approval | read_only
  sandbox: workspace_write        # none | workspace_write | read_only
  allowed_tools: []
  approval_timeout_ms: 300000

vcs:
  remote: origin
  branch_prefix: symphony/
  auto_open_pr: true
```

- [ ] **Step 2: Update `README.md`**

- New section "Slice 1 (v1.0): Claude Code end-to-end" describing the `linear_graphql` tool, approval toast, PR creation flow, and the `LINEAR_CLONE_ADMIN_TOKEN` env var.
- Mark items as complete in the v0.1 deferral list as appropriate.

- [ ] **Step 3: Commit**

```bash
git add WORKFLOW.md README.md
git commit -m "Document policy/vcs blocks and slice 1 behavior"
```

---

## Task 22: Full workspace check + final commit

- [ ] **Step 1: Run everything**

```bash
cargo build --workspace
cargo test --workspace
cd web && npm run build && cd ..
```

- [ ] **Step 2: Manual smoke**

1. Start linear-clone: `cargo run -p linear-clone -- --port 4000` (with `LINEAR_CLONE_ADMIN_TOKEN=dev` in env).
2. Edit `WORKFLOW.md` to point tracker at linear-clone, add `vcs:` block, set `policy.permission_mode: accept_edits`.
3. Configure a test GitHub remote in `data/workspaces/<seed>` or stub `gh` on `PATH` for the smoke.
4. Start symphony: `cargo run -p symphony -- --workflow WORKFLOW.md`.
5. Open <http://127.0.0.1:8080>, watch the SSE feed, claim an issue, see `tool_call → tool_result → vcs_pushed → pr_opened → tool_call(link_pull_request)` flow through.

- [ ] **Step 3: Final tag**

```bash
git tag v1.0.0-slice1
```

(Push later, when slice 2 and 3 are merged.)

---

## Self-Review Notes

**Spec coverage:**
- Codex full protocol client → **out of scope** for this plan; covered by slice 2 plan (separately written).
- `linear_graphql` tool (read + comment + PR linking) → Tasks 5, 11, 12.
- GitHub PR push + link_pull_request follow-up → Tasks 6, 14.
- Static workflow policy → Task 1, 12, 18.
- Interactive approvals through dashboard → Tasks 8, 9, 16. (Note Task 12's documented limitation: Claude Code's hosted permission UI gates the actual call; Symphony surfaces requests for transcript visibility but does not gate Claude Code's tool calls itself. The interactive gate becomes load-bearing in slice 2 with Codex, where Symphony fully owns the approval round-trip.)
- linear-clone attachments → Task 3.
- linear-clone token-scoped auth → Task 4, 13.
- SSE channel → Task 7.

**Type consistency:** `OrchestratorEvent`, `Policy`, `HarnessContext`, `ApprovalRouter`, `Decision` defined once and referenced consistently across tasks. Web `OrchestratorEvent` discriminator matches Rust serde tag (`kind`, snake_case).

**Open caveat:** Task 12's interactive-approval limitation under Claude Code is the most likely surprise. Slice 2 is where the approval round-trip gets exercised end-to-end.
