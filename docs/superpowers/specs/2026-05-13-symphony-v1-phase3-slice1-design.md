# Symphony v1.0 — Phase 3 Slice 1: Lifecycle Plumbing

**Date:** 2026-05-13
**Status:** Approved (design); pending implementation plan
**Scope:** Phase 3 slice 1. Triage of three concrete gaps surfaced by a local end-to-end smoke run.

## Background

A local smoke run with `harness: claude_code` + file_mock tracker exposed:

1. **Agent files weren't committed.** Per-issue workspace ran `git init -q` via the `after_create` hook but never set `user.name`/`user.email`, so any `git commit` the agent attempted failed silently. Two issues produced files (`README.md`, `NOTES.md`) but no commits — the slice 1 VCS pipeline can't push anything in this state.
2. **Infinite continuation loop.** After a successful `TurnCompleted` on a turn with no follow-up (i.e. `vcs.auto_open_pr: false` and `reviewer.enabled: false`), the orchestrator schedules a continuation via `schedule_retry(... CONTINUATION_DELAY_MS)`. The continuation runs the agent again with the same prompt; on success it schedules another continuation. On a tracker that never transitions issue state (file_mock, or Linear with no human moving the issue), the loop runs forever — burning tokens, never producing a terminal outcome.
3. **Per-issue token tracking shows 0.** `codex_totals.total_tokens` updates correctly across the orchestrator-wide total, but each `running` entry's `tokens` reports zeroes. Root cause: `process_run_request` calls `s.running.insert(...)` at the start of every dispatch, which overwrites the previous `RunningEntry` (including its `session.tokens`). When the continuation loop re-dispatches, the per-issue token counter resets.

All three are bounded fixes; combined diff is small.

## Non-Goals

- Tracker-side state writeback (file_mock or Linear). Issue terminal state still comes from the tracker.
- New harness types, new tools, new policy primitives.
- Web UI build (separate concern — phase 4).
- Auth on linear-clone for humans (still deferred to a later phase 3 slice).

## Components

### Fix 1 — Workspace git identity (~5 LOC)

Update the default `after_create` hook in `WORKFLOW.md` to include git identity. Single-file change:

```yaml
hooks:
  after_create: |
    git init -q
    git config user.email "symphony@localhost"
    git config user.name "Symphony Agent"
  timeout_ms: 30000
```

Users with custom hooks see no change. Docs note: if you override `after_create`, set `user.email`/`user.name` yourself or commits will fail silently.

### Fix 2 — Bounded continuation after success (~30 LOC)

Add `continue_after_success: bool` to `AgentConfig` with default `false`. In `orchestrator::process_run_request`'s success branch, the existing fallback that schedules `CONTINUATION_DELAY_MS` retry is gated on this flag:

```rust
} else if cfg.agent.continue_after_success {
    self.schedule_retry(&issue, 1, CONTINUATION_DELAY_MS, None, request.policy.clone());
}
// else: leave the issue in the running map; next tracker poll will see if its
// state has progressed terminally, in which case the running entry is cleaned up
// by startup_cleanup / poll-driven state reconciliation.
```

Default `false` preserves the visible state machine semantics (issue moves through `Todo → InProgress → Done` driven by the tracker) without the orchestrator manufacturing extra turns on the agent's behalf.

### Fix 3 — Preserve session across re-dispatches (~10 LOC)

Change `process_run_request` to use `entry().or_insert_with(...)` for the `RunningEntry`, so a re-dispatch (continuation, retry-after-failure, follow-up turn) doesn't wipe out the existing session and its accumulated tokens. Update mutable fields (workspace path, started_at) on the existing entry instead.

```rust
s.running
    .entry(issue.id.clone())
    .and_modify(|e| {
        e.workspace_path = workspace.path.display().to_string();
        // started_at left untouched — it's the *first* time the issue started.
    })
    .or_insert_with(|| RunningEntry {
        issue: issue.clone(),
        started_at: Utc::now(),
        workspace_path: workspace.path.display().to_string(),
        session: None,
    });
```

## Data Flow

Nothing meaningful changes at the data-flow level — these are local correctness fixes inside the existing flow.

## Error Handling

Unchanged. Fix 1 makes the agent's `git commit` succeed instead of silently failing; fix 2 makes successful turns terminate cleanly instead of looping; fix 3 makes telemetry accurate. No new error variants.

## Testing

### `AgentConfig::continue_after_success` default + parsing

`crates/symphony-core/tests/agent_config_continue_tests.rs`:
- Default `false` when omitted from WORKFLOW.md.
- Honors explicit `true` / `false` when present.

### `RunningEntry` persistence across `apply_event` re-dispatches

The state-machine effect is hard to test through public APIs (the `running` map is internal). Cover it with a focused unit test that:
- Constructs an `OrchestratorState` directly.
- Inserts a `RunningEntry` with a session that has `tokens.total_tokens = 100`.
- Calls a helper that mimics the `process_run_request` `running.insert(...)` path under the new `entry().or_insert_with()` rule.
- Asserts session is preserved (tokens still 100).

### Workspace git identity (manual)

The git identity fix is config-only. Verify manually: claim a DEMO issue, observe `git -C <workspace> log` shows commits with `Symphony Agent <symphony@localhost>` as the author.

## Pre-merge checklist

- [ ] `cargo test -p symphony-core --test agent_config_continue_tests` green.
- [ ] `cargo test -p symphony-core` no regressions.
- [ ] `cargo test --workspace` no regressions.
- [ ] `cargo clippy --workspace -- -D warnings` clean.
- [ ] Manual smoke against file_mock: claim DEMO-1, observe TurnCompleted, observe NO continuation loop, observe per-issue tokens > 0 in `/api/v1/state`, observe a committed `README.md` change in the workspace.

## Open Questions

None blocking. Two intentionally deferred:

- Should we *release* the issue from the running map after success-with-no-follow-up? The current proposal leaves it in `running` until the tracker poll says otherwise. For file_mock, this means the issue stays "running" forever (slot held). Acceptable for slice 1 of phase 3; revisit when we tackle tracker writeback.
- Should we expose `continue_after_success` per-workflow vs per-issue? Per-workflow only for now; per-issue overrides are a phase 2+ concern.
