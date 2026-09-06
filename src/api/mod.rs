//! Server functions: public-facing reads in [public], comments/likes in [feedback], admin management in [admin].

mod admin;
mod feedback;
mod public;

pub use admin::*;
pub use feedback::*;
pub use public::*;

use leptos::prelude::*;

use crate::types::PostMeta;

#[cfg(feature = "ssr")]
use crate::state::AppState;
#[cfg(feature = "ssr")]
use crate::types::ContentKind;

/// Resource of published posts, shared by the home/list/archive pages
pub fn posts_resource() -> Resource<Result<Vec<PostMeta>, ServerFnError>> {
    Resource::new(|| (), |_| async move { list_posts().await })
}

#[cfg(feature = "ssr")]
fn server_err(e: anyhow::Error) -> ServerFnError {
    ServerFnError::new(e.to_string())
}

#[cfg(feature = "ssr")]
fn sha256_hex(s: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(s.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(feature = "ssr")]
fn expected_token(config: &crate::config::Config) -> String {
    sha256_hex(&format!("admin-token:{}", config.admin_password))
}

#[cfg(feature = "ssr")]
fn cookie_token(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|pair| {
            let mut kv = pair.trim().splitn(2, '=');
            match (kv.next()?, kv.next()?) {
                ("admin_token", v) => Some(v.to_string()),
                _ => None,
            }
        })
        .next()
}

#[cfg(feature = "ssr")]
async fn require_admin(state: &AppState) -> Result<(), ServerFnError> {
    let headers: axum::http::HeaderMap = leptos_axum::extract()
        .await
        .map_err(|_| ServerFnError::new("无法读取请求头"))?;
    if cookie_token(&headers).as_deref() == Some(expected_token(&state.config).as_str()) {
        Ok(())
    } else {
        Err(ServerFnError::new("未登录或登录已过期"))
    }
}

/// Rejects all content write operations in read-only mode (config.admin_readonly): content is sourced solely from the git repo
#[cfg(feature = "ssr")]
fn require_writable(state: &AppState) -> Result<(), ServerFnError> {
    if state.config.admin_readonly {
        Err(ServerFnError::new("只读模式：内容仅通过 git 同步更新"))
    } else {
        Ok(())
    }
}

/// Hashes the requester IP for like deduplication; falls back to "anon" when unavailable
#[cfg(feature = "ssr")]
async fn ip_hash() -> String {
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;
    let ip: Option<String> = match leptos_axum::extract::<ConnectInfo<SocketAddr>>().await {
        Ok(ConnectInfo(addr)) => Some(addr.ip().to_string()),
        Err(_) => leptos_axum::extract::<axum::http::HeaderMap>()
            .await
            .ok()
            .and_then(|h| {
                h.get("x-forwarded-for")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.split(',').next().unwrap_or("").trim().to_string())
            }),
    };
    sha256_hex(&format!("like:{}", ip.unwrap_or_else(|| "anon".into())))
}

#[cfg(feature = "ssr")]
fn content_path(content_dir: &str, kind: ContentKind, slug: &str) -> std::path::PathBuf {
    let sub = match kind {
        ContentKind::Post => "posts",
        ContentKind::Page => "pages",
    };
    std::path::Path::new(content_dir)
        .join(sub)
        .join(format!("{slug}.md"))
}

/// Resolves the actual file path by slug from the index (files may live in subdirectories)
#[cfg(feature = "ssr")]
fn resolve_existing(state: &AppState, kind: ContentKind, slug: &str) -> Option<std::path::PathBuf> {
    state.index.read().find(kind, slug).map(|p| p.path.clone())
}

#[cfg(feature = "ssr")]
fn validate_slug(slug: &str) -> Result<(), ServerFnError> {
    if slug.is_empty()
        || slug.len() > 128
        || slug.contains('/')
        || slug.contains('\\')
        || slug.contains("..")
    {
        Err(ServerFnError::new("slug 不合法"))
    } else {
        Ok(())
    }
}
