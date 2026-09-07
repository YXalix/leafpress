use leptos::prelude::*;

use crate::api::{add_comment, get_comments};
use crate::types::Comment;

/// Single comment: avatar initial + author/date + body
#[component]
fn CommentItem(comment: Comment) -> impl IntoView {
    let initial: String = comment
        .author
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .collect();
    view! {
        <div class="comment">
            <div class="comment-avatar" aria-hidden="true">
                {initial}
            </div>
            <div class="comment-main">
                <div class="comment-head">
                    <span class="comment-author">{comment.author}</span>
                    <span class="muted">{comment.created_at}</span>
                </div>
                <p class="comment-body">{comment.content}</p>
            </div>
        </div>
    }
}

#[component]
pub fn CommentSection(slug: String) -> impl IntoView {
    let refresh = RwSignal::new(0u32);
    let comments = Resource::new(
        {
            let slug = slug.clone();
            move || (slug.clone(), refresh.get())
        },
        |(slug, _)| async move { get_comments(slug).await.unwrap_or_default() },
    );

    let author_ref = NodeRef::<leptos::html::Input>::new();
    let content_ref = NodeRef::<leptos::html::Textarea>::new();
    let error = RwSignal::new(None::<String>);

    let on_submit = {
        let slug = slug.clone();
        move |ev: leptos::ev::SubmitEvent| {
            ev.prevent_default();
            let author = author_ref.get().map(|i| i.value()).unwrap_or_default();
            let content = content_ref.get().map(|t| t.value()).unwrap_or_default();
            let slug = slug.clone();
            leptos::task::spawn_local(async move {
                match add_comment(slug, author, content).await {
                    Ok(()) => {
                        if let Some(t) = content_ref.get() {
                            t.set_value("");
                        }
                        error.set(None);
                        refresh.update(|n| *n += 1);
                    }
                    Err(e) => error.set(Some(e.to_string())),
                }
            });
        }
    };

    view! {
        <section class="comments">
            <h3>"评论"</h3>
            <Suspense fallback=|| view! { <p class="muted">"加载中…"</p> }>
                {move || {
                    comments.get().map(|list| {
                        if list.is_empty() {
                            view! { <p class="muted">"还没有评论，来抢沙发～"</p> }.into_any()
                        } else {
                            list.into_iter()
                                .map(|c| view! { <CommentItem comment=c/> })
                                .collect_view()
                                .into_any()
                        }
                    })
                }}
            </Suspense>
            <form class="comment-form" on:submit=on_submit>
                <textarea node_ref=content_ref placeholder="写下你的评论…" rows="1" required></textarea>
                <div class="comment-form-extra">
                    <input node_ref=author_ref type="text" placeholder="昵称（可留空）" maxlength="32"/>
                    <button type="submit">"发表"</button>
                </div>
                {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
            </form>
        </section>
    }
}
