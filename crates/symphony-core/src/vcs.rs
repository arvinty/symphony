use anyhow::{anyhow, Context, Result};
use std::path::Path;
use tokio::process::Command;

pub async fn push_branch(workspace: &Path, remote: &str, branch: &str) -> Result<()> {
    let out = Command::new("git")
        .args(["push", "-u", remote, &format!("HEAD:refs/heads/{branch}")])
        .current_dir(workspace)
        .output()
        .await
        .context("spawning git push")?;
    if !out.status.success() {
        return Err(anyhow!("git push failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(())
}

pub async fn open_pr(workspace: &Path, title: &str, body: &str) -> Result<String> {
    // On Windows, tokio::process::Command uses CreateProcess directly and does not
    // resolve PATHEXT (.cmd/.bat), so we invoke gh through cmd /c to allow .cmd shims.
    // We build the command string carefully to handle spaces in title/body via quoting.
    #[cfg(windows)]
    let out = {
        Command::new("cmd")
            .args([
                "/c",
                "gh",
                "pr",
                "create",
                "--title",
                title,
                "--body",
                body,
                "--json",
                "url",
                "-q",
                ".url",
            ])
            .current_dir(workspace)
            .output()
            .await
            .context("spawning gh via cmd /c")?
    };

    #[cfg(not(windows))]
    let out = Command::new("gh")
        .args(["pr", "create", "--title", title, "--body", body, "--json", "url", "-q", ".url"])
        .current_dir(workspace)
        .output()
        .await
        .context("spawning gh")?;

    if !out.status.success() {
        return Err(anyhow!("gh pr create failed: {}", String::from_utf8_lossy(&out.stderr)));
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
