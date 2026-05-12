# Symphony v1.0 Phase 2 Slice 1 — Reviewer Agent Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** After implementer success + PR link, dispatch a reviewer agent run (read-only, separate prompt) that posts a single Linear comment summarizing the PR. ~250 LOC across config, orchestrator, events, and tests.

**Architecture:** New `ReviewerConfig`, new `RunPhase` enum on `RunRequest`, orchestrator hook after the link-PR follow-up turn succeeds. No new harnesses. See `docs/superpowers/specs/2026-05-13-symphony-v1-phase2-slice1-design.md`.

---

## File Structure

**Created:**
- `crates/symphony-core/tests/reviewer_config_tests.rs`
- `crates/symphony-core/tests/reviewer_prompt_tests.rs`
- `crates/symphony-core/tests/reviewer_dispatch_tests.rs`
- `crates/symphony-core/src/reviewer.rs` — `ReviewerConfig`, default template constant, `render_reviewer_prompt`.

**Modified:**
- `crates/symphony-core/src/config.rs` — add `ReviewerConfig` to `ServiceConfig` and `EffectiveConfig`.
- `crates/symphony-core/src/lib.rs` — `pub mod reviewer;`.
- `crates/symphony-core/src/orchestrator.rs` — `RunPhase` enum, `RunRequest.phase` field, PR URL capture, reviewer dispatch hook, harness selection branched on phase.
- `crates/symphony-core/src/events/broadcast.rs` — `OrchestratorEvent::ReviewerStarted` + `ReviewerCompleted` variants.
- `WORKFLOW.md` — example `reviewer:` block.
- `README.md` — short paragraph on reviewer behavior.

---

## Task 1: ReviewerConfig + parsing

**Files:**
- Create: `crates/symphony-core/src/reviewer.rs`
- Create: `crates/symphony-core/tests/reviewer_config_tests.rs`
- Modify: `crates/symphony-core/src/lib.rs`
- Modify: `crates/symphony-core/src/config.rs`

- [ ] **Step 1: Write failing tests**

`tests/reviewer_config_tests.rs`:

```rust
use std::path::PathBuf;
use symphony_core::config::EffectiveConfig;
use symphony_core::policy::PermissionMode;
use symphony_core::workflow::load_workflow;

fn write_temp(body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("symphony_rev_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("WORKFLOW.md");
    std::fs::write(&p, body).unwrap();
    p
}

#[test]
fn default_disables_reviewer() {
    let p = write_temp(r#"---
tracker:
  kind: file_mock
  endpoint: ./issues.json
---
prompt
"#);
    let wf = load_workflow(&p).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();
    assert!(!cfg.reviewer.enabled);
}

#[test]
fn enabled_with_defaults_applies_read_only_policy() {
    let p = write_temp(r#"---
tracker:
  kind: file_mock
  endpoint: ./issues.json
reviewer:
  enabled: true
---
prompt
"#);
    let wf = load_workflow(&p).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();
    assert!(cfg.reviewer.enabled);
    let default_policy = cfg.reviewer.effective_policy();
    assert_eq!(default_policy.permission_mode, PermissionMode::ReadOnly);
}

#[test]
fn explicit_harness_override_is_respected() {
    let p = write_temp(r#"---
tracker:
  kind: file_mock
  endpoint: ./issues.json
reviewer:
  enabled: true
  harness: codex
---
prompt
"#);
    let wf = load_workflow(&p).unwrap();
    let cfg = EffectiveConfig::from_workflow(&wf).unwrap();
    assert_eq!(cfg.reviewer.harness.as_deref(), Some("codex"));
}
```

- [ ] **Step 2: Run tests, expect compile failure**

Run: `cargo test -p symphony-core --test reviewer_config_tests` → fails (no `cfg.reviewer`, no `effective_policy`).

- [ ] **Step 3: Implement `reviewer.rs`**

