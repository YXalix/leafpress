use std::collections::BTreeMap;

use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::posts_resource;
use crate::types::PostMeta;

/// Archive page: timeline grouped by year
#[component]
pub fn Archive() -> impl IntoView {
    let posts = posts_resource();
    view! {
        <h1>"归档"</h1>
        <Suspense fallback=|| view! { <p class="muted">"加载中…"</p> }>
            {move || {
                posts.get().map(|res| match res {
                    Ok(list) => {
                        let mut groups: BTreeMap<String, Vec<PostMeta>> = BTreeMap::new();
                        for p in list {
                            let year = p
                                .date
                                .as_deref()
                                .and_then(|d| d.get(..4))
                                .unwrap_or("未注明日期")
                                .to_string();
                            groups.entry(year).or_default().push(p);
                        }
                        groups
                            .into_iter()
                            .rev()
                            .map(|(year, items)| {
                                view! {
                                    <section class="archive-year">
                                        <h2>{year}</h2>
                                        <ul class="archive-list">
                                            {items
                                                .into_iter()
                                                .map(|p| {
                                                    let href = p.href();
                                                    let date = p
                                                        .date
                                                        .as_deref()
                                                        .and_then(|d| d.get(5..10))
                                                        .unwrap_or("--")
                                                        .to_string();
                                                    view! {
                                                        <li>
                                                            <span class="muted">{date}</span>
                                                            <A href=href>{p.title.clone()}</A>
                                                        </li>
                                                    }
                                                })
                                                .collect_view()}
                                        </ul>
                                    </section>
                                }
                            })
                            .collect_view()
                            .into_any()
                    }
                    Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                })
            }}
        </Suspense>
    }
}
