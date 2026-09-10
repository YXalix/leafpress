//! Admin API: git operations on the content repo (status / diff / pull / commit+push / file edit).
//! All guarded by the same admin session — no separate permission model.

use leptos::prelude::*;

#[cfg(feature = "ssr")]
use super::require_admin;
#[cfg(feature = "ssr")]
use crate::state::AppState;
#[cfg(feature = "ssr")]
use crate::types::{DiffCell, DiffLineKind, DiffRow};
use crate::types::{FileDiff, GitStatus};

/// Runs git with fixed args (no shell); on failure returns stderr/stdout as the error message
#[cfg(feature = "ssr")]
fn run_git(dir: &std::path::Path, args: &[&str]) -> Result<String, ServerFnError> {
    let out = crate::util::git_command(dir, args)
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

/// Network-looking stderr fragments that usually mean "proxy not on / not configured"
#[cfg(feature = "ssr")]
const NET_ERR_PATTERNS: &[&str] = &[
    "could not resolve host",
    "failed to connect",
    "timed out",
    "connection refused",
    "connection reset",
    "unable to connect",
    "couldn't connect",
    "network is unreachable",
    "temporary failure in name resolution",
    "the proxy tunnel",
    "received http code 407",
];

/// Appends a Chinese proxy hint when the error looks like a network failure
#[cfg(feature = "ssr")]
fn with_net_hint(err: ServerFnError, proxy_configured: bool) -> ServerFnError {
    let msg = err.to_string();
    let lower = msg.to_lowercase();
    if !NET_ERR_PATTERNS.iter().any(|p| lower.contains(p)) {
        return err;
    }
    let hint = if proxy_configured {
        "网络连接失败：已配置 git_proxy 但仍连不上，请检查代理是否已开启、地址端口是否正确。"
    } else {
        "网络连接失败：可能是代理未开启或未配置。若远程仓库需要代理，请在 config.toml 设置 git_proxy（如 http://127.0.0.1:7890）后重启。"
    };
    ServerFnError::new(format!("{msg}\n{hint}"))
}

/// Like [run_git], but with explicit proxy control for network ops (fetch/pull/push):
/// `Some(proxy)` routes through config.git_proxy; `None` forces a direct connection
/// (`-c http.proxy=` empty), so a proxy inherited from the server process's environment
/// (http_proxy etc.) is never used unless configured. Network failures get a hint.
#[cfg(feature = "ssr")]
fn run_git_net(
    dir: &std::path::Path,
    args: &[&str],
    proxy: Option<&str>,
) -> Result<String, ServerFnError> {
    let mut owned: Vec<String> = vec!["-c".into(), format!("http.proxy={}", proxy.unwrap_or(""))];
    owned.extend(args.iter().map(|s| s.to_string()));
    let full: Vec<&str> = owned.iter().map(String::as_str).collect();
    run_git(dir, &full).map_err(|e| with_net_hint(e, proxy.is_some()))
}

/// Records a git network op's outcome into the shared slot surfaced in /admin: failures are
/// stored with a timestamp, success clears the previous error. Returns the result unchanged.
#[cfg(feature = "ssr")]
fn track_sync<T>(
    slot: &std::sync::RwLock<Option<String>>,
    result: Result<T, ServerFnError>,
) -> Result<T, ServerFnError> {
    *slot.write().unwrap() = match &result {
        Ok(_) => None,
        Err(e) => Some(format!(
            "{} {e}",
            chrono::Local::now().format("%Y-%m-%d %H:%M")
        )),
    };
    result
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

/// Rejects repo-relative paths that are absolute or escape the repo via `..`
#[cfg(feature = "ssr")]
fn validate_rel_path(rel: &str) -> Result<(), ServerFnError> {
    if rel.is_empty()
        || rel.starts_with('/')
        || rel.starts_with('\\')
        || rel.split(['/', '\\']).any(|part| part == "..")
    {
        return Err(ServerFnError::new("路径不合法"));
    }
    Ok(())
}

/// Resolves a repo-relative path to an existing file inside the repo (rejects escapes)
#[cfg(feature = "ssr")]
fn resolve_in_repo(dir: &std::path::Path, rel: &str) -> Result<std::path::PathBuf, ServerFnError> {
    validate_rel_path(rel)?;
    let base = dir
        .canonicalize()
        .map_err(|e| ServerFnError::new(format!("内容目录不可读: {e}")))?;
    let path = base
        .join(rel)
        .canonicalize()
        .map_err(|_| ServerFnError::new("文件不存在"))?;
    if !path.starts_with(&base) || !path.is_file() {
        return Err(ServerFnError::new("路径不合法"));
    }
    Ok(path)
}

/// Paths with staged changes (porcelain X column not blank/?); -z keeps paths raw (no quoting)
#[cfg(feature = "ssr")]
fn staged_paths(dir: &std::path::Path) -> Result<std::collections::HashSet<String>, ServerFnError> {
    let out = run_git(dir, &["status", "--porcelain=v1", "-z"])?;
    let mut set = std::collections::HashSet::new();
    let mut fields = out.split('\0');
    while let Some(entry) = fields.next() {
        if entry.len() < 4 {
            continue;
        }
        // "XY path": X = staged status, Y = worktree status
        let (xy, path) = entry.split_at(3);
        let x = xy.chars().next().unwrap_or(' ');
        if x != ' ' && x != '?' {
            set.insert(path.to_string());
        }
        // rename/copy entries carry a second NUL-separated field (the source path)
        if xy.starts_with('R') || xy.starts_with('C') {
            fields.next();
        }
    }
    Ok(set)
}

/// Pairs a run of deleted lines with the following added lines onto shared rows
#[cfg(feature = "ssr")]
fn flush_pairs(file: &mut FileDiff, dels: &mut Vec<DiffCell>, adds: &mut Vec<DiffCell>) -> usize {
    let n = dels.len().max(adds.len());
    let mut dels = dels.drain(..);
    let mut adds = adds.drain(..);
    for _ in 0..n {
        file.rows.push(DiffRow::Line {
            left: dels.next(),
            right: adds.next(),
        });
    }
    n
}

/// Parses `git -c core.quotePath=false diff HEAD --no-color` into per-file dual-pane rows.
/// Del/add runs are paired index-wise onto the same row (standard side-by-side rendering).
#[cfg(feature = "ssr")]
fn parse_diff(text: &str) -> Vec<FileDiff> {
    const MAX_ROWS_PER_FILE: usize = 400;
    const MAX_ROWS_TOTAL: usize = 2000;
    let mut files: Vec<FileDiff> = Vec::new();
    let mut cur: Option<FileDiff> = None;
    let mut in_hunk = false;
    let mut old_no = 0u32;
    let mut new_no = 0u32;
    let mut dels: Vec<DiffCell> = Vec::new();
    let mut adds: Vec<DiffCell> = Vec::new();
    // Running count of rows in `files` + `cur`, so the total cap check below is O(1)
    let mut total_rows = 0usize;
    let mut stop = false;

    for line in text.lines() {
        if stop {
            break;
        }
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some(mut f) = cur.take() {
                total_rows += flush_pairs(&mut f, &mut dels, &mut adds);
                files.push(f);
            }
            // "a/path b/path": take the b/ side as a fallback (binary files have no ---/+++
            // header); text files get their real path from the ---/+++ lines below
            let path = rest
                .rsplit(" b/")
                .next()
                .unwrap_or(rest)
                .trim_matches('"')
                .to_string();
            cur = Some(FileDiff {
                path,
                status: "modified".into(),
                staged: false,
                rows: Vec::new(),
                truncated: false,
            });
            in_hunk = false;
            continue;
        }
        let Some(f) = cur.as_mut() else { continue };
        if line.starts_with("new file mode") {
            f.status = "added".into();
        } else if line.starts_with("deleted file mode") {
            f.status = "deleted".into();
        } else if let Some(hunk) = line.strip_prefix("@@") {
            total_rows += flush_pairs(f, &mut dels, &mut adds);
            // "@@ -old[,n] +new[,n] @@ ...": recover the two start line numbers
            let mut nums = hunk.split_whitespace().take(2).filter_map(|tok| {
                tok.trim_start_matches(['-', '+'])
                    .split(',')
                    .next()
                    .and_then(|n| n.parse::<u32>().ok())
            });
            old_no = nums.next().unwrap_or(0);
            new_no = nums.next().unwrap_or(0);
            f.rows.push(DiffRow::Hunk(line.to_string()));
            total_rows += 1;
            in_hunk = true;
        } else if !in_hunk {
            // File header lines. The path comes from ---/+++ (authoritative even for renames
            // and paths with spaces/CJK); a deleted "--- …" line only appears inside a hunk,
            // which the in_hunk guard above excludes. index/mode lines carry no content.
            if let Some(p) = line.strip_prefix("rename to ") {
                f.path = p.trim().to_string();
            } else if let Some(p) = line.strip_prefix("--- ") {
                let p = p.trim();
                if p != "/dev/null" {
                    f.path = p.strip_prefix("a/").unwrap_or(p).to_string();
                }
            } else if let Some(p) = line.strip_prefix("+++ ") {
                let p = p.trim();
                if p != "/dev/null" {
                    f.path = p.strip_prefix("b/").unwrap_or(p).to_string();
                }
            }
        } else if f.rows.len() >= MAX_ROWS_PER_FILE {
            f.truncated = true;
            continue;
        } else if let Some(rest) = line.strip_prefix('-') {
            dels.push(DiffCell {
                no: old_no,
                text: rest.to_string(),
                kind: DiffLineKind::Del,
            });
            old_no += 1;
        } else if let Some(rest) = line.strip_prefix('+') {
            adds.push(DiffCell {
                no: new_no,
                text: rest.to_string(),
                kind: DiffLineKind::Add,
            });
            new_no += 1;
        } else if line.starts_with('\\') {
            // "\ No newline at end of file" — no row of its own
            continue;
        } else {
            total_rows += flush_pairs(f, &mut dels, &mut adds);
            let text = line.strip_prefix(' ').unwrap_or(line).to_string();
            f.rows.push(DiffRow::Line {
                left: Some(DiffCell {
                    no: old_no,
                    text: text.clone(),
                    kind: DiffLineKind::Context,
                }),
                right: Some(DiffCell {
                    no: new_no,
                    text,
                    kind: DiffLineKind::Context,
                }),
            });
            total_rows += 1;
            old_no += 1;
            new_no += 1;
        }
        if total_rows >= MAX_ROWS_TOTAL {
            if let Some(f) = cur.as_mut() {
                f.truncated = true;
            }
            stop = true;
        }
    }
    if let Some(mut f) = cur.take() {
        flush_pairs(&mut f, &mut dels, &mut adds);
        files.push(f);
    }
    files
}

/// Untracked working-tree files ("?? " entries) as fully-added diffs
#[cfg(feature = "ssr")]
fn untracked_files(dir: &std::path::Path) -> Result<Vec<String>, ServerFnError> {
    let out = run_git(dir, &["ls-files", "-z", "--others", "--exclude-standard"])?;
    Ok(out
        .split('\0')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

/// Builds a FileDiff for an untracked file: every line shown as added on the right pane
#[cfg(feature = "ssr")]
fn untracked_diff(dir: &std::path::Path, path: &str) -> FileDiff {
    const MAX_BYTES: usize = 256 * 1024;
    let full = dir.join(path);
    let mut truncated = false;
    let rows = match std::fs::read(&full) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(if bytes.len() > MAX_BYTES {
                truncated = true;
                &bytes[..MAX_BYTES]
            } else {
                &bytes[..]
            });
            if text.contains('\0') {
                vec![DiffRow::Hunk("（二进制文件，不显示内容）".into())]
            } else {
                text.lines()
                    .enumerate()
                    .map(|(i, l)| DiffRow::Line {
                        left: None,
                        right: Some(DiffCell {
                            no: i as u32 + 1,
                            text: l.to_string(),
                            kind: DiffLineKind::Add,
                        }),
                    })
                    .collect()
            }
        }
        Err(_) => vec![DiffRow::Hunk("（无法读取文件内容）".into())],
    };
    FileDiff {
        path: path.to_string(),
        status: "added".into(),
        staged: false,
        rows,
        truncated,
    }
}

#[server]
pub async fn admin_git_status() -> Result<GitStatus, ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    let dir = std::path::PathBuf::from(&state.config.content_dir);
    // Not a repo is a normal state (content works without git), not an error: report it via
    // `repo: false` so the panel can show a setup guide instead of a login-looking error
    if !dir.join(".git").exists() {
        return Ok(GitStatus {
            repo: false,
            content_dir: state.config.content_dir.clone(),
            branch: String::new(),
            ahead: 0,
            behind: 0,
            dirty: Vec::new(),
            staged: 0,
            last_commit: String::new(),
            pull_interval_secs: state.config.content_pull_interval_secs,
            proxy: state.config.git_proxy.clone(),
            last_error: None,
        });
    }
    let proxy = state.config.git_proxy.clone();
    // Refresh remote refs in the background: a dead proxy/network must never stall page load
    // (the periodic auto-pull keeps refs fresh anyway; the fetch result shows on next load)
    let bg_dir = dir.clone();
    let bg_proxy = proxy.clone();
    let bg_slot = state.git_last_error.clone();
    tokio::task::spawn_blocking(move || {
        let _ = track_sync(
            &bg_slot,
            run_git_net(&bg_dir, &["fetch", "--quiet"], bg_proxy.as_deref()),
        );
    });
    // First line of `git status -sb`: "## main...origin/main [ahead 1, behind 2]" (or "## No commits yet on main")
    // quotePath=false keeps non-ASCII paths readable in the dirty list
    let sb = run_git(&dir, &["-c", "core.quotePath=false", "status", "-sb"])?;
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
    // Porcelain X column: staged when neither blank nor '?' (untracked)
    let staged = sb
        .lines()
        .skip(1)
        .filter(|l| !matches!(l.chars().next(), Some(' ') | Some('?') | None))
        .count() as u32;
    let last_commit = if branch.starts_with("No commits yet") {
        "（还没有提交）".to_string()
    } else {
        run_git(&dir, &["log", "-1", "--pretty=%h %s (%cr)"]).unwrap_or_default()
    };
    let last_error = state.git_last_error.read().unwrap().clone();
    Ok(GitStatus {
        repo: true,
        content_dir: state.config.content_dir.clone(),
        branch,
        ahead,
        behind,
        dirty: sb.lines().skip(1).map(str::to_string).collect(),
        staged,
        last_commit,
        pull_interval_secs: state.config.content_pull_interval_secs,
        proxy,
        last_error,
    })
}

