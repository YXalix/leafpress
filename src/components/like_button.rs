use leptos::prelude::*;

use crate::api::{get_likes, toggle_like};

#[component]
pub fn LikeButton(slug: String) -> impl IntoView {
    // Local state overrides the server value; refreshes instantly after a click
    let local = RwSignal::new(None::<(i64, bool)>);
    let remote = Resource::new(
        {
            let slug = slug.clone();
            move || slug.clone()
        },
        |slug| async move { get_likes(slug).await.ok() },
    );

    let on_click = {
        let slug = slug.clone();
        move |_| {
            let slug = slug.clone();
            leptos::task::spawn_local(async move {
                if let Ok(state) = toggle_like(slug).await {
                    local.set(Some(state));
                }
            });
        }
    };

    view! {
        <div class="like-button">
            <button on:click=on_click>
                {move || {
                    let state = local.get().or_else(|| remote.get().flatten());
                    match state {
                        Some((count, liked)) => {
                            if liked { format!("❤️ 已赞 {count}") } else { format!("🤍 点赞 {count}") }
                        }
                        None => "🤍 点赞".to_string(),
                    }
                }}
            </button>
        </div>
    }
}
