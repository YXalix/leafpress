use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::posts_resource;
use crate::components::post_card::PostCard;

#[component]
pub fn Home() -> impl IntoView {
    let posts = posts_resource();
    view! {
        <section class="hero">
            <h1>"你好，我是 Nash 👋"</h1>
            <p class="muted">"这里记录我的技术学习与思考：论文阅读、Linux 内核、编译器与各种折腾。"</p>
        </section>
        <section>
            <div class="section-head">
                <h2>"最新文章"</h2>
                <A href="/posts" attr:class="muted">"全部文章 →"</A>
            </div>
            <Suspense fallback=|| view! { <p class="muted">"加载中…"</p> }>
                {move || {
                    posts.get().map(|res| match res {
                        Ok(mut list) => {
                            list.truncate(5);
                            list.into_iter().map(|m| view! { <PostCard meta=m/> }).collect_view().into_any()
                        }
                        Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                    })
                }}
            </Suspense>
        </section>
    }
}
