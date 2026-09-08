//! CLI subcommands: init / doctor / passwd / update / serve (default).
//! Only compiled under the ssr feature (not included in the WASM build).

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};

use crate::config::Config;

#[derive(Parser)]
#[command(name = "leafpress", version, about = "Full-stack Rust blog engine")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Start the server (default when no arguments are given)
    Serve,
    /// Interactive setup: generate config.toml and prepare the content dir
    Init(InitArgs),
    /// Health check: config / content dir / database / git sync status
    Doctor,
    /// Change the admin password (restart the service to apply)
    Passwd,
    /// Self-update: download the latest binary from GitHub Releases, replace itself, and restart the service
    Update,
}

#[derive(Args, Default)]
pub struct InitArgs {
    /// Site name
    #[arg(long)]
    pub site_name: Option<String>,
    /// Content git repository URL (for private repos use https://<token>@github.com/...)
    #[arg(long)]
    pub content_url: Option<String>,
    /// Content dir path
    #[arg(long)]
    pub content_dir: Option<String>,
    /// Admin password (must not contain double quotes)
    #[arg(long)]
    pub admin_password: Option<String>,
    /// Skip questions, use arguments/defaults (for scripted installs)
    #[arg(long, short)]
    pub yes: bool,
    /// Overwrite an existing config.toml
    #[arg(long)]
    pub force: bool,
}

pub fn parse() -> Cmd {
    Cli::parse().cmd.unwrap_or(Cmd::Serve)
}

// ---------- Interactive helpers ----------

fn is_tty() -> bool {
    std::io::stdin().is_terminal()
}

fn ask(prompt: &str, default: &str) -> Result<String> {
    print!("{prompt} [{default}]: ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let line = line.trim();
    Ok(if line.is_empty() {
        default.to_string()
    } else {
        line.to_string()
    })
}

fn ask_secret(prompt: &str) -> Result<String> {
    Ok(rpassword::prompt_password(format!("{prompt}: "))?)
}

/// Safety constraint for TOML string values: no quotes/backslashes/newlines
fn check_toml_safe(key: &str, val: &str) -> Result<()> {
    if val.contains(['"', '\\', '\n']) {
        bail!("{key} must not contain double quotes, backslashes, or newlines");
    }
    Ok(())
}

// ---------- init ----------

pub fn init(args: InitArgs) -> Result<()> {
    let config_path = Config::path();
    if config_path.exists() && !args.force {
        println!(
            "{} already exists, leaving it as-is (add --force to regenerate)",
            config_path.display()
        );
        return Ok(());
    }
    let interactive = is_tty() && !args.yes;

    // Site name
    let site_name = match &args.site_name {
        Some(v) => v.clone(),
        None if interactive => ask("Site name", "My Blog")?,
        None => "My Blog".to_string(),
    };
    check_toml_safe("site_name", &site_name)?;

    // Content source
    let (content_dir, content_action) = resolve_content(&args, interactive)?;

    // Admin password
    let admin_password = match args.admin_password {
        Some(v) => v,
        None if interactive => {
            let p = ask_secret("Admin password (for /admin login)")?;
            if p.is_empty() {
                bail!("Password must not be empty (or pass it via --admin-password)");
            }
            p
        }
        None => bail!("--admin-password is required in non-interactive mode"),
    };
    check_toml_safe("admin_password", &admin_password)?;

    // Prepare the content dir first; don't write config on failure
    setup_content(&content_dir, &content_action)?;
    std::fs::create_dir_all(content_dir.join("images"))?;

    // Only write non-default entries to keep config.toml minimal
    let mut config =
        format!("site_name = \"{site_name}\"\nadmin_password = \"{admin_password}\"\n");
    if content_dir != Path::new("content") {
        config.push_str(&format!("content_dir = \"{}\"\n", content_dir.display()));
    }
    if let Some(parent) = config_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&config_path, config)?;
    println!(">> Generated {}", config_path.display());
    println!(">> Content dir: {}", content_dir.display());
    println!();
    println!("Next steps:");
    println!("  leafpress serve     # Start the service");
    println!("  leafpress doctor    # Health check");
    Ok(())
}

