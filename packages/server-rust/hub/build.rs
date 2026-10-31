fn main() {
    println!("cargo:rerun-if-changed=../../hub-ui/dist");
    assert!(
        std::path::Path::new("../../hub-ui/dist/index.html").is_file(),
        "Build dashboard assets with bun run build before compiling the native hub"
    );
}
