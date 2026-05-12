//! Reviewer agent configuration and prompt rendering.
//!
//! Phase 2 slice 1: after the implementer succeeds and the PR is opened +
//! linked, an optional reviewer run executes with a separate (typically
//! read-only) policy and a reviewer-specific prompt template.

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
    /// Returns the configured policy, falling back to a strict read-only
    /// default when nothing is specified.
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
