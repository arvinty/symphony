use crate::config::HooksConfig;
use crate::error::{Result, SymphonyError};
use crate::hooks::{run_hook, HookKind};
use crate::model::sanitize_workspace_key;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Workspace {
    pub path: PathBuf,
    pub workspace_key: String,
    pub created_now: bool,
}

#[derive(Clone)]
pub struct WorkspaceManager {
    root: PathBuf,
    hooks: HooksConfig,
}

impl WorkspaceManager {
    pub fn new(root: PathBuf, hooks: HooksConfig) -> Self {
        Self { root, hooks }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn workspace_path_for(&self, identifier: &str) -> PathBuf {
        self.root.join(sanitize_workspace_key(identifier))
    }

    pub async fn ensure(&self, identifier: &str) -> Result<Workspace> {
        let key = sanitize_workspace_key(identifier);
        let path = self.root.join(&key);
        verify_inside_root(&self.root, &path)?;

        let created_now = if !path.exists() {
            tokio::fs::create_dir_all(&path)
                .await
                .map_err(SymphonyError::Io)?;
            true
        } else {
            false
        };

        if created_now {
            if let Err(e) = run_hook(
                HookKind::AfterCreate,
                self.hooks.after_create.as_deref(),
                &path,
                self.hooks.timeout_ms,
            )
            .await
            {
                // After-create failure is fatal — rollback partially created dir.
                let _ = tokio::fs::remove_dir_all(&path).await;
                return Err(e);
            }
        }

        Ok(Workspace {
            path,
            workspace_key: key,
            created_now,
        })
    }

    pub async fn before_run(&self, ws: &Workspace) -> Result<()> {
        run_hook(
            HookKind::BeforeRun,
            self.hooks.before_run.as_deref(),
            &ws.path,
            self.hooks.timeout_ms,
        )
        .await
    }

    pub async fn after_run(&self, ws: &Workspace) {
        let _ = run_hook(
            HookKind::AfterRun,
            self.hooks.after_run.as_deref(),
            &ws.path,
            self.hooks.timeout_ms,
        )
        .await;
    }

    pub async fn remove(&self, identifier: &str) -> Result<()> {
        let path = self.workspace_path_for(identifier);
        verify_inside_root(&self.root, &path)?;
        if path.exists() {
            let _ = run_hook(
                HookKind::BeforeRemove,
                self.hooks.before_remove.as_deref(),
                &path,
                self.hooks.timeout_ms,
            )
            .await;
            tokio::fs::remove_dir_all(&path)
                .await
                .map_err(SymphonyError::Io)?;
        }
        Ok(())
    }
}

pub fn verify_inside_root(root: &Path, candidate: &Path) -> Result<()> {
    let root_abs = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(root)
    };
    let cand_abs = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(candidate)
    };
    if !cand_abs.starts_with(&root_abs) {
        return Err(SymphonyError::WorkspaceOutsideRoot);
    }
    Ok(())
}
