//! Admin pages: login (this module) and the git console ([git]).
//! Content itself is managed purely through git: edit locally and push, or use the
//! console's diff editor + commit/push for server-side fixes.

mod git;

pub use git::AdminGit;

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::api::admin_login;

/// Error block shown when login is required
fn login_prompt(e: ServerFnError) -> impl IntoView {
    view! {
        <div>
            <p class="error">{e.to_string()}</p>
            <p><A href="/admin/login">"去登录 →"</A></p>
        </div>
    }
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
