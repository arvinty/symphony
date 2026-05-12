// End-to-end smoke for Slice 2 (Codex `app-server`).
//
// Gated behind both the `e2e_codex` cargo feature AND the env var
// `CODEX_E2E=1`. Without those, this test is invisible.
//
// Run:
//   $env:CODEX_E2E=1
//   $env:LINEAR_CLONE_ADMIN_TOKEN="dev-admin"
//   cargo test -p symphony-core --features e2e_codex --test slice2_smoke -- --nocapture
//
// Requirements:
//   - Real `codex` CLI installed (>= 0.130) with `codex app-server` available.
//   - linear-clone running locally on port 4000 (same as slice 1).
//   - WORKFLOW.md configured with harness=codex and tracker.kind=linear.
//   - Network access for the MCP bridge to call linear-clone.
//
// Programmatic assertion is intentionally limited: a full orchestrator boot
// requires a real Codex subscription and a configured linear-clone. The
// happy-path notification pump is already covered by the harness_codex.rs
// integration tests against a scripted DuplexStream "server" — those run in
// CI. This file documents the manual verification steps for slice 2.

#![cfg(feature = "e2e_codex")]

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn slice2_smoke_codex_runs_a_turn() {
    if std::env::var("CODEX_E2E").is_err() {
        eprintln!("skipping: set CODEX_E2E=1 to run");
        return;
    }
    eprintln!("Slice 2 smoke is documented as a manual verification gate.");
    eprintln!("Steps:");
    eprintln!("  1. Start linear-clone on :4000 with LINEAR_CLONE_ADMIN_TOKEN=dev-admin.");
    eprintln!("  2. WORKFLOW.md: tracker.kind=linear, harness: codex,");
    eprintln!("     policy.permission_mode=accept_edits.");
    eprintln!("  3. Add a test issue assigned to a state in active_states.");
    eprintln!("  4. Start `cargo run -p symphony -- --workflow WORKFLOW.md`.");
    eprintln!("  5. Open http://127.0.0.1:8080. Expected:");
    eprintln!("     - issue enters in_progress");
    eprintln!("     - event feed shows AgentEvent::TurnStarted");
    eprintln!("     - bus shows codex.* ToolCall events");
    eprintln!("     - event feed terminates with TurnCompleted (success)");
    eprintln!("  6. Re-run with policy.permission_mode=require_approval and a prompt that");
    eprintln!("     triggers a guardian-denied action. Expected: dashboard approval toast");
    eprintln!("     fires. Click Approve. Expected: turn proceeds past the override.");
}
