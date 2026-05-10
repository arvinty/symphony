use crate::config::EffectiveConfig;
use crate::error::{Result, SymphonyError};
use crate::events::{AgentEvent, AgentEventKind};
use crate::harness::approvals::ApprovalRouter;
use crate::harness::{select_harness, Harness};
use crate::model::{Issue, LiveSession, RetryEntry, RunningEntry, UsageTokens};
use crate::prompt::render_prompt;
use crate::state::OrchestratorState;
use crate::tracker::Tracker;
use crate::workflow::WorkflowDefinition;
use crate::workspace::WorkspaceManager;
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex, RwLock};

const CONTINUATION_DELAY_MS: u64 = 1_000;
const RETRY_BASE_MS: u64 = 10_000;

/// One scheduled retry, drained by the retry-dispatcher task.
#[derive(Debug, Clone)]
struct RetryRequest {
    issue_id: String,
    #[allow(dead_code)]
    identifier: String,
    attempt: u32,
    delay_ms: u64,
    #[allow(dead_code)]
    error: Option<String>,
}

/// One run dispatch, drained by the run-dispatcher task.
#[derive(Debug, Clone)]
struct RunRequest {
    issue: Issue,
    attempt: Option<u32>,
}

#[derive(Clone)]
pub struct Orchestrator {
    inner: Arc<OrchestratorInner>,
}

struct OrchestratorInner {
    workflow: RwLock<WorkflowDefinition>,
    config: RwLock<EffectiveConfig>,
    tracker: RwLock<Arc<dyn Tracker>>,
    workspace: RwLock<WorkspaceManager>,
    state: Mutex<OrchestratorState>,
    shutdown: tokio::sync::Notify,
    run_tx: mpsc::UnboundedSender<RunRequest>,
    run_rx: Mutex<Option<mpsc::UnboundedReceiver<RunRequest>>>,
    retry_tx: mpsc::UnboundedSender<RetryRequest>,
    retry_rx: Mutex<Option<mpsc::UnboundedReceiver<RetryRequest>>>,
    event_bus: crate::events::broadcast::OrchestratorEventBus,
    approval_router: ApprovalRouter,
}

impl Orchestrator {
    pub fn new(
        workflow: WorkflowDefinition,
        config: EffectiveConfig,
        tracker: Arc<dyn Tracker>,
        workspace: WorkspaceManager,
    ) -> Self {
        let state = OrchestratorState {
            poll_interval_ms: config.polling.interval_ms,
            max_concurrent_agents: config.agent.max_concurrent_agents,
            started_at: Some(Utc::now()),
            ..Default::default()
        };
        let (run_tx, run_rx) = mpsc::unbounded_channel();
        let (retry_tx, retry_rx) = mpsc::unbounded_channel();
        Self {
            inner: Arc::new(OrchestratorInner {
                workflow: RwLock::new(workflow),
                config: RwLock::new(config),
                tracker: RwLock::new(tracker),
                workspace: RwLock::new(workspace),
                state: Mutex::new(state),
                shutdown: tokio::sync::Notify::new(),
                run_tx,
                run_rx: Mutex::new(Some(run_rx)),
                retry_tx,
                retry_rx: Mutex::new(Some(retry_rx)),
                event_bus: crate::events::broadcast::OrchestratorEventBus::new(256),
                approval_router: ApprovalRouter::new(),
            }),
        }
    }

    pub fn event_bus(&self) -> crate::events::broadcast::OrchestratorEventBus {
        self.inner.event_bus.clone()
    }

    pub fn approval_router(&self) -> ApprovalRouter {
        self.inner.approval_router.clone()
    }

    pub async fn retry_open_pr(&self, _identifier: &str) -> anyhow::Result<String> {
        Err(anyhow::anyhow!("not_implemented"))
    }

    pub async fn snapshot(&self) -> OrchestratorState {
        self.inner.state.lock().await.clone()
    }

    pub fn shutdown(&self) {
        self.inner.shutdown.notify_waiters();
    }

