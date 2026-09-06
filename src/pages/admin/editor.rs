//! Editor: create/edit posts and pages (slug + raw markdown + preview).

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

use super::{login_prompt, params_kind_slug, readonly_signal};
use crate::api::{admin_get_raw, admin_save, preview_markdown};
use crate::components::comment::AdminCommentList;

fn new_template(kind: &str) -> String {
    if kind == "page" {
        "---\ntitle: \"新页面\"\nstatus: published\n---\n\n正文……\n".to_string()
    } else {
        "---\ntitle: \"新文章\"\ndate: \"2026-01-01 12:00\"\ntags: []\ncategory: \"\"\nstatus: draft\n---\n\n正文……\n".to_string()
    }
}

/// Editor shared by create/update: slug + raw markdown (with front matter) + preview
#[component]
fn Editor(
    kind: String,
    old_slug: Option<String>,
    initial_raw: String,
    initial_slug: String,
) -> impl IntoView {
    let slug_ref = NodeRef::<leptos::html::Input>::new();
    let raw_ref = NodeRef::<leptos::html::Textarea>::new();
    let message = RwSignal::new(None::<String>);
    let preview = RwSignal::new(None::<String>);
    let is_page = kind == "page";
    let ro = readonly_signal();
    let navigate = use_navigate();

    let comment_slug = initial_slug.clone();
    let on_save = {
        let kind = kind.clone();
        let old_slug = old_slug.clone();
        move |_| {
            let kind = kind.clone();
            let old_slug = old_slug.clone();
            let slug = slug_ref
                .get()
                .map(|i| i.value().trim().to_string())
                .unwrap_or_default();
            let raw = raw_ref.get().map(|t| t.value()).unwrap_or_default();
            if slug.is_empty() {
                message.set(Some("slug 不能为空".into()));
                return;
            }
            let navigate = navigate.clone();
            leptos::task::spawn_local(async move {
                match admin_save(kind, old_slug, slug, raw).await {
                    Ok(()) => navigate("/admin", Default::default()),
                    Err(e) => message.set(Some(e.to_string())),
                }
            });
        }
    };

    let on_preview = move |_| {
        let raw = raw_ref.get().map(|t| t.value()).unwrap_or_default();
        leptos::task::spawn_local(async move {
            match preview_markdown(raw).await {
                Ok(html) => preview.set(Some(html)),
                Err(e) => message.set(Some(e.to_string())),
            }
        });
    };

    let on_close_preview = move |_| preview.set(None);

    view! {
        <div class="editor admin-wide">
            <div class="editor-toolbar">
                <label>
                    "slug（文件名）"
                    <input node_ref=slug_ref type="text" value=initial_slug/>
                </label>
                <div class="admin-actions">
                    {move || ro.get().then(|| view! { <span class="badge">"只读模式"</span> })}
                    <button class="button-secondary" on:click=on_preview>"预览"</button>
                    <button class="button" disabled=move || ro.get() on:click=on_save>"保存"</button>
                </div>
            </div>
            <textarea node_ref=raw_ref class="editor-area" spellcheck="false">
                {initial_raw}
            </textarea>
            {move || message.get().map(|m| view! { <p class="error">{m}</p> })}
            {move || preview.get().map(|html| view! {
                <div class="preview">
                    <div class="editor-toolbar">
                        <h3>"预览"</h3>
                        <button class="button-secondary" on:click=on_close_preview>"关闭"</button>
                    </div>
                    <div class="markdown-body" inner_html=html></div>
                </div>
            })}
            {(!is_page).then(|| view! { <AdminCommentList slug=comment_slug/> })}
        </div>
    }
}

#[component]
pub fn AdminNew() -> impl IntoView {
    let (kind, _) = params_kind_slug();
    view! {
        <h1>{move || if kind() == "page" { "新建页面" } else { "新建文章" }}</h1>
        <Editor kind=kind() old_slug=None initial_raw=new_template(&kind()) initial_slug="new-post".to_string()/>
    }
}

#[component]
pub fn AdminEdit() -> impl IntoView {
    let (kind, slug) = params_kind_slug();
    let raw = Resource::new(
        move || (kind(), slug()),
        |(kind, slug)| async move { admin_get_raw(kind, slug).await },
    );
    view! {
        <h1>"编辑"</h1>
        <Suspense fallback=|| view! { <p class="muted">"加载中…"</p> }>
            {move || {
                raw.get().map(|res| match res {
                    Ok(content) => {
                        let slug = slug();
                        let old_slug = if slug == "new-post" { None } else { Some(slug.clone()) };
                        view! { <Editor kind=kind() old_slug=old_slug initial_raw=content initial_slug=slug/> }.into_any()
                    }
                    Err(e) => login_prompt(e).into_any(),
                })
            }}
        </Suspense>
    }
}
