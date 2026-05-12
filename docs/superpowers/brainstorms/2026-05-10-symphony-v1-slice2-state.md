# Slice 2 brainstorm — complete (resumed and closed 2026-05-13)

**Status:** Brainstorm complete. Sections 2–5 worked through and incorporated.
Output artifacts:
- Spec: `docs/superpowers/specs/2026-05-13-symphony-v1-slice2-design.md`
- Plan: `docs/superpowers/plans/2026-05-13-symphony-v1-slice2.md`

Notable correction during resume: v2 protocol review revealed approvals are
notification-driven (no `ServerRequest` enum in v2). Decision 4 was revised
to reflect this; see the spec for the canonical model.

Branch: `v1-slice1` (slice 1 PR open at #1). Slice 2 work
will land on a new branch `v1-slice2` off `main` once slice 1 merges.

## Decisions captured so far

1. **Protocol source of truth:** Run `codex app-server` and probe live.
   Schemas already captured in `docs/codex-protocol/` via
   `codex app-server generate-json-schema --out docs/codex-protocol/`.
   Codex CLI version on this machine: `codex-cli 0.130.0`.
2. **Type coverage:** Codegen the full v2 schema (243 messages) via `typify`
   build script. Hand-roll v1 (`Initialize` only).
3. **Tool routing:** Reuse the existing `symphony mcp-bridge` subcommand by
   configuring Codex's `mcp_servers` to spawn it. Same bridge, no new tool
   plumbing.
4. **Approval routing (REVISED after v2 schema review).** v2 inverts v1:
   approvals are *notification-driven*, not request-driven. There is no
   `ServerRequest` enum in v2. Codex's internal guardian assesses each action
   based on the `permissions` we set on `turn/start` and emits
   `ItemGuardianApprovalReviewStartedNotification` →
   `ItemGuardianApprovalReviewCompletedNotification` (status:
   `approved` / `denied` / `timedOut` / `aborted`). The client overrides
   a denied action by sending `ClientRequest: thread/approveGuardianDeniedAction`.
   Translation per `Policy.permission_mode` (option C):
   - `AcceptEdits` → guardian auto-approves per sandbox; on `denied`,
     emit `OrchestratorEvent::ApprovalRequest` for operator override.
   - `RequireApproval` → set `permissions` to most-restrictive profile so
     guardian denies most actions; every denial reaches the operator.
   - `ReadOnly` → strict read-only profile; denials stand, no override path.
5. **Sandbox mapping:** Direct, and now load-bearing (the primary policy
   gate, not redundant with operator approvals). `Policy.sandbox` →
   `WorkspaceWrite` → `workspace-write`,
   `ReadOnly` → `read-only`,
   `Unrestricted` → `danger-full-access`.
   Passed via `permissions` on `turn/start`.
   Concrete `Permissions` struct shape in v2 is open question OQ-2.
6. **Structuring approach:** Bottom-up staged commits — types → transport →
   client+harness — all in a new crate `crates/codex-client` plus a thin
   harness adapter in `symphony-core`.

## Where in the brainstorm flow we paused

User just approved the architecture (section 1 of 5). Next steps when
resuming:

1. Present **section 2: components** and get approval.
2. Present **section 3: data flow**.
3. Present **section 4: error handling**.
4. Present **section 5: testing**.
5. Write the spec doc to `docs/superpowers/specs/2026-05-10-symphony-v1-slice2-design.md`.
6. Self-review.
7. Ask user to review.
8. Invoke `superpowers:writing-plans` to produce the implementation plan.

## Architecture (already approved)

Three sequential commit stages on `v1-slice2`:

- **Stage 1 — Types.** New `crates/codex-client` crate. `build.rs` runs
  `typify` over `docs/codex-protocol/codex_app_server_protocol.v2.schemas.json`
  to generate `protocol::v2::*`. Hand-rolled `protocol::v1::Initialize*`.
  `protocol::messages::{ClientRequest, ClientNotification, ServerRequest,
  ServerNotification}` enums dispatch on method names.
- **Stage 2 — Transport + correlation.** `transport::StdioTransport`
  (newline-delimited JSON over child stdio). `dispatcher::Dispatcher` owns
  the read loop and demultiplexes responses (oneshot by id), server requests
  (channel), and notifications (channel). Public surface:
  `Client::connect(child) -> (Client, NotificationStream, ServerRequestStream)`.
- **Stage 3 — Client API + harness.** `Client::{initialize, start_turn,
  interrupt}`. New `harness::codex::CodexHarness` in `symphony-core` that
  spawns `codex app-server --listen stdio://` with `-c mcp_servers.linear=...`,
  calls `initialize`, drives `turn/start` with `permissions` from
  `Policy.sandbox`, forwards `ServerNotification::*` to `AgentEvent` /
  `OrchestratorEvent::ToolCall`, routes approval `ServerRequest`s through
  `ApprovalRouter` and sends typed responses back. Replaces `codex_stub`.

Reused unchanged from slice 1: `OrchestratorEventBus`, `ApprovalRouter`,
`Policy`, `HarnessContext`, `mcp_bridge::generate_mcp_config_json`,
`vcs::{push_branch, open_pr}`, captured-policy flow.