    pub async fn reload(&self, workflow: WorkflowDefinition, config: EffectiveConfig) -> Result<()> {
        {
            let mut s = self.inner.state.lock().await;
            s.poll_interval_ms = config.polling.interval_ms;
            s.max_concurrent_agents = config.agent.max_concurrent_agents;
        }
        let new_tracker = build_tracker(&config)?;
        let new_workspace =
            WorkspaceManager::new(config.workspace_root.clone(), config.hooks.clone());
        *self.inner.tracker.write().await = new_tracker;
        *self.inner.workspace.write().await = new_workspace;
        *self.inner.workflow.write().await = workflow;
        *self.inner.config.write().await = config;
        Ok(())
    }

    pub async fn startup_cleanup(&self) {
        let cfg = self.inner.config.read().await.clone();
        let tracker = self.inner.tracker.read().await.clone();
        let ws = self.inner.workspace.read().await.clone();
        match tracker.fetch_issues_by_states(&cfg.tracker.terminal_states).await {
            Ok(issues) => {
                for i in issues {
                    if let Err(e) = ws.remove(&i.identifier).await {
                        tracing::warn!(issue_identifier = %i.identifier, "startup_cleanup_remove_failed: {e}");
                    }
                }
            }
            Err(e) => tracing::warn!("startup_terminal_fetch_failed: {e}"),
        }
    }

