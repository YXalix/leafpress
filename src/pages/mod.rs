pub mod admin;
pub mod archive;
pub mod home;
pub mod not_found;
pub mod page_view;
pub mod post_detail;
pub mod post_list;
pub mod search;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

/// The "slug" route param as a reactive getter, shared by the post/page detail routes
pub fn slug_param() -> impl Fn() -> String {
    let params = use_params_map();
    move || {
        params
            .read()
            .get("slug")
            .map(|s| s.to_string())
            .unwrap_or_default()
    }
}
