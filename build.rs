// rust-embed reads target/site at compile time; recompile whenever assets change
fn main() {
    println!("cargo:rerun-if-changed=target/site");
}
