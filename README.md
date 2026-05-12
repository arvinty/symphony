# Symphony + Linear-clone

A Rust implementation of the [Symphony](https://github.com/openai/symphony) agent
orchestrator, plus a self-hosted Linear-shaped issue tracker so it can be exercised
end-to-end without depending on Linear.com.

## Layout

```
crates/
  symphony-core/        Core orchestrator library (workflow, config, state machine, harnesses).
  symphony/             `symphony` daemon binary + HTTP dashboard.
  linear-clone/         `linear-clone` backend (axum + async-graphql + SQLite).
web/                    React + Tailwind UI (Linear-styled dark theme).
WORKFLOW.md             Sample workflow used by `symphony`.
issues.mock.json        Sample tracker payload for the file-mock adapter.
```

## Reviewer agent (phase 2)

When `reviewer.enabled: true` is set in `WORKFLOW.md`, Symphony runs a
second agent after the implementer succeeds and the PR is linked back to
the Linear issue. The reviewer runs with a separate (default ReadOnly+ReadOnly)
policy, reads the diff, posts a single summary comment via
`linear_graphql.add_comment`, and ends. Reviewer success is best-effort —
the issue's terminal state isn't gated on it. See
`docs/superpowers/specs/2026-05-13-symphony-v1-phase2-slice1-design.md`.

## Agent harnesses

Symphony's `agent.harness` config selects which subprocess runs in each per-issue
workspace.

| Harness        | Spawned                                                        | Notes                                                |
| -------------- | -------------------------------------------------------------- | ---------------------------------------------------- |
| `claude_code`  | `claude -p <prompt> --output-format stream-json --verbose`     | Uses your existing Claude Code subscription auth.   |
| `hermes`       | `hermes run … --mcp-config <path> --permission-mode <mode>`    | Wires Linear MCP + policy flags. Approval gating is native to Hermes' hosted UI. Requires Nous Research's Hermes CLI on `$PATH`. |
| `codex`        | `codex app-server --listen stdio -c mcp_servers.linear={…}`    | Full v2 JSON-RPC client; requires `codex-cli >= 0.130`. |

Switch harnesses by editing `WORKFLOW.md` — Symphony hot-reloads the config.

The Codex harness translates `policy.permission_mode` and `policy.sandbox` to
the v2 `approvalPolicy` (`AskForApproval::Untrusted` under `require_approval`,
`Never` otherwise) and `sandboxPolicy` (`DangerFullAccess` / `WorkspaceWrite`
/ `ReadOnly`). Guardian-denied actions surface on the dashboard via the same
approval toast as slice 1; allow → `thread/approveGuardianDeniedAction` RPC.
See `docs/superpowers/specs/2026-05-13-symphony-v1-slice2-design.md`.

## Running

### 1. Build the workspace

```pwsh
cargo build --workspace
```

### 2. Run Symphony against the file-mock tracker

```pwsh
cargo run -p symphony -- --workflow WORKFLOW.md
```

Open <http://127.0.0.1:8080> for the operator dashboard.
JSON state at <http://127.0.0.1:8080/api/v1/state>.

### 3. Run the Linear clone (separate terminal)

```pwsh
cargo run -p linear-clone -- --port 4000
cd web
npm install
npm run dev   # dev: http://localhost:5173 (proxies /graphql to :4000)
# or for production:
npm run build && cd ..
cargo run -p linear-clone -- --port 4000
```

Then point Symphony at the clone by editing `WORKFLOW.md`:

```yaml
tracker:
  kind: linear
  endpoint: http://127.0.0.1:4000/graphql
  api_key: dev-token       # not validated locally
  project_slug: symphony
```

## v1.0 Slice 1 (Claude Code end-to-end)

On top of v0.1, slice 1 adds:

- **`linear_graphql` MCP tool** exposed to Claude Code: `get_issue`, `list_comments`,
  `add_comment`, `link_pull_request`. Bridged via a hidden `symphony mcp-bridge`
  subcommand wired through `--mcp-config`. Tokens are issue-scoped.
- **Issue-scoped bearer-token auth on linear-clone.** The orchestrator mints a
  short-lived token via `POST /admin/tokens` (gated by `LINEAR_CLONE_ADMIN_TOKEN`)
  and provisions it to the agent's MCP server via env. Mutations against any
  issue other than the bound one return `issue_token_scope` errors.
- **Workflow-level policy** (`policy:` block in `WORKFLOW.md`): `permission_mode`,
  `sandbox`, `allowed_tools`, `approval_timeout_ms`. Captured at claim time so
  workflow reload doesn't flap policy across continuations.
- **Live event channel** at `GET /api/v1/events` (SSE). Emits `tool_call`,
  `tool_result`, `approval_request`, `approval_decision`, `vcs_pushed`,
  `pr_opened`, `vcs_error`, plus a `resync` signal on lag.
- **Approval round-trip** at `POST /api/v1/approvals/{id}` with body
  `{"allow": bool, "reason": string}`. Wired to a dashboard toast that surfaces
  pending requests and submits the operator's decision. **Note:** under Claude
  Code, the actual gating happens inside the CLI based on `--permission-mode` —
  Symphony surfaces tool calls and approvals for transcript visibility but does
  not itself decide. Slice 2 (Codex) is where Symphony fully owns the round-trip.
- **GitHub PR pipeline**: when an agent finishes a turn successfully and `vcs:`
  is configured, the orchestrator pushes `HEAD` to `<remote>/<branch_prefix><id>`
  and (if `auto_open_pr`) calls `gh pr create`. The agent gets one follow-up turn
  prompted to call `linear_graphql.link_pull_request` so the URL is recorded as
  an attachment on the issue.
- **Attachments table** on linear-clone (`addAttachment`/`removeAttachment`
  mutations). Issue panel renders chips for attached PRs.

### Running with the new pipeline

```pwsh
$env:LINEAR_CLONE_ADMIN_TOKEN="dev-admin"
cargo run -p linear-clone -- --port 4000

# In another terminal — point WORKFLOW.md at the linear-clone:
#   tracker.kind=linear
#   tracker.endpoint=http://127.0.0.1:4000/graphql
#   policy.permission_mode=accept_edits
#   vcs.remote=origin (workspace must have a real GitHub remote for PR push)
cargo run -p symphony -- --workflow WORKFLOW.md
```

## Spec coverage

This is **v0.1**. Implemented:

- Workflow loader (front matter + body), Liquid template rendering with strict checking.
- Config layer with `$VAR` indirection, defaults, validation.
- Tracker trait with Linear (paginated GraphQL) and file-mock adapters.
- Workspace manager with sanitization + path-inside-root invariant + lifecycle hooks.
- Orchestrator state machine: claim/run/retry/release, dispatch eligibility,
  per-state and global concurrency, blocker-on-Todo gating, priority/created_at sort.
- Continuation retries (~1s) and exponential backoff failure retries.
- Reconciliation: stall detection (event-inactivity) + tracker state refresh.
- Startup terminal-state workspace cleanup.
- Dynamic `WORKFLOW.md` reload via filesystem watch.
- HTTP extension: `GET /`, `GET /api/v1/state`, `GET /api/v1/<id>`, `POST /api/v1/refresh`.
- Three coding-agent harnesses (Claude Code, Hermes, Codex via the v2 JSON-RPC `codex-client` crate).

Deliberately **not** in v0.1 (would extend scope significantly):

- `linear_graphql` client-side tool extension end-to-end (the trait exists; bridging to subprocess
  harnesses needs harness-specific tool advertisements).
- Pixel-perfect Linear UI parity (the UI is Linear-styled but not a 1:1 reproduction).
- Approval/sandbox policy plumbed through to harnesses (Claude Code defaults to `acceptEdits`).

## Running symphony with the live Linear-clone

1. `cargo run -p linear-clone -- --port 4000`
2. Edit `WORKFLOW.md` to set `tracker.kind: linear` and `endpoint: http://127.0.0.1:4000/graphql`.
3. `cargo run -p symphony` — it'll poll your local clone and dispatch agents per issue.
