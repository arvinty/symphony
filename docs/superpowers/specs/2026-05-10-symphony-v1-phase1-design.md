# Symphony v1.0 — Phase 1: Fill v0.1 Gaps

**Date:** 2026-05-10
**Status:** Approved (design); pending implementation plan
**Scope:** v1.0 phase 1 of 4. Subsequent phases — multi-agent + workflow depth (2), production-readiness (3), polish + DX (4) — get their own specs.

## Goal

Close the four items the v0.1 README explicitly deferred:

1. Full Codex app-server JSON-RPC protocol client (replaces `codex_stub`).
2. `linear_graphql` tool bridged to all three harnesses, scope **read + comment + PR linking**.
3. Real GitHub PR integration: orchestrator pushes the per-issue branch and opens a PR; agent records the URL on the issue via the tool.
4. Approval/sandbox policy plumbed end-to-end: static workflow-level default **plus** interactive approvals through the dashboard.

## Non-Goals

- State transitions or label edits via `linear_graphql` (orchestrator already drives those).
- Pixel-perfect Linear UI parity (deferred to phase 4).
- Auth on linear-clone for human users (deferred to phase 3).
- Per-issue policy overrides — only workflow-level policy in phase 1.
- Branch-protection / mergeability checks beyond opening the PR.

## Architecture

Phase 1 ships in three vertical slices, each landing on `main` independently:

- **Slice 1 — Claude Code end-to-end.** MCP server exposes the four `linear_graphql` tools. Policy translates to `--permission-mode` / `--allowedTools`. Post-success VCS hook pushes branch and opens PR via `gh`; the agent itself calls `linkPullRequest` with the URL. Approvals: agent emits `tool_approval_request` events; orchestrator broadcasts on a new SSE endpoint; dashboard renders Approve/Deny.
- **Slice 2 — Codex.** New `crates/codex-client` implements JSON-RPC 2.0 over stdio (capability negotiation, `session/start`, `session/turn`, streaming `event` notifications, `tool/call`, `approval/request`, sandbox config). Replaces `codex_stub`; reuses the MCP tool definitions, approvals channel, and VCS hook from slice 1.
- **Slice 3 — Hermes.** Same tool definitions advertised in Hermes' tool format; same policy translation; same VCS hook; same approvals channel.

Cross-cutting pieces shared by all slices, built once in slice 1 and reused:

- `symphony-core::tools::linear_graphql` — tool contract (input/output schemas).
- `symphony-core::policy::{Policy, SandboxProfile}` — typed policy struct, parsed from WORKFLOW.md.
- `symphony-core::events::{ApprovalRequest, ApprovalDecision}` + SSE endpoint at `/api/v1/events`.
- `symphony-core::vcs::{push_branch, open_pr}` — wraps `git push` + `gh pr create`.

## Components

### `symphony-core::tools::linear_graphql` (~150 LOC)

Pure tool contract: input/output JSON Schemas plus a `LinearGraphqlClient` wrapping `reqwest` against linear-clone's GraphQL endpoint. Four methods:

- `get_issue(id)` — read.
- `list_comments(id)` — read.
- `add_comment(id, body)` — mutate (own issue only).
- `link_pull_request(id, url, title)` — mutate (own issue only).

Auth via `Bearer` token from agent process env (`SYMPHONY_LINEAR_TOKEN`), provisioned per-issue by the orchestrator. Token is bound to a single `issue_id`; mutations against any other issue return 403.

### `symphony-core::policy` (~80 LOC)

```rust
pub struct Policy {
    pub permission_mode: PermissionMode,   // AcceptEdits | RequireApproval | ReadOnly
    pub sandbox: SandboxProfile,           // None | WorkspaceWrite | ReadOnly
    pub allowed_tools: Vec<String>,
}
```

Parsed from a new `policy:` block in WORKFLOW.md; defaults preserve current `acceptEdits` behavior. Each harness implements `translate(&Policy)` producing harness-specific flags/env.

### `symphony-core::events` (~120 LOC)

