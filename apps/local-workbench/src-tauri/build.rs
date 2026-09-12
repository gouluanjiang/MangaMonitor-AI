fn main() {
    // Application commands otherwise bypass Tauri's capability checks by default.
    let manifest = tauri_build::AppManifest::new().commands(&[
        "read_preferences",
        "write_preferences",
        "read_booklists",
        "write_booklists",
        "choose_background",
        "discovery_read",
        "discovery_start",
        "discovery_cancel",
        "completeness_read",
        "completeness_start",
        "completeness_cancel",
        "completeness_settings_read",
        "completeness_family_confirm",
        "completeness_family_unlink",
        "completeness_language_set",
        "jm_download_prepare",
        "jm_download_confirm",
        "jm_download_read",
        "jm_download_control",
        "jm_download_batch_prepare",
        "jm_download_batch_confirm",
        "jm_download_pause_all",
        "jm_download_resume_many",
        "jm_download_history_remove",
        "source_matches_read",
        "source_matches_confirm",
        "source_matches_unlink",
        "library_read",
        "library_choose",
        "library_scan",
        "library_cover",
        "library_link",
        "phone_library_read",
        "phone_library_import",
        "phone_library_mark",
        "phone_library_unmark",
        "source_accounts",
        "source_login",
        "source_logout",
        "source_query",
        "source_catalog",
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