/// Content source: clone / existing dir / generate sample
enum ContentAction {
    Clone(String),
    Existing,
    Sample,
}

fn resolve_content(args: &InitArgs, interactive: bool) -> Result<(PathBuf, ContentAction)> {
    if let Some(url) = &args.content_url {
        let dir = args
            .content_dir
            .clone()
            .unwrap_or_else(|| "content".to_string());
        return Ok((PathBuf::from(dir), ContentAction::Clone(url.clone())));
    }
    if let Some(dir) = &args.content_dir {
        return Ok((PathBuf::from(dir), ContentAction::Existing));
    }
    if !interactive {
        return Ok((PathBuf::from("content"), ContentAction::Sample));
    }
    println!();
    println!("Content (markdown posts) source:");
    println!("  1) Clone a git repository (e.g. your private content repo)");
    println!("  2) An existing directory");
    println!("  3) Generate a sample post (good for a quick start)");
    let choice = ask("Select", "3")?;
    match choice.as_str() {
        "1" => {
            let url = ask("Git repository URL", "")?;
            if url.is_empty() {
                bail!("URL must not be empty");
            }
            let dir = ask("Directory to clone into", "content")?;
            Ok((PathBuf::from(dir), ContentAction::Clone(url)))
        }
        "2" => {
            let dir = ask("Content dir path", "content")?;
            if !Path::new(&dir).is_dir() {
                bail!("Directory does not exist: {dir}");
            }
            Ok((PathBuf::from(dir), ContentAction::Existing))
        }
        _ => {
            let dir = ask("Content dir path", "content")?;
            Ok((PathBuf::from(dir), ContentAction::Sample))
        }
    }
}

fn setup_content(dir: &Path, action: &ContentAction) -> Result<()> {
    match action {
        ContentAction::Clone(url) => {
            if dir.join(".git").exists() {
                println!(
                    ">> {} is already a git repository, running git pull",
                    dir.display()
                );
                run_git(dir, &["pull", "--ff-only"])?;
            } else if dir.exists() && dir.read_dir()?.next().is_some() {
                bail!(
                    "Directory is not empty and not a git repository: {}",
                    dir.display()
                );
            } else {
                println!(">> git clone {url} -> {}", dir.display());
                let status = Command::new("git")
                    .args(["clone", url])
                    .arg(dir)
                    .status()
                    .context("Failed to run git; is it installed?")?;
                if !status.success() {
                    bail!("git clone failed");
                }
            }
        }
        ContentAction::Existing => {
            println!(">> Using existing directory {}", dir.display());
        }
        ContentAction::Sample => {
            let sample = dir.join("posts/hello.md");
            if !sample.exists() {
                std::fs::create_dir_all(dir.join("posts"))?;
                std::fs::write(
                    &sample,
                    "---\ntitle: \"Hello, Leafpress\"\ndate: \"2026-01-01 12:00\"\nstatus: published\n---\n\nLeafpress 已经跑起来了。在内容目录里增删 `.md` 文件即可更新（热加载，无需重启）。这篇示例可以放心删除。\n",
                )?;
                println!(">> Generated sample post {}", sample.display());
            }
        }
    }
    Ok(())
}

// ---------- doctor ----------

struct Report {
    warns: u32,
    fails: u32,
}

impl Report {
    fn ok(&mut self, msg: impl std::fmt::Display) {
        println!("  [✓] {msg}");
    }
    fn info(&mut self, msg: impl std::fmt::Display) {
        println!("  [i] {msg}");
    }
    fn warn(&mut self, msg: impl std::fmt::Display) {
        self.warns += 1;
        println!("  [!] {msg}");
    }
    fn fail(&mut self, msg: impl std::fmt::Display) {
        self.fails += 1;
        println!("  [✗] {msg}");
    }
}

