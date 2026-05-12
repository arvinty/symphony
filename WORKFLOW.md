---
tracker:
  kind: file_mock
  endpoint: ./issues.mock.json
  active_states: ["Todo", "In Progress"]
  terminal_states: ["Done", "Cancelled"]

polling:
  interval_ms: 5000

workspace:
  root: ./data/workspaces

hooks:
  after_create: |
    git init -q
  timeout_ms: 30000

agent:
  harness: claude_code     # claude_code | hermes | codex (requires codex-cli >= 0.130)
  max_concurrent_agents: 2
  max_turns: 5
  max_retry_backoff_ms: 60000

codex:
  command: "claude -p --output-format stream-json"
  turn_timeout_ms: 600000
  read_timeout_ms: 5000
  stall_timeout_ms: 120000

server:
  port: 8080

policy:
  permission_mode: accept_edits   # accept_edits | require_approval | read_only
  sandbox: workspace_write        # unrestricted | workspace_write | read_only
  allowed_tools: []
  approval_timeout_ms: 300000

vcs:
  remote: origin
  branch_prefix: symphony/
  auto_open_pr: false

# Phase 2 slice 1: optional reviewer agent that fires after the
# implementer PR is opened + linked. Reads the diff with `git diff
# origin/main...HEAD`, posts a single summary comment on the issue
# via linear_graphql.add_comment, ends. Disabled by default.
reviewer:
  enabled: false
  # harness: codex            # defaults to agent.harness
  # policy:                   # defaults to read_only + read_only
  #   permission_mode: read_only
  #   sandbox: read_only
  # prompt_template: |        # defaults to the built-in template;
  #   ...                      # variables: issue_identifier, issue_title, pr_url
---

You are Symphony's coding agent working on issue **{{ issue.identifier }}: {{ issue.title }}**.

Description:
{{ issue.description }}

{% if attempt %}This is attempt #{{ attempt }} for this issue.{% endif %}

Open the workspace, complete the work, and commit any changes. When done, summarize
what changed in a short final message.
