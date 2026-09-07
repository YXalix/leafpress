// rust-embed reads target/site at compile time (release builds embed it into the binary).
// A plain `cargo:rerun-if-changed` is not enough: it only re-runs this script, and since
// the script's output stays identical cargo keeps the crate — and its embedded assets —
// cached. cargo-leptos builds the front and server in parallel, so the server crate may
// compile while pkg/ is still being written. Fingerprinting the directory into a rustc-env
// var forces a recompile (and re-embed) whenever the assets change after that.
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::Path,
};

fn hash_dir(hasher: &mut DefaultHasher, dir: &Path) {
    let entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(it) => it.filter_map(Result::ok).collect(),
        Err(_) => return, // not built yet (e.g. plain cargo build before cargo leptos)
    };
    let mut paths: Vec<_> = entries.into_iter().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            hash_dir(hasher, &path);
        } else if let Ok(bytes) = std::fs::read(&path) {
            path.hash(hasher);
            bytes.hash(hasher);
        }
    }
}

fn main() {
    let mut hasher = DefaultHasher::new();
    hash_dir(&mut hasher, Path::new("target/site"));
    println!(
        "cargo:rustc-env=LEAFPRESS_ASSETS_HASH={:016x}",
        hasher.finish()
    );
    println!("cargo:rerun-if-changed=target/site");
}
