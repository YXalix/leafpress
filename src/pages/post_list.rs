use leptos::prelude::*;

use crate::api::posts_resource;
use crate::components::post_card::PostCard;
use crate::types::PostMeta;

/// Post list page: supports filtering by category
#[component]
pub fn PostList() -> impl IntoView {
    let posts = posts_resource();
    let filter = RwSignal::new(None::<String>);

    let categories = Memo::new(move |_| {
        let mut cats: Vec<String> = posts
            .get()
            .and_then(|r| r.ok())
            .unwrap_or_default()
            .iter()
            .filter_map(|p| p.category.clone())
            .collect();
        cats.sort();
        cats.dedup();
        cats
    });

    view! {
        <h1>"文章"</h1>
        <div class="filter-bar">
            <button
                class=move || if filter.get().is_none() { "chip active" } else { "chip" }
                on:click=move |_| filter.set(None)
            >
                "全部"
            </button>
            {move || {
                categories.get().into_iter().map(|c| {
                    let c2 = c.clone();
                    let c3 = c.clone();
                    view! {
                        <button
                            class=move || if filter.get().as_deref() == Some(c2.as_str()) { "chip active" } else { "chip" }
                            on:click=move |_| filter.set(Some(c3.clone()))
                        >
                            {c}
                        </button>
                    }
                }).collect_view()
            }}
        </div>
        <Suspense fallback=|| view! { <p class="muted">"加载中…"</p> }>
            {move || {
                posts.get().map(|res| match res {
                    Ok(list) => {
                        let cur = filter.get();
                        let filtered: Vec<PostMeta> = list
                            .into_iter()
                            .filter(|p| match (&cur, &p.category) {
                                (Some(f), Some(c)) => f == c,
                                (Some(_), None) => false,
                                (None, _) => true,
                            })
                            .collect();
                        if filtered.is_empty() {
                            view! { <p class="muted">"该分类下暂无文章"</p> }.into_any()
                        } else {
                            filtered.into_iter().map(|m| view! { <PostCard meta=m/> }).collect_view().into_any()
                        }
                    }
                    Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                })
            }}
        </Suspense>
    }
}
