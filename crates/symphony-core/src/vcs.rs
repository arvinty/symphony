use anyhow::{anyhow, Context, Result};
use std::path::Path;
use tokio::process::Command;

/// Commit any uncommitted changes in the workspace as a single commit.
/// Returns `Ok(Some(sha))` with the new commit's short SHA on success,
/// `Ok(None)` if the working tree was already clean, or `Err` on failure.
///
/// Useful as a defensive "make sure the agent's work is committed" step
/// before pushing — agents sometimes modify files but skip the commit step
/// (the prompt asks, but doesn't always get followed).
pub async fn commit_pending(workspace: &Path, message: &str) -> Result<Option<String>> {
    // Check if there's anything to commit.
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(workspace)
        .output()
        .await
        .context("spawning git status")?;
    if !status.status.success() {
        return Err(anyhow!(
            "git status failed: {}",
            String::from_utf8_lossy(&status.stderr)
        ));
    }
    if status.stdout.iter().all(|b| b.is_ascii_whitespace()) {
        return Ok(None);
    }

    // Stage everything.
    let add = Command::new("git")
        .args(["add", "-A"])
        .current_dir(workspace)
        .output()
        .await
        .context("spawning git add")?;
    if !add.status.success() {
        return Err(anyhow!(
            "git add failed: {}",
            String::from_utf8_lossy(&add.stderr)
        ));
    }

    // Commit. Use --allow-empty-message=false implicitly; pass message via -m.
    let commit = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(workspace)
        .output()
        .await
        .context("spawning git commit")?;
    if !commit.status.success() {
        return Err(anyhow!(
            "git commit failed: {}",
            String::from_utf8_lossy(&commit.stderr)
        ));
    }

    // Capture the short SHA.
    let rev = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(workspace)
        .output()
        .await
        .context("spawning git rev-parse")?;
    if !rev.status.success() {
        return Err(anyhow!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&rev.stderr)
        ));
    }
    Ok(Some(
        String::from_utf8_lossy(&rev.stdout).trim().to_string(),
    ))
}

pub async fn push_branch(workspace: &Path, remote: &str, branch: &str) -> Result<()> {
    let out = Command::new("git")
        .args(["push", "-u", remote, &format!("HEAD:refs/heads/{branch}")])
        .current_dir(workspace)
        .output()
        .await
        .context("spawning git push")?;
    if !out.status.success() {
        return Err(anyhow!(
            "git push failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

pub async fn open_pr(workspace: &Path, title: &str, body: &str, head: &str) -> Result<String> {
    // On Windows, tokio::process::Command uses CreateProcess directly and does not
    // resolve PATHEXT (.cmd/.bat), so we invoke gh through cmd /c to allow .cmd shims.
    // We build the command string carefully to handle spaces in title/body via quoting.
    #[cfg(windows)]
    let out = {
        Command::new("cmd")
            .args([
                "/c", "gh", "pr", "create", "--title", title, "--body", body, "--head", head,
                "--json", "url", "-q", ".url",
            ])
            .current_dir(workspace)
            .output()
            .await
            .context("spawning gh via cmd /c")?
    };

    #[cfg(not(windows))]
    let out = Command::new("gh")
        .args([
            "pr", "create", "--title", title, "--body", body, "--head", head, "--json", "url",
            "-q", ".url",
        ])
        .current_dir(workspace)
        .output()
        .await
        .context("spawning gh")?;

    if !out.status.success() {
        return Err(anyhow!(
            "gh pr create failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
        if let Some(u) = v["url"].as_str() {
            return Ok(u.to_string());
        }
    }
    if s.starts_with("http") {
        return Ok(s);
    }
    Err(anyhow!("could not parse gh output: {s}"))
}
