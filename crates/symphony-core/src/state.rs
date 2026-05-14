use crate::model::{RetryEntry, RunningEntry, UsageTokens};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

/// Upper bound on remembered completed-issue ids. The set only exists to stop
/// a finished issue from being re-claimed on every poll within one orchestrator
/// run; the tracker remains the source of truth across restarts. Capping it
/// keeps a long-lived orchestrator from leaking one id-string per issue forever.
const COMPLETED_CAP: usize = 8192;

/// Insertion-ordered, size-capped set of completed issue ids. O(1) `contains`
/// via the `HashSet`; `order` drives FIFO eviction once `COMPLETED_CAP` is hit.
#[derive(Debug, Clone, Serialize, Default)]
pub struct CompletedIssues {
    set: HashSet<String>,
    order: VecDeque<String>,
}

impl CompletedIssues {
    pub fn insert(&mut self, id: String) {
        if self.set.insert(id.clone()) {
            self.order.push_back(id);
            while self.order.len() > COMPLETED_CAP {
                if let Some(evicted) = self.order.pop_front() {
                    self.set.remove(&evicted);
                }
            }
        }
    }

    pub fn contains(&self, id: &str) -> bool {
        self.set.contains(id)
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct CodexTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    /// Cumulative seconds of completed sessions (active sessions added at snapshot time).
    pub seconds_running_completed: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct OrchestratorState {
    pub poll_interval_ms: u64,
    pub max_concurrent_agents: u32,
    pub running: HashMap<String, RunningEntry>,
    pub claimed: HashSet<String>,
    pub retry_attempts: HashMap<String, RetryEntry>,
    pub completed: CompletedIssues,
    pub codex_totals: CodexTotals,
    pub codex_rate_limits: Option<serde_json::Value>,
    pub last_validation_error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
}

impl OrchestratorState {
    pub fn add_tokens(&mut self, latest: &UsageTokens, last_reported: &mut UsageTokens) {
        // Treat passed-in `latest` as absolute thread totals; add deltas vs last_reported.
        if latest.input_tokens > last_reported.input_tokens {
            self.codex_totals.input_tokens += latest.input_tokens - last_reported.input_tokens;
        }
        if latest.output_tokens > last_reported.output_tokens {
            self.codex_totals.output_tokens += latest.output_tokens - last_reported.output_tokens;
        }
        if latest.total_tokens > last_reported.total_tokens {
            self.codex_totals.total_tokens += latest.total_tokens - last_reported.total_tokens;
        }
        *last_reported = latest.clone();
    }
}
