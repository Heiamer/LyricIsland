//! Embed the activation manifest without a resource compiler.
//!
//! The Windows SDK's rc.exe is not installed on this machine, so the manifest is written into
//! the PE by `embed-manifest` (pure Rust) rather than by gpui's own `windows-manifest` feature.
//! PerMonitorV2 matters here: the tray menu and every layout measurement assume physical pixels.
fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_manifest::embed_manifest_file("app.manifest")
            .expect("the Windows activation manifest must be embedded");
    }
}
