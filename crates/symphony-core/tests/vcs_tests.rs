use std::process::Command;
use symphony_core::vcs::{open_pr, push_branch};
use tempfile::TempDir;

fn init_workspace_with_remote() -> (TempDir, TempDir) {
    let remote = TempDir::new().unwrap();
    Command::new("git").args(["init", "--bare", "-q"]).current_dir(remote.path()).status().unwrap();
    let work = TempDir::new().unwrap();
    Command::new("git").args(["init", "-q", "-b", "main"]).current_dir(work.path()).status().unwrap();
    Command::new("git").args(["config", "user.email", "t@t"]).current_dir(work.path()).status().unwrap();
    Command::new("git").args(["config", "user.name", "t"]).current_dir(work.path()).status().unwrap();
    std::fs::write(work.path().join("a.txt"), "hi").unwrap();
    Command::new("git").args(["add", "-A"]).current_dir(work.path()).status().unwrap();
    Command::new("git").args(["commit", "-qm", "init"]).current_dir(work.path()).status().unwrap();
    Command::new("git").args(["remote", "add", "origin", &remote.path().to_string_lossy()]).current_dir(work.path()).status().unwrap();
    (work, remote)
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
        "@echo {\"url\":\"https://github.com/o/r/pull/42\"}\r\n"
    } else {
        "#!/usr/bin/env bash\necho '{\"url\":\"https://github.com/o/r/pull/42\"}'\n"
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

    let url = open_pr(work.path(), "feat: x", "body").await.unwrap();
    assert_eq!(url, "https://github.com/o/r/pull/42");
}
