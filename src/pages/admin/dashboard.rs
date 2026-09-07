//! Content management list: filters + table + row actions.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use super::{login_prompt, readonly_signal, GitPanel};
use crate::api::{admin_delete, admin_list_all, admin_logout};
use crate::types::{ContentKind, PostMeta};

/// Status badge label and style
fn status_badge(status: &str) -> (String, &'static str) {
    match status {
        "published" => ("已发布".into(), "badge badge-published"),
        "draft" => ("草稿".into(), "badge badge-draft"),
        "hidden" => ("隐藏".into(), "badge"),
        other => (other.to_string(), "badge"),
    }
}

/// List filtering: kind + status + keywords (title/slug/tags/category)
fn matches_filter(m: &PostMeta, kind: &str, status: &str, terms: &[&str]) -> bool {
    if kind != "all" && m.kind.as_str() != kind {
        return false;
    }
    if status != "all" && m.status != status {
        return false;
    }
    if terms.is_empty() {
        return true;
    }
    let haystack = format!(
        "{} {} {} {}",
        m.title.to_lowercase(),
        m.slug.to_lowercase(),
        m.tags.join(" ").to_lowercase(),
        m.category.clone().unwrap_or_default().to_lowercase(),
    );
    terms.iter().all(|t| haystack.contains(t))
}

/// Chip filter bar: label + options; the selected value is written back to value
#[component]
fn ChipBar(
    label: &'static str,
    options: Vec<(&'static str, &'static str)>,
    value: RwSignal<&'static str>,
) -> impl IntoView {
    view! {
        <div class="filter-bar">
            <span class="filter-label">{label}</span>
            {options
                .into_iter()
                .map(|(v, text)| view! {
                    <button
                        class="chip"
                        class:active=move || value.get() == v
                        on:click=move |_| value.set(v)
                    >
                        {text}
                    </button>
                })
                .collect_view()}
        </div>
    }
}

/// Table row: title/kind/status/date + edit/delete (read-only mode shows a hint only)
#[component]
fn AdminRow(meta: PostMeta, ro: Memo<bool>, refresh: RwSignal<u32>) -> impl IntoView {
    let kind = meta.kind.as_str().to_string();
    let slug = meta.slug.clone();
    let edit_href = format!("/admin/edit/{kind}/{slug}");
    let kind_label = if meta.kind == ContentKind::Page {
        "页面"
    } else {
        "文章"
    };
    let (status_label, status_class) = status_badge(&meta.status);
    let date = meta
        .date
        .clone()
        .unwrap_or_default()
        .chars()
        .take(10)
        .collect::<String>();
    view! {
        <tr>
            <td>{meta.title.clone()}</td>
            <td>{kind_label}</td>
            <td><span class=status_class>{status_label}</span></td>
            <td class="muted col-date">{date}</td>
            <td>
                <div class="admin-row-actions">
                    {move || if ro.get() {
                        view! { <span class="muted">"只读"</span> }.into_any()
                    } else {
                        let edit_href = edit_href.clone();
                        let kind = kind.clone();
                        let slug = slug.clone();
                        view! {
                            <A href=edit_href attr:class="link">"编辑"</A>
                            <button
                                class="link-danger"
                                on:click=move |_| {
                                    let kind = kind.clone();
                                    let slug = slug.clone();
                                    leptos::task::spawn_local(async move {
                                        if admin_delete(kind, slug).await.is_ok() {
                                            refresh.update(|n| *n += 1);
                                        }
                                    });
                                }
                            >
                                "删除"
                            </button>
                        }.into_any()
                    }}
                </div>
            </td>
        </tr>
    }
}

#[component]
pub fn AdminDashboard() -> impl IntoView {
    let refresh = RwSignal::new(0u32);
    let all = Resource::new(
        move || refresh.get(),
        |_| async move { admin_list_all().await },
    );
    let ro = readonly_signal();
    let keyword = RwSignal::new(String::new());
    let kind_filter = RwSignal::new("all");
    let status_filter = RwSignal::new("all");
    let navigate = use_navigate();

    let on_logout = move |_| {
        let navigate = navigate.clone();
        leptos::task::spawn_local(async move {
            let _ = admin_logout().await;
            navigate("/", Default::default());
        });
    };

    view! {
        <div class="admin-wide">
        <div class="admin-head">
            <h1>"内容管理"</h1>
            <div class="admin-actions">
                {move || if ro.get() {
                    view! { <span class="badge">"只读模式 · 内容仅通过 git 同步"</span> }.into_any()
                } else {
                    view! {
                        <A href="/admin/new/post" attr:class="button">"+ 新文章"</A>
                        <A href="/admin/new/page" attr:class="button">"+ 新页面"</A>
                    }.into_any()
                }}
                <button class="button-secondary" on:click=on_logout>"退出登录"</button>
            </div>
        </div>
        <GitPanel/>
        <div class="admin-filter">
            <input
                class="admin-search"
                type="search"
                placeholder="搜索标题 / slug / 标签 / 分类…"
                on:input=move |ev| keyword.set(event_target_value(&ev))
            />
            <ChipBar
                label="类型"
                options=vec![("all", "全部"), ("post", "文章"), ("page", "页面")]
                value=kind_filter
            />
            <ChipBar
                label="状态"
                options=vec![("all", "全部"), ("published", "已发布"), ("draft", "草稿"), ("hidden", "隐藏")]
                value=status_filter
            />
        </div>
        <Suspense fallback=|| view! { <p class="muted">"加载中…"</p> }>
            {move || {
                all.get().map(|res| match res {
                    Ok(list) => {
                        let keyword = keyword.get().to_lowercase();
                        let terms: Vec<&str> = keyword.split_whitespace().collect();
                        let kind = kind_filter.get();
                        let status = status_filter.get();
                        let total = list.len();
                        let filtered: Vec<PostMeta> = list
                            .into_iter()
                            .filter(|m| matches_filter(m, kind, status, &terms))
                            .collect();
                        let shown = filtered.len();
                        if filtered.is_empty() {
                            view! { <p class="muted">"没有匹配的内容"</p> }.into_any()
                        } else {
                            view! {
                                <p class="admin-count muted">{format!("显示 {shown} / {total} 条")}</p>
                                <div class="admin-table-wrap">
                                    <table class="admin-table">
                                        <thead>
                                            <tr><th>"标题"</th><th>"类型"</th><th>"状态"</th><th>"日期"</th><th>"操作"</th></tr>
                                        </thead>
                                        <tbody>
                                            {filtered
                                                .into_iter()
                                                .map(|m| view! { <AdminRow meta=m ro=ro refresh=refresh/> })
                                                .collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                            }.into_any()
                        }
                    }
                    Err(e) => login_prompt(e).into_any(),
                })
            }}
        </Suspense>
        </div>
    }
}
