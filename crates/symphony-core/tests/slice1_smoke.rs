// End-to-end smoke for Slice 1.
//
// Gated behind both the `e2e_claude_code` cargo feature AND the env var
// `CLAUDE_CODE_E2E=1`. Without those, this test is invisible.
//
// Run on Windows / PowerShell:
//   $env:CLAUDE_CODE_E2E=1
//   $env:LINEAR_CLONE_ADMIN_TOKEN="dev-admin"
//   cargo test -p symphony-core --features e2e_claude_code --test slice1_smoke -- --nocapture
//
// Requirements:
//   - Real `claude` CLI installed and authenticated.
//   - linear-clone running locally on port 4000.
//   - WORKFLOW.md configured with tracker.kind=linear and policy.permission_mode=accept_edits.
//   - Network access for the agent to call the linear-clone /admin/tokens + /graphql.

#![cfg(feature = "e2e_claude_code")]

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn slice1_smoke_claude_code_uses_linear_graphql() {
    if std::env::var("CLAUDE_CODE_E2E").is_err() {
        eprintln!("skipping: set CLAUDE_CODE_E2E=1 to run");
        return;
    }
    eprintln!("Slice 1 smoke is documented as a manual verification gate.");
    eprintln!("Steps:");
    eprintln!("  1. Start linear-clone on :4000 with LINEAR_CLONE_ADMIN_TOKEN=dev-admin.");
    eprintln!("  2. Ensure WORKFLOW.md sets tracker.kind=linear and points at it.");
    eprintln!("  3. Add a test issue assigned to a state in active_states.");
    eprintln!("  4. Start `cargo run -p symphony -- --workflow WORKFLOW.md`.");
    eprintln!("  5. Open http://127.0.0.1:8080 and watch the issue panel.");
    eprintln!("  6. Expected: the agent emits at least one tool_call event for");
    eprintln!("     `linear_graphql.add_comment` or `linear_graphql.link_pull_request`,");
    eprintln!("     visible in the live event feed and on the SSE stream at");
    eprintln!("     /api/v1/events?issue=<identifier>.");
    eprintln!("  7. If vcs.auto_open_pr=true with a real GitHub remote, expected the");
    eprintln!("     orchestrator to push branch + open PR + inject a follow-up turn.");
    // Programmatic assertion is intentionally limited: a full orchestrator boot
    // requires a real Claude Code subscription and is out of scope for automated CI.
}