/// Structured dual-pane diff of the working tree vs HEAD (untracked files shown as added)
#[server]
pub async fn admin_git_diff() -> Result<Vec<FileDiff>, ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    let dir = git_repo(&state)?;
    // quotePath=false: non-ASCII paths must come out as raw UTF-8, not "\344\270…" octal
    // escapes — escaped paths don't resolve on disk (the editor then fails with 文件不存在)
    let raw = run_git(
        &dir,
        &[
            "-c",
            "core.quotePath=false",
            "diff",
            "HEAD",
            "--no-color",
            "--",
        ],
    )?;
    let mut files = parse_diff(&raw);
    for path in untracked_files(&dir)? {
        if !files.iter().any(|f| f.path == path) {
            files.push(untracked_diff(&dir, &path));
        }
    }
    let staged = staged_paths(&dir)?;
    for f in &mut files {
        f.staged = staged.contains(&f.path);
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Max file size the admin diff-pane editor will read or write
#[cfg(feature = "ssr")]
const MAX_EDIT_BYTES: usize = 1024 * 1024;

/// Reads a working-tree file (repo-relative path) for the diff pane editor
#[server]
pub async fn admin_git_read_file(path: String) -> Result<String, ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    let dir = git_repo(&state)?;
    let full = resolve_in_repo(&dir, &path)?;
    let bytes = std::fs::read(&full).map_err(|e| ServerFnError::new(format!("读取失败: {e}")))?;
    if bytes.len() > MAX_EDIT_BYTES {
        return Err(ServerFnError::new("文件过大（>1MB），不支持在线编辑"));
    }
    String::from_utf8(bytes).map_err(|_| ServerFnError::new("不是 UTF-8 文本文件，不支持编辑"))
}

