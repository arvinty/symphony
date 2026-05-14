use std::process::Command;
use symphony_core::vcs::{commit_pending, open_pr, push_branch};
use tempfile::TempDir;

fn git(workspace: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn git {args:?}: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_workspace_with_remote() -> (TempDir, TempDir) {
    let remote = TempDir::new().unwrap();
    git(remote.path(), &["init", "--bare", "-q"]);
    let work = TempDir::new().unwrap();
    git(work.path(), &["init", "-q", "-b", "main"]);
    git(work.path(), &["config", "user.email", "t@t"]);
    git(work.path(), &["config", "user.name", "t"]);
    std::fs::write(work.path().join("a.txt"), "hi").unwrap();
    git(work.path(), &["add", "-A"]);
    git(work.path(), &["commit", "-qm", "init"]);
    git(work.path(), &["remote", "add", "origin", &remote.path().to_string_lossy()]);
    (work, remote)
}

#[tokio::test]
async fn commit_pending_returns_none_on_clean_tree() {
    let work = TempDir::new().unwrap();
    git(work.path(), &["init", "-q", "-b", "main"]);
    git(work.path(), &["config", "user.email", "t@t"]);
    git(work.path(), &["config", "user.name", "t"]);
    let r = commit_pending(work.path(), "noop").await.unwrap();
    assert!(r.is_none(), "expected None on clean tree, got {r:?}");
}

#[tokio::test]
async fn commit_pending_commits_untracked_files() {
    let work = TempDir::new().unwrap();
    git(work.path(), &["init", "-q", "-b", "main"]);
    git(work.path(), &["config", "user.email", "t@t"]);
    git(work.path(), &["config", "user.name", "t"]);
    std::fs::write(work.path().join("a.txt"), "hi").unwrap();
    let sha = commit_pending(work.path(), "Symphony: DEMO-1").await.unwrap();
    assert!(sha.is_some(), "expected a SHA, got None");
    let log = Command::new("git")
        .args(["log", "--oneline"])
        .current_dir(work.path())
        .output()
        .unwrap();
    let s = String::from_utf8(log.stdout).unwrap();
    assert!(s.contains("Symphony: DEMO-1"), "log did not contain message: {s}");
}

#[tokio::test]
async fn commit_pending_commits_modified_files() {
    let work = TempDir::new().unwrap();
    git(work.path(), &["init", "-q", "-b", "main"]);
    git(work.path(), &["config", "user.email", "t@t"]);
    git(work.path(), &["config", "user.name", "t"]);
    std::fs::write(work.path().join("a.txt"), "v1").unwrap();
    git(work.path(), &["add", "-A"]);
    git(work.path(), &["commit", "-qm", "init"]);
    // Modify and commit_pending.
    std::fs::write(work.path().join("a.txt"), "v2").unwrap();
    let sha = commit_pending(work.path(), "Symphony: DEMO-2").await.unwrap();
    assert!(sha.is_some());
    let log = Command::new("git")
        .args(["log", "--oneline"])
        .current_dir(work.path())
        .output()
        .unwrap();
    let s = String::from_utf8(log.stdout).unwrap();
    assert_eq!(s.lines().count(), 2, "expected 2 commits, got: {s}");
}

#[tokio::test]
async fn push_branch_creates_ref_on_remote() {
    let (work, remote) = init_workspace_with_remote();
    push_branch(work.path(), "origin", "symphony/DEMO-1").await.unwrap();
    let out = Command::new("git")
        .args(["--git-dir", &remote.path().to_string_lossy(), "show-ref"])
        .output().unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("refs/heads/symphony/DEMO-1"), "ref missing in remote: {s}");
}

#[tokio::test]
async fn open_pr_uses_gh_shim_and_returns_url() {
    let (work, _r) = init_workspace_with_remote();
    let shim_dir = TempDir::new().unwrap();
    let shim = shim_dir.path().join(if cfg!(windows) { "gh.cmd" } else { "gh" });
    let body = if cfg!(windows) {
        "@echo %* | find \"--head symphony/DEMO-1\" >nul || exit /b 2\r\n@echo {\"url\":\"https://github.com/o/r/pull/42\"}\r\n"
    } else {
        "#!/usr/bin/env bash\ncase \" $* \" in *\" --head symphony/DEMO-1 \"*) ;; *) exit 2;; esac\necho '{\"url\":\"https://github.com/o/r/pull/42\"}'\n"
    };
    std::fs::write(&shim, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(&shim).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(&shim, p).unwrap();
    }
    let path = format!(
        "{}{}{}",
        shim_dir.path().display(),
        if cfg!(windows) { ";" } else { ":" },
        std::env::var("PATH").unwrap_or_default()
    );
    std::env::set_var("PATH", path);

    let url = open_pr(work.path(), "feat: x", "body", "symphony/DEMO-1").await.unwrap();
    assert_eq!(url, "https://github.com/o/r/pull/42");
}

/// Exercises the three VCS helpers in the exact order the orchestrator's
/// post-success branch invokes them: commit_pending → push_branch → open_pr.
/// Mirrors the real pipeline against a bare-repo remote + a gh shim, and
/// asserts the agent's *uncommitted* work makes it all the way onto the
/// remote branch before the PR URL comes back.
#[cfg(unix)]
#[tokio::test]
async fn pipeline_commit_then_push_then_open_pr() {
    let (work, remote) = init_workspace_with_remote();

    // Simulate an agent that wrote a file but skipped the commit step.
    std::fs::write(work.path().join("agent_output.txt"), "work product").unwrap();

    // Step 1: commit_pending picks up the agent's stray file.
    let sha = commit_pending(work.path(), "Symphony: Demo work (DEMO-1)")
        .await
        .unwrap();
    assert!(sha.is_some(), "commit_pending should have produced a commit");

    // Step 2: push the per-issue branch to the remote.
    push_branch(work.path(), "origin", "symphony/DEMO-1")
        .await
        .unwrap();
    let show_ref = Command::new("git")
        .args(["--git-dir", &remote.path().to_string_lossy(), "show-ref"])
        .output()
        .unwrap();
    let refs = String::from_utf8(show_ref.stdout).unwrap();
    assert!(
        refs.contains("refs/heads/symphony/DEMO-1"),
        "remote missing branch ref: {refs}"
    );

    // The commit content must be on the remote — not just an empty branch.
    let remote_log = Command::new("git")
        .args([
            "--git-dir",
            &remote.path().to_string_lossy(),
            "log",
            "--oneline",
            "symphony/DEMO-1",
        ])
        .output()
        .unwrap();
    let remote_log = String::from_utf8(remote_log.stdout).unwrap();
    assert!(
        remote_log.contains("Symphony: Demo work (DEMO-1)"),
        "remote branch missing the auto-commit: {remote_log}"
    );

    // Step 3: open_pr via a gh shim that verifies the head branch.
    let shim_dir = TempDir::new().unwrap();
    let shim = shim_dir.path().join("gh");
    std::fs::write(
        &shim,
        "#!/usr/bin/env bash\ncase \" $* \" in *\" --head symphony/DEMO-1 \"*) ;; *) exit 2;; esac\necho '{\"url\":\"https://github.com/o/r/pull/99\"}'\n",
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(&shim).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(&shim, p).unwrap();
    }
    std::env::set_var(
        "PATH",
        format!(
            "{}:{}",
            shim_dir.path().display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );

    let url = open_pr(
        work.path(),
        "DEMO-1: Demo work",
        "Authored by Symphony.",
        "symphony/DEMO-1",
    )
    .await
    .unwrap();
    assert_eq!(url, "https://github.com/o/r/pull/99");
}
