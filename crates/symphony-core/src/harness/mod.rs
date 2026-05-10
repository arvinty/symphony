use crate::config::EffectiveConfig;
use crate::error::Result;
use crate::events::AgentEvent;
use async_trait::async_trait;
use std::path::Path;
use tokio::sync::mpsc;

pub mod claude_code;
pub mod hermes;
pub mod codex_stub;
pub mod approvals;

/// One agent run within one workspace. Produces a stream of events and a final outcome.
#[async_trait]
pub trait Harness: Send + Sync {
    fn name(&self) -> &'static str;
    /// Run a single turn (or session) and stream events.
    async fn run(
        &self,
        workspace: &Path,
        prompt: &str,
        cfg: &EffectiveConfig,
        tx: mpsc::Sender<AgentEvent>,
    ) -> Result<HarnessOutcome>;
}

#[derive(Debug, Clone)]
pub struct HarnessOutcome {
    pub thread_id: String,
    pub turn_id: String,
    pub success: bool,
    pub error: Option<String>,
}

pub fn select_harness(name: &str) -> Box<dyn Harness + Send + Sync> {
    match name {
        "claude_code" | "claude" | "claude-code" => Box::new(claude_code::ClaudeCodeHarness::default()),
        "hermes" => Box::new(hermes::HermesHarness::default()),
        _ => Box::new(codex_stub::CodexStubHarness::default()),
    }
}
