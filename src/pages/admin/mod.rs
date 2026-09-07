//! Admin pages: login (this module), content management ([dashboard]), editor ([editor]).

mod dashboard;
mod editor;
mod git;

pub use dashboard::AdminDashboard;
pub use editor::{AdminEdit, AdminNew};
pub use git::GitPanel;

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::api::{admin_login, admin_readonly};

/// Result of the admin_readonly query; on failure treat as writable (server-side writes still reject as a fallback)
fn readonly_signal() -> Memo<bool> {
    let readonly = OnceResource::new(admin_readonly());
    Memo::new(move |_| readonly.get().and_then(|r| r.ok()).unwrap_or(false))
}

/// Error block shown when login is required (shared by dashboard / editor)
fn login_prompt(e: ServerFnError) -> impl IntoView {
    view! {
        <div>
            <p class="error">{e.to_string()}</p>
            <p><A href="/admin/login">"去登录 →"</A></p>
        </div>
    }
}

/// Route param extraction shared by /admin/new/:kind and /admin/edit/:kind/:slug
fn params_kind_slug() -> (impl Fn() -> String + Copy, impl Fn() -> String + Copy) {
    let params = use_params_map();
    let kind = move || {
        params
            .read()
            .get("kind")
            .map(|s| s.to_string())
            .unwrap_or_else(|| "post".into())
    };
    let slug = move || {
        params
            .read()
            .get("slug")
            .map(|s| s.to_string())
            .unwrap_or_default()
    };
    (kind, slug)
}

#[component]
pub fn AdminLogin() -> impl IntoView {
    let password_ref = NodeRef::<leptos::html::Input>::new();
    let error = RwSignal::new(None::<String>);
    let navigate = use_navigate();

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let password = password_ref.get().map(|i| i.value()).unwrap_or_default();
        let navigate = navigate.clone();
        leptos::task::spawn_local(async move {
            match admin_login(password).await {
                Ok(()) => navigate("/admin", Default::default()),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    view! {
        <div class="admin-login">
            <h1>"管理登录"</h1>
            <form on:submit=on_submit>
                <input node_ref=password_ref type="password" placeholder="管理密码" required autofocus/>
                <button type="submit">"登录"</button>
            </form>
            {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
        </div>
    }
}
