use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::Router;
use leafpress::app::App;
use leafpress::assets::Assets;
use leafpress::config::Config;
use leafpress::state::AppState;
use leptos::prelude::*;
use leptos_axum::{generate_route_list, LeptosRoutes};

/// build.rs fingerprints target/site into LEAPTRESS_ASSETS_HASH; asset filenames
/// carry no content hash, so it doubles as a strong ETag: clients revalidate on
/// every visit (no-cache) but get an empty 304 until a new binary is deployed.
const ASSETS_ETAG: &str = concat!("\"leafpress-", env!("LEAFPRESS_ASSETS_HASH"), "\"");

/// Static assets are served from the embedded target/site first (single-binary
/// release deployment); anything not found falls through to the leptos routes /
/// fallback to render pages.
async fn embedded_static(req: Request, next: Next) -> Response {
    let path = req.uri().path().trim_start_matches('/');
    if !path.is_empty() && !path.contains("..") {
        if let Some(file) = Assets::get(path) {
            if req
                .headers()
                .get(header::IF_NONE_MATCH)
                .and_then(|v| v.to_str().ok())
                == Some(ASSETS_ETAG)
            {
                return Response::builder()
                    .status(StatusCode::NOT_MODIFIED)
                    .header(header::ETAG, ASSETS_ETAG)
                    .header(header::CACHE_CONTROL, "no-cache")
                    .body(Body::empty())
                    .unwrap();
            }
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let body = match file.data {
                Cow::Borrowed(b) => Body::from(b),
                Cow::Owned(v) => Body::from(v),
            };
            let mut resp = Response::new(body);
            resp.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime.as_ref())
                    .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
            );
            resp.headers_mut()
                .insert(header::ETAG, HeaderValue::from_static(ASSETS_ETAG));
            resp.headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            return resp;
        }
    }
    next.run(req).await
}

fn shell(options: LeptosOptions, site_name: String) -> impl IntoView {
    // hot-reload inserts `<!>` anchors into dynamic text nodes, and <title> content is
    // parsed as plain text, so the anchor would show up as title text; injecting via
    // the inner_html attribute creates no text node but requires manual escaping
    let title_html = site_name
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    view! {
        <!DOCTYPE html>
        <html lang="zh-CN" data-theme="light">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <link rel="icon" href="/favicon.svg" type="image/svg+xml"/>
                <link rel="stylesheet" href="/pkg/leafpress.css"/>
                <AutoReload options=options.clone()/>
                <HydrationScripts options=options/>
                <script>{r#"(function(){try{var t=localStorage.getItem('theme');if(!t){t=window.matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light';}document.documentElement.setAttribute('data-theme',t);}catch(e){document.documentElement.setAttribute('data-theme','light');}})();"#}</script>
                <title inner_html={title_html}></title>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use leafpress::cli::Cmd;
    match leafpress::cli::parse() {
        Cmd::Init(args) => leafpress::cli::init(args),
        Cmd::Doctor => {
            let ok = leafpress::cli::doctor().await;
            std::process::exit(if ok { 0 } else { 1 });
        }
        Cmd::Passwd => leafpress::cli::passwd(),
        Cmd::Update => leafpress::cli::update(),
        Cmd::Serve => serve().await,
    }
}

async fn serve() -> anyhow::Result<()> {
    let config = Arc::new(Config::load());
    let leptos_options = leafpress::config::leptos_options();
    let addr = leptos_options.site_addr;
    // Read the site name once at startup; like other config, changes require a restart
    let site_name = config.site_name.clone();

    let pool = leafpress::db::init(&config.database).await?;
    let content_dir = std::path::PathBuf::from(&config.content_dir);
    let images_dir = content_dir.join("images");
    let index = leafpress::content::SharedIndex::default();
    *index.write() = leafpress::content::scan(&content_dir)?;
    leafpress::content::spawn_watcher(content_dir, index.clone());

    let state = AppState {
        leptos_options: leptos_options.clone(),
        pool,
        index,
        config,
    };
    let routes = generate_route_list(App);

    let app = Router::new()
        // Map the content repo's images/ dir to /images/; markdown references it as ![alt](/images/xxx.png)
        .nest_service("/images", tower_http::services::ServeDir::new(images_dir))
        .leptos_routes_with_context(
            &state,
            routes,
            {
                let state = state.clone();
                move || provide_context(state.clone())
            },
            {
                let options = leptos_options.clone();
                let site_name = site_name.clone();
                move || shell(options.clone(), site_name.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler::<AppState, _>(
            move |options| shell(options, site_name.clone()),
        ))
        .layer(middleware::from_fn(embedded_static))
        // outermost layer: compress everything (SSR HTML, wasm/js/css); images are
        // skipped by tower-http's default compression predicate
        .layer(tower_http::compression::CompressionLayer::new())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("listening on http://{addr}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
