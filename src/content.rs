use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use comrak::nodes::{AstNode, NodeValue};
use gray_matter::{engine::YAML, Matter};
use parking_lot::RwLock;

use crate::types::{ContentKind, PostMeta, TocItem};

pub type SharedIndex = Arc<RwLock<ContentIndex>>;

#[derive(Default)]
pub struct ContentIndex {
    pub posts: Vec<Post>,
    pub pages: Vec<Post>,
}

pub struct Post {
    pub meta: PostMeta,
    pub html: String,
    pub toc: Vec<TocItem>,
    /// Plain-text body with markdown syntax stripped, used for search matching and snippets
    pub text: String,
}

#[derive(Debug, serde::Deserialize)]
struct FrontMatter {
    title: Option<String>,
    slug: Option<String>,
    date: Option<String>,
    tags: Option<Vec<String>>,
    category: Option<String>,
    status: Option<String>,
}

fn comrak_options() -> comrak::Options<'static> {
    let mut opts = comrak::Options::default();
    opts.extension.table = true;
    opts.extension.tasklist = true;
    opts.extension.strikethrough = true;
    opts.extension.autolink = true;
    opts.extension.header_ids = Some(String::new());
    opts.extension.math_dollars = true;
    opts.render.unsafe_ = true;
    opts.render.github_pre_lang = true;
    opts
}

pub fn render_markdown(md: &str) -> String {
    let opts = comrak_options();
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, md, &opts);
    // Server-side KaTeX: replace each Math node with the rendered HTML so pages need
    // no client JS. On a render error keep the node, which comrak emits as a
    // <span data-math-style> with the raw literal.
    let inline_opts = katex::Opts::builder().display_mode(false).build().unwrap();
    let display_opts = katex::Opts::builder().display_mode(true).build().unwrap();
    for node in root.descendants() {
        let mut data = node.data.borrow_mut();
        if let NodeValue::Math(math) = &data.value {
            let kopts = if math.display_math {
                &display_opts
            } else {
                &inline_opts
            };
            if let Ok(html) = katex::render_with_opts(&math.literal, kopts) {
                data.value = NodeValue::HtmlInline(html);
            }
        }
    }
    let mut buf = Vec::new();
    comrak::html::format_document(root, &opts, &mut buf).expect("comrak render");
    String::from_utf8(buf).expect("comrak output is utf-8")
}

/// Mirrors comrak html.rs collect_text so heading plain text (and thus anchor ids) matches
/// the rendered output
fn collect_heading_text<'a>(node: &'a AstNode<'a>, out: &mut Vec<u8>) {
    match &node.data.borrow().value {
        NodeValue::Text(literal) => out.extend_from_slice(literal.as_bytes()),
        NodeValue::Code(code) => out.extend_from_slice(code.literal.as_bytes()),
        NodeValue::LineBreak | NodeValue::SoftBreak => out.push(b' '),
        NodeValue::Math(math) => out.extend_from_slice(math.literal.as_bytes()),
        _ => {
            for child in node.children() {
                collect_heading_text(child, out);
            }
        }
    }
}

/// Extract the TOC and plain-text body in a single AST pass;
/// the Anchorizer is the same one the renderer uses, keeping -1/-2 suffixes for duplicate
/// headings consistent
fn extract_toc_and_text(md: &str) -> (Vec<TocItem>, String) {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, md, &comrak_options());
    let mut anchorizer = comrak::Anchorizer::new();
    let mut toc = Vec::new();
    let mut text = String::new();
    for node in root.descendants() {
        match &node.data.borrow().value {
            NodeValue::Heading(h) => {
                let mut buf = Vec::new();
                collect_heading_text(node, &mut buf);
                let heading = String::from_utf8_lossy(&buf).into_owned();
                // The renderer anchorizes/dedupes every level in order, so we must too,
                // otherwise the suffixes would mismatch
                let id = anchorizer.anchorize(heading.clone());
                if (1..=3).contains(&h.level) {
                    toc.push(TocItem {
                        level: h.level,
                        text: heading,
                        id,
                    });
                }
            }
            NodeValue::Text(literal) => text.push_str(literal),
            NodeValue::Code(code) => text.push_str(&code.literal),
            NodeValue::Math(math) => {
                text.push_str(&math.literal);
                text.push(' ');
            }
            NodeValue::CodeBlock(block) => {
                text.push_str(&block.literal);
                text.push(' ');
            }
            NodeValue::LineBreak | NodeValue::SoftBreak => text.push(' '),
            _ => {}
        }
    }
    (toc, text)
}

fn slugify(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join("-")
}

