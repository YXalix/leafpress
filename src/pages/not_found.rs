use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <div class="not-found">
            <h1>"404"</h1>
            <p class="muted">"页面不存在"</p>
            <A href="/">"回到首页"</A>
        </div>
    }
}
