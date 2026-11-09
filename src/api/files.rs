//! Public file-sharing API over [crate::config::Config::files_dir] — decoupled from the
//! content repo: no git, no admin session, no database. The listing server fn is public;
//! PUT/DELETE on /f/<rel path> are plain axum handlers gated by `files_upload_token`
//! (curl-friendly), wired in main.rs next to the ServeDir that answers the downloads.
//! Subdirectories under files_dir are supported: uploads create parent dirs on the fly,
//! listings take a validated relative dir param.

use leptos::prelude::*;

#[cfg(feature = "ssr")]
use crate::state::AppState;
#[cfg(feature = "ssr")]
use axum::extract::{Path, State};
#[cfg(feature = "ssr")]
use comrak::nodes::NodeValue;

use crate::types::SharedFile;

#[server]
pub async fn list_shared_files(dir: String) -> Result<Vec<SharedFile>, ServerFnError> {
    let state = expect_context::<AppState>();
    let root = std::path::PathBuf::from(&state.config.files_dir);
    // hand-typed URLs like ?dir=docs/ or ?dir=/docs stay valid
    let dir = dir.trim_matches('/');
    let list_dir = resolve_list_dir(&root, dir)?;
    Ok(with_rel_paths(read_shared_files(&list_dir), &root, dir))
}

/// Direct children of `dir` — files and subdirectories, dirs first (name asc), then
/// files (newest first); missing dir = empty listing. `SharedFile::path` is filled by
/// the caller (it needs the listing dir as context)
#[cfg(feature = "ssr")]
fn read_shared_files(dir: &std::path::Path) -> Vec<SharedFile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<SharedFile> = entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            if !meta.is_file() && !meta.is_dir() {
                return None; // symlinks and specials stay invisible, as before
            }
            let is_dir = meta.is_dir();
            let modified = meta
                .modified()
                .ok()
                .map(|t| {
                    chrono::DateTime::<chrono::Utc>::from(t)
                        .with_timezone(&chrono::Local)
                        .format("%Y-%m-%d %H:%M")
                        .to_string()
                })
                .unwrap_or_default();
            Some(SharedFile {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: String::new(),
                is_dir,
                size: if is_dir { 0 } else { meta.len() },
                modified,
                is_text: false,
            })
        })
        .collect();
    out.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, true) => a.name.cmp(&b.name),
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (false, false) => b.modified.cmp(&a.modified).then(b.name.cmp(&a.name)),
    });
    out
}

/// Relative paths under files_dir, slash-separated: every component must be a plain
/// single name (no leading `.`, which also rules out `.`/`..` traversal components).
/// Covers file paths (PUT/DELETE /f/...) and directory params (?dir=...)
#[cfg(feature = "ssr")]
fn valid_shared_rel_path(rel: &str) -> bool {
    !rel.is_empty()
        && rel.len() <= 1024
        && rel.trim() == rel
        && !rel.starts_with('/')
        && !rel.ends_with('/')
        && !rel.contains('\\')
        && !rel.contains('\0')
        && rel.split('/').all(valid_shared_name)
}

/// One plain path component: no leading dot (hidden files), bounded length
#[cfg(feature = "ssr")]
fn valid_shared_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 200 && name.trim() == name && !name.starts_with('.')
}

/// Percent-decodes a raw URI path segment (invalid escapes pass through; lossy UTF-8)
#[cfg(feature = "ssr")]
fn pct_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = if bytes[i] == b'%' && i + 2 < bytes.len() {
            std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        } else {
            None
        };
        match hex {
            Some(b) => {
                out.push(b);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// NUL-free UTF-8 in the first 4 KiB → treat as text (empty files count as text); an
/// incomplete multibyte sequence at the buffer edge still counts (sampling artifact)
#[cfg(feature = "ssr")]
fn sniff_is_text(path: &std::path::Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 4096];
    let n = match file.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return false,
    };
    if buf[..n].contains(&0) {
        return false;
    }
    match std::str::from_utf8(&buf[..n]) {
        Ok(_) => true,
        // a valid sequence prefix cut by the 4096-byte sampling edge (error_len is
        // None exactly for an incomplete tail) is a truncation artifact, not a bad
        // file — CJK text lands a multibyte char across the boundary most of the time
        Err(e) if e.error_len().is_none() => true,
        Err(_) => false,
    }
}

