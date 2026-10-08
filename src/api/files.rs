//! Public file-sharing API over [crate::config::Config::files_dir] — decoupled from the
//! content repo: no git, no admin session, no database. The listing server fn is public;
//! PUT/DELETE on /f/<name> are plain axum handlers gated by `files_upload_token`
//! (curl-friendly), wired in main.rs next to the ServeDir that answers the downloads.

use leptos::prelude::*;

#[cfg(feature = "ssr")]
use axum::extract::{Path, State};
#[cfg(feature = "ssr")]
use crate::state::AppState;

use crate::types::SharedFile;

#[server]
pub async fn list_shared_files() -> Result<Vec<SharedFile>, ServerFnError> {
    let state = expect_context::<AppState>();
    Ok(read_shared_files(&state.config.files_dir))
}

/// Top-level regular files of `dir`, newest first; missing dir = empty listing
#[cfg(feature = "ssr")]
fn read_shared_files(dir: &str) -> Vec<SharedFile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<SharedFile> = entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            let modified = meta
                .modified()
                .ok()
                .map(|t| {
                    chrono::DateTime::<chrono::Utc>::from(t)
                        .with_timezone(&chrono::Local)
                        .format("%Y-%m-%d %H:%M")
                        .to_string()
                })
                .unwrap_or_default();
            Some(SharedFile {
                name: entry.file_name().to_string_lossy().into_owned(),
                size: meta.len(),
                modified,
            })
        })
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified).then(b.name.cmp(&a.name)));
    out
}

/// Flat single-path-component names only: the upload URL maps 1:1 onto files_dir
#[cfg(feature = "ssr")]
fn valid_shared_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && name.trim() == name
        && !name.starts_with('.')
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\'])
}

/// Token gate for the mutating /f handlers: config token unset → 403 (feature off),
/// header mismatch → 401
#[cfg(feature = "ssr")]
fn check_upload_token(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<(), axum::response::Response> {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    let Some(expected) = state
        .config
        .files_upload_token
        .as_deref()
        .filter(|t| !t.is_empty())
    else {
        return Err(
            (StatusCode::FORBIDDEN, "文件上传未启用：在 config.toml 设置 files_upload_token 并重启\n")
                .into_response(),
        );
    };
    let got = headers
        .get("x-upload-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if got != expected {
        return Err((StatusCode::UNAUTHORIZED, "上传 token 错误\n").into_response());
    }
    Ok(())
}

/// PUT /f/<name>: streams the request body straight into files_dir (no full buffering),
/// enforcing files_max_mb; same-name files are overwritten. Responds with the share link.
#[cfg(feature = "ssr")]
pub async fn upload_shared_file(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
    body: axum::body::Body,
) -> axum::response::Response {
    use std::future::poll_fn;
    use std::pin::Pin;
    use tokio::io::AsyncWriteExt;

    use axum::body::HttpBody;
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;

    if let Err(resp) = check_upload_token(&state, &headers) {
        return resp;
    }
    if !valid_shared_name(&name) {
        return (StatusCode::BAD_REQUEST, "文件名不合法（仅支持单层、不以 . 开头的文件名）\n")
            .into_response();
    }
    let mb = state.config.files_max_mb;
    let limit = if mb == 0 {
        u64::MAX
    } else {
        mb.saturating_mul(1024 * 1024)
    };
    // curl -T sends Content-Length up front — reject oversized uploads before reading
    if let Some(len) = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
    {
        if len > limit {
            return (StatusCode::PAYLOAD_TOO_LARGE, "文件超过 files_max_mb 上限\n").into_response();
        }
    }

    let dir = std::path::PathBuf::from(&state.config.files_dir);
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("无法创建文件目录 {}: {e}\n", dir.display()),
        )
            .into_response();
    }
    let path = dir.join(&name);
    let mut file = match tokio::fs::File::create(&path).await {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("写入失败: {e}\n"),
            )
                .into_response()
        }
    };

    // Stream frame by frame; any failure or limit overrun removes the partial file
    let mut stream = body;
    let mut written: u64 = 0;
    let result: Result<(), String> = loop {
        match poll_fn(|cx| Pin::new(&mut stream).poll_frame(cx)).await {
            Some(Ok(frame)) => {
                let Ok(data) = frame.into_data() else { continue };
                written += data.len() as u64;
                if written > limit {
                    break Err("文件超过 files_max_mb 上限".into());
                }
                if let Err(e) = file.write_all(&data).await {
                    break Err(format!("写入失败: {e}"));
                }
            }
            Some(Err(e)) => break Err(format!("读取上传流失败: {e}")),
            None => break Ok(()),
        }
    };
    if let Err(msg) = result {
        drop(file);
        let _ = tokio::fs::remove_file(&path).await;
        let status = if msg.contains("files_max_mb") {
            StatusCode::PAYLOAD_TOO_LARGE
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        return (status, format!("{msg}\n")).into_response();
    }
    let _ = file.flush().await;

    let encoded = crate::util::url_encode(&name);
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    let link = match host {
        Some(h) if !h.is_empty() => format!("https://{h}/f/{encoded}\n"),
        _ => format!("/f/{encoded}\n"),
    };
    link.into_response()
}

/// DELETE /f/<name>: removes one shared file (same token gate as upload)
#[cfg(feature = "ssr")]
pub async fn delete_shared_file(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    if let Err(resp) = check_upload_token(&state, &headers) {
        return resp;
    }
    if !valid_shared_name(&name) {
        return (StatusCode::BAD_REQUEST, "文件名不合法\n").into_response();
    }
    let path = std::path::PathBuf::from(&state.config.files_dir).join(&name);
    match tokio::fs::remove_file(&path).await {
        Ok(()) => "已删除\n".into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "文件不存在\n").into_response(),
    }
}
