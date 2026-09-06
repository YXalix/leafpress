use leptos::ev::KeyboardEvent;
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;
use wasm_bindgen::JsCast;

use crate::api::search;
use crate::types::SearchResult;
use crate::util::{debounce_string, highlight, search_terms, url_encode};

/// Max number of results shown in the modal; full results live on the /search page
const MAX_RESULTS: usize = 8;

/// Result list: ↑↓ keys and hover stay in sync for highlighting; clicking closes the modal
#[component]
fn SearchResults(
    results: Resource<Result<Vec<SearchResult>, ServerFnError>>,
    debounced: RwSignal<String>,
    active: RwSignal<usize>,
    open: RwSignal<bool>,
) -> impl IntoView {
    move || {
        let q = debounced.get();
        if q.trim().is_empty() {
            return view! { <p class="muted search-modal-hint">"输入关键词开始搜索"</p> }
                .into_any();
        }
        let terms = search_terms(&q);
        view! {
            <Suspense fallback=|| view! { <p class="muted search-modal-hint">"搜索中…"</p> }>
                {move || {
                    let terms = terms.clone();
                    results.get().map(|res| match res {
                        Err(e) => {
                            view! { <p class="error search-modal-hint">{e.to_string()}</p> }.into_any()
                        }
                        Ok(list) if list.is_empty() => {
                            view! { <p class="muted search-modal-hint">"没有找到相关内容"</p> }.into_any()
                        }
                        Ok(list) => list
                            .into_iter()
                            .take(MAX_RESULTS)
                            .enumerate()
                            .map(|(i, r)| {
                                let href = r.meta.href();
                                let title = highlight(&r.meta.title, &terms);
                                let snippet = highlight(&r.snippet, &terms);
                                view! {
                                    <A
                                        href=href
                                        attr:class=move || {
                                            if i == active.get() { "search-opt active" } else { "search-opt" }
                                        }
                                        on:click=move |_| open.set(false)
                                        on:mouseenter=move |_| active.set(i)
                                    >
                                        <span class="search-opt-title">{title}</span>
                                        <span class="search-opt-snippet">{snippet}</span>
                                    </A>
                                }
                                .into_any()
                            })
                            .collect_view()
                            .into_any(),
                    })
                }}
            </Suspense>
        }
        .into_any()
    }
}

/// Global search modal: opened via ⌘K / Ctrl+K or /, closed with Esc; ↑↓ to select, Enter to open
#[component]
pub fn SearchModal(open: RwSignal<bool>) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let active = RwSignal::new(0usize);
    // Input debounce: keystrokes within 300ms only trigger the last search
    let debounced = debounce_string(query, std::time::Duration::from_millis(300), move || {
        active.set(0)
    });
    let input_ref = NodeRef::<leptos::html::Input>::new();

    // Global shortcut (the callback runs outside the reactive owner, so required signals are moved in)
    let handle = window_event_listener(leptos::ev::keydown, move |ev: KeyboardEvent| {
        let key = ev.key();
        if (ev.meta_key() || ev.ctrl_key()) && key.eq_ignore_ascii_case("k") {
            ev.prevent_default();
            open.update(|o| *o = !*o);
            return;
        }
        if open.get() {
            if key == "Escape" {
                open.set(false);
            }
            return;
        }
        if key == "/" {
            // Don't hijack the slash key while focus is in an input/textarea
            let typing = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                .map(|el| matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA"))
                .unwrap_or(false);
            if !typing {
                ev.prevent_default();
                open.set(true);
            }
        }
    });
    on_cleanup(move || handle.remove());

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

    // Reset state and focus the input when opened
    Effect::new(move || {
        if open.get() {
            query.set(String::new());
            debounced.set(String::new());
            active.set(0);
            if let Some(el) = input_ref.get() {
                let _ = el.focus();
            }
        }
    });

    let current_results =
        move || -> Vec<SearchResult> { results.get().and_then(|r| r.ok()).unwrap_or_default() };

    let navigate = use_navigate();
    let on_keydown = move |ev: KeyboardEvent| {
        let len = current_results().len().min(MAX_RESULTS);
        match ev.key().as_str() {
            "ArrowDown" => {
                ev.prevent_default();
                active.update(|i| *i = (*i + 1).min(len.saturating_sub(1)));
            }
            "ArrowUp" => {
                ev.prevent_default();
                active.update(|i| *i = i.saturating_sub(1));
            }
            "Enter" => {
                ev.prevent_default();
                let list = current_results();
                if let Some(r) = list.get(active.get()) {
                    let href = r.meta.href();
                    open.set(false);
                    navigate(&href, Default::default());
                }
            }
            _ => {}
        }
    };

    view! {
        <Show when=move || open.get()>
            <div class="search-overlay" on:click=move |_| open.set(false)>
                <div
                    class="search-modal"
                    role="dialog"
                    aria-modal="true"
                    on:click=move |ev| ev.stop_propagation()
                >
                    <input
                        class="search-modal-input"
                        type="search"
                        placeholder="搜索文章和页面…"
                        node_ref=input_ref
                        bind:value=query
                        on:keydown=on_keydown.clone()
                    />
                    <div class="search-modal-results">
                        <SearchResults results=results debounced=debounced active=active open=open/>
                    </div>
                    <div class="search-modal-footer">
                        <span>"↑↓ 选择 · Enter 打开 · Esc 关闭"</span>
                        {move || {
                            let q = debounced.get();
                            (!q.trim().is_empty()).then(|| {
                                let href = format!("/search?q={}", url_encode(&q));
                                view! {
                                    <A href=href on:click=move |_| open.set(false)>"查看全部结果"</A>
                                }
                            })
                        }}
                    </div>
                </div>
            </div>
        </Show>
    }
}