/// Text files above this size skip the HTML preview and fall back to plain text —
/// the preview page is built in memory and unbounded otherwise
#[cfg(feature = "ssr")]
const TEXT_VIEW_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// Shapes /f responses so the two actions stay unambiguous:
/// - plain link (click) → always `Content-Disposition: attachment`, i.e. download,
///   regardless of content
/// - `?view=1` → text data (NUL-free UTF-8 first 4 KiB) serves as a self-contained
///   HTML preview page ([text_view_html]): header bar with one-click copy plus
///   download/raw/listing links, markdown rendered to formatted HTML, patch/diff
///   bodies line-highlighted, other text as a plain block. `&raw=1` opts out to
///   inline text/plain; binary files ignore `view`
///   and download. Explicit text/plain is required on the raw path because
///   Chrome/Firefox download octet-stream bodies; it also neutralizes uploaded
///   .html/.svg (rendered as source, no stored XSS — the preview page escapes too).
///   The preview page is answered directly without consulting ServeDir — a stale
///   cached plain-text view then revalidates into fresh HTML instead of pinning a
///   304.
///
/// View responses: the HTML preview page is `Cache-Control: no-cache` (cache but
/// always revalidate) because its *format* evolves with deploys and a
/// heuristically-fresh cached copy would pin the old one — the exact bug where
/// pre-deploy views stayed plain text forever. The plain-text fallbacks (raw=1,
/// oversize files) keep default caching instead: their format never changes, and
/// ServeDir's Last-Modified already drives cheap conditional 304s.
///
/// Mounted inside the /f nest, so the URI path here has the /f prefix stripped and is
/// the files_dir-relative path (percent-encoded); nested paths work the same as flat ones
#[cfg(feature = "ssr")]
pub async fn file_disposition(
    State(state): State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::{header, HeaderValue, StatusCode};
    use axum::response::IntoResponse;

    let is_get = matches!(req.method().as_str(), "GET" | "HEAD");
    let query_flags: Vec<&str> = req
        .uri()
        .query()
        .map(|q| q.split('&').collect())
        .unwrap_or_default();
    let wants_view = query_flags.iter().any(|kv| *kv == "view=1");
    let wants_raw = query_flags.iter().any(|kv| *kv == "raw=1");
    let rel = pct_decode(req.uri().path().trim_start_matches('/'));

    let root = std::path::PathBuf::from(&state.config.files_dir);
    if is_get && wants_view && valid_shared_rel_path(&rel) && sniff_is_text(&root.join(&rel)) {
        if !wants_raw {
            if let Some(html) = text_view_html(&root, &rel) {
                let mut resp = axum::response::Html(html).into_response();
                resp.headers_mut()
                    .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
                return resp;
            }
        }
        // plain text: format is stable, so keep stock caching — ServeDir's
        // Last-Modified conditional requests already make re-checks a cheap 304
        let mut resp = next.run(req).await;
        if matches!(resp.status(), StatusCode::OK | StatusCode::PARTIAL_CONTENT) {
            resp.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            );
        }
        return resp;
    }

    let mut resp = next.run(req).await;
    if !is_get || !matches!(resp.status(), StatusCode::OK | StatusCode::PARTIAL_CONTENT) {
        return resp;
    }
    if !valid_shared_rel_path(&rel) {
        return resp;
    }
    // attachment filename is the basename: browsers treat it as one file's name
    let basename = rel.rsplit('/').next().unwrap_or(&rel);
    if let Ok(value) = HeaderValue::from_str(&format!(
        "attachment; filename*=UTF-8''{}",
        crate::util::url_encode(basename)
    )) {
        resp.headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }
    resp
}

