use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::api::get_page;

/// Standalone pages (resume, about, etc.), driven by content/pages/*.md
#[component]
pub fn PageView() -> impl IntoView {
    let params = use_params_map();
    let slug = move || {
        params
            .read()
            .get("slug")
            .map(|s| s.to_string())
            .unwrap_or_default()
    };
    let page = Resource::new(slug, |slug| async move { get_page(slug).await });

    view! {
        <Suspense fallback=|| view! { <p class="muted">"加载中…"</p> }>
            {move || {
                page.get().map(|res| match res {
                    Ok(Some(page)) => view! {
                        <article class="post">
                            <div class="markdown-body" inner_html=page.html></div>
                        </article>
                    }.into_any(),
                    Ok(None) => view! { <p class="muted">"页面不存在"</p> }.into_any(),
                    Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                })
            }}
        </Suspense>
    }
}