    pub async fn run(&self) {
        self.startup_cleanup().await;

        // Spawn run-dispatcher
        if let Some(mut rx) = self.inner.run_rx.lock().await.take() {
            let this = self.clone();
            tokio::spawn(async move {
                while let Some(req) = rx.recv().await {
                    let inner = this.clone();
                    tokio::spawn(async move {
                        inner.run_worker(req.issue, req.attempt).await;
                    });
                }
            });
        }

        // Spawn retry-dispatcher
        if let Some(mut rx) = self.inner.retry_rx.lock().await.take() {
            let this = self.clone();
            tokio::spawn(async move {
                while let Some(req) = rx.recv().await {
                    let inner = this.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(req.delay_ms)).await;
                        inner.handle_retry_due(req.issue_id, req.attempt).await;
                    });
                }
            });
        }

        // Immediate first tick
        let first = self.clone();
        tokio::spawn(async move { first.tick().await });

        loop {
            let interval_ms = self.inner.state.lock().await.poll_interval_ms;
            let interval = Duration::from_millis(interval_ms);
            tokio::select! {
                _ = self.inner.shutdown.notified() => {
                    tracing::info!("shutdown_received");
                    break;
                }
                _ = tokio::time::sleep(interval) => {
                    let this = self.clone();
                    tokio::spawn(async move { this.tick().await });
                }
            }
        }
    }

    pub async fn tick(&self) {
        if let Err(e) = self.reconcile_running().await {
            tracing::warn!("reconciliation_failed: {e}");
        }
        let cfg = self.inner.config.read().await.clone();
        if let Err(e) = cfg.validate_for_dispatch() {
            tracing::warn!("dispatch_preflight_failed: {e}");
            self.inner.state.lock().await.last_validation_error = Some(e.to_string());
            return;
        }
        self.inner.state.lock().await.last_validation_error = None;

        let tracker = self.inner.tracker.read().await.clone();
        let candidates = match tracker.fetch_candidate_issues().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("candidate_fetch_failed: {e}");
                return;
            }
        };

        let sorted = sort_candidates(candidates);
        for issue in sorted {
            if !self.is_dispatch_eligible(&issue, &cfg).await {
                continue;
            }
            self.dispatch(issue).await;
        }
    }

    async fn is_dispatch_eligible(&self, issue: &Issue, cfg: &EffectiveConfig) -> bool {
        let state = self.inner.state.lock().await;
        if issue.id.is_empty() || issue.identifier.is_empty() || issue.title.is_empty() || issue.state.is_empty() {
            return false;
        }
        let state_lower = issue.state.to_lowercase();
        let active = cfg.tracker.active_states.iter().any(|s| s.eq_ignore_ascii_case(&issue.state));
        let terminal = cfg.tracker.terminal_states.iter().any(|s| s.eq_ignore_ascii_case(&issue.state));
        if !active || terminal {
            return false;
        }
        if state.running.contains_key(&issue.id) || state.claimed.contains(&issue.id) {
            return false;
        }
        let running_count = state.running.len() as u32;
        if running_count >= cfg.agent.max_concurrent_agents {
            return false;
        }
        if let Some(per_limit) = cfg.agent.max_concurrent_agents_by_state.get(&state_lower) {
            let in_state = state
                .running
                .values()
                .filter(|r| r.issue.state.eq_ignore_ascii_case(&issue.state))
                .count() as u32;
            if in_state >= *per_limit {
                return false;
            }
        }
        if state_lower == "todo" {
            let cfg_terminal_lower: Vec<String> = cfg
                .tracker
                .terminal_states
                .iter()
                .map(|s| s.to_lowercase())
                .collect();
            if issue.blocked_by.iter().any(|b| {
                let s = b.state.as_deref().unwrap_or("").to_lowercase();
                !cfg_terminal_lower.contains(&s)
            }) {
                return false;
            }
        }
        true
    }

    async fn dispatch(&self, issue: Issue) {
        {
            let mut s = self.inner.state.lock().await;
            s.claimed.insert(issue.id.clone());
            s.retry_attempts.remove(&issue.id);
        }
        let _ = self.inner.run_tx.send(RunRequest { issue, attempt: None });
    }

    async fn run_worker(&self, mut issue: Issue, attempt: Option<u32>) {
        let cfg = self.inner.config.read().await.clone();
        let workflow = self.inner.workflow.read().await.clone();
        let ws_mgr = self.inner.workspace.read().await.clone();

        let workspace = match ws_mgr.ensure(&issue.identifier).await {
            Ok(w) => w,
            Err(e) => {
                tracing::error!(issue_identifier = %issue.identifier, "workspace_failed: {e}");
                self.fail_and_schedule_retry(&issue, e.to_string(), attempt.unwrap_or(0));
                return;
            }
        };

        if let Err(e) = ws_mgr.before_run(&workspace).await {
            tracing::error!(issue_identifier = %issue.identifier, "before_run_failed: {e}");
            self.fail_and_schedule_retry(&issue, e.to_string(), attempt.unwrap_or(0));
            return;
        }

        let prompt = match render_prompt(&workflow.prompt_template, &issue, attempt) {
            Ok(p) if !p.trim().is_empty() => p,
            Ok(_) => "You are working on an issue from the issue tracker.".to_string(),
            Err(e) => {
                tracing::error!(issue_identifier = %issue.identifier, "prompt_render_failed: {e}");
                self.fail_and_schedule_retry(&issue, e.to_string(), attempt.unwrap_or(0));
                return;
            }
        };

        {
            let mut s = self.inner.state.lock().await;
            s.running.insert(
                issue.id.clone(),
                RunningEntry {
                    issue: issue.clone(),
                    started_at: Utc::now(),
                    workspace_path: workspace.path.display().to_string(),
                    session: None,
                },
            );
        }

        let harness: Box<dyn Harness + Send + Sync> = select_harness(&cfg.agent.harness);
        let mut turns = 0u32;
        let mut last_outcome_success = true;
        let mut last_error: Option<String> = None;

        while turns < cfg.agent.max_turns {
            turns += 1;
            let (tx, mut rx) = mpsc::channel::<AgentEvent>(64);
            let this = self.clone();
            let issue_id = issue.id.clone();
            let event_handle = tokio::spawn(async move {
                while let Some(ev) = rx.recv().await {
                    this.apply_event(&issue_id, ev).await;
                }
            });

            let turn_prompt = if turns == 1 {
                prompt.clone()
            } else {
                "Continue working on this issue. If complete, conclude.".to_string()
            };

            let outcome = tokio::time::timeout(
                Duration::from_millis(cfg.codex.turn_timeout_ms),
                harness.run(&workspace.path, &turn_prompt, &cfg, tx),
            )
            .await;
            let _ = event_handle.await;

            match outcome {
                Err(_) => {
                    last_outcome_success = false;
                    last_error = Some("turn_timeout".into());
                    break;
                }
                Ok(Err(e)) => {
                    last_outcome_success = false;
                    last_error = Some(e.to_string());
                    break;
                }
                Ok(Ok(o)) => {
                    last_outcome_success = o.success;
                    last_error = o.error.clone();
                    if !o.success {
                        break;
                    }
                    let tracker = self.inner.tracker.read().await.clone();
                    let states = tracker.fetch_issue_states_by_ids(&[issue.id.clone()]).await;
                    let new_state = states.ok().and_then(|m| m.get(&issue.id).cloned());
                    let still_active = new_state.as_deref().map(|s| {
                        cfg.tracker.active_states.iter().any(|a| a.eq_ignore_ascii_case(s))
                    }).unwrap_or(false);
                    if !still_active {
                        break;
                    }
                    if let Some(s) = new_state {
                        issue.state = s;
                    }
                }
            }
        }

        ws_mgr.after_run(&workspace).await;

        let started_at = {
            let mut s = self.inner.state.lock().await;
            s.running.remove(&issue.id).map(|e| e.started_at)
        };
        if let Some(start) = started_at {
            let dur = (Utc::now() - start).num_milliseconds().max(0) as f64 / 1000.0;
            self.inner.state.lock().await.codex_totals.seconds_running_completed += dur;
        }

        if last_outcome_success {
            self.schedule_retry(&issue, 1, CONTINUATION_DELAY_MS, None);
        } else {
            self.fail_and_schedule_retry(&issue, last_error.unwrap_or_else(|| "unknown".into()), attempt.unwrap_or(0));
        }
    }

    async fn apply_event(&self, issue_id: &str, ev: AgentEvent) {
        let mut s = self.inner.state.lock().await;

        let token_delta = {
            let Some(running) = s.running.get_mut(issue_id) else { return };
            let session = running.session.get_or_insert(LiveSession {
                session_id: format!(
                    "{}-{}",
                    ev.thread_id.clone().unwrap_or_default(),
                    ev.turn_id.clone().unwrap_or_default()
                ),
                thread_id: ev.thread_id.clone().unwrap_or_default(),
                turn_id: ev.turn_id.clone().unwrap_or_default(),
                agent_pid: ev.agent_pid.clone(),
                last_event: None,
                last_timestamp: None,
                last_message: None,
                tokens: UsageTokens::default(),
                last_reported_tokens: UsageTokens::default(),
                turn_count: 0,
            });
            session.last_event = Some(format!("{:?}", ev.kind));
            session.last_timestamp = Some(ev.timestamp);
            session.last_message = ev.message.clone();
            if matches!(ev.kind, AgentEventKind::TurnStarted) {
                session.turn_count += 1;
            }
            ev.tokens.as_ref().map(|t| {
                let prev_reported = session.last_reported_tokens.clone();
                if t.total_tokens > session.tokens.total_tokens {
                    session.tokens = t.clone();
                }
                session.last_reported_tokens = t.clone();
                (t.clone(), prev_reported)
            })
        };

        if let Some((latest, prev)) = token_delta {
            if latest.input_tokens > prev.input_tokens {
                s.codex_totals.input_tokens += latest.input_tokens - prev.input_tokens;
            }
            if latest.output_tokens > prev.output_tokens {
                s.codex_totals.output_tokens += latest.output_tokens - prev.output_tokens;
            }
            if latest.total_tokens > prev.total_tokens {
                s.codex_totals.total_tokens += latest.total_tokens - prev.total_tokens;
            }
        }
    }

    fn fail_and_schedule_retry(&self, issue: &Issue, err: String, prev_attempt: u32) {
        let attempt = prev_attempt + 1;
        // Read max backoff from a snapshot via blocking try_lock — we can't await here.
        // Use the unbounded retry channel and let the dispatcher compute the cap when it
        // fires the timer.
        let backoff = (RETRY_BASE_MS.saturating_mul(1u64 << (attempt.saturating_sub(1).min(20))))
            .min(/* cap is enforced again at dispatcher */ u64::MAX);
        self.schedule_retry(issue, attempt, backoff, Some(err));
    }

    fn schedule_retry(&self, issue: &Issue, attempt: u32, delay_ms: u64, err: Option<String>) {
        // Cap delay against the configured max backoff using a try_read snapshot if possible;
        // otherwise the dispatcher will at least cap to a hard ceiling.
        let capped = if let Ok(cfg) = self.inner.config.try_read() {
            delay_ms.min(cfg.agent.max_retry_backoff_ms)
        } else {
            delay_ms.min(600_000)
        };
        let req = RetryRequest {
            issue_id: issue.id.clone(),
            identifier: issue.identifier.clone(),
            attempt,
            delay_ms: capped,
            error: err.clone(),
        };
        // Record retry entry synchronously via blocking try_lock; if contended, fall back.
        if let Ok(mut s) = self.inner.state.try_lock() {
            s.retry_attempts.insert(
                issue.id.clone(),
                RetryEntry {
                    issue_id: issue.id.clone(),
                    identifier: issue.identifier.clone(),
                    attempt,
                    due_at_ms: now_monotonic_ms() + capped as i64,
                    error: err,
                },
            );
        } else {
            // Mutex contended: spawn an async task to record the retry entry without holding up
            // the caller. The retry will still fire because we send the request below.
            let inner = self.inner.clone();
            let issue_id = issue.id.clone();
            let identifier = issue.identifier.clone();
            let err_clone = err.clone();
            tokio::spawn(async move {
                let mut s = inner.state.lock().await;
                s.retry_attempts.insert(
                    issue_id.clone(),
                    RetryEntry {
                        issue_id,
                        identifier,
                        attempt,
                        due_at_ms: now_monotonic_ms() + capped as i64,
                        error: err_clone,
                    },
                );
            });
        }
        let _ = self.inner.retry_tx.send(req);
    }

    async fn handle_retry_due(&self, issue_id: String, attempt: u32) {
        let cfg = self.inner.config.read().await.clone();
        let tracker = self.inner.tracker.read().await.clone();
        let candidates = match tracker.fetch_candidate_issues().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("retry_candidate_fetch_failed: {e}");
                return;
            }
        };
        let found = candidates.into_iter().find(|i| i.id == issue_id);
        match found {
            None => {
                let mut s = self.inner.state.lock().await;
                s.retry_attempts.remove(&issue_id);
                s.claimed.remove(&issue_id);
            }
            Some(issue) => {
                let active = cfg.tracker.active_states.iter().any(|a| a.eq_ignore_ascii_case(&issue.state));
                if !active {
                    let mut s = self.inner.state.lock().await;
                    s.retry_attempts.remove(&issue_id);
                    s.claimed.remove(&issue_id);
                    return;
                }
                let running_count = self.inner.state.lock().await.running.len() as u32;
                if running_count >= cfg.agent.max_concurrent_agents {
                    self.schedule_retry(&issue, attempt, 5_000, Some("no available orchestrator slots".into()));
                    return;
                }
                {
                    let mut s = self.inner.state.lock().await;
                    s.retry_attempts.remove(&issue_id);
                }
                let _ = self.inner.run_tx.send(RunRequest { issue, attempt: Some(attempt) });
            }
        }
    }

    async fn reconcile_running(&self) -> Result<()> {
        let cfg = self.inner.config.read().await.clone();
        let tracker = self.inner.tracker.read().await.clone();
        let ws_mgr = self.inner.workspace.read().await.clone();

        // Stall detection
        let stall_ms = cfg.codex.stall_timeout_ms;
        let mut to_stall: Vec<String> = vec![];
        if stall_ms > 0 {
            let now = Utc::now();
            let s = self.inner.state.lock().await;
            for (id, run) in s.running.iter() {
                let last = run
                    .session
                    .as_ref()
                    .and_then(|s| s.last_timestamp)
                    .unwrap_or(run.started_at);
                let elapsed = (now - last).num_milliseconds();
                if elapsed > stall_ms {
                    to_stall.push(id.clone());
                }
            }
        }
        for id in to_stall {
            tracing::warn!(issue_id = %id, "stall_detected");
            let mut s = self.inner.state.lock().await;
            if let Some(entry) = s.running.remove(&id) {
                s.retry_attempts.insert(
                    id.clone(),
                    RetryEntry {
                        issue_id: id.clone(),
                        identifier: entry.issue.identifier.clone(),
                        attempt: 1,
                        due_at_ms: now_monotonic_ms() + 5_000,
                        error: Some("stall_timeout".into()),
                    },
                );
            }
        }

        let ids: Vec<String> = self.inner.state.lock().await.running.keys().cloned().collect();
        if ids.is_empty() {
            return Ok(());
        }
        let states = match tracker.fetch_issue_states_by_ids(&ids).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("state_refresh_failed: {e}");
                return Ok(());
            }
        };
        let term_lower: Vec<String> = cfg.tracker.terminal_states.iter().map(|s| s.to_lowercase()).collect();
        let act_lower: Vec<String> = cfg.tracker.active_states.iter().map(|s| s.to_lowercase()).collect();
        for id in ids {
            let st_opt = states.get(&id).cloned();
            match st_opt {
                None => continue,
                Some(st) => {
                    let lower = st.to_lowercase();
                    if term_lower.contains(&lower) {
                        let identifier = {
                            let mut s = self.inner.state.lock().await;
                            s.running.remove(&id).map(|r| r.issue.identifier)
                        };
                        if let Some(identifier) = identifier {
                            let _ = ws_mgr.remove(&identifier).await;
                        }
                    } else if act_lower.contains(&lower) {
                        let mut s = self.inner.state.lock().await;
                        if let Some(r) = s.running.get_mut(&id) {
                            r.issue.state = st;
                        }
                    } else {
                        let mut s = self.inner.state.lock().await;
                        s.running.remove(&id);
                    }
                }
            }
        }
        Ok(())
    }
}