A `tokio::sync::broadcast::Sender<OrchestratorEvent>` owned by the orchestrator. New variants: `ToolCall`, `ApprovalRequest { issue_id, approval_id, tool, input }`, `ApprovalDecision { approval_id, allow, reason }`. Existing event log gains realtime subscribers without changing the file-backed log format.

### `symphony::http`

Two new endpoints:

- `GET /api/v1/events` — SSE stream of `OrchestratorEvent`. Optional `?issue=DEMO-1` filter.
- `POST /api/v1/approvals/{approval_id}` — body `{allow: bool, reason?: string}`. Resolves the pending oneshot.

### `symphony-core::vcs` (~100 LOC)

- `push_branch(workspace, remote, branch)` shells out to `git push -u <remote> HEAD:<branch>` with workspace as cwd.
- `open_pr(workspace, title, body)` shells out to `gh pr create --json url --title … --body …`, returns the URL.

Failures are best-effort — they emit `VcsError` events but do not fail the issue. Operator can retry via `POST /api/v1/<id>/open-pr`.

### Slice 1 — `harness/claude_code` changes (~100 LOC delta)

- Spin up an in-process MCP server (stdio over a socketpair) exposing the four `linear_graphql` tools, before spawning `claude`.
- Pass MCP socket path via `--mcp-config <path>`.
- Translate `Policy` → `--permission-mode`, `--allowedTools`, sandbox env vars.
- Parse `tool_use` events from stream-json. When approval is needed: publish `ApprovalRequest`, await decision via oneshot, write `tool_approval_response` to agent stdin.

### Slice 2 — `crates/codex-client` (new crate, ~600 LOC)

Standalone JSON-RPC 2.0 client over stdio. Modules:

- `transport` — framed reader/writer (LSP-style `Content-Length` framing).
- `session` — handshake, `session/start`, lifecycle.
- `events` — typed notification stream (`tokio::sync::mpsc::Receiver<Notification>`).
- `tools` — request/response correlation by `id`.
- `approvals` — typed `approval/request` round-trips.

Exercised by `harness/codex` (~150 LOC) which reuses MCP tool defs and the events channel from slice 1.

### Slice 3 — `harness/hermes` (~80 LOC delta)

Tool advertisements in Hermes' format; policy translation; reuses everything else.

### Web UI (~200 LOC delta)

- `useEventStream(issueId?)` hook subscribes to `/api/v1/events` (with auto-reconnect).
- `<ApprovalToast>` surfaces pending approvals with Approve/Deny buttons calling `POST /api/v1/approvals/{id}`.
- Issue panel renders a live event feed.

### linear-clone (~150 LOC delta)

Schema gains `attachments(id, issue_id, kind, url, title, created_at)` via migration `0003_attachments.sql`. Mutations: `addAttachment`, `removeAttachment`. UI renders attachment chips on the issue panel.

## Data Flow

### Issue claim → agent ready

1. Orchestrator picks an eligible issue, creates the per-issue workspace.
2. Mints a short-lived `SYMPHONY_LINEAR_TOKEN` bound to that `issue_id`.
3. Spawns the harness with: workspace cwd, token in env, policy translated to harness flags, MCP/tool config wired, broadcast `Sender` cloned in.

### Agent uses a `linear_graphql` tool

1. Agent emits `tool_use{name=linear_graphql.add_comment, input=…}`.
2. Harness adapter checks policy: `AcceptEdits` → run; `RequireApproval` → publish `ApprovalRequest`, block on `ApprovalDecision` (timeout 5min, default deny).
3. On approval (or auto-approve), `LinearGraphqlClient` issues the GraphQL mutation; result returned to the agent as `tool_result`.
4. Each step (`ToolCall`, `ApprovalRequest`, `ApprovalDecision`, `ToolResult`) flows out the broadcast channel → SSE → dashboard.

### Approval round-trip (interactive)

1. Harness publishes `ApprovalRequest{approval_id, issue_id, tool, input}`.
2. Orchestrator stores `approval_id → oneshot::Sender<bool>` and broadcasts the event.
3. Dashboard receives via SSE, renders toast.
4. Operator clicks Approve → `POST /api/v1/approvals/{id} {allow:true}`.
5. HTTP handler resolves the oneshot; harness adapter unblocks; broadcasts `ApprovalDecision`.
6. On timeout, oneshot drops → adapter denies the tool call → agent receives `tool_result{error:"approval_timeout"}`.

