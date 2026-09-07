//! Admin API: git operations on the content repo (status / diff / pull / commit+push).
//! All guarded by the same admin session as content CRUD — no separate permission model.

use leptos::prelude::*;

#[cfg(feature = "ssr")]
use super::require_admin;
#[cfg(feature = "ssr")]
use crate::state::AppState;
use crate::types::GitStatus;

/// Runs git with fixed args (no shell); on failure returns stderr/stdout as the error message
#[cfg(feature = "ssr")]
fn run_git(dir: &std::path::Path, args: &[&str]) -> Result<String, ServerFnError> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| ServerFnError::new(format!("无法运行 git: {e}")))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if out.status.success() {
        Ok(if stdout.is_empty() { stderr } else { stdout })
    } else if stderr.is_empty() {
        Err(ServerFnError::new(stdout))
    } else {
        Err(ServerFnError::new(stderr))
    }
}

#[cfg(feature = "ssr")]
fn git_repo(state: &AppState) -> Result<std::path::PathBuf, ServerFnError> {
    let dir = std::path::PathBuf::from(&state.config.content_dir);
    if dir.join(".git").exists() {
        Ok(dir)
    } else {
        Err(ServerFnError::new("内容目录不是 git 仓库"))
    }
}

#[server]
pub async fn admin_git_status() -> Result<GitStatus, ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    let dir = git_repo(&state)?;
    // Refresh remote refs so ahead/behind is accurate; tolerate offline (stale refs still shown)
    let _ = run_git(&dir, &["fetch", "--quiet"]);
    // First line of `git status -sb`: "## main...origin/main [ahead 1, behind 2]" (or "## No commits yet on main")
    let sb = run_git(&dir, &["status", "-sb"])?;
    let header = sb.lines().next().unwrap_or("").trim_start_matches("## ");
    let (branch, tracking) = match header.split_once(" [") {
        Some((b, t)) => (b.to_string(), t.trim_end_matches(']').to_string()),
        None => (header.to_string(), String::new()),
    };
    let mut ahead = 0;
    let mut behind = 0;
    for part in tracking.split(',') {
        let part = part.trim();
        if let Some(n) = part.strip_prefix("ahead ") {
            ahead = n.parse().unwrap_or(0);
        }
        if let Some(n) = part.strip_prefix("behind ") {
            behind = n.parse().unwrap_or(0);
        }
    }
    let last_commit = if branch.starts_with("No commits yet") {
        "（还没有提交）".to_string()
    } else {
        run_git(&dir, &["log", "-1", "--pretty=%h %s (%cr)"]).unwrap_or_default()
    };
    Ok(GitStatus {
        branch,
        ahead,
        behind,
        dirty: sb.lines().skip(1).map(str::to_string).collect(),
        last_commit,
        pull_interval_secs: state.config.content_pull_interval_secs,
    })
}

#[server]
pub async fn admin_git_diff() -> Result<String, ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    let dir = git_repo(&state)?;
    // Untracked files are listed in the status panel; the diff covers tracked changes only
    let mut diff = run_git(&dir, &["diff", "HEAD", "--stat", "--patch"])?;
    const MAX: usize = 64 * 1024;
    if diff.len() > MAX {
        diff.truncate(MAX);
        diff.push_str("\n…（diff 过长，已截断）");
    }
    if diff.is_empty() {
        diff = "（已跟踪文件没有改动）".into();
    }
    Ok(diff)
}

#[server]
pub async fn admin_git_pull() -> Result<String, ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    let dir = git_repo(&state)?;
    let out = run_git(&dir, &["pull", "--ff-only"])?;
    // Apply immediately rather than waiting for the file watcher's debounce
    match crate::content::scan(&dir) {
        Ok(new_index) => *state.index.write() = new_index,
        Err(e) => return Err(ServerFnError::new(format!("拉取成功但重载内容失败: {e}"))),
    }
    Ok(if out.is_empty() { "已是最新".into() } else { out })
}

#[server]
pub async fn admin_git_commit_push(message: String) -> Result<String, ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    let dir = git_repo(&state)?;
    let message = message.trim().to_string();
    if message.len() > 200 {
        return Err(ServerFnError::new("提交信息过长"));
    }
    let message = if message.is_empty() {
        format!("content update {}", chrono::Local::now().format("%Y-%m-%d %H:%M"))
    } else {
        message
    };
    run_git(&dir, &["add", "-A"])?;
    let mut log = if run_git(&dir, &["diff", "--cached", "--name-only"])?.is_empty() {
        "没有需要提交的改动\n".to_string()
    } else {
        // Server clones typically have no git identity configured; pass one explicitly
        run_git(
            &dir,
            &[
                "-c",
                "user.name=Leafpress Admin",
                "-c",
                "user.email=leafpress@localhost",
                "commit",
                "-m",
                &message,
            ],
        )? + "\n"
    };
    log.push_str(&run_git(&dir, &["push"])?);
    Ok(log)
}
