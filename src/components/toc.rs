use std::rc::Rc;

use leptos::ev;
use leptos::prelude::*;

use crate::types::TocItem;

/// Marks the last heading above the viewport-top line (56px sticky header + margin)
/// as the current section; at the page bottom, highlight the last item directly
/// since its heading may still sit below the line.
fn compute_active(ids: &[String], active: RwSignal<String>) {
    let document = document();
    let line = 80.0;
    let mut current = String::new();
    for id in ids {
        let Some(el) = document.get_element_by_id(id) else {
            continue;
        };
        if el.get_bounding_client_rect().top() <= line {
            current.clone_from(id);
        } else {
            break;
        }
    }
    if let (Some(root), Some(last)) = (document.document_element(), ids.last()) {
        let bottom = window().scroll_y().unwrap_or(0.0)
            + window()
                .inner_height()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
        if bottom >= f64::from(root.scroll_height()) - 2.0 {
            current.clone_from(last);
        }
    }
    active.set(current);
}

/// Post table of contents: floats beside the content on wide screens, highlights the current section while scrolling (scroll-spy)
#[component]
pub fn Toc(items: Vec<TocItem>) -> impl IntoView {
    let active = RwSignal::new(String::new());
    let ids = Rc::new(items.iter().map(|i| i.id.clone()).collect::<Vec<_>>());

    // Initial highlight: the effect runs once the browser DOM is ready; skipped during SSR since there is no window
    Effect::new({
        let ids = ids.clone();
        move || {
            if !is_server() {
                compute_active(&ids, active);
            }
        }
    });

    let handle = window_event_listener(ev::scroll, move |_| compute_active(&ids, active));
    on_cleanup(move || handle.remove());

    view! {
        <nav class="toc" aria-label="目录">
            <p class="toc-title">"目录"</p>
            <ul>
                {items
                    .into_iter()
                    .map(|item| {
                        let id = item.id.clone();
                        let id_for_class = item.id.clone();
                        view! {
                            <li class=format!("toc-item toc-l{}", item.level)>
                                <a href=format!("#{id}") class:active=move || active.get() == id_for_class>
                                    {item.text}
                                </a>
                            </li>
                        }
                    })
                    .collect_view()}
            </ul>
        </nav>
    }
}
