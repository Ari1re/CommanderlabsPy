// Make sure the embedded UI folder exists so `cargo build` works before the
// frontend has been built (CI always builds the frontend first).
fn main() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../frontend/dist");
    if !dist.join("index.html").exists() {
        std::fs::create_dir_all(&dist).ok();
        std::fs::write(
            dist.join("index.html"),
            "<!doctype html><title>Commander Lab</title><p>The UI has not been built. Run <code>npm run build</code> in <code>frontend/</code>.</p>",
        )
        .ok();
    }
    println!("cargo:rerun-if-changed=../frontend/dist");
}
