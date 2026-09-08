//! Admin git console: branch/ahead/behind status, auto-pull & proxy info, dual-pane diff
//! viewer with in-place editing, pull, commit+push. The whole /admin page is this console.

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

use super::login_prompt;
use crate::api::{
    admin_git_commit_push, admin_git_diff, admin_git_pull, admin_git_read_file,
    admin_git_save_file, admin_git_stage, admin_git_stage_all, admin_git_status, admin_git_unstage,
    admin_git_update_line, admin_logout,
};
use crate::types::{DiffCell, DiffLineKind, DiffRow, FileDiff, GitOp};

/// (text, changed) segments of one diff line, for intra-line highlighting
type Segments = Vec<(String, bool)>;

/// Splits a del/add line pair into (text, changed) segments via common prefix/suffix —
/// a cheap approximation of VSCode's intra-line word highlighting
fn inline_segments(old: &str, new: &str) -> (Segments, Segments) {
    let a: Vec<char> = old.chars().collect();
    let b: Vec<char> = new.chars().collect();
    let mut pre = 0;
    while pre < a.len() && pre < b.len() && a[pre] == b[pre] {
        pre += 1;
    }
    let mut suf = 0;
    while suf < a.len() - pre && suf < b.len() - pre && a[a.len() - 1 - suf] == b[b.len() - 1 - suf]
    {
        suf += 1;
    }
    fn build(v: &[char], pre: usize, suf: usize) -> Segments {
        let mut out = Vec::new();
        if pre > 0 {
            out.push((v[..pre].iter().collect(), false));
        }
        if v.len() > pre + suf {
            out.push((v[pre..v.len() - suf].iter().collect(), true));
        }
        if suf > 0 {
            out.push((v[v.len() - suf..].iter().collect(), false));
        }
        out
    }
    (build(&a, pre, suf), build(&b, pre, suf))
}

/// DOM selectionStart/End are UTF-16 offsets; convert one to a byte index (emoji-safe)
fn utf16_to_byte(s: &str, target: usize) -> usize {
    let mut units = 0;
    for (i, ch) in s.char_indices() {
        if units >= target {
            return i;
        }
        units += ch.len_utf16();
    }
    s.len()
}

/// Cell text with optional intra-line highlight segments (changed parts get .cell-hl)
fn cell_text_view(text: String, segs: Option<Segments>) -> impl IntoView {
    match segs {
        Some(segs) => segs
            .into_iter()
            .map(|(s, changed)| {
                if changed {
                    view! { <span class="cell-hl">{s}</span> }.into_any()
                } else {
                    s.into_any()
                }
            })
            .collect_view()
            .into_any(),
        None => text.into_any(),
    }
}

/// Diff-cell CSS class for a line kind
fn cell_class(kind: DiffLineKind) -> &'static str {
    match kind {
        DiffLineKind::Del => "diff-cell cell-del",
        DiffLineKind::Add => "diff-cell cell-add",
        DiffLineKind::Context => "diff-cell",
    }
}

/// One cell of a dual-pane diff row (line number + text, colored by kind).
/// `segs` carries intra-line highlight segments when the row pairs a deletion with an addition.
fn cell_view(cell: &Option<DiffCell>, segs: Option<Segments>) -> impl IntoView {
    match cell {
        Some(c) => {
            let class = cell_class(c.kind);
            let text = c.text.clone();
            view! {
                <div class=class>
                    <span class="cell-num">{c.no}</span>
                    <span class="cell-text">{cell_text_view(text, segs)}</span>
                </div>
            }
            .into_any()
        }
        None => view! { <div class="diff-cell cell-empty"></div> }.into_any(),
    }
}