/// HTML preview for `?view=1`: reads the whole file (within [TEXT_VIEW_MAX_BYTES])
/// and returns the self-contained preview page, or None to fall back to plain text.
/// UTF-8 is decoded lossily — one stray byte degrades to U+FFFD in the view instead
/// of dropping the whole file to plain text (the 4 KiB sniff has only sampled the
/// head). Markdown is rendered; patch/diff content is line-highlighted; other text
/// renders as one plain block. Each body supplies its own wrapper (the shell's
/// <main> is content-agnostic)
#[cfg(feature = "ssr")]
fn text_view_html(root: &std::path::Path, rel: &str) -> Option<String> {
    let path = root.join(rel);
    let meta = std::fs::metadata(&path).ok()?;
    if meta.len() > TEXT_VIEW_MAX_BYTES {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let body = if is_markdown(name) {
        markdown_body(&text)
    } else {
        let inner = if looks_like_patch(name, &text) {
            patch_body(&text)
        } else {
            let mut inner = String::with_capacity(text.len() + text.len() / 16);
            html_escape_into(&mut inner, &text);
            inner
        };
        let mut body = String::with_capacity(inner.len() + 11);
        body.push_str("<pre>");
        body.push_str(&inner);
        body.push_str("</pre>");
        body
    };
    // href must be site-absolute: the view URL is /f/<dir>/<name>, and a relative
    // href would resolve against <dir>/ and double every subdirectory segment
    Some(render_view_html(
        name,
        &body,
        &format!("/f/{}", crate::util::url_encode_path(rel)),
    ))
}

/// Markdown detection: .md / .markdown extensions only — no content sniffing, which
/// would misfire on ordinary prose that happens to contain `#` or `*`
#[cfg(feature = "ssr")]
fn is_markdown(name: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

/// Rendered body for a markdown preview. Shares comrak with the blog renderer but
/// deliberately not its options: these files are uploads viewed publicly, so
/// `render.unsafe_` stays off (embedded HTML comes out as escaped comments, matching
/// the preview page's escape-everything stance) and there is no KaTeX. Link/image
/// URLs are additionally neutralized when they carry a dangerous scheme
/// (javascript: and friends — comrak passes those through even with unsafe_ off).
/// A hidden `#lpsrc` <pre> carries the escaped source so the copy button stays
/// byte-faithful to the file rather than to the rendered text
#[cfg(feature = "ssr")]
fn markdown_body(text: &str) -> String {
    let mut opts = comrak::Options::default();
    opts.extension.table = true;
    opts.extension.tasklist = true;
    opts.extension.strikethrough = true;
    opts.extension.autolink = true;
    opts.render.github_pre_lang = true;
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, text, &opts);
    for node in root.descendants() {
        let mut data = node.data.borrow_mut();
        let url = match &mut data.value {
            NodeValue::Link(l) => &mut l.url,
            NodeValue::Image(i) => &mut i.url,
            _ => continue,
        };
        if !safe_url(url) {
            *url = "#".into();
        }
    }
    let mut buf = Vec::new();
    comrak::html::format_document(root, &opts, &mut buf).expect("comrak render");
    let html = String::from_utf8(buf).expect("comrak output is utf-8");
    let mut body = String::with_capacity(html.len() + text.len() + text.len() / 16 + 64);
    body.push_str("<div class=\"md\">");
    body.push_str(&html);
    body.push_str("</div>\n<pre id=\"lpsrc\" hidden>");
    html_escape_into(&mut body, text);
    body.push_str("</pre>");
    body
}

/// True when a link/image URL is safe to keep in the rendered markdown. Relative
/// references and bare fragments have no scheme; a "scheme" candidate containing
/// / ? # is a relative path like "a/b:c" per RFC 3986. Only http/https/mailto pass
#[cfg(feature = "ssr")]
fn safe_url(url: &str) -> bool {
    let Some((head, _)) = url.split_once(':') else {
        return true;
    };
    if head.is_empty() || head.contains(['/', '?', '#']) {
        return true;
    }
    matches!(
        head.to_ascii_lowercase().as_str(),
        "http" | "https" | "mailto"
    )
}

/// Patch detection: *.patch / *.diff extensions count outright; otherwise sniff the
/// first 64 lines for unified/git diff landmarks, so extensionless drops work without
/// misfiring on ordinary text
#[cfg(feature = "ssr")]
fn looks_like_patch(name: &str, text: &str) -> bool {
    if let Some(ext) = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
    {
        if ext.eq_ignore_ascii_case("patch") || ext.eq_ignore_ascii_case("diff") {
            return true;
        }
    }
    let mut prev_old_file = false;
    for line in text.lines().take(64) {
        if line.starts_with("diff --git ")
            || line.starts_with("Index: ")
            || line.starts_with("@@ -")
        {
            return true;
        }
        if prev_old_file && line.starts_with("+++ ") {
            return true;
        }
        prev_old_file = line.starts_with("--- ");
    }
    false
}

/// Line class for the patch view. Git extended headers all render as meta, so the
/// `+++`/`---` file headers must be tested before bare `+`/`-`
#[cfg(feature = "ssr")]
fn patch_line_class(line: &str) -> &'static str {
    const GIT_HEADERS: [&str; 14] = [
        "diff --git ",
        "index ",
        "old mode ",
        "new mode ",
        "new file mode ",
        "deleted file mode ",
        "similarity index ",
        "dissimilarity index ",
        "rename from ",
        "rename to ",
        "copy from ",
        "copy to ",
        "Binary files ",
        "GIT binary patch",
    ];
    if line.starts_with("@@") {
        "d-hunk"
    } else if line.starts_with("+++")
        || line.starts_with("---")
        || line.starts_with('\\') // "\ No newline at end of file"
        || line.starts_with("Only in ")
        || GIT_HEADERS.iter().any(|h| line.starts_with(h))
    {
        "d-meta"
    } else if line.starts_with('+') {
        "d-add"
    } else if line.starts_with('-') {
        "d-del"
    } else {
        "d-ctx"
    }
}

