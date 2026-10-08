//! Public file-sharing page: a flat, newest-first listing of files_dir with direct
//! download links (/f/<name>). No auth by design — everything in files_dir is public.

use leptos::prelude::*;

use crate::api::list_shared_files;
use crate::util::url_encode;

/// 856 B / 1.4 MB / 2.0 GB — binary units matching most file managers
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

#[component]
pub fn FilesPage() -> impl IntoView {
    let files = Resource::new(|| (), |_| async move { list_shared_files().await });
    view! {
        <div class="files-page">
            <h1>"文件"</h1>
            <p class="muted">"公开文件分发目录：点击文件名下载；文本文件可点「查看」在新标签页浏览和复制。"</p>
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
                                            let href = format!("/f/{}", url_encode(&f.name));
                                            let view_href = format!("{}?view=1", href);
                                            view! {
                                                <li class="files-item">
                                                    // download attr: fetch in place, no navigation
                                                    <a class="files-name" href=href download=f.name.clone()>{f.name.clone()}</a>
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
                                                    <span class="files-meta">{human_size(f.size)}</span>
                                                    <span class="files-meta">{f.modified.clone()}</span>
                                                </li>
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
