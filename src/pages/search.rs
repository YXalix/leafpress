use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_query_map};

use crate::api::search;
use crate::components::card_meta::CardMeta;
use crate::types::SearchResult;
use crate::util::{debounce_string, highlight, search_terms, url_encode};

fn result_card(r: SearchResult, terms: &[String]) -> AnyView {
    let href = r.meta.href();
    let title = highlight(&r.meta.title, terms);
    let snippet = highlight(&r.snippet, terms);
    view! {
        <article class="card search-result">
            <A href=href attr:class="card-title">{title}</A>
            <CardMeta meta=r.meta/>
            <p class="snippet">{snippet}</p>
        </article>
    }
    .into_any()
}

/// Search page: instant search as you type (debounced), shareable URL with ?q=, SSR renders results directly
#[component]
pub fn Search() -> impl IntoView {
    let query_map = use_query_map();
    let query = RwSignal::new(query_map.get_untracked().get("q").unwrap_or_default());
    let debounced = debounce_string(query, std::time::Duration::from_millis(300), || {});

    // On browser back/forward, sync q from the URL back into the input
    Effect::new(move || {
        if let Some(q) = query_map.get().get("q") {
            if q != query.get_untracked() {
                query.set(q);
            }
        }
    });

    let results = Resource::new(
        move || debounced.get(),
        |q| async move {
            if q.trim().is_empty() {
                Ok(Vec::new())
            } else {
                search(q).await
            }
        },
    );

    let navigate = use_navigate();
    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let q = url_encode(&query.get());
        navigate(&format!("/search?q={q}"), Default::default());
    };

    view! {
        <h1>"搜索"</h1>
        <form class="search-form" on:submit=on_submit>
            <input
                class="search-input"
                type="search"
                placeholder="输入关键词，支持空格分隔多个词…"
                autofocus
                bind:value=query
            />
        </form>
        {move || {
            let q = debounced.get();
            if q.trim().is_empty() {
                return view! { <p class="muted">"搜索文章的标题、标签、分类和正文"</p> }.into_any();
            }
            let terms = search_terms(&q);
            view! {
                <Suspense fallback=|| view! { <p class="muted">"搜索中…"</p> }>
                    {move || {
                        let terms = terms.clone();
                        results.get().map(|res| match res {
                            Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                            Ok(list) if list.is_empty() => {
                                view! { <p class="muted">"没有找到相关内容"</p> }.into_any()
                            }
                            Ok(list) => list
                                .into_iter()
                                .map(|r| result_card(r, &terms))
                                .collect_view()
                                .into_any(),
                        })
                    }}
                </Suspense>
            }
            .into_any()
        }}
    }
}
