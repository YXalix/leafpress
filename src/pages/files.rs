//! Public file-sharing page: browsable files_dir listing (?dir=<rel path> selects the
//! subdirectory) with direct download links (/f/<rel path>). No auth by design —
//! everything in files_dir is public.

use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use crate::api::list_shared_files;
use crate::util::{human_size, url_encode, url_encode_path};

/// Breadcrumb over the current ?dir= value: ancestors are links, the current dir is
/// plain text. `base` is the page URL the ?dir= param attaches to (/files or /admin);
/// called inside a reactive closure so it tracks query changes
pub fn files_crumb(base: String, dir: String) -> impl IntoView {
    let mut items: Vec<(String, String)> = vec![("根目录".into(), String::new())];
    let mut acc = String::new();
    for seg in dir.split('/').filter(|s| !s.is_empty()) {
        if !acc.is_empty() {
            acc.push('/');
        }
        acc.push_str(seg);
        items.push((seg.to_string(), acc.clone()));
    }
    let base2 = base.clone();
    let last = items.len() - 1;
    view! {
        <nav class="files-crumb">
            {items
                .into_iter()
                .enumerate()
                .map(|(i, (label, rel))| {
                    let sep = (i > 0)
                        .then(|| view! { <span class="files-crumb-sep">"/"</span> }.into_any());
                    let inner = if i == last {
                        view! { <span class="files-crumb-here">{label}</span> }.into_any()
                    } else if rel.is_empty() {
                        view! { <a href=base2.clone()>{label}</a> }.into_any()
                    } else {
                        let href = format!("{}?dir={}", base, url_encode(&rel));
                        view! { <a href=href>{label}</a> }.into_any()
                    };
                    view! { <>{sep} {inner}</> }.into_any()
                })
                .collect_view()}
        </nav>
    }
}

#[component]
pub fn FilesPage() -> impl IntoView {
    let query = use_query_map();
    let dir = Memo::new(move |_| {
        query
            .get()
            .get("dir")
            .map(|d| d.to_string())
            .unwrap_or_default()
    });
    let files = Resource::new(
        move || dir.get(),
        |d: String| async move { list_shared_files(d).await },
    );
    view! {
        <div class="files-page">
            <h1>"文件"</h1>
            <p class="muted">
                "公开文件分发目录：点击文件名下载，点目录名进入子目录；文本文件可点「查看」在新标签页浏览和复制。"
            </p>
            {move || files_crumb("/files".into(), dir.get())}
            <Suspense fallback=|| view! { <p class="muted">"文件列表加载中…"</p> }>
                {move || {
                    files.get().map(|res| match res {
                        Err(e) => {
                            view! { <p class="error">{e.to_string()}</p> }.into_any()
                        }
                        Ok(list) if list.is_empty() => {
                            view! { <p class="muted">"目录为空"</p> }.into_any()
                        }
                        Ok(list) => {
                            view! {
                                <ul class="files-list">
                                    {list
                                        .into_iter()
                                        .map(|f| {
                                            if f.is_dir {
                                                let href =
                                                    format!("/files?dir={}", url_encode_path(&f.path));
                                                let label = format!("📁 {}", f.name);
                                                view! {
                                                    <li class="files-item">
                                                        <a class="files-name files-dir" href=href>
                                                            {label}
                                                        </a>
                                                        <span class="files-meta">"—"</span>
                                                        <span class="files-meta">
                                                            {f.modified.clone()}
                                                        </span>
                                                    </li>
                                                }
                                                    .into_any()
                                            } else {
                                                let href = format!("/f/{}", url_encode_path(&f.path));
                                                let view_href = format!("{}?view=1", href);
                                                view! {
                                                    <li class="files-item">
                                                        // download attr: fetch in place, no navigation
                                                        <a
                                                            class="files-name"
                                                            href=href
                                                            download=f.name.clone()
                                                        >
                                                            {f.name.clone()}
                                                        </a>
                                                        {f.is_text.then(|| {
                                                            view! {
                                                                <a
                                                                    class="files-view"
                                                                    href=view_href
                                                                    target="_blank"
                                                                    rel="noopener"
                                                                >
                                                                    "查看"
                                                                </a>
                                                            }
                                                        })}
                                                        <span class="files-meta">
                                                            {human_size(f.size)}
                                                        </span>
                                                        <span class="files-meta">
                                                            {f.modified.clone()}
                                                        </span>
                                                    </li>
                                                }
                                                    .into_any()
                                            }
                                        })
                                        .collect_view()}
                                </ul>
                            }
                                .into_any()
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}
