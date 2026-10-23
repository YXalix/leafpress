//! Admin shared-files panel: the files_dir listing (subdirectories browsable via
//! ?dir=) with per-file delete (confirm via window.confirm). Reuses the /files page's
//! list styles; deletion goes through the admin session (server fn), not the curl
//! upload token. Uploads stay CLI-side.

use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use super::login_prompt;
use crate::api::{admin_delete_shared_file, admin_list_shared_files};
use crate::pages::files::files_crumb;
use crate::util::{human_size, url_encode_path};

#[component]
pub fn AdminFiles() -> impl IntoView {
    let query = use_query_map();
    let dir = Memo::new(move |_| {
        query
            .get()
            .get("dir")
            .map(|d| d.to_string())
            .unwrap_or_default()
    });
    let refresh = RwSignal::new(0u32);
    let files = Resource::new(
        move || (dir.get(), refresh.get()),
        |(d, _)| async move { admin_list_shared_files(d).await },
    );
    let busy = RwSignal::new(false);
    // (is_error, text) from the last delete
    let feedback = RwSignal::new(None::<(bool, String)>);

    let on_delete = move |path: String| {
        let confirmed = window()
            .confirm_with_message(&format!(
                "确认删除 {path}？分享链接会随之失效，且不可恢复。"
            ))
            .unwrap_or(false);
        if !confirmed {
            return;
        }
        busy.set(true);
        feedback.set(None);
        leptos::task::spawn_local(async move {
            let result = admin_delete_shared_file(path).await;
            busy.set(false);
            match result {
                Ok(()) => {
                    feedback.set(Some((false, "已删除".into())));
                    refresh.update(|n| *n += 1);
                }
                Err(e) => feedback.set(Some((true, e.to_string()))),
            }
        });
    };

    view! {
        <div class="admin-section">
            <h2>"共享文件"</h2>
            <p class="muted">"文件分发目录的管理入口：删除立即生效，可进入子目录。上传仍走 curl + token，列表与 /files 公开页一致。"</p>
            {move || files_crumb("/admin".into(), dir.get())}
            <Suspense fallback=|| view! { <p class="muted">"文件列表加载中…"</p> }>
                {move || {
                    files.get().map(|res| match res {
                        Err(e) => login_prompt(e).into_any(),
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
                                                    format!("/admin?dir={}", url_encode_path(&f.path));
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
                                                let view_href = format!("{href}?view=1");
                                                let delete_path = f.path.clone();
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
                                                        <button
                                                            class="button-secondary files-del"
                                                            title=format!("删除 {}", f.path)
                                                            disabled=move || busy.get()
                                                            on:click=move |_| {
                                                                on_delete(delete_path.clone())
                                                            }
                                                        >
                                                            "删除"
                                                        </button>
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
            {move || {
                feedback.get().map(|(is_err, text)| {
                    let class = if is_err { "git-log error" } else { "git-log" };
                    view! { <pre class=class>{text}</pre> }
                })
            }}
        </div>
    }
}
