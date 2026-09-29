//! Synthetic ZIPs only. These tests never inspect or register a user's library.
use image::{DynamicImage, ImageFormat};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Cursor, Write},
};
use tempfile::TempDir;
use workbench_library::{
    import_reviewed_library, preview_reviewed_library, reviewed_library_presence, LibraryPhase,
    LibraryService, ReviewedFilePresence, ScanAction,
};
use workbench_storage::WorkbenchStore;
use zip::{write::SimpleFileOptions, ZipWriter};

struct Fixture {
    _temp: TempDir,
    store: WorkbenchStore,
    root: std::path::PathBuf,
}

fn fixture() -> Fixture {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("library");
    fs::create_dir(&root).unwrap();
    let mut image = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(8, 8)
        .write_to(&mut image, ImageFormat::Png)
        .unwrap();
    for name in ["first.zip", "second.zip"] {
        let mut zip = ZipWriter::new(File::create(root.join(name)).unwrap());
        // Deliberate old partial chapter: explicit review accepts this file as
        // owned without claiming it is complete or mutating the media.
        for page in [".下载中-chapter/001.png", ".下载中-chapter/003.png"] {
            zip.start_file(page, SimpleFileOptions::default()).unwrap();
            zip.write_all(image.get_ref()).unwrap();
        }
        zip.finish().unwrap();
    }
    let store = WorkbenchStore::open(temp.path().join("private")).unwrap();
    scan(&store, &root);
    Fixture {
        _temp: temp,
        store,
        root,
    }
}

