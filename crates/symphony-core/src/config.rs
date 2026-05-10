use crate::error::{Result, SymphonyError};
use crate::workflow::WorkflowDefinition;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerConfig {
    pub kind: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub project_slug: Option<String>,
    #[serde(default = "default_active_states")]
    pub active_states: Vec<String>,
    #[serde(default = "default_terminal_states")]
    pub terminal_states: Vec<String>,
}

fn default_active_states() -> Vec<String> {
    vec!["Todo".into(), "In Progress".into()]
}
fn default_terminal_states() -> Vec<String> {
    ["Closed", "Cancelled", "Canceled", "Duplicate", "Done"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollingConfig {
    #[serde(default = "default_poll_interval")]
    pub interval_ms: u64,
}
fn default_poll_interval() -> u64 {
    30_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    #[serde(default)]
    pub root: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HooksConfig {
    #[serde(default)]
    pub after_create: Option<String>,
    #[serde(default)]
    pub before_run: Option<String>,
    #[serde(default)]
    pub after_run: Option<String>,
    #[serde(default)]
    pub before_remove: Option<String>,
    #[serde(default = "default_hook_timeout")]
    pub timeout_ms: u64,
}
fn default_hook_timeout() -> u64 {
    60_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_agents: u32,
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    #[serde(default = "default_max_retry_backoff")]
    pub max_retry_backoff_ms: u64,
    #[serde(default)]
    pub max_concurrent_agents_by_state: HashMap<String, u32>,
    /// Which harness to use: "claude_code" | "hermes" | "codex" (codex is stub)
    #[serde(default = "default_harness")]
    pub harness: String,
}
fn default_max_concurrent() -> u32 {
    10
}
fn default_max_turns() -> u32 {
    20
}
fn default_max_retry_backoff() -> u64 {
    300_000
}
fn default_harness() -> String {
    "claude_code".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexConfig {
    #[serde(default = "default_codex_command")]
    pub command: String,
    #[serde(default)]
    pub approval_policy: Option<String>,
    #[serde(default)]
    pub thread_sandbox: Option<String>,
    #[serde(default)]
    pub turn_sandbox_policy: Option<String>,
    #[serde(default = "default_turn_timeout")]
    pub turn_timeout_ms: u64,
    #[serde(default = "default_read_timeout")]
    pub read_timeout_ms: u64,
    #[serde(default = "default_stall_timeout")]
    pub stall_timeout_ms: i64,
}
fn default_codex_command() -> String {
    "codex app-server".into()
}
fn default_turn_timeout() -> u64 {
    3_600_000
}
fn default_read_timeout() -> u64 {
    5_000
}
fn default_stall_timeout() -> i64 {
    300_000
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerConfig {
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VcsConfig {
    #[serde(default)]
    pub remote: Option<String>,
    #[serde(default)]
    pub branch_prefix: Option<String>,
    #[serde(default)]
    pub auto_open_pr: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub tracker: TrackerConfig,
    #[serde(default)]
    pub polling: Option<PollingConfig>,
    #[serde(default)]
    pub workspace: Option<WorkspaceConfig>,
    #[serde(default)]
    pub hooks: Option<HooksConfig>,
    #[serde(default)]
    pub agent: Option<AgentConfig>,
    #[serde(default)]
    pub codex: Option<CodexConfig>,
    #[serde(default)]
    pub server: Option<ServerConfig>,
    #[serde(default)]
    pub policy: Option<crate::policy::Policy>,
    #[serde(default)]
    pub vcs: Option<VcsConfig>,
}

#[derive(Debug, Clone)]
pub struct EffectiveConfig {
    pub tracker: TrackerConfig,
    pub polling: PollingConfig,
    pub workspace_root: PathBuf,
    pub hooks: HooksConfig,
    pub agent: AgentConfig,
    pub codex: CodexConfig,
    pub server: ServerConfig,
    pub policy: crate::policy::Policy,
    pub vcs: VcsConfig,
}

impl EffectiveConfig {
    pub fn from_workflow(wf: &WorkflowDefinition) -> Result<Self> {
        let cfg: ServiceConfig = serde_yaml::from_value(wf.config.clone())
            .map_err(|e| SymphonyError::ConfigInvalid(e.to_string()))?;

        let mut tracker = cfg.tracker;
        if tracker.kind.eq_ignore_ascii_case("linear") {
            if tracker.endpoint.is_none() {
                tracker.endpoint = Some("https://api.linear.app/graphql".into());
            }
            tracker.api_key = tracker.api_key.map(resolve_env_indirection);
            if tracker.api_key.as_deref().map(str::is_empty).unwrap_or(false) {
                tracker.api_key = None;
            }
        } else if tracker.kind != "file_mock" {
            // unknown kinds are accepted at parse but rejected by validation
        }

        let workspace_cfg = cfg.workspace.unwrap_or(WorkspaceConfig { root: None });
        let workspace_root = resolve_workspace_root(&workspace_cfg.root, &wf.source_path)?;

        Ok(Self {
            tracker,
            polling: cfg.polling.unwrap_or(PollingConfig {
                interval_ms: default_poll_interval(),
            }),
            workspace_root,
            hooks: cfg.hooks.unwrap_or_default(),
            agent: cfg.agent.unwrap_or(AgentConfig {
                max_concurrent_agents: default_max_concurrent(),
                max_turns: default_max_turns(),
                max_retry_backoff_ms: default_max_retry_backoff(),
                max_concurrent_agents_by_state: HashMap::new(),
                harness: default_harness(),
            }),
            codex: cfg.codex.unwrap_or(CodexConfig {
                command: default_codex_command(),
                approval_policy: None,
                thread_sandbox: None,
                turn_sandbox_policy: None,
                turn_timeout_ms: default_turn_timeout(),
                read_timeout_ms: default_read_timeout(),
                stall_timeout_ms: default_stall_timeout(),
            }),
            server: cfg.server.unwrap_or_default(),
            policy: cfg.policy.unwrap_or_default(),
            vcs: cfg.vcs.unwrap_or_default(),
        })
    }

    pub fn validate_for_dispatch(&self) -> Result<()> {
        match self.tracker.kind.as_str() {
            "linear" => {
                if self.tracker.api_key.as_deref().unwrap_or("").is_empty() {
                    return Err(SymphonyError::MissingTrackerApiKey);
                }
                if self
                    .tracker
                    .project_slug
                    .as_deref()
                    .unwrap_or("")
                    .is_empty()
                {
                    return Err(SymphonyError::MissingTrackerProjectSlug);
                }
            }
            "file_mock" => {}
            other => return Err(SymphonyError::UnsupportedTrackerKind(other.into())),
        }
        if self.codex.command.trim().is_empty() {
            return Err(SymphonyError::ConfigInvalid("codex.command empty".into()));
        }
        Ok(())
    }
}

fn resolve_env_indirection(value: String) -> String {
    if let Some(var_name) = value.strip_prefix('$') {
        std::env::var(var_name).unwrap_or_default()
    } else {
        value
    }
}

fn resolve_workspace_root(raw: &Option<String>, workflow_path: &Path) -> Result<PathBuf> {
    let workflow_dir = workflow_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let raw = raw.clone().unwrap_or_else(|| {
        std::env::temp_dir()
            .join("symphony_workspaces")
            .display()
            .to_string()
    });
    let expanded = expand_path(&raw);
    let p = PathBuf::from(expanded);
    let abs = if p.is_absolute() {
        p
    } else {
        workflow_dir.join(p)
    };
    Ok(normalize_absolute(&abs))
}

fn expand_path(raw: &str) -> String {
    let mut s = raw.to_string();
    if let Some(stripped) = s.strip_prefix("~") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            s = format!("{}{}", home.to_string_lossy(), stripped);
        }
    }
    if let Some(rest) = s.strip_prefix('$') {
        // Allow $VAR style for whole-value path
        if !rest.contains('/') && !rest.contains('\\') {
            if let Ok(v) = std::env::var(rest) {
                return v;
            }
        }
    }
    s
}

fn normalize_absolute(p: &Path) -> PathBuf {
    if p.exists() {
        std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
    } else {
        p.to_path_buf()
    }
}