### Agent finishes turn → PR creation

1. Harness reports `turn_complete{success:true}`.
2. Orchestrator runs the VCS pipeline: `git push -u <remote> HEAD:symphony/<issue_identifier>`, then `gh pr create --json url --title … --body …`.
3. Orchestrator does *not* call `linkPullRequest` itself. It injects a final follow-up turn into the agent: "PR opened at {url}; call `linear_graphql.link_pull_request` to attach it." Keeps the audit trail in the agent transcript and exercises the tool in the happy path.
4. linear-clone records the attachment; UI renders it.

### Workflow reload mid-flight

- Policy change applies to **new** claims only; in-flight agents keep their original policy. Avoids approval-mode flapping mid-turn.

## Error Handling

- **Policy violations.** Tool call returns `tool_result{error:"policy_denied", reason}`; orchestrator logs but does not fail the issue.
- **Linear-clone tool failures.** `LinearGraphqlClient` retries on connect / 5xx with backoff (3 tries, capped at 2s). 4xx surfaces immediately. Token-scope mismatch (403) is non-retryable and returned to the agent verbatim so the model self-corrects.
- **Approval timeout.** Default deny after 5min, configurable per workflow. Emits `ApprovalDecision{allow:false, reason:"timeout"}`.
- **SSE client disconnects / lag.** On `broadcast::error::RecvError::Lagged`, server sends a `resync` event; client re-fetches `/api/v1/state` and resumes streaming. Orchestrator's broadcast channel is unaffected.
- **`gh pr create` failure** (auth missing, network, duplicate PR). Best-effort: log, emit `VcsError`, do not fail the issue. Operator can retry via `POST /api/v1/<id>/open-pr`. Agent is not told — avoids a wasted turn re-trying something only the operator can fix.
- **Codex protocol errors** (slice 2). JSON-RPC error responses → typed `CodexError`. Transport errors → kill subprocess, mark turn failed with `retry_reason="transport"`, exponential-backoff retry path takes over. Capability negotiation mismatch is a hard fail.
- **MCP server crash** (slice 1). Heartbeat ping every 5s. On crash, restart once; second crash within 60s fails the turn permanently.
- **linear-clone migration.** `0003_attachments.sql` adds the table; rollback removes only that table.

## Testing

### Unit tests

- `policy::translate` for each harness — assert exact flag/env outputs per `(PermissionMode, SandboxProfile)` combo.
- `vcs::push_branch` / `open_pr` against a `git init --bare` tmpdir and a fake `gh` shim on `PATH`.
- `LinearGraphqlClient` against `wiremock` for retry/backoff behavior and 403 token-scope rejection.
- `events` broadcast → SSE serialization round-trip.
- `codex-client` transport: framed encoder/decoder, request correlation, notification dispatch (slice 2).

### Integration tests (`tests/`)

- `linear_graphql_tool_integration` — real linear-clone with sqlite tmpfile; mint token; exercise all four tool methods including token-scope rejection.
- `approval_flow_integration` — orchestrator + a stub harness that emits a fake approval request; `POST /api/v1/approvals/{id}`; assert decision propagates and event log is correct.
- `slice1_smoke` — existing end-to-end smoke + assert agent transcript contains a `linear_graphql` tool call. Gated by `CLAUDE_CODE_E2E=1`.
- `codex_protocol_integration` (slice 2) — Python fake speaking the JSON-RPC protocol; exercise capability negotiation, tool round-trip, approval round-trip, transport-error recovery.

### UI tests

- Playwright spec for the approval toast (mock SSE, click Approve, assert POST). Single happy path; gated in CI.

### Deliberately not tested

- Real GitHub `gh pr create` — too much CI auth ceremony; covered by manual smoke.
- Real Claude Code / Codex subscriptions — gated by env flag, manual.

## Open Questions

None blocking. Configuration shape for the `policy:` block in WORKFLOW.md and the GitHub remote (token source, default branch convention) will be pinned during the implementation plan.
