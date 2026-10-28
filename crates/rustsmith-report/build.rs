//! Bake the building commit into `RUSTSMITH_GIT_SHA` (read by
//! `export::exporter_version` via `option_env!`). Outside a git checkout
//! (sdist builds) this degrades to `"unknown"` — never fail the build.
fn main() {
    let sha = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=RUSTSMITH_GIT_SHA={sha}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}
