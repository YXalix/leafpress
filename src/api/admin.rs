//! Admin API: login session only. Content management is done through git (see [super::git]).

use leptos::prelude::*;

#[cfg(feature = "ssr")]
use super::expected_token;
#[cfg(feature = "ssr")]
use crate::state::AppState;

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
