use std::net::SocketAddr;

use leptos::prelude::LeptosOptions;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub site_name: String,
    pub admin_password: String,
    pub content_dir: String,
    pub database: String,
    /// true = /admin is read-only (its save/delete disabled); content syncs via git only
    pub admin_readonly: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            site_name: "My Blog".into(),
            admin_password: "changeme".into(),
            content_dir: "content".into(),
            database: "site.db".into(),
            admin_readonly: false,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        match std::fs::read_to_string("config.toml") {
            Ok(s) => match toml::from_str(&s) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[config] failed to parse config.toml, using defaults: {e}");
                    Self::default()
                }
            },
            Err(_) => {
                eprintln!("[config] config.toml not found, using defaults");
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
