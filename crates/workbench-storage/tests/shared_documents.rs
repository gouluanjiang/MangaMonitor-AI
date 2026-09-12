use std::{fs, path::Path, sync::Arc};
use tempfile::TempDir;
use workbench_storage::{DownloadsDocument, LibraryDocument, WorkbenchStore, PRIVATE_DIRECTORY};

fn replace_preserving_modified(path: &Path, bytes: &[u8]) {
    let before = fs::metadata(path).unwrap();
    assert_eq!(before.len(), bytes.len() as u64);
    let modified = before.modified().unwrap();
    fs::write(path, bytes).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), modified);
}

#[test]
fn shared_reads_reuse_exact_bytes_and_keep_two_documents_independent() {
    let temp = TempDir::new().unwrap();
    let store = WorkbenchStore::open(temp.path()).unwrap();
    store.write_library(0, LibraryDocument::default()).unwrap();
    store
        .write_downloads(0, DownloadsDocument::default())
        .unwrap();
    let library = store.read_library_shared().unwrap();
    let downloads = store.read_downloads_shared().unwrap();
    assert!(Arc::ptr_eq(&library, &store.read_library_shared().unwrap()));
    assert!(Arc::ptr_eq(
        &downloads,
        &store.read_downloads_shared().unwrap()
    ));
    assert_eq!(store.read_library().unwrap(), *library);
    assert_eq!(store.read_downloads().unwrap(), *downloads);

    store
        .write_downloads(1, DownloadsDocument::default())
        .unwrap();
    let next = store.read_downloads_shared().unwrap();
    assert!(!Arc::ptr_eq(&downloads, &next));
    assert_eq!(downloads.revision, 1);
    assert_eq!(next.revision, 2);
    assert!(Arc::ptr_eq(&library, &store.read_library_shared().unwrap()));
}

#[test]
fn equal_length_and_modified_time_never_hide_changed_or_invalid_bytes() {
    let temp = TempDir::new().unwrap();
    let store = WorkbenchStore::open(temp.path()).unwrap();
    store.write_library(0, LibraryDocument::default()).unwrap();
    let original = store.read_library_shared().unwrap();
    let path = temp.path().join(PRIVATE_DIRECTORY).join("library.json");
    let bytes = fs::read_to_string(&path).unwrap();
    let changed = bytes.replace("\"revision\":1", "\"revision\":2");
    assert_ne!(changed, bytes);
    replace_preserving_modified(&path, changed.as_bytes());
    let refreshed = store.read_library_shared().unwrap();
    assert_eq!(refreshed.revision, 2);
    assert!(!Arc::ptr_eq(&original, &refreshed));

    let invalid = changed.replace("\"revision\":2", "\"revision\":0");
    replace_preserving_modified(&path, invalid.as_bytes());
    assert_eq!(
        store.read_library_shared().unwrap_err().code,
        "DOCUMENT_CORRUPT"
    );
    assert_eq!(store.read_library().unwrap_err().code, "DOCUMENT_CORRUPT");
    assert_eq!(
        store
            .write_library(2, LibraryDocument::default())
            .unwrap_err()
            .code,
        "DOCUMENT_CORRUPT"
    );
    assert_eq!(fs::read(&path).unwrap(), invalid.as_bytes());
}

#[test]
fn another_store_atomic_commit_invalidates_cached_revision_before_cas() {
    let temp = TempDir::new().unwrap();
    let first = WorkbenchStore::open(temp.path()).unwrap();
    let second = WorkbenchStore::open(temp.path()).unwrap();
    first
        .write_downloads(0, DownloadsDocument::default())
        .unwrap();
    let original = first.read_downloads_shared().unwrap();
    second
        .write_downloads(1, DownloadsDocument::default())
        .unwrap();
    assert_eq!(
        first
            .write_downloads(1, DownloadsDocument::default())
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
    let current = first.read_downloads_shared().unwrap();
    assert_eq!(current.revision, 2);
    assert!(!Arc::ptr_eq(&original, &current));
    assert!(Arc::ptr_eq(
        &current,
        &first.read_downloads_shared().unwrap()
    ));
}

#[test]
fn cached_documents_do_not_mask_future_schema_replacement_or_unsafe_files() {
    let temp = TempDir::new().unwrap();
    let store = WorkbenchStore::open(temp.path()).unwrap();
    store.write_library(0, LibraryDocument::default()).unwrap();
    let path = temp.path().join(PRIVATE_DIRECTORY).join("library.json");
    let valid = fs::read_to_string(&path).unwrap();
    for future in [
        valid.replace("\"schemaVersion\":1", "\"schemaVersion\":2"),
        valid.replace("\"version\":1", "\"version\":2"),
    ] {
        fs::write(&path, &valid).unwrap();
        let _cached = store.read_library_shared().unwrap();
        assert_ne!(future, valid);
        let replacement = path.with_extension("replacement");
        fs::write(&replacement, &future).unwrap();
        fs::rename(&replacement, &path).unwrap();
        assert_eq!(
            store.read_library_shared().unwrap_err().code,
            "UNSUPPORTED_SCHEMA"
        );
        assert_eq!(
            store
                .write_library(1, LibraryDocument::default())
                .unwrap_err()
                .code,
            "UNSUPPORTED_SCHEMA"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), future);
    }

    fs::write(&path, &valid).unwrap();
    let _cached = store.read_library_shared().unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(32 * 1024 * 1024 + 1)
        .unwrap();
    assert_eq!(
        store.read_library_shared().unwrap_err().code,
        "DOCUMENT_TOO_LARGE"
    );
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert_eq!(store.read_library_shared().unwrap_err().code, "UNSAFE_PATH");
    fs::remove_dir(&path).unwrap();
    assert_eq!(store.read_library_shared().unwrap().revision, 0);
}

#[cfg(unix)]
#[test]
fn cached_document_still_rejects_a_symlink_to_identical_bytes() {
    let temp = TempDir::new().unwrap();
    let store = WorkbenchStore::open(temp.path()).unwrap();
    store.write_library(0, LibraryDocument::default()).unwrap();
    let path = temp.path().join(PRIVATE_DIRECTORY).join("library.json");
    let outside = temp.path().join("outside.json");
    fs::write(&outside, fs::read(&path).unwrap()).unwrap();
    let _cached = store.read_library_shared().unwrap();
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&outside, &path).unwrap();
    assert_eq!(store.read_library_shared().unwrap_err().code, "UNSAFE_PATH");
    assert_eq!(
        store
            .write_library(1, LibraryDocument::default())
            .unwrap_err()
            .code,
        "UNSAFE_PATH"
    );
}