fn scan(store: &WorkbenchStore, root: &std::path::Path) {
    let mut service = LibraryService::new();
    let mut snapshot = service.choose(store, root).unwrap();
    while snapshot.phase == LibraryPhase::Reading {
        snapshot = service
            .scan(
                store,
                snapshot.root_id.as_deref().unwrap(),
                snapshot.generation,
                ScanAction::Next,
            )
            .unwrap();
    }
    assert_eq!(snapshot.phase, LibraryPhase::Complete);
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn manifest(f: &Fixture) -> Value {
    let content = fs::read(f.root.join("first.zip")).unwrap();
    json!({"schemaVersion":1,"root":f.store.read_library().unwrap().value.root,
        "items":[{"relativePath":"first.zip","bytes":content.len(),"sha256":sha(&content),
        "references":[{"source":"JM","workId":"7654321"},{"source":"Pica","workId":"0123456789abcdef01234567"}]}]})
}
fn apply(
    f: &Fixture,
    value: &Value,
) -> workbench_library::Result<workbench_library::ReviewedLibraryImport> {
    let bytes = serde_json::to_vec(value).unwrap();
    import_reviewed_library(
        &f.store,
        &bytes,
        f.store.read_library().unwrap().revision,
        &sha(&bytes),
    )
}

#[test]
fn explicit_review_is_separate_from_download_history_and_partial_media_is_unchanged() {
    let f = fixture();
    let value = manifest(&f);
    let library_before = f.store.read_library().unwrap();
    let downloads_before = f.store.read_downloads().unwrap();
    let media_before = fs::read(f.root.join("first.zip")).unwrap();
    let preview = preview_reviewed_library(&f.store, &serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(preview.references, 2);
    assert_eq!(preview.added, 2);
    assert!(!preview.applied);
    assert!(!preview.file_hashes_verified);
    assert_eq!(library_before, f.store.read_library().unwrap());
    assert!(reviewed_library_presence(&library_before.value).is_empty());
    let report = apply(&f, &value).unwrap();
    assert!(report.applied && report.file_hashes_verified);
    let after = f.store.read_library().unwrap();
    assert_eq!(after.value.records, library_before.value.records);
    assert_eq!(after.value.reviewed_works.len(), 2);
    assert!(reviewed_library_presence(&after.value)
        .iter()
        .all(|r| r.presence == ReviewedFilePresence::Present));
    assert_eq!(downloads_before, f.store.read_downloads().unwrap());
    assert_eq!(media_before, fs::read(f.root.join("first.zip")).unwrap());
}

#[test]
fn repeat_import_is_idempotent_and_keeps_original_review_evidence() {
    let f = fixture();
    let value = manifest(&f);
    apply(&f, &value).unwrap();
    let before = f.store.read_library().unwrap();
    let result = apply(&f, &value).unwrap();
    assert_eq!(result.added, 0);
    assert_eq!(result.unchanged, 2);
    assert_eq!(f.store.read_library().unwrap(), before);
}

#[test]
fn stale_revision_or_changed_manifest_cannot_import() {
    let f = fixture();
    let bytes = serde_json::to_vec(&manifest(&f)).unwrap();
    let before = f.store.read_library().unwrap();
    assert_eq!(
        import_reviewed_library(&f.store, &bytes, before.revision + 1, &sha(&bytes))
            .unwrap_err()
            .code,
        "LIBRARY_STALE_SNAPSHOT"
    );
    assert_eq!(
        import_reviewed_library(&f.store, &bytes, before.revision, &"0".repeat(64))
            .unwrap_err()
            .code,
        "LIBRARY_REVIEW_CHANGED"
    );
    assert_eq!(before, f.store.read_library().unwrap());
}

#[test]
fn wrong_root_unsafe_paths_and_duplicate_source_ids_are_rejected() {
    let f = fixture();
    let original = manifest(&f);
    let before = f.store.read_library().unwrap();
    let mut wrong = original.clone();
    wrong["root"]["id"] = json!("0".repeat(64));
    assert_eq!(
        apply(&f, &wrong).unwrap_err().code,
        "LIBRARY_REVIEW_ROOT_MISMATCH"
    );
    let mut wrong = original.clone();
    wrong["items"][0]["relativePath"] = json!("../outside.zip");
    assert_eq!(
        apply(&f, &wrong).unwrap_err().code,
        "LIBRARY_REVIEW_INVALID"
    );
    let mut wrong = original.clone();
    let mut second = wrong["items"][0].clone();
    second["relativePath"] = json!("second.zip");
    wrong["items"].as_array_mut().unwrap().push(second);
    assert_eq!(
        apply(&f, &wrong).unwrap_err().code,
        "LIBRARY_REVIEW_CONFLICT"
    );
    assert_eq!(before, f.store.read_library().unwrap());
}

#[test]
fn a_bad_last_hash_leaves_the_whole_batch_unregistered() {
    let f = fixture();
    let mut value = manifest(&f);
    let content = fs::read(f.root.join("second.zip")).unwrap();
    value["items"]
        .as_array_mut()
        .unwrap()
        .push(json!({"relativePath":"second.zip","bytes":content.len(),
        "sha256":"0".repeat(64),"references":[{"source":"JM","workId":"7654322"}]}));
    let before = f.store.read_library().unwrap();
    assert_eq!(apply(&f, &value).unwrap_err().code, "LIBRARY_FILE_CHANGED");
    assert_eq!(before, f.store.read_library().unwrap());
}

#[test]
fn confirmed_reference_cannot_silently_switch_to_another_file() {
    let f = fixture();
    let mut value = manifest(&f);
    apply(&f, &value).unwrap();
    let before = f.store.read_library().unwrap();
    let content = fs::read(f.root.join("second.zip")).unwrap();
    value["items"][0]["relativePath"] = json!("second.zip");
    value["items"][0]["sha256"] = json!(sha(&content));
    assert_eq!(
        apply(&f, &value).unwrap_err().code,
        "LIBRARY_REVIEW_CONFLICT"
    );
    assert_eq!(before, f.store.read_library().unwrap());
}

#[test]
fn same_root_rescan_preserves_reviews_but_new_root_does_not_inherit_them() {
    let f = fixture();
    apply(&f, &manifest(&f)).unwrap();
    let before = f.store.read_library().unwrap().value.reviewed_works;
    scan(&f.store, &f.root);
    let after = f.store.read_library().unwrap().value;
    assert_eq!(after.reviewed_works, before);
    assert!(reviewed_library_presence(&after)
        .iter()
        .all(|r| r.presence == ReviewedFilePresence::Present));
    let other = f._temp.path().join("other");
    fs::create_dir(&other).unwrap();
    scan(&f.store, &other);
    assert!(f
        .store
        .read_library()
        .unwrap()
        .value
        .reviewed_works
        .is_empty());
}

#[test]
fn missing_or_replaced_files_are_not_owned_even_after_a_rescan() {
    let f = fixture();
    apply(&f, &manifest(&f)).unwrap();
    let saved = f.store.read_library().unwrap();
    let bytes = fs::read(f.root.join("first.zip")).unwrap();
    fs::rename(f.root.join("first.zip"), f.root.join("moved.zip")).unwrap();
    assert!(reviewed_library_presence(&saved.value)
        .iter()
        .all(|r| r.presence == ReviewedFilePresence::Missing));
    // Same name and byte count do not permit adoption of a new file identity.
    fs::write(f.root.join("first.zip"), bytes).unwrap();
    assert!(reviewed_library_presence(&saved.value)
        .iter()
        .all(|r| r.presence == ReviewedFilePresence::Changed));
    scan(&f.store, &f.root);
    assert!(
        reviewed_library_presence(&f.store.read_library().unwrap().value)
            .iter()
            .all(|r| r.presence == ReviewedFilePresence::Changed)
    );
}
