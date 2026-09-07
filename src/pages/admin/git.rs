//! Git panel for the content repo: branch/ahead/behind status, diff viewer, pull, commit+push.
//! Shown on the admin dashboard; all operations reuse the admin session (no separate auth).

use leptos::prelude::*;

use crate::api::{admin_git_commit_push, admin_git_diff, admin_git_pull, admin_git_status};

#[component]
pub fn GitPanel() -> impl IntoView {
    let refresh = RwSignal::new(0u32);
    let status = Resource::new(
        move || refresh.get(),
        |_| async move { admin_git_status().await },
    );
    let diff = RwSignal::new(None::<String>);
    let message = RwSignal::new(String::new());
    let pending = RwSignal::new(false);
    // (is_error, text) from the last pull/push
    let feedback = RwSignal::new(None::<(bool, String)>);

    let run_op = move |op: crate::types::GitOp| {
        pending.set(true);
        feedback.set(None);
        let msg = message.get();
        leptos::task::spawn_local(async move {
            let result = match op {
                crate::types::GitOp::Pull => admin_git_pull().await,
                crate::types::GitOp::Push => admin_git_commit_push(msg).await,
            };
            feedback.set(Some(match result {
                Ok(out) => (false, out),
                Err(e) => (true, e.to_string()),
            }));
            pending.set(false);
            message.set(String::new());
            refresh.update(|n| *n += 1);
        });
    };

    let on_pull = move |_| run_op(crate::types::GitOp::Pull);
    let on_push = move |_| run_op(crate::types::GitOp::Push);

    let toggle_diff = move |_| {
        if diff.get().is_some() {
            diff.set(None);
            return;
        }
        diff.set(Some("加载中…".into()));
        leptos::task::spawn_local(async move {
            diff.set(Some(match admin_git_diff().await {
                Ok(d) => d,
                Err(e) => format!("diff 失败: {e}"),
            }));
        });
    };

    view! {
        <div class="git-panel">
            <Suspense fallback=|| view! { <p class="muted">"Git 状态加载中…"</p> }>
                {move || {
                    status.get().map(|res| match res {
                        Err(e) => view! { <p class="muted">"Git：" {e.to_string()}</p> }.into_any(),
                        Ok(st) => {
                            let dirty_count = st.dirty.len();
                            let sync_text = if st.pull_interval_secs > 0 {
                                format!("每 {}s 自动拉取", st.pull_interval_secs)
                            } else {
                                "自动拉取已关闭".to_string()
                            };
                            view! {
                                <div class="git-panel-head">
                                    <span class="badge">{st.branch.clone()}</span>
                                    {(st.ahead > 0).then(|| view! { <span class="badge">{format!("领先 {}", st.ahead)}</span> })}
                                    {(st.behind > 0).then(|| view! { <span class="badge badge-draft">{format!("落后 {}", st.behind)}</span> })}
                                    <span class="muted">{st.last_commit.clone()}</span>
                                    <span class="muted git-panel-tail">{sync_text}</span>
                                </div>
                                <div class="git-panel-actions">
                                    <button
                                        class="button-secondary"
                                        disabled=move || pending.get()
                                        on:click=on_pull
                                    >
                                        "拉取 (pull)"
                                    </button>
                                    <button class="button-secondary" on:click=toggle_diff>
                                        {move || if diff.get().is_some() { "收起 diff" } else { "查看 diff" }}
                                    </button>
                                    {(dirty_count > 0)
                                        .then(|| {
                                            view! {
                                                <input
                                                    class="git-msg"
                                                    type="text"
                                                    placeholder="提交信息（留空自动生成）"
                                                    prop:value=move || message.get()
                                                    on:input=move |ev| message.set(event_target_value(&ev))
                                                />
                                                <button
                                                    disabled=move || pending.get()
                                                    on:click=on_push
                                                >
                                                    {format!("提交并推送 ({dirty_count})")}
                                                </button>
                                            }
                                        })}
                                </div>
                                {(dirty_count > 0)
                                    .then(|| {
                                        let items = st.dirty.join("\n");
                                        view! {
                                            <details class="git-dirty">
                                                <summary>{format!("{dirty_count} 个未提交文件")}</summary>
                                                <pre class="git-diff">{items}</pre>
                                            </details>
                                        }
                                    })}
                                {move || {
                                    feedback.get().map(|(is_err, text)| {
                                        let class = if is_err { "git-log error" } else { "git-log" };
                                        view! { <pre class=class>{text}</pre> }
                                    })
                                }}
                                {move || diff.get().map(|d| view! { <pre class="git-diff">{d}</pre> })}
                            }
                                .into_any()
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}
