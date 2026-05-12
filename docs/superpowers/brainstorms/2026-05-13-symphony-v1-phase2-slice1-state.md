# Phase 2 Slice 1 brainstorm — complete (2026-05-13)

**Status:** Brainstormed in one pass (user delegated scope: "you wrote this").
Output artifacts:
- Spec: `docs/superpowers/specs/2026-05-13-symphony-v1-phase2-slice1-design.md`
- Plan: `docs/superpowers/plans/2026-05-13-symphony-v1-phase2-slice1.md`

## Scope chosen

Reviewer agent. After the implementer succeeds and the PR is opened and
linked back to the Linear issue, dispatch a second agent run (the
"reviewer") with a read-only policy and a reviewer-specific prompt
template. The reviewer inspects the diff, posts a single review summary
comment via `linear_graphql.add_comment`, and ends. Reviewer success is
best-effort — the issue reaches its terminal state regardless.

This earns the "multi-agent" name (two distinct agent runs per issue)
without adding new harnesses, new external tools, or new state machine
states. Subsequent slices in phase 2 can add: GitHub PR comment tool,
reviewer-requested-changes loop, parallel reviewers, etc.

## Decisions captured

1. **Trigger.** Reviewer dispatches after the link-PR follow-up turn
   completes successfully (`follow_up_count == 1`, success).
2. **Run mechanism.** New `RunPhase::Implementer | Reviewer { pr_url }`
   variant on `RunRequest`. Reviewer reuses `HarnessContext` and existing
   harness implementations — no new harness trait.
3. **Configurable in WORKFLOW.md.** New `reviewer:` block with
   `enabled`, optional `harness`, optional `policy`, optional
   `prompt_template`. If `enabled: false` (default), nothing changes
   versus phase 1.
4. **Default reviewer policy.** `permission_mode: read_only`,
   `sandbox: read_only`. Override per workflow.
5. **Default reviewer harness.** Same as `agent.harness`. Override per
   workflow.
6. **Default prompt template.** Hard-coded fallback if `prompt_template`
   not provided. Liquid-rendered with `issue_identifier`, `issue_title`,
   `pr_url`.
7. **Events.** New `OrchestratorEvent::ReviewerStarted { issue_id,
   pr_url }` and `ReviewerCompleted { issue_id, success }`.
8. **Failure handling.** Reviewer failure does not block issue
   completion. Operator sees the failure on the dashboard via existing
   `AgentEvent::TurnFailed` flow, but state advances normally.

## Where we ended

Brainstorm closed. Implementation proceeds against `v1-phase2-slice1`
branch (already created off `main`).
