use crate::error::{Result, SymphonyError};
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Copy, Clone, Debug)]
pub enum HookKind {
    AfterCreate,
    BeforeRun,
    AfterRun,
    BeforeRemove,
}

impl HookKind {
    pub fn name(self) -> &'static str {
        match self {
            HookKind::AfterCreate => "after_create",
            HookKind::BeforeRun => "before_run",
            HookKind::AfterRun => "after_run",
            HookKind::BeforeRemove => "before_remove",
        }
    }
    pub fn fatal_on_failure(self) -> bool {
        matches!(self, HookKind::AfterCreate | HookKind::BeforeRun)
    }
}

pub async fn run_hook(
    kind: HookKind,
    script: Option<&str>,
    cwd: &Path,
    timeout_ms: u64,
) -> Result<()> {
    let Some(script) = script else { return Ok(()) };
    let script = script.trim();
    if script.is_empty() {
        return Ok(());
    }
    tracing::info!(hook = kind.name(), cwd = %cwd.display(), "hook_start");

    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("powershell.exe");
        c.arg("-NoProfile").arg("-Command").arg(script);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("bash");
        c.arg("-lc").arg(script);
        c
    };
    cmd.current_dir(cwd);

    let fut = cmd.status();
    let res = timeout(Duration::from_millis(timeout_ms), fut).await;
    match res {
        Err(_) => {
            tracing::warn!(hook = kind.name(), "hook_timeout");
            Err(SymphonyError::HookTimeout {
                hook: kind.name().into(),
            })
        }
        Ok(Err(e)) => Err(SymphonyError::HookFailed {
            hook: kind.name().into(),
            reason: e.to_string(),
        }),
        Ok(Ok(status)) if !status.success() => Err(SymphonyError::HookFailed {
            hook: kind.name().into(),
            reason: format!("exit_status={:?}", status.code()),
        }),
        Ok(Ok(_)) => Ok(()),
    }
}