fn run_git(dir: &Path, git_args: &[&str]) -> Result<String> {
    let out = crate::util::git_command(dir, git_args)
        .output()
        .context("Failed to run git")?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// All checks are read-only with no side effects. Returns true = no failures.
pub async fn doctor() -> bool {
    let mut r = Report { warns: 0, fails: 0 };

    println!("Config:");
    let cfg = match check_config(&mut r) {
        Some(c) => c,
        None => return summary(&r),
    };

    println!("Content dir ({}):", cfg.content_dir);
    check_content(&mut r, &cfg).await;

    println!("Database ({}):", cfg.database);
    check_database(&mut r, &cfg).await;

    println!("Static assets:");
    check_assets(&mut r);

    println!("Listen address:");
    check_addr(&mut r);

    summary(&r)
}

fn summary(r: &Report) -> bool {
    println!();
    if r.fails > 0 {
        println!("Done: {} failed, {} warnings", r.fails, r.warns);
        false
    } else if r.warns > 0 {
        println!("Done: no failures, {} warnings", r.warns);
        true
    } else {
        println!("Done: all checks passed");
        true
    }
}

fn check_config(r: &mut Report) -> Option<Config> {
    let path = Config::path();
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => {
            r.fail(format!("{} not found → run leafpress init", path.display()));
            return None;
        }
    };
    match toml::from_str::<Config>(&raw) {
        Ok(mut cfg) => {
            cfg.anchor_to_config_dir(&path);
            r.ok(format!("{} parsed successfully", path.display()));
            if cfg.admin_password.is_empty() || cfg.admin_password == "changeme" {
                r.warn("admin_password is empty or default → change it with leafpress passwd");
            } else {
                r.ok("admin_password is set");
            }
            if let Some(proxy) = &cfg.git_proxy {
                r.info(format!(
                    "git_proxy = {proxy}: git fetch/pull/push go through this proxy"
                ));
            }
            Some(cfg)
        }
        Err(e) => {
            r.fail(format!("config.toml failed to parse: {e}"));
            None
        }
    }
}

async fn check_content(r: &mut Report, cfg: &Config) {
    let dir = PathBuf::from(&cfg.content_dir);
    if !dir.is_dir() {
        r.fail(format!(
            "{} does not exist → run leafpress init or fix content_dir",
            dir.display()
        ));
        return;
    }
    if dir.read_dir().is_ok() {
        r.ok("Directory exists and is readable");
    } else {
        r.fail(format!(
            "{} is not readable (permission problem)",
            dir.display()
        ));
        return;
    }

    if !dir.join("images").is_dir() {
        r.warn("Missing images/ subdirectory (/images/... in markdown will 404)");
    }

    if !dir.join(".git").exists() {
        r.info("Not a git repository: no auto-sync, content must be maintained manually");
        return;
    }
    match run_git(&dir, &["status", "--porcelain"]) {
        Ok(out) if out.is_empty() => r.ok("git working tree clean"),
        Ok(_) => r.warn(
            "git has uncommitted changes/untracked files → periodic pull --ff-only may fail silently; run git status to inspect",
        ),
        Err(e) => r.warn(format!("git status failed: {e}")),
    }
    if let Ok(out) = run_git(&dir, &["status", "-sb"]) {
        let head = out.lines().next().unwrap_or("");
        if head.contains("[behind") {
            r.info(format!(
                "Behind remote ({}) → run git -C {} pull",
                head,
                dir.display()
            ));
        } else if head.contains("[ahead") {
            r.warn(format!(
                "Local commits ahead of remote ({}) → push from /admin or run git -C {} push",
                head,
                dir.display()
            ));
        }
    }
    if cfg.content_pull_interval_secs > 0 {
        r.info(format!(
            "built-in git pull every {}s (content_pull_interval_secs)",
            cfg.content_pull_interval_secs
        ));
    } else {
        r.info("built-in git pull disabled (content_pull_interval_secs = 0)");
    }
    check_ssh_remote(r, &dir);
}

