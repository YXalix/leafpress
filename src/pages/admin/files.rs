//! Admin shared-files panel: the files_dir listing with per-file delete (confirm via
//! window.confirm). Reuses the /files page's list styles; deletion goes through the
//! admin session (server fn), not the curl upload token. Uploads stay CLI-side.

use leptos::prelude::*;

use super::login_prompt;
use crate::api::{admin_delete_shared_file, admin_list_shared_files};
use crate::util::{human_size, url_encode};

#[component]
pub fn AdminFiles() -> impl IntoView {
    let refresh = RwSignal::new(0u32);
    let files = Resource::new(
        move || refresh.get(),
        |_| async move { admin_list_shared_files().await },
    );
    let busy = RwSignal::new(false);
    // (is_error, text) from the last delete
    let feedback = RwSignal::new(None::<(bool, String)>);

    let on_delete = move |name: String| {
        let confirmed = window()
            .confirm_with_message(&format!("确认删除 {name}？分享链接会随之失效，且不可恢复。"))
            .unwrap_or(false);
        if !confirmed {
            return;
        }
        busy.set(true);
        feedback.set(None);
        leptos::task::spawn_local(async move {
            let result = admin_delete_shared_file(name).await;
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
            <p class="muted">"文件分发目录的管理入口：删除立即生效。上传仍走 curl + token，列表与 /files 公开页一致。"</p>
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
                                            let href = format!("/f/{}", url_encode(&f.name));
                                            let view_href = format!("{href}?view=1");
                                            let delete_name = f.name.clone();
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
                                                    <button
                                                        class="button-secondary files-del"
                                                        title=format!("删除 {}", f.name)
                                                        disabled=move || busy.get()
                                                        on:click=move |_| on_delete(delete_name.clone())
                                                    >
                                                        "删除"
                                                    </button>
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
            {move || {
                feedback.get().map(|(is_err, text)| {
                    let class = if is_err { "git-log error" } else { "git-log" };
                    view! { <pre class=class>{text}</pre> }
                })
            }}
        </div>
    }
}
