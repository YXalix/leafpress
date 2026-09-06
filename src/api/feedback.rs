//! Comments and likes API.

use leptos::prelude::*;

use crate::types::Comment;

#[cfg(feature = "ssr")]
use super::{ip_hash, require_admin, server_err};
#[cfg(feature = "ssr")]
use crate::state::AppState;

#[server]
pub async fn get_comments(slug: String) -> Result<Vec<Comment>, ServerFnError> {
    let state = expect_context::<AppState>();
    crate::db::comments_for(&state.pool, &slug)
        .await
        .map_err(server_err)
}

#[server]
pub async fn add_comment(
    slug: String,
    author: String,
    content: String,
) -> Result<(), ServerFnError> {
    let state = expect_context::<AppState>();
    let author = author.trim();
    let author = if author.is_empty() { "匿名" } else { author };
    let content = content.trim();
    if content.is_empty() {
        return Err(ServerFnError::new("评论内容不能为空"));
    }
    if author.chars().count() > 32 || content.chars().count() > 2000 {
        return Err(ServerFnError::new("昵称或内容过长"));
    }
    crate::db::add_comment(&state.pool, &slug, author, content)
        .await
        .map_err(server_err)
}

#[server]
pub async fn delete_comment(id: i64) -> Result<(), ServerFnError> {
    let state = expect_context::<AppState>();
    require_admin(&state).await?;
    crate::db::delete_comment(&state.pool, id)
        .await
        .map_err(server_err)
}

#[server]
pub async fn get_likes(slug: String) -> Result<(i64, bool), ServerFnError> {
    let state = expect_context::<AppState>();
    let ip = ip_hash().await;
    crate::db::like_state(&state.pool, &slug, &ip)
        .await
        .map_err(server_err)
}

#[server]
pub async fn toggle_like(slug: String) -> Result<(i64, bool), ServerFnError> {
    let state = expect_context::<AppState>();
    let ip = ip_hash().await;
    crate::db::toggle_like(&state.pool, &slug, &ip)
        .await
        .map_err(server_err)
}