```rust
use crate::policy::{PermissionMode, Policy, SandboxProfile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ReviewerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub harness: Option<String>,
    #[serde(default)]
    pub policy: Option<Policy>,
    #[serde(default)]
    pub prompt_template: Option<String>,
}

impl ReviewerConfig {
    pub fn effective_policy(&self) -> Policy {
        self.policy.clone().unwrap_or_else(|| Policy {
            permission_mode: PermissionMode::ReadOnly,
            sandbox: SandboxProfile::ReadOnly,
            allowed_tools: vec![],
            approval_timeout_ms: 300_000,
        })
    }
}

pub const DEFAULT_REVIEWER_PROMPT: &str = r#"You are reviewing a Symphony-authored pull request.

Issue: {{issue_identifier}} — {{issue_title}}
PR: {{pr_url}}

The branch has been pushed; the workspace contains the implementation. Read
the diff between origin/main and HEAD with `git diff origin/main...HEAD`,
then post a single review comment on the issue via `linear_graphql.add_comment`
summarizing your findings. Focus on correctness, security, and clarity. End
the turn after posting the comment."#;

pub fn render_reviewer_prompt(
    template: &str,
    issue_identifier: &str,
    issue_title: &str,
    pr_url: &str,
) -> Result<String, liquid::Error> {
    let parser = liquid::ParserBuilder::with_stdlib().build()?;
    let tmpl = parser.parse(template)?;
    let globals = liquid::object!({
        "issue_identifier": issue_identifier,
        "issue_title": issue_title,
        "pr_url": pr_url,
    });
    tmpl.render(&globals)
}
```

- [ ] **Step 4: Wire into `ServiceConfig` + `EffectiveConfig`**

In `crates/symphony-core/src/config.rs`:

Add to `ServiceConfig`:
```rust
#[serde(default)]
pub reviewer: Option<crate::reviewer::ReviewerConfig>,
```

Add to `EffectiveConfig`:
```rust
pub reviewer: crate::reviewer::ReviewerConfig,
```

In `EffectiveConfig::from_workflow`:
```rust
reviewer: cfg.reviewer.unwrap_or_default(),
```

In `crates/symphony-core/src/lib.rs`: `pub mod reviewer;`

- [ ] **Step 5: Verify tests pass**

Run: `cargo test -p symphony-core --test reviewer_config_tests` → green.
Run: `cargo check --workspace` → clean.

- [ ] **Step 6: Commit**

```bash
git add crates/symphony-core/src/reviewer.rs crates/symphony-core/src/lib.rs crates/symphony-core/src/config.rs crates/symphony-core/tests/reviewer_config_tests.rs
git commit -m "Add ReviewerConfig with defaults and effective_policy resolver"
```

---

## Task 2: Reviewer prompt template + render tests

**Files:**
- Create: `crates/symphony-core/tests/reviewer_prompt_tests.rs`

- [ ] **Step 1: Write failing tests**

```rust
use symphony_core::reviewer::{render_reviewer_prompt, DEFAULT_REVIEWER_PROMPT};

#[test]
fn default_template_substitutes_variables() {
    let out = render_reviewer_prompt(
        DEFAULT_REVIEWER_PROMPT,
        "DEMO-1",
        "Add feature X",
        "https://github.com/foo/bar/pull/42",
    )
    .unwrap();
    assert!(out.contains("DEMO-1"));
    assert!(out.contains("Add feature X"));
    assert!(out.contains("https://github.com/foo/bar/pull/42"));
    assert!(out.contains("linear_graphql.add_comment"));
}

#[test]
fn custom_template_renders_with_same_vars() {
    let tpl = "Review {{issue_identifier}} at {{pr_url}}";
    let out = render_reviewer_prompt(tpl, "DEMO-1", "ignored", "https://x").unwrap();
    assert_eq!(out, "Review DEMO-1 at https://x");
}

#[test]
fn missing_variable_errors_on_strict_parse() {
    let tpl = "{{nonexistent}}";
    // liquid renders missing vars as empty by default; assert that's the behavior
    let out = render_reviewer_prompt(tpl, "DEMO-1", "t", "u").unwrap();
    assert_eq!(out, "");
}
```

- [ ] **Step 2: Verify pass**

Run: `cargo test -p symphony-core --test reviewer_prompt_tests` → green.

- [ ] **Step 3: Commit**

```bash
git add crates/symphony-core/tests/reviewer_prompt_tests.rs
git commit -m "Cover reviewer prompt template rendering"
```

---

