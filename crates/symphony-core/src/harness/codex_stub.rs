use super::{Harness, HarnessOutcome};
use crate::config::EffectiveConfig;
use crate::error::Result;
use crate::events::AgentEvent;
use async_trait::async_trait;
use std::path::Path;
use tokio::sync::mpsc;

/// Stub Codex harness. The full Codex app-server protocol is out of scope for v0.1.
/// Returns immediate success with a synthesized session id so the orchestrator can be
/// exercised end-to-end without a Codex binary.
#[derive(Default, Clone)]
pub struct CodexStubHarness {}

#[async_trait]
impl Harness for CodexStubHarness {
    fn name(&self) -> &'static str {
        "codex_stub"
    }

    async fn run(
        &self,
        _workspace: &Path,
        _prompt: &str,
        _cfg: &EffectiveConfig,
        _tx: mpsc::Sender<AgentEvent>,
    ) -> Result<HarnessOutcome> {
        Ok(HarnessOutcome {
            thread_id: format!("codex-stub-{}", uuid::Uuid::new_v4()),
            turn_id: format!("turn-{}", uuid::Uuid::new_v4()),
            success: true,
            error: None,
        })
    }
}
