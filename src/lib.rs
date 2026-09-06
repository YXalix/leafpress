pub mod api;
pub mod app;
pub mod components;
pub mod pages;
pub mod types;
pub mod util;

#[cfg(feature = "ssr")]
pub mod assets;
#[cfg(feature = "ssr")]
pub mod cli;
#[cfg(feature = "ssr")]
pub mod config;
#[cfg(feature = "ssr")]
pub mod content;
#[cfg(feature = "ssr")]
pub mod db;
#[cfg(feature = "ssr")]
pub mod state;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
}
