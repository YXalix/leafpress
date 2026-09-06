//! Admin API: auth state, content CRUD, markdown preview.

use leptos::prelude::*;

use crate::types::PostMeta;

#[cfg(feature = "ssr")]
use super::{
    content_path, expected_token, require_admin, require_writable, resolve_existing, validate_slug,
};
#[cfg(feature = "ssr")]
use crate::state::AppState;
#[cfg(feature = "ssr")]
use crate::types::ContentKind;

#[server]
pub async fn admin_login(password: String) -> Result<(), ServerFnError> {
    let state = expect_context::<AppState>();
    if password != state.config.admin_password {
        return Err(ServerFnError::new("密码错误"));
    }
    let token = expected_token(&state.config);
    let res = expect_context::<leptos_axum::ResponseOptions>();
    let cookie = format!("admin_token={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=604800");
    if let Ok(value) = axum::http::HeaderValue::from_str(&cookie) {
        res.append_header(axum::http::header::SET_COOKIE, value);
    }
    Ok(())
}

#[server]
pub async fn admin_logout() -> Result<(), ServerFnError> {
    let res = expect_context::<leptos_axum::ResponseOptions>();
    if let Ok(value) =
        axum::http::HeaderValue::from_str("admin_token=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
    {
        res.append_header(axum::http::header::SET_COOKIE, value);
    }
    Ok(())
}

#[server]
pub async fn admin_list_all() -> Result<Vec<PostMeta>, ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    let index = state.index.read();
    Ok(index
        .posts
        .iter()
        .chain(index.pages.iter())
        .map(|p| p.meta.clone())
        .collect())
}

#[server]
pub async fn admin_get_raw(kind: String, slug: String) -> Result<String, ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    let kind: ContentKind = kind
        .parse()
        .map_err(|_| ServerFnError::new("kind 不合法"))?;
    validate_slug(&slug)?;
    let path = resolve_existing(&state, kind, &slug)
        .unwrap_or_else(|| content_path(&state.config.content_dir, kind, &slug));
    std::fs::read_to_string(&path).map_err(|e| ServerFnError::new(format!("读取失败: {e}")))
}

#[server]
pub async fn admin_readonly() -> Result<bool, ServerFnError> {
    let state = expect_context::<AppState>();
    Ok(state.config.admin_readonly)
}

#[server]
pub async fn admin_save(
    kind: String,
    old_slug: Option<String>,
    slug: String,
    raw: String,
) -> Result<(), ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    require_writable(&state)?;
    let kind: ContentKind = kind
        .parse()
        .map_err(|_| ServerFnError::new("kind 不合法"))?;
    validate_slug(&slug)?;
    if let Some(old) = &old_slug {
        validate_slug(old)?;
    }
    let old_path = old_slug
        .as_deref()
        .and_then(|old| resolve_existing(&state, kind, old));
    // Taking a slug already used by other content would conflict at scan time (the latter gets dropped), so reject outright
    if old_slug.as_deref() != Some(slug.as_str()) && resolve_existing(&state, kind, &slug).is_some()
    {
        return Err(ServerFnError::new("slug 已被占用"));
    }
    // Files in subdirectories stay in place (slug rename renames within the same directory); new files are written to the root
    let path = match &old_path {
        Some(p) => p.with_file_name(format!("{slug}.md")),
        None => content_path(&state.config.content_dir, kind, &slug),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ServerFnError::new(format!("创建目录失败: {e}")))?;
    }
    std::fs::write(&path, raw).map_err(|e| ServerFnError::new(format!("写入失败: {e}")))?;
    if let (Some(old), Some(old_file)) = (old_slug.as_deref(), old_path) {
        if old != slug {
            let _ = std::fs::remove_file(old_file);
        }
    }
    Ok(())
}

#[server]
pub async fn admin_delete(kind: String, slug: String) -> Result<(), ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    require_writable(&state)?;
    let kind: ContentKind = kind
        .parse()
        .map_err(|_| ServerFnError::new("kind 不合法"))?;
    validate_slug(&slug)?;
    let path = resolve_existing(&state, kind, &slug)
        .unwrap_or_else(|| content_path(&state.config.content_dir, kind, &slug));
    std::fs::remove_file(&path).map_err(|e| ServerFnError::new(format!("删除失败: {e}")))
}

#[server]
pub async fn preview_markdown(raw: String) -> Result<String, ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    let matter = gray_matter::Matter::<gray_matter::engine::YAML>::new();
    let parsed = matter.parse(&raw);
    Ok(crate::content::render_markdown(&parsed.content))
}