fn parse_post(raw: &str, path: &Path, root: &Path, kind: ContentKind) -> Option<Post> {
    let matter = Matter::<YAML>::new();
    let parsed = matter.parse(raw);
    let fm: FrontMatter = parsed
        .data
        .as_ref()
        .and_then(|d| d.deserialize().ok())
        .unwrap_or(FrontMatter {
            title: None,
            slug: None,
            date: None,
            tags: None,
            category: None,
            status: None,
        });
    let stem = path.file_stem()?.to_str()?;
    let slug = fm.slug.clone().unwrap_or_else(|| slugify(stem));
    // Subdirectories are physical organization only (no URL effect); if front matter omits
    // category, use the folder name
    let folder_category = path
        .parent()
        .filter(|p| *p != root)
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(str::to_string);
    let category = fm
        .category
        .filter(|c| !c.trim().is_empty())
        .or(folder_category);
    let meta = PostMeta {
        title: fm.title.unwrap_or_else(|| slug.clone()),
        slug,
        date: fm.date,
        tags: fm.tags.unwrap_or_default(),
        category,
        status: fm.status.unwrap_or_else(|| "published".into()),
        kind,
    };
    let (toc, text) = extract_toc_and_text(&parsed.content);
    Some(Post {
        meta,
        html: render_markdown(&parsed.content),
        toc,
        text,
    })
}

fn parse_date(s: &str) -> Option<chrono::NaiveDateTime> {
    for fmt in ["%Y-%m-%d %H:%M", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt);
        }
    }
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
}

fn sort_key(p: &Post) -> chrono::NaiveDateTime {
    p.meta
        .date
        .as_deref()
        .and_then(parse_date)
        .unwrap_or(chrono::NaiveDateTime::MIN)
}

fn collect_md_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_md_files(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
    Ok(())
}

pub fn scan(content_dir: &Path) -> Result<ContentIndex> {
    let mut posts = Vec::new();
    let mut pages = Vec::new();
    for (kind, sub) in [(ContentKind::Post, "posts"), (ContentKind::Page, "pages")] {
        let dir = content_dir.join(sub);
        let mut files = Vec::new();
        collect_md_files(&dir, &mut files)?;
        files.sort();
        let kind_name = kind.as_str();
        let mut seen: std::collections::HashMap<String, PathBuf> = std::collections::HashMap::new();
        for path in files {
            let raw = std::fs::read_to_string(&path)?;
            let Some(post) = parse_post(&raw, &path, &dir, kind) else {
                continue;
            };
            // The slug is the site-wide unique ID (URLs, comments, and likes key on it);
            // on duplicate names across folders, keep the first one scanned
            if let Some(first) = seen.get(&post.meta.slug) {
                eprintln!(
                    "[content] {kind_name} slug conflict: {:?} and {:?} share slug \"{}\", ignoring the latter",
                    first, path, post.meta.slug
                );
                continue;
            }
            seen.insert(post.meta.slug.clone(), path.clone());
            match kind {
                ContentKind::Post => posts.push(post),
                ContentKind::Page => pages.push(post),
            }
        }
    }
    posts.sort_by_key(|p| std::cmp::Reverse(sort_key(p)));
    pages.sort_by(|a, b| a.meta.slug.cmp(&b.meta.slug));
    Ok(ContentIndex { posts, pages })
}

/// Watch the content dir; fully rescan on any change (few files, negligible cost) to
/// hot-reload posts
pub fn spawn_watcher(content_dir: PathBuf, index: SharedIndex) {
    std::thread::spawn(move || {
        use notify::{Config, RecursiveMode, Watcher};
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::RecommendedWatcher::new(tx, Config::default()) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("[content] failed to start file watcher: {e}");
                return;
            }
        };
        if let Err(e) = watcher.watch(&content_dir, RecursiveMode::Recursive) {
            eprintln!("[content] failed to watch {content_dir:?}: {e}");
            return;
        }
        while rx.recv().is_ok() {
            // Simple debounce: coalesce events within 150ms
            std::thread::sleep(std::time::Duration::from_millis(150));
            while rx.try_recv().is_ok() {}
            match scan(&content_dir) {
                Ok(new_index) => {
                    *index.write() = new_index;
                    eprintln!("[content] hot-reloaded");
                }
                Err(e) => eprintln!("[content] rescan failed: {e}"),
            }
        }
    });
}

/// Periodically fast-forward the content dir from its git remote; the watcher picks up any
/// changes and hot-reloads. No-op when disabled or the content dir is not a git repo.
/// The last pull failure (timestamped) is shared via `last_error` so /admin can show it.
pub fn spawn_git_sync(
    content_dir: PathBuf,
    interval_secs: u64,
    proxy: Option<String>,
    last_error: Arc<std::sync::RwLock<Option<String>>>,
) {
    if interval_secs == 0 || !content_dir.join(".git").exists() {
        return;
    }
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(interval_secs));
        let mut cmd = crate::util::git_command(&content_dir, &[]);
        // Explicit proxy control: configured proxy when set, otherwise force direct
        // (empty value also disables any http_proxy env inherited from the server process)
        cmd.arg("-c")
            .arg(format!("http.proxy={}", proxy.as_deref().unwrap_or("")));
        let err = match cmd.args(["pull", "--ff-only", "--quiet"]).output() {
            Ok(o) if o.status.success() => None,
            Ok(o) => {
                let msg = String::from_utf8_lossy(&o.stderr).trim().to_string();
                eprintln!("[content] git pull failed: {msg}");
                Some(msg)
            }
            Err(e) => {
                eprintln!("[content] failed to run git: {e}");
                Some(e.to_string())
            }
        };
        *last_error.write().unwrap() = err.map(|msg| {
            let first = msg
                .lines()
                .next()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .unwrap_or("unknown error");
            format!("{} {first}", chrono::Local::now().format("%Y-%m-%d %H:%M"))
        });
    });
}
