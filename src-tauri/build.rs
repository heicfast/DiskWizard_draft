//! Tauri build script: generates the WebView context + resources from
//! tauri.conf.json and capabilities/.
//!
//! The application MANIFEST (requireAdministrator + Common-Controls
//! v6) is supplied by the LINKER, not the resource compiler, so that
//! TEST binaries receive it too — the DiskGenie base recipe (the
//! STATUS_ENTRYPOINT_NOT_FOUND / TaskDialogIndirect fix: tauri's
//! windowing stack imports comctl32 v6 APIs, the resource-embedded
//! manifest lands in the app BIN only, and `cargo test` exes would
//! fail to LOAD without the side-by-side declaration).
fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
    )
    .expect("failed to run tauri-build");

    // Embed the Windows application manifest for every linked exe on
    // the msvc toolchain (app, test binaries, benches — all of them).
    #[cfg(windows)]
    {
        let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
        let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
        if target_os == "windows" && target_env == "msvc" {
            let manifest = std::env::current_dir()
                .expect("build script cwd")
                .join("windows-app-manifest.xml");
            println!("cargo:rerun-if-changed={}", manifest.display());
            println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
            println!(
                "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
                manifest.to_str().expect("utf-8 manifest path")
            );
        }
    }
}
