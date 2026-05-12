# Slice 3 brainstorm — complete (2026-05-13)

**Status:** Sections 1–5 worked through. User confirmed Hermes natively
supports `--mcp-config`-style MCP wiring, `--permission-mode`-style
policy flags, and `tool_use`-block JSON streaming — so slice 3 takes
"Option B: native Hermes flags, mirror Claude harness exactly".

Output artifacts:
- Spec: `docs/superpowers/specs/2026-05-13-symphony-v1-slice3-design.md`
- Plan: `docs/superpowers/plans/2026-05-13-symphony-v1-slice3.md`

Branched off `v1-slice2` so slice 3 stacks on top of slice 2 until
both merge. After both merge to `main`, the stacking is invisible.

## Background

Per `docs/superpowers/specs/2026-05-10-symphony-v1-phase1-design.md`,
slice 3 is roughly: "Same tool definitions advertised in Hermes' tool
format; same policy translation; same VCS hook; same approvals channel"
(~80 LOC delta). Reuses everything slice 1 built: `Policy`, `mcp_bridge`,
`OrchestratorEventBus`, `ApprovalRouter`, `vcs::{push_branch, open_pr}`.

The existing Hermes harness (`crates/symphony-core/src/harness/hermes.rs`,
101 LOC) spawns `hermes run` with `--prompt`, reads line-delimited JSON
from stdout, emits each line as `AgentEvent::Notification`. No MCP, no
policy, no approval interception, no tool-call bus events.

## Information gap

I do not have the Hermes CLI installed locally and the codebase doesn't
document its full flag set. Slice 3's design depends on which native
capabilities Hermes exposes:

- **MCP server configuration.** Does Hermes accept a flag like Claude's
  `--mcp-config <path>`, or does it use a different mechanism (config
  file, env var, stdin handshake)?
- **Permission mode / sandbox.** Does Hermes expose flags equivalent to
  Claude's `--permission-mode` / `--allowedTools`? Or only auto-approval?
- **Tool-call event shape.** Does the `--json` stream emit
  `tool_use`-style blocks we can parse for `OrchestratorEvent::ToolCall`?
- **Approval round-trip.** Does Hermes pause and ask for approval, or is
  every action auto-executed?

These determine which option from section 1 below applies. The user has
to answer for the brainstorm to proceed.

## Where we paused

After section 1 (architecture) was presented with three options
(symphony-side approximation / native Hermes flags / hybrid). User
selection pending.