/// Extracts (host, port) from SSH git URLs: git@host:path or ssh://[user@]host[:port]/path
fn ssh_remote_host(url: &str) -> Option<(String, Option<u16>)> {
    if let Some(rest) = url.strip_prefix("git@") {
        let host = rest.split(':').next().unwrap_or("");
        return (!host.is_empty()).then(|| (host.to_string(), None));
    }
    let rest = url.strip_prefix("ssh://")?;
    let hostport = rest.rsplit('@').next()?.split('/').next()?;
    let (host, port) = match hostport.split_once(':') {
        Some((h, p)) => (h, p.parse().ok()),
        None => (hostport, None),
    };
    (!host.is_empty()).then(|| (host.to_string(), port))
}

/// SSH remotes fail with "Host key verification failed" unless the service user's HOME has
/// the host key and an auth key. Under the standard install the config file's directory IS
/// the leafpress user's HOME, so check <config-dir>/.ssh. Skipped for a relative
/// ./config.toml (local dev: git uses the real user's ~/.ssh).
fn check_ssh_remote(r: &mut Report, repo: &Path) {
    let Ok(url) = run_git(repo, &["remote", "get-url", "origin"]) else {
        return;
    };
    let Some((host, port)) = ssh_remote_host(&url) else {
        return;
    };
    let config_path = Config::path();
    let Some(config_dir) = config_path.parent().filter(|p| !p.as_os_str().is_empty()) else {
        return;
    };
    let ssh_dir = config_dir.join(".ssh");
    let kh_host = match port {
        Some(p) => format!("[{host}]:{p}"),
        None => host.clone(),
    };
    let fix = format!(
        "re-run deploy/install.sh (it seeds the leafpress user's SSH), or copy the host key / a key pair into {} owned by leafpress",
        ssh_dir.display()
    );
    let known_hosts = ssh_dir.join("known_hosts");
    let host_ok = Command::new("ssh-keygen")
        .args(["-F", &kh_host, "-f"])
        .arg(&known_hosts)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if host_ok {
        r.ok(format!(
            "SSH remote: {kh_host} trusted in {}",
            known_hosts.display()
        ));
    } else {
        r.warn(format!(
            "SSH remote: {kh_host} not in {} → git pull/push fails with \"Host key verification failed\"; {fix}",
            known_hosts.display()
        ));
    }
    if ["id_ed25519", "id_rsa"].iter().any(|k| ssh_dir.join(k).is_file()) {
        r.ok(format!(
            "SSH remote: private key present in {}",
            ssh_dir.display()
        ));
    } else {
        r.warn(format!(
            "SSH remote: no private key (id_ed25519/id_rsa) in {} → git pull/push cannot authenticate; {fix}",
            ssh_dir.display()
        ));
    }
}

async fn check_database(r: &mut Report, cfg: &Config) {
    let path = Path::new(&cfg.database);
    if !path.exists() {
        r.info(
            "Database does not exist; it will be created on first start (comments/likes tables)",
        );
        return;
    }
    use sqlx::sqlite::SqliteConnectOptions;
    let opts = SqliteConnectOptions::new().filename(path).read_only(true);
    let pool = match sqlx::SqlitePool::connect_with(opts).await {
        Ok(p) => p,
        Err(e) => {
            r.fail(format!("Failed to open database: {e}"));
            return;
        }
    };
    let tables: Result<Vec<String>, _> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table'")
            .fetch_all(&pool)
            .await;
    match tables {
        Ok(names) => {
            let missing: Vec<_> = ["comments", "likes"]
                .iter()
                .filter(|t| !names.iter().any(|n| n == **t))
                .collect();
            if missing.is_empty() {
                r.ok("comments/likes tables present");
            } else {
                r.info(format!(
                    "Missing tables {missing:?}; they will be created on first start"
                ));
            }
        }
        Err(e) => r.fail(format!("Database query failed: {e}")),
    }
    pool.close().await;
}

fn check_assets(r: &mut Report) {
    // Release builds embed target/site into the binary; debug builds read from disk
    let missing: Vec<_> = ["pkg/leafpress.js", "pkg/leafpress.css", "favicon.svg"]
        .iter()
        .filter(|f| crate::assets::Assets::get(f).is_none())
        .collect();
    if missing.is_empty() {
        r.ok("WASM/CSS/favicon embedded");
    } else {
        r.fail(format!(
            "Missing static assets {missing:?} → re-run cargo leptos build --release"
        ));
    }
}

