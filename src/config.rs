use std::net::SocketAddr;
use std::path::PathBuf;

use leptos::prelude::LeptosOptions;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub site_name: String,
    pub admin_password: String,
    pub content_dir: String,
    pub database: String,
    /// Seconds between built-in `git pull --ff-only` on content_dir; 0 disables.
    /// No-op when content_dir is not a git repo. Pulled changes hot-reload via the watcher.
    pub content_pull_interval_secs: u64,
    /// Proxy for git network ops (fetch/pull/push), e.g. "http://127.0.0.1:7890".
    /// Default None = direct connection (any http_proxy env inherited from the server
    /// process is explicitly disabled). Set when the remote is unreachable without a proxy.
    pub git_proxy: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            site_name: "My Blog".into(),
            admin_password: "changeme".into(),
            content_dir: "content".into(),
            database: "site.db".into(),
            content_pull_interval_secs: 300,
            git_proxy: None,
        }
    }
}

impl Config {
    /// Lookup order: $LEAFPRESS_CONFIG > ./config.toml > $XDG_CONFIG_HOME/leafpress/config.toml
    /// (~/.config/leafpress/config.toml). Falls back to ./config.toml when none exist, so
    /// load() reports "not found" as before and init keeps writing to the current directory.
    pub fn path() -> PathBuf {
        if let Some(p) = std::env::var_os("LEAFPRESS_CONFIG") {
            return PathBuf::from(p);
        }
        let local = PathBuf::from("config.toml");
        if local.exists() {
            return local;
        }
        Self::global_path().filter(|p| p.exists()).unwrap_or(local)
    }

    fn global_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("leafpress").join("config.toml"))
    }

    pub fn load() -> Self {
        match std::fs::read_to_string(Self::path()) {
            Ok(s) => match toml::from_str(&s) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[config] failed to parse config.toml, using defaults: {e}");
                    Self::default()
                }
            },
            Err(_) => {
                eprintln!("[config] config.toml not found (searched $LEAFPRESS_CONFIG, ./config.toml, ~/.config/leafpress), using defaults");
                Self::default()
            }
        }
    }
}

/// Leptos runtime config: does not rely on cargo-leptos compile-time env vars,
/// kept consistent with [package.metadata.leptos]; listen addr overridable via LEPTOS_SITE_ADDR.
pub fn leptos_options() -> LeptosOptions {
    let site_addr = std::env::var("LEPTOS_SITE_ADDR")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 3000)));
    let reload_port = std::env::var("LEPTOS_RELOAD_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3001);
    LeptosOptions::builder()
        .output_name("leafpress")
        .site_root("target/site")
        .site_addr(site_addr)
        .reload_port(reload_port)
        .build()
}
