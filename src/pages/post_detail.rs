use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::api::get_post;
use crate::components::card_meta::CardMeta;
use crate::components::comment::CommentSection;
use crate::components::like_button::LikeButton;
use crate::components::toc::Toc;

#[component]
pub fn PostDetail() -> impl IntoView {
    let params = use_params_map();
    let slug = move || {
        params
            .read()
            .get("slug")
            .map(|s| s.to_string())
            .unwrap_or_default()
    };
    let post = Resource::new(slug, |slug| async move { get_post(slug).await });

    view! {
        <Suspense fallback=|| view! { <p class="muted">"加载中…"</p> }>
            {move || {
                post.get().map(|res| match res {
                    Ok(Some(post)) => {
                        let meta = post.meta.clone();
                        let title = meta.title.clone();
                        let slug = meta.slug.clone();
                        let toc = post.toc.clone();
                        view! {
                            <div class="post-layout">
                                <article class="post">
                                    <h1>{title}</h1>
                                    <CardMeta meta=meta/>
                                    <div class="markdown-body" inner_html=post.html></div>
                                    <LikeButton slug=slug.clone()/>
                                    <CommentSection slug=slug/>
                                </article>
                                {if toc.len() >= 2 {
                                    view! { <Toc items=toc/> }.into_any()
                                } else {
                                    ().into_any()
                                }}
                            </div>
                        }
                            .into_any()
                    }
                    Ok(None) => view! { <p class="muted">"文章不存在或未发布"</p> }.into_any(),
                    Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                })
            }}
        </Suspense>
    }
}
