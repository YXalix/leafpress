use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::card_meta::CardMeta;
use crate::types::PostMeta;

#[component]
pub fn PostCard(meta: PostMeta) -> impl IntoView {
    let href = meta.href();
    let title = meta.title.clone();
    view! {
        <article class="card">
            <A href=href attr:class="card-title">
                {title}
            </A>
            <CardMeta meta=meta/>
        </article>
    }
}