## Task 3: OrchestratorEvent variants for reviewer

**Files:**
- Modify: `crates/symphony-core/src/events/broadcast.rs`

- [ ] **Step 1: Add the variants**

```rust
ReviewerStarted {
    issue_id: String,
    pr_url: String,
},
ReviewerCompleted {
    issue_id: String,
    success: bool,
    error: Option<String>,
},
```

- [ ] **Step 2: Verify**

Run: `cargo check --workspace` → clean.

- [ ] **Step 3: Commit**

```bash
git add crates/symphony-core/src/events/broadcast.rs
git commit -m "Add ReviewerStarted/ReviewerCompleted OrchestratorEvent variants"
```

---

## Task 4: RunPhase + RunRequest extension

**Files:**
- Modify: `crates/symphony-core/src/orchestrator.rs`

- [ ] **Step 1: Add `RunPhase` enum and field on `RunRequest`**

```rust
#[derive(Debug, Clone)]
pub(crate) enum RunPhase {
    Implementer,
    Reviewer,
}
```

Add `phase: RunPhase` to `RunRequest` struct. Update every `RunRequest { ... }` literal in the file to set `phase: RunPhase::Implementer` for existing call sites (claim path, retry path, follow-up path).

- [ ] **Step 2: Add PR URL capture state**

In `OrchestratorInner` (or wherever per-issue runtime state lives), add:
```rust
pr_urls: tokio::sync::RwLock<std::collections::HashMap<String, String>>, // issue_id -> pr_url
```

In the `open_pr` success branch, insert `(issue.id.clone(), url.clone())`.

In terminal-state transition logic (if any explicit cleanup exists), remove from the map. Otherwise let it grow — it's small.

- [ ] **Step 3: Verify**

Run: `cargo check -p symphony-core` → clean.
Run: `cargo test --workspace` → no regressions.

- [ ] **Step 4: Commit**

```bash
git add crates/symphony-core/src/orchestrator.rs
git commit -m "Add RunPhase enum and PR URL capture for reviewer dispatch"
```

---

## Task 5: Reviewer dispatch + harness selection

**Files:**
- Modify: `crates/symphony-core/src/orchestrator.rs`

- [ ] **Step 1: Dispatch reviewer after link-PR follow-up succeeds**

In `process_run_request`'s success branch, after the existing follow-up logic, add:

```rust
// Reviewer dispatch — fires after the link-PR follow-up turn succeeds.
if request.follow_up_count == 1
    && last_outcome_success
    && cfg.reviewer.enabled
    && matches!(request.phase, RunPhase::Implementer)
{
    let pr_url = self.inner.pr_urls.read().await.get(&issue.id).cloned();
    if let Some(pr_url) = pr_url {
        let template = cfg
            .reviewer
            .prompt_template
            .as_deref()
            .unwrap_or(crate::reviewer::DEFAULT_REVIEWER_PROMPT);
        match crate::reviewer::render_reviewer_prompt(
            template,
            &issue.identifier,
            &issue.title,
            &pr_url,
        ) {
            Ok(prompt) => {
                let _ = self.event_bus().send(
                    crate::events::broadcast::OrchestratorEvent::ReviewerStarted {
                        issue_id: issue.id.clone(),
                        pr_url: pr_url.clone(),
                    },
                );
                let _ = self.inner.run_tx.send(RunRequest {
                    issue: issue.clone(),
                    attempt: None,
                    prompt_override: Some(prompt),
                    follow_up_count: 0,
                    policy: cfg.reviewer.effective_policy(),
                    phase: RunPhase::Reviewer,
                });
            }
            Err(e) => {
                let _ = self.event_bus().send(
                    crate::events::broadcast::OrchestratorEvent::ReviewerCompleted {
                        issue_id: issue.id.clone(),
                        success: false,
                        error: Some(format!("prompt_render_failed: {e}")),
                    },
                );
            }
        }
    }
}

// Emit ReviewerCompleted at the end of a Reviewer-phase run.
if matches!(request.phase, RunPhase::Reviewer) {
    let _ = self.event_bus().send(
        crate::events::broadcast::OrchestratorEvent::ReviewerCompleted {
            issue_id: issue.id.clone(),
            success: last_outcome_success,
            error: last_error.clone(),
        },
    );
}
```

