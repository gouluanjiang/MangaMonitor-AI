use std::fs;
use tempfile::TempDir;
use workbench_storage::{
    LibraryDocument, LibraryPhase, LibraryReference, LibraryRoot, Source, WorkbenchStore,
    PRIVATE_DIRECTORY,
};

fn configured(root: &std::path::Path) -> LibraryDocument {
    LibraryDocument {
        root: Some(LibraryRoot {
            id: "a".repeat(64),
            path: root.to_string_lossy().into_owned(),
            file_key: "b".repeat(64),
        }),
        generation: 1,
        phase: LibraryPhase::Reading,
        updated_at: Some(1),
        ..LibraryDocument::default()
    }
}

#[test]
fn library_document_roundtrip_cas_and_other_documents_are_independent() {
    let temporary = TempDir::new().unwrap();
    let store = WorkbenchStore::open(temporary.path()).unwrap();
    assert_eq!(store.read_library().unwrap().revision, 0);
    let phone_before = store.read_phone_library().unwrap();
    let prefs_before = store.read_preferences().unwrap();
    let books_before = store.read_booklists().unwrap();
    let following_before = store.read_following().unwrap();
    let saved = store
        .write_library(0, configured(temporary.path()))
        .unwrap();
    assert_eq!(
        store
            .write_library(0, saved.value.clone())
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
    drop(store);
    let reopened = WorkbenchStore::open(temporary.path()).unwrap();
    assert_eq!(reopened.read_library().unwrap(), saved);
    assert_eq!(reopened.read_phone_library().unwrap(), phone_before);
    assert_eq!(reopened.read_preferences().unwrap(), prefs_before);
    assert_eq!(reopened.read_booklists().unwrap(), books_before);
    assert_eq!(reopened.read_following().unwrap(), following_before);
}

#[test]
fn corrupt_and_future_library_documents_are_never_replaced() {
    for (original, code) in [
        (b"{broken".as_slice(), "DOCUMENT_CORRUPT"),
        (
            br#"{"schemaVersion":2,"revision":1,"value":{"version":2}}"#.as_slice(),
            "UNSUPPORTED_SCHEMA",
        ),
    ] {
        let temporary = TempDir::new().unwrap();
        let store = WorkbenchStore::open(temporary.path()).unwrap();
        let path = temporary
            .path()
            .join(PRIVATE_DIRECTORY)
            .join("library.json");
        fs::write(&path, original).unwrap();
        assert_eq!(store.read_library().unwrap_err().code, code);
        assert_eq!(
            store
                .write_library(0, configured(temporary.path()))
                .unwrap_err()
                .code,
            code
        );
        assert_eq!(fs::read(path).unwrap(), original);
    }
}

#[test]
fn temporary_candidate_is_not_valid_state_and_invalid_scope_cannot_persist() {
    let temporary = TempDir::new().unwrap();
    let store = WorkbenchStore::open(temporary.path()).unwrap();
    fs::write(
        temporary
            .path()
            .join(PRIVATE_DIRECTORY)
            .join(".library.json.999.1.tmp"),
        b"not current state",
    )
    .unwrap();
    assert_eq!(store.read_library().unwrap().revision, 0);
    let mut value = configured(temporary.path());
    value.generation = 0;
    assert_eq!(
        store.write_library(0, value).unwrap_err().code,
        "VALIDATION_FAILED"
    );
    let mut value = configured(temporary.path());
    value.root.as_mut().unwrap().path = "../not-an-absolute-root".into();
    assert_eq!(
        store.write_library(0, value).unwrap_err().code,
        "VALIDATION_FAILED"
    );
    assert_eq!(store.read_library().unwrap().revision, 0);
}

#[test]
fn references_require_canonical_platform_ids() {
    for id in ["0", "01", "-1", "123x", "123456789012345678901", ""] {
        assert!(!LibraryReference {
            source: Source::Jm,
            work_id: id.into()
        }
        .is_valid());
    }
    assert!(LibraryReference {
        source: Source::Jm,
        work_id: "12345678901234567890".into()
    }
    .is_valid());
    assert!(LibraryReference {
        source: Source::Pica,
        work_id: "0123456789abcdef01234567".into()
    }
    .is_valid());
    assert!(!LibraryReference {
        source: Source::Pica,
        work_id: "0123456789ABCDEF01234567".into()
    }
    .is_valid());
}
