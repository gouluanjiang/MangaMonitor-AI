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

fn synthetic_seed() -> workbench_storage::LibraryScanSeed {
    workbench_storage::LibraryScanSeed {
        id: "c".repeat(64),
        identity: Some(workbench_storage::LibraryFileIdentity {
            file_key: "d".repeat(64),
            bytes: 128,
            modified: "1:2".into(),
        }),
        added_at: Some(12),
        version_updated_at: Some("2026-09-01".into()),
        manual_override: true,
        source_ref: Some(LibraryReference {
            source: Source::Jm,
            work_id: "12345".into(),
        }),
        links: Vec::new(),
    }
}

#[test]
fn legacy_library_migrates_in_memory_and_only_an_explicit_write_publishes_v2() {
    let temporary = TempDir::new().unwrap();
    let store = WorkbenchStore::open(temporary.path()).unwrap();
    let path = temporary
        .path()
        .join(PRIVATE_DIRECTORY)
        .join("library.json");
    let mut legacy = serde_json::to_value(configured(temporary.path())).unwrap();
    legacy["version"] = serde_json::json!(1);
    let bytes =
        serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"revision":7,"value":legacy}))
            .unwrap();
    fs::write(&path, &bytes).unwrap();
    let loaded = store.read_library().unwrap();
    assert_eq!(loaded.revision, 7);
    assert_eq!(loaded.value.version, 2);
    assert!(loaded.value.scan_baseline.is_empty());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    store.write_library(loaded.revision, loaded.value).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["value"]["version"], 2);
    assert_eq!(saved["revision"], 8);
    assert!(saved["value"].get("scanBaseline").is_none());
}

#[test]
fn scan_seed_metadata_is_bounded_validated_and_never_valid_as_a_complete_catalog() {
    let temporary = TempDir::new().unwrap();
    let store = WorkbenchStore::open(temporary.path()).unwrap();
    let mut value = configured(temporary.path());
    value.scan_baseline = vec![synthetic_seed()];
    let saved = store.write_library(0, value).unwrap();
    assert_eq!(store.read_library().unwrap(), saved);
    let mut duplicate = saved.value.clone();
    duplicate.scan_baseline.push(synthetic_seed());
    let mut complete = saved.value.clone();
    complete.phase = LibraryPhase::Complete;
    let mut unsupported_manual = saved.value.clone();
    unsupported_manual.scan_baseline[0].identity = None;
    let mut invalid_identity = saved.value.clone();
    invalid_identity.scan_baseline[0]
        .identity
        .as_mut()
        .unwrap()
        .file_key = "not-a-hash".into();
    let mut invalid_date = saved.value.clone();
    invalid_date.scan_baseline[0].version_updated_at = Some("not-a-date".into());
    let mut too_many = saved.value.clone();
    too_many.scan_baseline = (0..=workbench_storage::MAX_LIBRARY_ITEMS * 2)
        .map(|index| {
            let mut seed = synthetic_seed();
            seed.id = format!("{index:064x}");
            seed
        })
        .collect();
    for invalid in [
        duplicate,
        complete,
        unsupported_manual,
        invalid_identity,
        invalid_date,
        too_many,
    ] {
        assert_eq!(
            store
                .write_library(saved.revision, invalid)
                .unwrap_err()
                .code,
            "VALIDATION_FAILED"
        );
        assert_eq!(store.read_library().unwrap(), saved);
    }
    let path = temporary
        .path()
        .join(PRIVATE_DIRECTORY)
        .join("library.json");
    for version in [1, 3] {
        let mut raw = serde_json::to_value(&saved.value).unwrap();
        raw["version"] = serde_json::json!(version);
        let bytes = serde_json::to_vec(
            &serde_json::json!({"schemaVersion":1,"revision":saved.revision,"value":raw}),
        )
        .unwrap();
        fs::write(&path, &bytes).unwrap();
        assert_eq!(
            store.read_library().unwrap_err().code,
            if version == 3 {
                "UNSUPPORTED_SCHEMA"
            } else {
                "DOCUMENT_CORRUPT"
            }
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}
