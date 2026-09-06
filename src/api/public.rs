//! Public-facing API: site info, post/page reads, full-text search.

use leptos::prelude::*;

use crate::types::{PostMeta, PostView, SearchResult};

#[cfg(feature = "ssr")]
use crate::state::AppState;
#[cfg(feature = "ssr")]
use crate::util::search_terms;

#[server]
pub async fn site_info() -> Result<String, ServerFnError> {
    let state = expect_context::<AppState>();
    Ok(state.config.site_name.clone())
}

#[server]
pub async fn list_posts() -> Result<Vec<PostMeta>, ServerFnError> {
    let state = expect_context::<AppState>();
    let index = state.index.read();
    Ok(index
        .posts
        .iter()
        .filter(|p| p.meta.is_published())
        .map(|p| p.meta.clone())
        .collect())
}

#[server]
pub async fn get_post(slug: String) -> Result<Option<PostView>, ServerFnError> {
    let state = expect_context::<AppState>();
    let index = state.index.read();
    Ok(index
        .posts
        .iter()
        .find(|p| p.meta.slug == slug && p.meta.is_published())
        .map(|p| PostView {
            meta: p.meta.clone(),
            html: p.html.clone(),
            toc: p.toc.clone(),
        }))
}

#[server]
pub async fn get_page(slug: String) -> Result<Option<PostView>, ServerFnError> {
    let state = expect_context::<AppState>();
    let index = state.index.read();
        // hidden pages are excluded from listings but remain directly accessible via URL (e.g. resume)
    Ok(index
        .pages
        .iter()
        .find(|p| p.meta.slug == slug && !p.meta.is_draft())
        .map(|p| PostView {
            meta: p.meta.clone(),
            html: p.html.clone(),
            toc: p.toc.clone(),
        }))
}

/// Builds a snippet of CONTEXT chars before/after the first body hit; falls back to the opening when the body has no hit
#[cfg(feature = "ssr")]
fn make_snippet(text: &str, terms: &[String]) -> String {
    const CONTEXT: usize = 60;
    let total = text.chars().count();
    let hit = terms
        .iter()
        .filter_map(|t| text.to_lowercase().find(t.as_str()))
        .min()
        .and_then(|byte| text.get(..byte).map(|s| s.chars().count()));
    let (start, end) = match hit {
        Some(pos) => (pos.saturating_sub(CONTEXT), (pos + CONTEXT).min(total)),
        None => (0, (2 * CONTEXT).min(total)),
    };
    let mut s = String::new();
    if start > 0 {
        s.push('…');
    }
    s.extend(text.chars().skip(start).take(end - start));
    if end < total {
        s.push('…');
    }
    s
}

/// Full-text search: multi-term AND, case-insensitive; scope = published posts + non-draft pages (same visibility as get_page)
#[server]
pub async fn search(query: String) -> Result<Vec<SearchResult>, ServerFnError> {
    let terms = search_terms(&query);
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let state = expect_context::<AppState>();
    let index = state.index.read();
    let docs = index
        .posts
        .iter()
        .filter(|p| p.meta.is_published())
        .chain(index.pages.iter().filter(|p| !p.meta.is_draft()));
    let mut scored: Vec<(i64, SearchResult)> = Vec::new();
    for post in docs {
        let title = post.meta.title.to_lowercase();
        let tags = post.meta.tags.join(" ").to_lowercase();
        let category = post
            .meta
            .category
            .clone()
            .unwrap_or_default()
            .to_lowercase();
        let text = post.text.to_lowercase();
        if !terms.iter().all(|t| {
            title.contains(t) || tags.contains(t) || category.contains(t) || text.contains(t)
        }) {
            continue;
        }
        let mut score = 0i64;
        for t in &terms {
            if title.contains(t) {
                score += 100;
            }
            if tags.contains(t) || category.contains(t) {
                score += 20;
            }
            score += text.matches(t.as_str()).count() as i64;
        }
        scored.push((
            score,
            SearchResult {
                meta: post.meta.clone(),
                snippet: make_snippet(&post.text, &terms),
            },
        ));
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(scored.into_iter().map(|(_, r)| r).collect())
}
