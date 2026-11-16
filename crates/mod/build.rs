// eidos calls a wasm mod's handlers through its function table, so the
// module exports the table (docs/mod-code-wasm.md, "The wasm module").
fn main() {
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo:rustc-link-arg=--export-table");
    }
}
