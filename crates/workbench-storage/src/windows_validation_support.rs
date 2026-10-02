//! Shared by the two Windows-only, ignored validation test modules. Never linked
//! into an application build. Every writable case is newly created beneath an
//! explicitly marked synthetic parent; existing application profiles are refused.
use std::{fs, os::windows::fs::MetadataExt, path::{Component, Path, PathBuf}};

pub(crate) const ROOT_ENV: &str = "MANGAMONITOR_WINDOWS_VALIDATION_ROOT";
pub(crate) const ROOT_MARKER: &str = ".mangamonitor-windows-validation";
pub(crate) const CASE_MARKER: &str = ".mangamonitor-windows-validation-case";
pub(crate) const MARKER_TEXT: &str = "synthetic Windows validation only; no application profile or credentials";

fn no_redirects(path: &Path) {
    assert!(path.is_absolute(), "validation path must be absolute");
    let mut current = PathBuf::new();
    for component in path.components() {
        assert!(!matches!(component, Component::ParentDir | Component::CurDir));
        current.push(component);
        if matches!(component, Component::Prefix(_)) { continue; }
        let metadata = fs::symlink_metadata(&current).expect("validation ancestor must exist");
        assert!(metadata.is_dir() && metadata.file_attributes() & 0x400 == 0,
            "validation directories must not be redirected");
    }
}

fn marker(path: &Path, name: &str) {
    let path = path.join(name);
    let metadata = fs::symlink_metadata(&path).expect("synthetic marker required");
    assert!(metadata.is_file() && metadata.file_attributes() & 0x400 == 0);
    assert!(metadata.len() < 256);
    assert_eq!(fs::read_to_string(path).unwrap().trim(), MARKER_TEXT);
}

pub(crate) fn parent() -> PathBuf {
    let path = PathBuf::from(std::env::var_os(ROOT_ENV).expect("explicit synthetic root required"));
    no_redirects(&path);
    assert!(path.file_name().and_then(|v| v.to_str()).is_some_and(|v|
        v.starts_with("mangamonitor-windows-validation-")), "dedicated validation root required");
    marker(&path, ROOT_MARKER);
    fs::canonicalize(path).unwrap()
}

pub(crate) fn new_case(name: &str) -> tempfile::TempDir {
    assert!(name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'));
    let directory = tempfile::Builder::new()
        .prefix(&format!("case-{name}-"))
        .tempdir_in(parent()).unwrap();
    fs::write(directory.path().join(CASE_MARKER), MARKER_TEXT).unwrap();
    directory
}

pub(crate) fn verify_case(path: &Path) -> PathBuf {
    no_redirects(path);
    marker(path, CASE_MARKER);
    let path = fs::canonicalize(path).unwrap();
    assert_eq!(path.parent(), Some(parent().as_path()), "case must be a direct owned child");
    assert!(path.file_name().and_then(|v| v.to_str()).is_some_and(|v| v.starts_with("case-")));
    path
}
