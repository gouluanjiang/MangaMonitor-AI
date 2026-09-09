fn main() {
    // Application commands otherwise bypass Tauri's capability checks by default.
    let manifest = tauri_build::AppManifest::new().commands(&[
        "read_preferences",
        "write_preferences",
        "read_booklists",
        "write_booklists",
        "choose_background",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("failed to build the development workbench");
}
