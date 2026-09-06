use leptos::prelude::*;

/// Light/dark theme toggle: writes <html data-theme> and persists it to localStorage
#[component]
pub fn ThemeToggle() -> impl IntoView {
    let on_click = move |_| {
        let Some(el) = document().document_element() else {
            return;
        };
        let cur = el.get_attribute("data-theme").unwrap_or_default();
        let next = if cur == "dark" { "light" } else { "dark" };
        let _ = el.set_attribute("data-theme", next);
        if let Ok(Some(storage)) = window().local_storage() {
            let _ = storage.set_item("theme", next);
        }
    };
    view! {
        <button class="theme-toggle" title="切换明暗主题" on:click=on_click>
            "◐"
        </button>
    }
}
