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
  harness: claude_code     # or: hermes | codex_stub
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
---

You are Symphony's coding agent working on issue **{{ issue.identifier }}: {{ issue.title }}**.

Description:
{{ issue.description }}

{% if attempt %}This is attempt #{{ attempt }} for this issue.{% endif %}

Open the workspace, complete the work, and commit any changes. When done, summarize
what changed in a short final message.