/// Writes a working-tree file (repo-relative path); the file must already exist
#[server]
pub async fn admin_git_save_file(path: String, content: String) -> Result<(), ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    let dir = git_repo(&state)?;
    if content.len() > MAX_EDIT_BYTES {
        return Err(ServerFnError::new("内容过大（>1MB）"));
    }
    let full = resolve_in_repo(&dir, &path)?;
    std::fs::write(&full, content).map_err(|e| ServerFnError::new(format!("写入失败: {e}")))
}

/// Replaces a single working-tree line (1-based) — inline edits straight from the diff view
#[server]
pub async fn admin_git_update_line(
    path: String,
    line_no: u32,
    content: String,
) -> Result<(), ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    let dir = git_repo(&state)?;
    if line_no == 0 {
        return Err(ServerFnError::new("行号不合法"));
    }
    if content.contains(['\n', '\r']) {
        return Err(ServerFnError::new("行内编辑只支持单行内容"));
    }
    let full = resolve_in_repo(&dir, &path)?;
    let text =
        std::fs::read_to_string(&full).map_err(|e| ServerFnError::new(format!("读取失败: {e}")))?;
    if text.len() > MAX_EDIT_BYTES {
        return Err(ServerFnError::new("文件过大（>1MB），不支持行内编辑"));
    }
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let idx = (line_no - 1) as usize;
    let Some(old) = lines.get(idx) else {
        return Err(ServerFnError::new("行号超出范围（文件可能已变化，请刷新）"));
    };
    let keep_cr = old.ends_with('\r');
    lines[idx] = if keep_cr {
        format!("{content}\r")
    } else {
        content
    };
    std::fs::write(&full, lines.join("\n"))
        .map_err(|e| ServerFnError::new(format!("写入失败: {e}")))
}

