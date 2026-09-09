fn main() {
    // Application commands otherwise bypass Tauri's capability checks by default.
    let manifest = tauri_build::AppManifest::new().commands(&[
        "read_preferences",
        "write_preferences",
        "read_booklists",
        "write_booklists",
        "choose_background",
        "source_accounts",
        "source_login",
        "source_logout",
        "source_query",
        "source_favorite",
        "source_cover",
        "source_following",
        "source_follow",
    ]);
    let windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let mut attributes = tauri_build::Attributes::new().app_manifest(manifest);
    if windows_msvc {
        // Link the same manifest into the application and its unit-test harness.
        // Tauri's resource-only default leaves tests unable to load comctl32 v6:
        // https://github.com/tauri-apps/tauri/issues/13419
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    }
    tauri_build::try_build(attributes).expect("failed to build the development workbench");

    if windows_msvc {
        let manifest = std::path::PathBuf::from(
            std::env::var_os("CARGO_MANIFEST_DIR").expect("package directory missing"),
        )
        .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}