/// One changed file: header (status chip + staged badge + path + stats + stage/edit buttons)
/// and dual-pane diff rows. Working-tree lines are click-to-edit inline (Enter/blur saves,
/// Esc cancels); the whole-file textarea editor remains available via 编辑全文.
#[component]
fn FileDiffView(
    file: FileDiff,
    refresh: RwSignal<u32>,
    feedback: RwSignal<Option<(bool, String)>>,
) -> impl IntoView {
    let FileDiff {
        path,
        status,
        staged,
        rows,
        truncated,
    } = file;
    // Collapsed by default — with many changed files, expanding every diff is unreadable
    let open = RwSignal::new(false);
    let editing = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let draft = RwSignal::new(String::new());

    let (chip_label, chip_class) = match status.as_str() {
        "added" => ("新增", "badge badge-published"),
        "deleted" => ("删除", "badge badge-draft"),
        _ => ("修改", "badge"),
    };
    let can_edit = status != "deleted";
    let mut stat_add = 0usize;
    let mut stat_del = 0usize;
    for row in &rows {
        if let DiffRow::Line { left, right } = row {
            if matches!(left, Some(c) if c.kind == DiffLineKind::Del) {
                stat_del += 1;
            }
            if matches!(right, Some(c) if c.kind == DiffLineKind::Add) {
                stat_add += 1;
            }
        }
    }

    let area_ref = NodeRef::<leptos::html::Textarea>::new();
    // Tab inserts two spaces at the caret instead of moving focus (markdown/YAML-safe)
    let on_keydown = move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() != "Tab" {
            return;
        }
        ev.prevent_default();
        let Some(area) = area_ref.get() else { return };
        let value = area.value();
        let start = utf16_to_byte(
            &value,
            area.selection_start().ok().flatten().unwrap_or(0) as usize,
        );
        let end = utf16_to_byte(
            &value,
            area.selection_end().ok().flatten().unwrap_or(0) as usize,
        );
        let mut next = value;
        next.replace_range(start..end, "  ");
        area.set_value(&next);
        let cursor = (start + 2) as u32;
        let _ = area.set_selection_range(cursor, cursor);
        draft.set(next);
    };

    let on_edit = {
        let path = path.clone();
        move |_| {
            if editing.get() {
                editing.set(false);
                return;
            }
            busy.set(true);
            let path = path.clone();
            leptos::task::spawn_local(async move {
                match admin_git_read_file(path).await {
                    Ok(content) => {
                        draft.set(content);
                        editing.set(true);
                        open.set(true);
                    }
                    Err(e) => feedback.set(Some((true, e.to_string()))),
                }
                busy.set(false);
            });
        }
    };

    let on_save = {
        let path = path.clone();
        move |_| {
            busy.set(true);
            let path = path.clone();
            leptos::task::spawn_local(async move {
                let result = admin_git_save_file(path, draft.get()).await;
                busy.set(false);
                match result {
                    Ok(()) => {
                        editing.set(false);
                        feedback.set(Some((false, "已保存到工作区（尚未提交）".into())));
                        refresh.update(|n| *n += 1);
                    }
                    Err(e) => feedback.set(Some((true, e.to_string()))),
                }
            });
        }
    };

    let on_cancel = move |_| editing.set(false);

    // Inline (per-line) editing in the working-tree pane — click a line, Enter/blur saves,
    // Esc cancels; no need to open the whole-file editor for one-line fixes
    let editing_line = RwSignal::new(None::<u32>);
    let edit_value = RwSignal::new(String::new());
    let edit_orig = RwSignal::new(String::new());
    let line_ref = NodeRef::<leptos::html::Input>::new();
    // Focus the inline input (caret at end) whenever a line enters edit mode
    Effect::new(move |_| {
        if editing_line.get().is_some() {
            if let Some(input) = line_ref.get() {
                let _ = input.focus();
                let n = input.value().chars().map(|c| c.len_utf16() as u32).sum();
                let _ = input.set_selection_range(n, n);
            }
        }
    });

    let save_line = {
        let path = path.clone();
        move |no: u32| {
            let content = edit_value.get();
            if content == edit_orig.get() {
                editing_line.set(None);
                return;
            }
            busy.set(true);
            let path = path.clone();
            leptos::task::spawn_local(async move {
                let result = admin_git_update_line(path, no, content).await;
                busy.set(false);
                match result {
                    Ok(()) => {
                        // Don't clobber a newer edit session opened while this save was in flight
                        if editing_line.get() == Some(no) {
                            editing_line.set(None);
                        }
                        feedback.set(Some((false, format!("已保存第 {no} 行（尚未暂存/提交）"))));
                        refresh.update(|n| *n += 1);
                    }
                    Err(e) => feedback.set(Some((true, e.to_string()))),
                }
            });
        }
    };

    let on_stage = {
        let path = path.clone();
        move |ev: leptos::ev::MouseEvent| {
            ev.prevent_default();
            busy.set(true);
            let path = path.clone();
            leptos::task::spawn_local(async move {
                let result = if staged {
                    admin_git_unstage(path).await
                } else {
                    admin_git_stage(path).await
                };
                busy.set(false);
                match result {
                    Ok(()) => {
                        let msg = if staged {
                            "已取消暂存"
                        } else {
                            "已暂存"
                        };
                        feedback.set(Some((false, msg.into())));
                        refresh.update(|n| *n += 1);
                    }
                    Err(e) => feedback.set(Some((true, e.to_string()))),
                }
            });
        }
    };

    view! {
        <details class="diff-file" open=move || open.get()>
            <summary class="diff-file-head">
                <span class=chip_class>{chip_label}</span>
                {staged.then(|| view! { <span class="badge badge-staged">"已暂存"</span> })}
                <span class="diff-path">{path.clone()}</span>
                <span class="diff-stat">
                    <span class="stat-add">{format!("+{stat_add}")}</span>
                    <span class="stat-del">{format!("−{stat_del}")}</span>
                </span>
                <button
                    class="button-secondary diff-edit-btn"
                    disabled=move || busy.get()
                    on:click=on_stage
                >
                    {if staged { "取消暂存" } else { "暂存" }}
                </button>
                {can_edit.then(|| {
                    view! {
                        <button
                            class="button-secondary diff-edit-btn"
                            disabled=move || busy.get()
                            on:click=move |ev| {
                                ev.prevent_default();
                                on_edit(ev);
                            }
                        >
                            {move || if editing.get() { "收起编辑" } else { "编辑全文" }}
                        </button>
                    }
                })}
            </summary>
            <div class="diff-scroll">
                <div class="diff-grid-head">
                    <span>"HEAD"</span>
                    <span>"工作区"</span>
                </div>
                <div class="diff-rows">
                    {rows
                        .iter()
                        .map(|row| match row {
                            DiffRow::Hunk(h) => {
                                view! { <div class="diff-hunk">{h.clone()}</div> }.into_any()
                            }
                            // Cells are direct grid children so the two columns align across rows
                            DiffRow::Line { left, right } => {
                                // Paired del/add lines get intra-line (word-level) highlights
                                let (lsegs, rsegs) = match (left, right) {
                                    (Some(l), Some(r))
                                        if l.kind == DiffLineKind::Del
                                            && r.kind == DiffLineKind::Add =>
                                    {
                                        let (l, r) = inline_segments(&l.text, &r.text);
                                        (Some(l), Some(r))
                                    }
                                    _ => (None, None),
                                };
                                let left_v = cell_view(left, lsegs).into_any();
                                let right_v = match right {
                                    // Working-tree lines are click-to-edit, VSCode-style
                                    Some(c) if can_edit => {
                                        let no = c.no;
                                        let text = c.text.clone();
                                        let class = cell_class(c.kind);
                                        let save_row = save_line.clone();
                                        view! {
                                            <div class=class>
                                                <span class="cell-num">{no}</span>
                                                {move || {
                                                    let save_key = save_row.clone();
                                                    let save_blur = save_row.clone();
                                                    if editing_line.get() == Some(no) {
                                                        view! {
                                                            <input
                                                                class="cell-input"
                                                                node_ref=line_ref
                                                                prop:value=move || edit_value.get()
                                                                on:input=move |ev| edit_value.set(event_target_value(&ev))
                                                                on:keydown=move |ev| match ev.key().as_str() {
                                                                    "Enter" => {
                                                                        ev.prevent_default();
                                                                        save_key(no);
                                                                    }
                                                                    "Escape" => editing_line.set(None),
                                                                    _ => {}
                                                                }
                                                                on:blur=move |_| {
                                                                    if editing_line.get() == Some(no) {
                                                                        save_blur(no);
                                                                    }
                                                                }
                                                            />
                                                        }
                                                            .into_any()
                                                    } else {
                                                        let click_text = text.clone();
                                                        view! {
                                                            <span
                                                                class="cell-text cell-editable"
                                                                title="点击编辑此行"
                                                                on:click=move |_| {
                                                                    edit_value.set(click_text.clone());
                                                                    edit_orig.set(click_text.clone());
                                                                    editing_line.set(Some(no));
                                                                }
                                                            >
                                                                {cell_text_view(text.clone(), rsegs.clone())}
                                                            </span>
                                                        }
                                                            .into_any()
                                                    }
                                                }}
                                            </div>
                                        }
                                            .into_any()
                                    }
                                    _ => cell_view(right, rsegs).into_any(),
                                };
                                (left_v, right_v).into_any()
                            }
                        })
                        .collect_view()}
                    {truncated.then(|| view! { <div class="diff-hunk">"…（diff 过长，已截断）"</div> })}
                </div>
            </div>
            {move || {
                editing.get().then(|| {
                    view! {
                        <div class="diff-editor">
                            <textarea
                                class="diff-edit-area"
                                spellcheck="false"
                                node_ref=area_ref
                                prop:value=move || draft.get()
                                on:input=move |ev| draft.set(event_target_value(&ev))
                                on:keydown=on_keydown
                            ></textarea>
                            <div class="diff-editor-actions">
                                <button disabled=move || busy.get() on:click=on_save.clone()>
                                    "保存到工作区"
                                </button>
                                <button class="button-secondary" on:click=on_cancel>
                                    "取消"
                                </button>
                                <span class="muted">"保存后可在上方查看最新 diff，确认无误再提交并推送"</span>
                            </div>
                        </div>
                    }
                })
            }}
        </details>
    }
}

