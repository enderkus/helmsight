//! Embeds the web UI. The built frontend in `web/dist` is copied into
//! `OUT_DIR/web`; when it has not been built, a placeholder page explains
//! how to build it so that Rust-only builds (CI lint jobs) still work.

use std::path::{Path, PathBuf};

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let dist = manifest.join("../../web/dist");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default()).join("web");
    println!("cargo:rerun-if-changed={}", dist.display());
    println!("cargo:rerun-if-env-changed=HELMSIGHT_WEB_DIST");
    println!("cargo:rerun-if-env-changed=HELMSIGHT_REQUIRE_WEB");
    let _ = std::fs::remove_dir_all(&out);
    let src = std::env::var_os("HELMSIGHT_WEB_DIST")
        .map(PathBuf::from)
        .unwrap_or(dist);
    if src.join("index.html").is_file() {
        if let Err(e) = copy_dir(&src, &out) {
            panic!("copying web UI from {}: {e}", src.display());
        }
    } else if std::env::var_os("HELMSIGHT_REQUIRE_WEB").is_some() {
        panic!(
            "HELMSIGHT_REQUIRE_WEB is set but {} is missing; build the web UI first",
            src.join("index.html").display()
        );
    } else {
        println!(
            "cargo:warning=web UI not built ({} missing); embedding a placeholder. Run `npm ci && npm run build` in web/.",
            src.join("index.html").display()
        );
        let _ = std::fs::create_dir_all(&out);
        let _ = std::fs::write(
            out.join("index.html"),
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Web UI not built</title></head>\
             <body><p>The web UI was not built into this binary. Build it with <code>npm ci &amp;&amp; npm run build</code> \
             in <code>web/</code> and rebuild. The API is available under <code>/api/v1</code>.</p></body></html>",
        );
    }
}