pub fn sort_candidates(mut issues: Vec<Issue>) -> Vec<Issue> {
    issues.sort_by(|a, b| {
        let pa = a.priority.unwrap_or(i32::MAX);
        let pb = b.priority.unwrap_or(i32::MAX);
        pa.cmp(&pb)
            .then_with(|| match (a.created_at, b.created_at) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            })
            .then_with(|| a.identifier.cmp(&b.identifier))
    });
    issues
}

fn now_monotonic_ms() -> i64 {
    use std::time::Instant;
    static START: once_cell::sync::Lazy<Instant> = once_cell::sync::Lazy::new(Instant::now);
    START.elapsed().as_millis() as i64
}

pub fn build_tracker(cfg: &EffectiveConfig) -> Result<Arc<dyn Tracker>> {
    match cfg.tracker.kind.as_str() {
        "linear" => {
            let endpoint = cfg
                .tracker
                .endpoint
                .clone()
                .unwrap_or_else(|| "https://api.linear.app/graphql".into());
            let api_key = cfg.tracker.api_key.clone().ok_or(SymphonyError::MissingTrackerApiKey)?;
            let slug = cfg
                .tracker
                .project_slug
                .clone()
                .ok_or(SymphonyError::MissingTrackerProjectSlug)?;
            Ok(Arc::new(crate::tracker::linear::LinearTracker::new(
                endpoint,
                api_key,
                slug,
                cfg.tracker.active_states.clone(),
            )?))
        }
        "file_mock" => {
            let path = cfg
                .tracker
                .endpoint
                .clone()
                .ok_or_else(|| SymphonyError::ConfigInvalid("file_mock requires endpoint=path-to-json".into()))?;
            Ok(Arc::new(crate::tracker::file_mock::FileMockTracker::new(
                path.into(),
                cfg.tracker.active_states.clone(),
            )))
        }
        other => Err(SymphonyError::UnsupportedTrackerKind(other.into())),
    }
}
