fn main() {
    // Embed the Windows app manifest carrying requestedExecutionLevel
    // requireAdministrator (raw-disk access) + the Common-Controls v6
    // dependency (the DiskGenie base build recipe).
    let manifest = std::path::Path::new("windows-app-manifest.xml");
    tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new().app_manifest(manifest))
        .compile()
        .expect("failed to run tauri-build");
}