fn check_addr(r: &mut Report) {
    let addr = crate::config::leptos_options().site_addr;
    match std::net::TcpListener::bind(addr) {
        Ok(l) => {
            drop(l);
            r.ok(format!("{addr} can be bound"));
        }
        Err(_) => r.info(format!(
            "{addr} cannot be bound (normal if the service is already running)"
        )),
    }
}

// ---------- update ----------

const REPO: &str = "YXalix/leafpress";

fn update_target() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        (os, arch) => {
            bail!("No prebuilt binary for this platform ({os}/{arch}); run cargo leptos build --release yourself")
        }
    }
}

/// Download the latest release binary → replace the current exe in place → restart the systemd service.
/// Only the binary itself is touched; config.toml / content dir / database are unaffected.
/// Private repos require GITHUB_TOKEN (fine-grained PAT, Contents:read) to be set.
pub fn update() -> Result<()> {
    let repo = std::env::var("LEAFPRESS_REPO")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| REPO.to_string());
    let target = update_target()?;
    let url = format!("https://github.com/{repo}/releases/latest/download/leafpress-{target}");

    let exe = std::env::current_exe().context("Failed to locate the current executable")?;
    let mut tmp_name = exe.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".new");
    let tmp = exe.with_file_name(tmp_name);

    println!(">> Downloading {url}");
    let mut curl = Command::new("curl");
    curl.args(["-fSL", "--progress-bar"]);
    if let Some(token) = std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()) {
        curl.arg("-H").arg(format!("Authorization: Bearer {token}"));
    }
    let download = curl.arg(&url).arg("-o").arg(&tmp).status();
    match download {
        Ok(s) if s.success() => {}
        other => {
            let _ = std::fs::remove_file(&tmp);
            match other {
                Ok(s) => bail!("Download failed (curl exit code {s})"),
                Err(e) => bail!("Failed to run curl: {e}"),
            }
        }
    }

    // Replacing the running binary: on Linux, rename-overwrite is safe (the old process keeps using the unlinked inode)
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    std::fs::rename(&tmp, &exe)
        .context("Failed to replace the binary (insufficient permissions? run with sudo)")?;
    println!(">> Updated {}", exe.display());

    // Restart directly if running under systemd; otherwise prompt for a manual restart
    let restarted = Command::new("systemctl")
        .args(["restart", "leafpress"])
        .status()
        .is_ok_and(|s| s.success());
    if restarted {
        let active = Command::new("systemctl")
            .args(["is-active", "leafpress"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        println!(">> Service restarted, status: {active}");
    } else {
        println!(
            ">> Not restarted via systemd; restart the service manually or re-run the program"
        );
    }
    Ok(())
}

// ---------- passwd ----------

pub fn passwd() -> Result<()> {
    let path = Config::path();
    if !path.exists() {
        bail!("{} not found; run leafpress init first", path.display());
    }
    if !is_tty() {
        bail!("passwd requires an interactive terminal");
    }
    let p1 = ask_secret("New password")?;
    if p1.is_empty() {
        bail!("Password must not be empty");
    }
    check_toml_safe("admin_password", &p1)?;
    let p2 = ask_secret("Re-enter password")?;
    if p1 != p2 {
        bail!("Passwords do not match");
    }

    let content = std::fs::read_to_string(&path)?;
    let new_line = format!("admin_password = \"{p1}\"");
    let mut replaced = false;
    let out: Vec<String> = content
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("admin_password") {
                replaced = true;
                new_line.clone()
            } else {
                line.to_string()
            }
        })
        .collect();
    let mut out = out.join("\n");
    if !replaced {
        out.push_str(&format!("\n{new_line}"));
    }
    if content.ends_with('\n') && !out.ends_with('\n') {
        out.push('\n');
    }
    std::fs::write(&path, out)?;
    println!(">> Updated {}", path.display());
    println!("Note: restart the service to apply (sudo systemctl restart leafpress); all existing login sessions become invalid");
    Ok(())
}