/// Escapes the characters that matter in text nodes (quotes never land in attribute
/// values here)
#[cfg(feature = "ssr")]
fn html_escape_into(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

/// Highlighted body for the patch preview: one span per line, LF-joined. The final
/// LF is dropped when the file itself doesn't end in one, so the <pre> textContent
/// the copy button reads reproduces the viewable text exactly
#[cfg(feature = "ssr")]
fn patch_body(text: &str) -> String {
    let mut body = String::with_capacity(text.len() + text.len() / 4);
    for line in text.lines() {
        body.push_str("<span class=\"");
        body.push_str(patch_line_class(line));
        body.push_str("\">");
        html_escape_into(&mut body, line);
        body.push_str("</span>\n");
    }
    if body.ends_with('\n') && !text.ends_with('\n') {
        body.pop();
    }
    body
}

/// Builds the page from [TEXT_VIEW_SHELL]. Replacements run body-last: file content
/// is arbitrary, so no token inside it can be substituted
#[cfg(feature = "ssr")]
fn render_view_html(name: &str, body: &str, download_href: &str) -> String {
    let mut title = String::new();
    html_escape_into(&mut title, name);
    TEXT_VIEW_SHELL
        .replace("@DOWNLOAD@", download_href)
        .replace("@RAW@", &format!("{download_href}?view=1&raw=1"))
        .replace("@TITLE@", &title)
        .replace("@BODY@", body)
}

/// Standalone text-preview page for the /f view: inline CSS/JS only, no site assets,
/// so /f stays decoupled from the Leptos app; follows light/dark via
/// prefers-color-scheme. The copy button reads #lpsrc when present (markdown: the
/// hidden source <pre>) and falls back to the first <pre> in main; either way
/// [text_view_html] keeps it byte-faithful to the viewable text
/// @DOWNLOAD@/@RAW@/@TITLE@/@BODY@ are replaced per request
#[cfg(feature = "ssr")]
const TEXT_VIEW_SHELL: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>@TITLE@ · leafpress</title>
<style>
  :root { color-scheme: light dark; }
  * { box-sizing: border-box; }
  body {
    margin: 0;
    font: 13px/1.6 ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace;
    background: #ffffff; color: #1f2328;
  }
  header {
    position: sticky; top: 0; z-index: 1;
    display: flex; flex-wrap: wrap; gap: .35rem 1rem; align-items: baseline;
    padding: .55rem 1rem; border-bottom: 1px solid #d1d9e0; background: #f6f8fa;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  }
  header b { font-size: .95rem; overflow-wrap: anywhere; }
  header .sp { flex: 1; }
  header a { color: #0969da; text-decoration: none; white-space: nowrap; font-size: .85rem; }
  header a:hover { text-decoration: underline; }
  header button {
    border: 0; padding: 0; background: none; cursor: pointer;
    font: inherit; font-size: .85rem; color: #0969da; white-space: nowrap;
  }
  header button:hover { text-decoration: underline; }
  header button.ok { color: #1a7f37; }
  main { padding: 1rem; overflow-x: auto; }
  pre { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  .md {
    max-width: 60rem;
    font: 15px/1.7 -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif;
  }
  .md > :first-child { margin-top: 0; }
  .md h1, .md h2, .md h3, .md h4, .md h5, .md h6 {
    margin: 1.5em 0 .6em; line-height: 1.3; overflow-wrap: anywhere;
  }
  .md h1, .md h2 { border-bottom: 1px solid #d1d9e0; padding-bottom: .3em; }
  .md p { margin: .8em 0; }
  .md a { color: #0969da; }
  .md code {
    font: .9em ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace;
    background: #f6f8fa; border-radius: 4px; padding: .15em .35em;
    overflow-wrap: anywhere;
  }
  .md pre {
    background: #f6f8fa; border-radius: 6px; padding: .8rem 1rem;
    white-space: pre; overflow-x: auto;
  }
  .md pre code { background: none; padding: 0; font-size: 13px; }
  .md blockquote {
    margin: .8em 0; padding: 0 1em;
    border-left: 4px solid #d1d9e0; color: #59636e;
  }
  .md table { border-collapse: collapse; margin: .8em 0; }
  .md th, .md td { border: 1px solid #d1d9e0; padding: .35em .8em; }
  .md th { background: #f6f8fa; }
  .md img { max-width: 100%; }
  .md hr { border: 0; border-top: 2px solid #d1d9e0; margin: 1.5em 0; }
  .md ul, .md ol { padding-left: 1.6em; }
  .md li { margin: .25em 0; }
  .md input[type=checkbox] { margin-right: .4em; }
  .d-add { color: #1a7f37; background: #e6ffec; }
  .d-del { color: #cf222e; background: #ffebe9; }
  .d-hunk { color: #8250df; }
  .d-meta { color: #59636e; font-weight: 600; }
  .d-ctx { color: inherit; }
  @media (prefers-color-scheme: dark) {
    body { background: #0d1117; color: #e6edf3; }
    header { background: #161b22; border-bottom-color: #30363d; }
    header a { color: #4493f8; }
    header button { color: #4493f8; }
    header button.ok { color: #3fb950; }
    .d-add { color: #3fb950; background: #12261e; }
    .d-del { color: #f85149; background: #2d1517; }
    .d-hunk { color: #d2a8ff; }
    .d-meta { color: #8b949e; }
    .md a { color: #4493f8; }
    .md code, .md pre, .md th { background: #161b22; }
    .md h1, .md h2, .md th, .md td { border-color: #30363d; }
    .md blockquote { border-left-color: #30363d; color: #8b949e; }
    .md hr { border-top-color: #30363d; }
  }
</style>
</head>
<body>
<header>
  <b>@TITLE@</b>
  <span class="sp"></span>
  <button type="button" onclick="lpcopy(this)">复制</button>
  <a href="@DOWNLOAD@">下载</a>
  <a href="@RAW@">纯文本</a>
  <a href="/files">文件列表</a>
</header>
<main>@BODY@</main>
<script>
function lpcopyFallback(t, done) {
  var a = document.createElement('textarea');
  a.value = t;
  a.style.position = 'fixed';
  a.style.opacity = '0';
  document.body.appendChild(a);
  a.select();
  try { if (document.execCommand('copy')) done(); } catch (e) {}
  document.body.removeChild(a);
}
function lpcopy(b) {
  var el = document.getElementById('lpsrc') || document.querySelector('main pre');
  var t = el.textContent;
  var done = function () {
    b.textContent = '已复制';
    b.classList.add('ok');
    setTimeout(function () { b.textContent = '复制'; b.classList.remove('ok'); }, 1500);
  };
  if (navigator.clipboard && window.isSecureContext) {
    navigator.clipboard.writeText(t).then(done, function () { lpcopyFallback(t, done); });
  } else {
    lpcopyFallback(t, done);
  }
}
</script>
</body>
</html>
"#;

/// Token gate for the mutating /f handlers: config token unset → 403 (feature off),
/// header mismatch → 401
#[cfg(feature = "ssr")]
fn check_upload_token(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<(), axum::response::Response> {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    let Some(expected) = state
        .config
        .files_upload_token
        .as_deref()
        .filter(|t| !t.is_empty())
    else {
        return Err((
            StatusCode::FORBIDDEN,
            "文件上传未启用：在 config.toml 设置 files_upload_token 并重启\n",
        )
            .into_response());
    };
    let got = headers
        .get("x-upload-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if got != expected {
        return Err((StatusCode::UNAUTHORIZED, "上传 token 错误\n").into_response());
    }
    Ok(())
}

/// PUT /f/<rel path>: streams the request body straight into files_dir (no full
/// buffering), enforcing files_max_mb; missing parent directories are created, and
/// same-name files are overwritten. Responds with the share link.
#[cfg(feature = "ssr")]
pub async fn upload_shared_file(
    State(state): State<AppState>,
    Path(rel): Path<String>,
    headers: axum::http::HeaderMap,
    body: axum::body::Body,
) -> axum::response::Response {
    use std::future::poll_fn;
    use std::pin::Pin;
    use tokio::io::AsyncWriteExt;

    use axum::body::HttpBody;
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;

    if let Err(resp) = check_upload_token(&state, &headers) {
        return resp;
    }
    if !valid_shared_rel_path(&rel) {
        return (
            StatusCode::BAD_REQUEST,
            "路径不合法（仅支持 files_dir 下的相对路径，各段不以 . 开头）\n",
        )
            .into_response();
    }
    let mb = state.config.files_max_mb;
    let limit = if mb == 0 {
        u64::MAX
    } else {
        mb.saturating_mul(1024 * 1024)
    };
    // curl -T sends Content-Length up front — reject oversized uploads before reading
    if let Some(len) = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
    {
        if len > limit {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                "文件超过 files_max_mb 上限\n",
            )
                .into_response();
        }
    }

    let root = std::path::PathBuf::from(&state.config.files_dir);
    let path = root.join(&rel);
    if let Some(parent) = path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("无法创建目录 {}: {e}\n", parent.display()),
            )
                .into_response();
        }
    }
    let mut file = match tokio::fs::File::create(&path).await {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("写入失败: {e}\n"),
            )
                .into_response()
        }
    };

    // Stream frame by frame; any failure or limit overrun removes the partial file
    let mut stream = body;
    let mut written: u64 = 0;
    let result: Result<(), String> = loop {
        match poll_fn(|cx| Pin::new(&mut stream).poll_frame(cx)).await {
            Some(Ok(frame)) => {
                let Ok(data) = frame.into_data() else {
                    continue;
                };
                written += data.len() as u64;
                if written > limit {
                    break Err("文件超过 files_max_mb 上限".into());
                }
                if let Err(e) = file.write_all(&data).await {
                    break Err(format!("写入失败: {e}"));
                }
            }
            Some(Err(e)) => break Err(format!("读取上传流失败: {e}")),
            None => break Ok(()),
        }
    };
    if let Err(msg) = result {
        drop(file);
        let _ = tokio::fs::remove_file(&path).await;
        let status = if msg.contains("files_max_mb") {
            StatusCode::PAYLOAD_TOO_LARGE
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        return (status, format!("{msg}\n")).into_response();
    }
    let _ = file.flush().await;

    let encoded = crate::util::url_encode_path(&rel);
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    let link = match host {
        Some(h) if !h.is_empty() => format!("https://{h}/f/{encoded}\n"),
        _ => format!("/f/{encoded}\n"),
    };
    link.into_response()
}

/// DELETE /f/<rel path>: removes one shared file (same token gate as upload).
/// Directories are left alone — upload-created dirs hold no listing state, so removing
/// them is a plain filesystem job (scp/ssh), not an API concern
#[cfg(feature = "ssr")]
pub async fn delete_shared_file(
    State(state): State<AppState>,
    Path(rel): Path<String>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    if let Err(resp) = check_upload_token(&state, &headers) {
        return resp;
    }
    if !valid_shared_rel_path(&rel) {
        return (StatusCode::BAD_REQUEST, "路径不合法\n").into_response();
    }
    let path = std::path::PathBuf::from(&state.config.files_dir).join(&rel);
    if path.is_dir() {
        return (StatusCode::BAD_REQUEST, "目标是目录，仅支持删除文件\n").into_response();
    }
    match tokio::fs::remove_file(&path).await {
        Ok(()) => "已删除\n".into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "文件不存在\n").into_response(),
    }
}

/// Admin-panel listing: same data as the public listing, but gated by the admin
/// session so the /admin panel degrades to a login prompt instead of leaking nothing
#[server]
pub async fn admin_list_shared_files(dir: String) -> Result<Vec<SharedFile>, ServerFnError> {
    let state = expect_context::<AppState>();
    super::require_admin(&state).await?;
    let root = std::path::PathBuf::from(&state.config.files_dir);
    let dir = dir.trim_matches('/');
    let list_dir = resolve_list_dir(&root, dir)?;
    Ok(with_rel_paths(read_shared_files(&list_dir), &root, dir))
}

/// Validates the `?dir=` param (empty = files_dir root) and maps it onto the filesystem
#[cfg(feature = "ssr")]
fn resolve_list_dir(
    root: &std::path::Path,
    dir: &str,
) -> Result<std::path::PathBuf, ServerFnError> {
    if dir.is_empty() {
        Ok(root.to_path_buf())
    } else if valid_shared_rel_path(dir) {
        Ok(root.join(dir))
    } else {
        Err(ServerFnError::new("目录路径不合法"))
    }
}

/// Fills each entry's files_dir-relative path from the listing dir and sniffs text
/// content (dirs need neither)
#[cfg(feature = "ssr")]
fn with_rel_paths(list: Vec<SharedFile>, root: &std::path::Path, dir: &str) -> Vec<SharedFile> {
    list.into_iter()
        .map(|mut f| {
            f.path = if dir.is_empty() {
                f.name.clone()
            } else {
                format!("{dir}/{}", f.name)
            };
            if !f.is_dir {
                f.is_text = sniff_is_text(&root.join(&f.path));
            }
            f
        })
        .collect()
}

/// Admin-panel delete of one shared file: cookie session instead of the curl
/// upload token, same filesystem effect as DELETE /f/<rel path>
#[server]
pub async fn admin_delete_shared_file(path: String) -> Result<(), ServerFnError> {
    let state = expect_context::<AppState>();
    super::require_admin(&state).await?;
    if !valid_shared_rel_path(&path) {
        return Err(ServerFnError::new("路径不合法"));
    }
    let target = std::path::PathBuf::from(&state.config.files_dir).join(&path);
    if target.is_dir() {
        return Err(ServerFnError::new("目标是目录，仅支持删除文件"));
    }
    match tokio::fs::remove_file(&target).await {
        Ok(()) => Ok(()),
        Err(_) => Err(ServerFnError::new("删除失败：文件不存在或无权限")),
    }
}
