use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::site_info;
use crate::components::search_modal::SearchModal;
use crate::components::theme_toggle::ThemeToggle;

#[component]
pub fn Layout(children: Children) -> impl IntoView {
    let site_name = Resource::new(
        || (),
        |_| async move { site_info().await.unwrap_or_else(|_| "My Blog".into()) },
    );
    let search_open = RwSignal::new(false);
    view! {
        <header class="site-header">
            <div class="container nav">
                <A href="/" attr:class="brand">
                    <Suspense fallback=|| "My Blog">
                        {move || site_name.get().map(|n| view! { {n} })}
                    </Suspense>
                </A>
                <nav class="nav-links">
                    <A href="/posts">"文章"</A>
                    <A href="/archive">"归档"</A>
                    <A href="/pages/resume">"简历"</A>
                    // With JS: open the search modal (⌘K); without JS: falls back to the /search page
                    <A
                        href="/search"
                        attr:class="search-trigger"
                        on:click=move |ev| {
                            ev.prevent_default();
                            search_open.set(true);
                        }
                    >
                        <span class="kbd">"⌘K"</span>
                        "搜索"
                    </A>
                    <ThemeToggle/>
                </nav>
            </div>
        </header>
        <main class="container main">{children()}</main>
        <SearchModal open=search_open/>
        <footer class="site-footer">
            <div class="container">"Powered by Rust · Leptos + Axum"</div>
        </footer>
    }
}
