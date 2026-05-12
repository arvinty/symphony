# Symphony v1.0 — Phase 2 Slice 1: Reviewer Agent

**Date:** 2026-05-13
**Status:** Approved (design); pending implementation plan
**Scope:** Phase 2 slice 1 of v1.0. Builds on phase 1 (slices 1-3 merged to `main`). Lands on `v1-phase2-slice1` off `main`.

## Goal

After the implementer succeeds and the PR is opened + linked, dispatch a second agent run (the "reviewer") with a read-only policy and a reviewer-specific prompt. The reviewer inspects the diff, posts a single review summary via the existing `linear_graphql.add_comment` tool, ends.

First commit-level multi-agent flow. Reuses every harness and tool already wired in phase 1.

## Non-Goals

- New harnesses or external tools (no `github_review` tool yet; PR-side comments are a follow-up slice).
- Reviewer-requested-changes loop (reviewer is one-shot).
- Parallel reviewers / multi-stage review.
- New state machine states. Issue lifecycle is unchanged; reviewer is a side-channel run that fires post-success.
- Per-route harness selection (that's a separate phase 2 slice).

## Architecture

One workflow config block, one new variant on `RunRequest`, one orchestrator hook. No new harness types. Total: ~250 LOC.

```
                  ┌─────────────────── existing slice-1 flow ───────────────────┐
                  │                                                              │
claim_issue ─► RunRequest{Implementer} ─► harness::run ─► success ─► VCS pipeline
                                                                       │
                                                                       ▼
                                                            (PR opened, follow-up
                                                            prompt to link PR)
                                                                       │
                                                                       ▼
              RunRequest{Implementer, follow_up_count=1, prompt=link_pr_prompt}
                                                                       │
                                                                       ▼
                                                       success of link_pr turn
                                                                       │
                       ────── NEW (slice 1 of phase 2) ──────          │
                                                                       ▼
                                            if cfg.reviewer.enabled:
                                            RunRequest{Reviewer{pr_url},
                                                      prompt=reviewer_prompt,
                                                      policy=reviewer_policy}
                                                                       │
                                                                       ▼
                                                          harness::run (read-only)
                                                                       │
                                                                       ▼
                                            ReviewerCompleted event on bus
                                            (issue lifecycle unchanged)
```

## Components

### `symphony-core::config::ReviewerConfig` (~40 LOC)

```rust
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ReviewerConfig {
    #[serde(default)]
    pub enabled: bool,
    pub harness: Option<String>,        // defaults to AgentConfig::harness
    pub policy: Option<Policy>,         // defaults to ReadOnly+ReadOnly
    pub prompt_template: Option<String>, // defaults to a built-in Liquid template
}
```

Wired into `ServiceConfig` (parsed from WORKFLOW.md front matter) and projected into `EffectiveConfig`. Default is `enabled: false`, so absent config means "no reviewer" — no behavior change.

### `RunPhase` enum (in `orchestrator.rs`, ~10 LOC)

```rust
#[derive(Debug, Clone)]
enum RunPhase {
    Implementer,
    Reviewer,
}
```

`RunRequest` gains a `phase: RunPhase` field. Default is `Implementer` for all existing dispatch paths; the reviewer path explicitly sets `Reviewer`.

### Orchestrator hook (~50 LOC)

In `process_run_request`'s success branch, after the link-PR follow-up turn completes successfully (`follow_up_count == 1` and we have the PR URL in scope), check `cfg.reviewer.enabled` and dispatch a `RunRequest` with:
- `phase: RunPhase::Reviewer`
- `policy:` the resolved reviewer policy
- `prompt_override:` the rendered reviewer prompt (Liquid template + variables)

The PR URL needs to be captured during the initial `open_pr` call and threaded forward through the follow-up turn so it's available when scheduling the reviewer. Smallest change: store `last_pr_url: Option<String>` on the per-issue runtime state, set it on successful `open_pr`, read it when scheduling the reviewer.

### Harness dispatch (~30 LOC)

`process_run_request` picks the harness based on the `RunPhase`:
- `Implementer` → `cfg.agent.harness` (unchanged behavior).
- `Reviewer` → `cfg.reviewer.harness.unwrap_or(cfg.agent.harness)`.

And picks the policy based on phase:
- `Implementer` → `request.policy` (existing captured policy).
- `Reviewer` → reviewer policy from config (default ReadOnly+ReadOnly).

### Default reviewer prompt template (~25 LOC, built into the code)

```
You are reviewing a Symphony-authored pull request.

Issue: {{issue_identifier}} — {{issue_title}}
PR: {{pr_url}}

The branch has been pushed; the workspace contains the implementation. Read
the diff between `origin/main` and `HEAD` with `git diff origin/main...HEAD`,
then post a single review comment on the issue via `linear_graphql.add_comment`
summarizing your findings. Focus on correctness, security, and clarity. End
the turn after posting the comment.
```

Liquid-rendered. Override via `reviewer.prompt_template` in WORKFLOW.md.

### Events (~20 LOC)

Two new `OrchestratorEvent` variants:
- `ReviewerStarted { issue_id, pr_url }` — fired when the reviewer RunRequest is dispatched.
- `ReviewerCompleted { issue_id, success, error }` — fired when the reviewer run terminates.

Existing `AgentEvent` flow (turn-level events) continues unchanged — operators see the same per-turn detail.

### Reused unchanged

`HarnessContext`, all existing harnesses (`claude_code`, `codex`, `hermes`), `mcp_bridge`, `vcs::{push_branch, open_pr}`, the linear-clone GraphQL tools, the SSE event endpoint, the approval flow. The reviewer is a normal harness run with a different prompt and policy.

## Data Flow

### Reviewer dispatch (the new path)

1. Implementer completes → VCS pipeline runs → PR opens → link-PR follow-up turn dispatched.
2. Link-PR follow-up turn completes successfully (`follow_up_count == 1`).
3. Orchestrator checks `cfg.reviewer.enabled`. If false, no further action.
4. If true:
   - Resolve reviewer policy (`cfg.reviewer.policy` or default ReadOnly).
   - Resolve reviewer harness (`cfg.reviewer.harness` or fall back to `cfg.agent.harness`).
   - Render reviewer prompt (Liquid template + issue/PR variables).
   - Build `RunRequest { phase: Reviewer, policy, prompt_override, follow_up_count: 0 }`.
   - Send on `run_tx`.
   - Emit `OrchestratorEvent::ReviewerStarted`.
5. Worker pops the request, dispatches to the selected harness with the reviewer policy. Linear MCP bridge is provisioned the same way — same issue token, same endpoint.
6. Reviewer run produces `AgentEvent`s on the existing event stream. It calls `linear_graphql.add_comment(issue_id, body)` once.
7. Run terminates → orchestrator emits `OrchestratorEvent::ReviewerCompleted { success }`.
8. Issue state is unchanged by reviewer success/failure — it was already on its terminal trajectory.

### PR URL capture

Currently the PR URL only exists transiently inside the `open_pr` success branch. New requirement: persist it on per-issue runtime state for the reviewer dispatch.

Simplest: a `HashMap<IssueId, String>` on `Orchestrator::inner.pr_urls` guarded by a Tokio `RwLock`. Set on PR open success, read when dispatching reviewer, cleared when issue transitions to terminal state. ~15 LOC.

## Error Handling

| Failure | Behavior |
|---|---|
| `cfg.reviewer.enabled: false` (default) | No reviewer dispatch; phase 1 behavior unchanged. |
| Reviewer harness not on PATH | `OrchestratorEvent::ReviewerCompleted { success: false, error: "agent_not_found" }`; issue completes normally. |
| Reviewer turn fails (non-zero exit) | `ReviewerCompleted { success: false }`; issue completes normally. |
| Reviewer can't post comment (Linear-clone down, 403, etc.) | The MCP tool returns an error to the agent; agent's turn may still complete; reviewer's success status reflects the turn outcome. |
| PR URL was never captured (open_pr failed earlier) | Reviewer dispatch is skipped — no PR to review. |
| Reviewer prompt template renders to an error | Log + skip reviewer dispatch; emit `ReviewerCompleted { success: false, error: "prompt_render_failed" }`. |

No new error types beyond reusing `SymphonyError`.

## Testing

### Unit: `ReviewerConfig` parsing (`crates/symphony-core/tests/reviewer_config_tests.rs`)

- Defaults: no `reviewer:` block in WORKFLOW.md → `enabled: false`.
- Explicit `enabled: true` with no other fields → defaults applied for harness, policy, prompt_template.
- Full config with overrides → all fields parsed.

~50 LOC.

### Unit: default prompt template renders (`crates/symphony-core/tests/reviewer_prompt_tests.rs`)

- Variables substituted correctly.
- Template renders cleanly with all-empty inputs (issue title might be empty).

~30 LOC.

### Integration: orchestrator dispatches reviewer (`crates/symphony-core/tests/reviewer_dispatch_tests.rs`)

Build a minimal orchestrator with a stub harness that succeeds immediately. Inject a fake PR URL into the per-issue state. Manually trigger the post-success path that would normally fire after `link_pull_request` follow-up. Assert:
- A new `RunRequest` is sent on `run_tx` with `phase: Reviewer`.
- `OrchestratorEvent::ReviewerStarted` fires on the bus with the expected pr_url.
- The reviewer policy is the configured (or default) reviewer policy, not the implementer's.

~120 LOC. Existing orchestrator test scaffolding patterns from slice 1's `approval_flow_tests.rs` apply.

### Integration: reviewer disabled is a no-op

- Same fixture, `cfg.reviewer.enabled: false`. Assert no reviewer RunRequest is dispatched after the implementer success path.

### Deliberately not tested

- Real Linear-clone reviewer comment round-trip (covered by slice 1's `linear_graphql_tool_tests`).
- Real CLI smoke (gated behind existing `e2e_*` features; reviewer would work whether or not the implementer's harness was Claude/Codex/Hermes).

## Pre-merge checklist

- [ ] `cargo test -p symphony-core --test reviewer_config_tests` green.
- [ ] `cargo test -p symphony-core --test reviewer_prompt_tests` green.
- [ ] `cargo test -p symphony-core --test reviewer_dispatch_tests` green.
- [ ] `cargo test --workspace` no regressions.
- [ ] `cargo clippy --workspace -- -D warnings` clean.
- [ ] Manual: claim an issue end-to-end with `reviewer.enabled: true` in `WORKFLOW.md`, verify two distinct AgentEvent streams (implementer + reviewer), verify the reviewer posts a single Linear comment summarizing the PR.

## Open Questions

- **OQ-1.** Should the reviewer share the same per-issue MCP token, or get a fresh one? Sharing is simpler and matches the current per-issue token lifecycle; minting a separate reviewer token would let us audit "implementer vs reviewer" comments separately on linear-clone but adds plumbing. **Proposal: share.**
- **OQ-2.** Should reviewer failure emit any user-visible badge in the UI? Currently the existing `AgentEvent::TurnFailed` already surfaces, but tying a "review failed" label to the issue would be nice. **Proposal: skip in slice 1; the event bus already shows it.**
- **OQ-3.** PR URL capture mechanism — Mutex-guarded HashMap on Orchestrator vs. embedding in the link-PR follow-up RunRequest's metadata. **Proposal: HashMap; cleaner separation from RunRequest semantics.**

All three are non-blocking — default decisions noted, easy to revise post-merge.
