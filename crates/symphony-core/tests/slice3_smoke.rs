// End-to-end smoke for Slice 3 (Hermes).
//
// Gated behind both the `e2e_hermes` cargo feature AND the env var
// `HERMES_E2E=1`. Without those, this test is invisible.
//
// Run:
//   $env:HERMES_E2E=1
//   $env:LINEAR_CLONE_ADMIN_TOKEN="dev-admin"
//   cargo test -p symphony-core --features e2e_hermes --test slice3_smoke -- --nocapture
//
// Requirements:
//   - Real `hermes` CLI installed and authenticated.
//   - linear-clone running locally on port 4000.
//   - WORKFLOW.md configured with harness=hermes and tracker.kind=linear.
//
// Programmatic assertion is intentionally limited — the harness shape is
// already covered by hermes_integration.rs against a shim. This file
// documents the manual verification steps against the real Hermes CLI.

#![cfg(feature = "e2e_hermes")]

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "manual smoke requiring a real Hermes CLI and configured linear-clone"]
async fn slice3_smoke_hermes_uses_linear_mcp() {
    assert_eq!(
        std::env::var("HERMES_E2E").as_deref(),
        Ok("1"),
        "set HERMES_E2E=1 when explicitly running this ignored smoke test"
    );
    let status = tokio::process::Command::new("hermes")
        .arg("--version")
        .status()
        .await
        .expect("hermes CLI must be installed on PATH");
    assert!(status.success(), "`hermes --version` failed");
    eprintln!("Slice 3 smoke is documented as a manual verification gate.");
    eprintln!("Steps:");
    eprintln!("  1. Start linear-clone on :4000 with LINEAR_CLONE_ADMIN_TOKEN=dev-admin.");
    eprintln!("  2. WORKFLOW.md: tracker.kind=linear, harness: hermes,");
    eprintln!("     policy.permission_mode=accept_edits.");
    eprintln!("  3. Add a test issue assigned to a state in active_states.");
    eprintln!("  4. Start `cargo run -p symphony -- --workflow WORKFLOW.md`.");
    eprintln!("  5. Open http://127.0.0.1:8080. Expected:");
    eprintln!("     - Hermes spawn observed with --mcp-config and --permission-mode flags");
    eprintln!("     - At least one OrchestratorEvent::ToolCall for linear_graphql.* on the bus");
    eprintln!("     - TurnCompleted on success");
    eprintln!("  6. Verify the agent's hosted --permission-mode UI is the actual approval gate");
    eprintln!("     for write actions (Symphony itself does not gate Hermes tool calls).");
}
