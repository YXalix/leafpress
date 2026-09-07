//! Small utilities shared by frontend and admin: search tokenization/highlighting,
//! URL encoding, and input debouncing.
//! Compiled for both SSR and hydrate (Effects don't run on the server).

use leptos::prelude::*;

/// Search tokenization: split on whitespace + lowercase. Client-side highlighting must
/// stay consistent with server-side matching
pub fn search_terms(query: &str) -> Vec<String> {
    query.split_whitespace().map(|t| t.to_lowercase()).collect()
}

pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Wrap fragments of text matching terms in <mark> (case-insensitive; terms must be lowercased)
pub fn highlight(text: &str, terms: &[String]) -> Vec<AnyView> {
    let lower = text.to_lowercase();
    let mut views: Vec<AnyView> = Vec::new();
    let mut pos = 0usize;
    while pos < lower.len() {
        // Find the nearest hit of any term from the current position
        let mut nearest: Option<(usize, usize)> = None;
        for t in terms {
            if let Some(rel) = lower[pos..].find(t.as_str()) {
                let start = pos + rel;
                let end = start + t.len();
                if nearest.is_none_or(|(s, _)| start < s) {
                    nearest = Some((start, end));
                }
            }
        }
        match nearest {
            Some((start, end)) => match (text.get(pos..start), text.get(start..end)) {
                (Some(plain), Some(hit)) => {
                    if !plain.is_empty() {
                        views.push(plain.to_string().into_any());
                    }
                    views.push(view! { <mark>{hit.to_string()}</mark> }.into_any());
                    pos = end;
                }
                // If case folding misaligns byte offsets, skip highlighting and emit the rest as-is
                _ => {
                    views.push(text[pos..].to_string().into_any());
                    break;
                }
            },
            None => {
                if let Some(rest) = text.get(pos..) {
                    views.push(rest.to_string().into_any());
                }
                break;
            }
        }
    }
    views
}

/// Input debounce: rapid changes within delay only write the last value to the returned
/// signal; on_fire runs before the write
pub fn debounce_string(
    source: RwSignal<String>,
    delay: std::time::Duration,
    on_fire: impl Fn() + Copy + 'static,
) -> RwSignal<String> {
    let debounced = RwSignal::new(source.get_untracked());
    let tick = RwSignal::new(0u64);
    Effect::new(move || {
        let v = source.get();
        tick.update(|t| *t += 1);
        let mine = tick.get_untracked();
        set_timeout(
            move || {
                if tick.get_untracked() == mine {
                    on_fire();
                    debounced.set(v);
                }
            },
            delay,
        );
    });
    debounced
}

/// Builds `git -c safe.directory=<dir> -C <dir> <args...>` (no shell). The repo may be owned
/// by a different user than the caller (service user vs root, or vice versa) — without
/// safe.directory git refuses with "detected dubious ownership". Command-line -c is a
/// protected config scope, so git honors it. Canonicalized because git compares realpaths;
/// falls back to the non-canonical path on error
#[cfg(feature = "ssr")]
pub fn git_command(dir: &std::path::Path, args: &[&str]) -> std::process::Command {
    let safe = format!(
        "safe.directory={}",
        dir.canonicalize()
            .unwrap_or_else(|_| dir.to_path_buf())
            .display()
    );
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-c").arg(safe).arg("-C").arg(dir).args(args);
    cmd
}
