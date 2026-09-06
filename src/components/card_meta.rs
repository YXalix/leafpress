use leptos::prelude::*;

use crate::types::PostMeta;

/// Meta row shared by cards and detail views: date + category + tags
#[component]
pub fn CardMeta(meta: PostMeta) -> impl IntoView {
    view! {
        <div class="card-meta">
            {meta.date.unwrap_or_default()}
            {meta.category.map(|c| view! { <span class="tag">{c}</span> })}
            {meta.tags.into_iter().map(|t| view! { <span class="tag">{t}</span> }).collect_view()}
        </div>
    }
}