- [ ] **Step 2: Branch harness selection by phase**

In the harness selection block:

```rust
let harness_name = match &request.phase {
    RunPhase::Implementer => cfg.agent.harness.clone(),
    RunPhase::Reviewer => cfg
        .reviewer
        .harness
        .clone()
        .unwrap_or_else(|| cfg.agent.harness.clone()),
};
let harness = crate::harness::select_harness(&harness_name);
```

- [ ] **Step 3: Skip retry/schedule for reviewer-phase runs on failure**

Reviewer failure should not trigger the normal retry path. Guard the retry dispatch:

```rust
if !matches!(request.phase, RunPhase::Reviewer) {
    self.fail_and_schedule_retry(...);
}
```

- [ ] **Step 4: Verify**

Run: `cargo check --workspace` → clean.
Run: `cargo test --workspace` → no regressions.

- [ ] **Step 5: Commit**

```bash
git add crates/symphony-core/src/orchestrator.rs
git commit -m "Dispatch reviewer agent run after link-PR turn succeeds"
```

---

## Task 6: Integration test for reviewer dispatch

**Files:**
- Create: `crates/symphony-core/tests/reviewer_dispatch_tests.rs`

- [ ] **Step 1: Write the test**

The test should:
1. Build an `Orchestrator` with `reviewer.enabled: true` and a stub harness that records its calls.
2. Inject a fake PR URL into `inner.pr_urls`.
3. Push a `RunRequest` representing the link-PR follow-up turn completing (`follow_up_count: 1`, `phase: Implementer`).
4. Assert a new `RunRequest` is sent on `run_tx` within a short timeout, with `phase: RunPhase::Reviewer`.
5. Assert `OrchestratorEvent::ReviewerStarted` is broadcast.

Pattern follows existing `approval_flow_tests.rs`. ~120 LOC.

If exposing `RunPhase`/`RunRequest` publicly is undesirable for testing, alternatively assert behavior end-to-end: stub harness records its received policy + prompt; reviewer-phase invocation should arrive with `policy.permission_mode == ReadOnly` and a prompt containing the PR URL.

- [ ] **Step 2: Verify**

Run: `cargo test -p symphony-core --test reviewer_dispatch_tests` → green.

- [ ] **Step 3: Commit**

```bash
git add crates/symphony-core/tests/reviewer_dispatch_tests.rs
git commit -m "Cover reviewer dispatch flow with mock orchestrator harness"
```

---

## Task 7: Workflow + README docs

- [ ] **Step 1: Update `WORKFLOW.md`**

Add an example `reviewer:` block (commented out by default, with `enabled: false`).

- [ ] **Step 2: Update `README.md`**

Add a short paragraph under the harness section describing the reviewer behavior and pointing at the spec.

- [ ] **Step 3: Commit**

```bash
git add WORKFLOW.md README.md
git commit -m "Document reviewer agent config and behavior"
```

---

## Pre-merge checklist

- [ ] `cargo test -p symphony-core --test reviewer_config_tests` green.
- [ ] `cargo test -p symphony-core --test reviewer_prompt_tests` green.
- [ ] `cargo test -p symphony-core --test reviewer_dispatch_tests` green.
- [ ] `cargo test --workspace` no regressions.
- [ ] `cargo clippy --workspace -- -D warnings` clean.
- [ ] Manual: end-to-end run with `reviewer.enabled: true`; observe two AgentEvent streams + a Linear comment from the reviewer.
- [ ] `docs/superpowers/brainstorms/2026-05-13-symphony-v1-phase2-slice1-state.md` deleted.

## Risks & rollback

- **`open_pr` failure → no reviewer.** Acceptable — there's nothing to review.
- **Reviewer infinite-loop risk** if reviewer's own success somehow re-triggers dispatch. Guard: `matches!(request.phase, RunPhase::Implementer)` gate on the dispatch hook prevents reviewer success from spawning another reviewer.
- **Reviewer policy too permissive.** Default ReadOnly+ReadOnly is the safe baseline. Operators can opt into looser settings explicitly.

Rollback: revert the v1-phase2-slice1 branch. No data model changes, no migrations.
