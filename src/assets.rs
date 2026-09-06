//! Static assets (WASM/JS/CSS/favicon) are embedded into the binary, so deployment is a
//! single file.
//! Debug builds still read from disk (`cargo leptos watch` picks up style changes without
//! recompiling).

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "target/site"]
pub struct Assets;
