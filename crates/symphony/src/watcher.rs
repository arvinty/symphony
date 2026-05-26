use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::time::Duration;
use symphony_core::config::EffectiveConfig;
use symphony_core::orchestrator::Orchestrator;
use symphony_core::workflow::load_workflow;

pub async fn watch_workflow(orch: Orchestrator, workflow_path: PathBuf) -> Result<()> {
    let (tx, rx) = channel::<notify::Result<Event>>();
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })?;
    let parent = workflow_parent(&workflow_path);
    watcher.watch(&parent, RecursiveMode::NonRecursive)?;

    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(event)) => {
                if !matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                    continue;
                }
                if !event
                    .paths
                    .iter()
                    .any(|p| p.ends_with(workflow_path.file_name().unwrap_or_default()))
                {
                    continue;
                }
                match load_workflow(&workflow_path) {
                    Ok(wf) => match EffectiveConfig::from_workflow(&wf) {
                        Ok(cfg) => {
                            if let Err(e) = orch.reload(wf, cfg).await {
                                tracing::warn!("reload_failed: {e}");
                            } else {
                                tracing::info!("workflow_reloaded");
                            }
                        }
                        Err(e) => tracing::warn!("reload_config_invalid: {e}"),
                    },
                    Err(e) => tracing::warn!("reload_workflow_load_failed: {e}"),
                }
            }
            Ok(Err(e)) => tracing::debug!("watch_error: {e}"),
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

fn workflow_parent(workflow_path: &std::path::Path) -> PathBuf {
    workflow_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::workflow_parent;
    use std::path::Path;

    #[test]
    fn relative_workflow_in_cwd_watches_dot() {
        assert_eq!(workflow_parent(Path::new("WORKFLOW.md")), Path::new("."));
    }

    #[test]
    fn nested_workflow_watches_parent_dir() {
        assert_eq!(
            workflow_parent(Path::new("docs/WORKFLOW.md")),
            Path::new("docs")
        );
    }
}