#[component]
pub fn AdminGit() -> impl IntoView {
    let refresh = RwSignal::new(0u32);
    let status = Resource::new(
        move || refresh.get(),
        |_| async move { admin_git_status().await },
    );
    let diffs = Resource::new(
        move || refresh.get(),
        |_| async move { admin_git_diff().await },
    );
    let message = RwSignal::new(String::new());
    let pending = RwSignal::new(false);
    // (is_error, text) from the last pull/push/save
    let feedback = RwSignal::new(None::<(bool, String)>);
    let navigate = use_navigate();

    let run_op = move |op: GitOp| {
        pending.set(true);
        feedback.set(None);
        let msg = message.get();
        leptos::task::spawn_local(async move {
            let result = match op {
                GitOp::Pull => admin_git_pull().await,
                GitOp::Push => admin_git_commit_push(msg).await,
                GitOp::StageAll => admin_git_stage_all()
                    .await
                    .map(|()| "已暂存全部改动".to_string()),
            };
            feedback.set(Some(match result {
                Ok(out) => (false, out),
                Err(e) => (true, e.to_string()),
            }));
            pending.set(false);
            // pull/push consume the commit-message input; staging leaves it alone
            match op {
                GitOp::Pull | GitOp::Push => message.set(String::new()),
                GitOp::StageAll => {}
            }
            refresh.update(|n| *n += 1);
        });
    };

    let on_pull = move |_| run_op(GitOp::Pull);
    let on_push = move |_| run_op(GitOp::Push);
    let on_stage_all = move |_| run_op(GitOp::StageAll);

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
                <h1>"Git 同步"</h1>
                <div class="admin-actions">
                    <button class="button-secondary" on:click=on_logout>"退出登录"</button>
                </div>
            </div>
            <Suspense fallback=|| view! { <p class="muted">"Git 状态加载中…"</p> }>
                {move || {
                    status.get().map(|res| match res {
                        Err(e) => login_prompt(e).into_any(),
                        Ok(st) => {
                            let dirty_count = st.dirty.len();
                            let staged_count = st.staged as usize;
                            let sync_text = if st.pull_interval_secs > 0 {
                                format!("每 {}s 自动拉取", st.pull_interval_secs)
                            } else {
                                "自动拉取已关闭".to_string()
                            };
                            view! {
                                <div class="git-panel">
                                    <div class="git-panel-head">
                                        <span class="badge">{st.branch.clone()}</span>
                                        {(st.ahead > 0).then(|| view! { <span class="badge">{format!("领先 {}", st.ahead)}</span> })}
                                        {(st.behind > 0).then(|| view! { <span class="badge badge-draft">{format!("落后 {}", st.behind)}</span> })}
                                        <span class="muted">{st.last_commit.clone()}</span>
                                        <span class="muted git-panel-tail">{sync_text}</span>
                                        {st.proxy.clone().map(|p| view! { <span class="badge badge-published">{format!("代理 {p}")}</span> })}
                                    </div>
                                    {st.last_error.clone().map(|err| view! { <pre class="git-log error">{format!("最近同步失败：{err}")}</pre> })}
                                    <div class="git-panel-actions">
                                        <button
                                            class="button-secondary"
                                            disabled=move || pending.get()
                                            on:click=on_pull
                                        >
                                            "拉取 (pull)"
                                        </button>
                                        <button
                                            class="button-secondary"
                                            disabled=move || pending.get() || dirty_count == 0
                                            on:click=on_stage_all
                                        >
                                            "全部暂存"
                                        </button>
                                        <input
                                            class="git-msg"
                                            type="text"
                                            placeholder="提交信息（留空自动生成）"
                                            prop:value=move || message.get()
                                            on:input=move |ev| message.set(event_target_value(&ev))
                                        />
                                        <button
                                            disabled=move || pending.get() || staged_count == 0
                                            on:click=on_push
                                        >
                                            {format!("提交并推送 ({staged_count})")}
                                        </button>
                                    </div>
                                    {move || {
                                        feedback.get().map(|(is_err, text)| {
                                            let class = if is_err { "git-log error" } else { "git-log" };
                                            view! { <pre class=class>{text}</pre> }
                                        })
                                    }}
                                </div>
                            }
                                .into_any()
                        }
                    })
                }}
            </Suspense>
            <Suspense fallback=|| view! { <p class="muted">"Diff 加载中…"</p> }>
                {move || {
                    diffs.get().map(|res| match res {
                        Err(e) => view! { <p class="muted">"Diff：" {e.to_string()}</p> }.into_any(),
                        Ok(files) => {
                            if files.is_empty() {
                                view! { <p class="muted">"工作区干净，没有未提交的改动。"</p> }.into_any()
                            } else {
                                view! {
                                    <div class="diff-list">
                                        {files
                                            .into_iter()
                                            .map(|f| view! { <FileDiffView file=f refresh=refresh feedback=feedback/> })
                                            .collect_view()}
                                    </div>
                                }
                                    .into_any()
                            }
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}
