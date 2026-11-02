fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_BROWSER");
    if std::env::var_os("CARGO_FEATURE_BROWSER").is_none() {
        return;
    }
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let packaged = root.join("assets");
    let checkout = root.join("../../packages/hub-ui/dist");
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=../../packages/hub-ui/dist");
    let assets = if packaged.join("index.html").is_file() {
        packaged
    } else {
        checkout
    };
    assert!(
        assets.join("index.html").is_file(),
        "Missing dashboard assets: run bun run build:native in a source checkout"
    );
    println!(
        "cargo:rustc-env=OMOCHI_DASHBOARD_ASSETS={}",
        assets.canonicalize().unwrap().display()
    );
}
