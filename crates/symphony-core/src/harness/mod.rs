use crate::config::EffectiveConfig;
use crate::error::Result;
use crate::events::AgentEvent;
use crate::events::broadcast::OrchestratorEventBus;
use crate::harness::approvals::ApprovalRouter;
use crate::policy::Policy;
use async_trait::async_trait;
use std::path::Path;
use tokio::sync::mpsc;

pub mod claude_code;
pub mod hermes;
pub mod codex;
pub mod approvals;
pub mod mcp_bridge;

pub struct HarnessContext<'a> {
    pub workspace: &'a Path,
    pub prompt: &'a str,
    pub cfg: &'a EffectiveConfig,
    pub tx: mpsc::Sender<AgentEvent>,
    pub bus: OrchestratorEventBus,
    pub approval_router: ApprovalRouter,
    pub policy: Policy,
    pub linear_token: Option<String>,
    pub linear_endpoint: Option<String>,
    pub issue_id: String,
}

#[async_trait]
pub trait Harness: Send + Sync {
    fn name(&self) -> &'static str;
    async fn run(&self, ctx: HarnessContext<'_>) -> Result<HarnessOutcome>;
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
        "codex" | "codex-app-server" => Box::new(codex::CodexHarness::default()),
        other => Box::new(UnknownHarness { name: other.to_string() }),
    }
}

#[derive(Clone)]
struct UnknownHarness {
    name: String,
}

#[async_trait]
impl Harness for UnknownHarness {
    fn name(&self) -> &'static str {
        "unknown"
    }
    async fn run(&self, _ctx: HarnessContext<'_>) -> Result<HarnessOutcome> {
        Err(crate::error::SymphonyError::UnknownHarness(self.name.clone()))
    }
}