/// Stages one repo-relative path (`git add -- path`); covers new, modified and deleted files
#[server]
pub async fn admin_git_stage(path: String) -> Result<(), ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    let dir = git_repo(&state)?;
    validate_rel_path(&path)?;
    run_git(&dir, &["add", "--", &path])?;
    Ok(())
}

/// Removes one repo-relative path from the index (`git restore --staged`)
#[server]
pub async fn admin_git_unstage(path: String) -> Result<(), ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    let dir = git_repo(&state)?;
    validate_rel_path(&path)?;
    run_git(&dir, &["restore", "--staged", "--", &path])?;
    Ok(())
}

/// Stages everything (`git add -A`) — the one-click flow
#[server]
pub async fn admin_git_stage_all() -> Result<(), ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    let dir = git_repo(&state)?;
    run_git(&dir, &["add", "-A"])?;
    Ok(())
}

#[server]
pub async fn admin_git_pull() -> Result<String, ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    let dir = git_repo(&state)?;
    let out = track_sync(
        &state.git_last_error,
        run_git_net(
            &dir,
            &["pull", "--ff-only"],
            state.config.git_proxy.as_deref(),
        ),
    )?;
    // Apply immediately rather than waiting for the file watcher's debounce
    match crate::content::scan(&dir) {
        Ok(new_index) => *state.index.write() = new_index,
        Err(e) => return Err(ServerFnError::new(format!("拉取成功但重载内容失败: {e}"))),
    }
    Ok(if out.is_empty() {
        "已是最新".into()
    } else {
        out
    })
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
        format!(
            "content update {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M")
        )
    } else {
        message
    };
    if run_git(&dir, &["diff", "--cached", "--name-only"])?.is_empty() {
        return Err(ServerFnError::new(
            "没有已暂存的改动：请先在下方逐个暂存，或点「全部暂存」",
        ));
    }
    // Server clones typically have no git identity configured; pass one explicitly
    let mut log = run_git(
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
    )? + "\n";
    let proxy = state.config.git_proxy.as_deref();
    let pushed = match run_git_net(&dir, &["push"], proxy) {
        Ok(out) => Ok(out),
        // Fresh server clones often have no upstream configured; set it up once
        Err(e)
            if e.to_string().contains("no upstream branch")
                || e.to_string().contains("set-upstream") =>
        {
            run_git_net(&dir, &["push", "-u", "origin", "HEAD"], proxy)
        }
        Err(e) => Err(e),
    };
    log.push_str(&track_sync(&state.git_last_error, pushed)?);
    Ok(log)
}
