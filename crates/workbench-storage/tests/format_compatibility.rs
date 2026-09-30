use serde_json::{json, Value};
use std::fs;
use tempfile::TempDir;
use workbench_storage::{
    DiscoveryDocument, DownloadsDocument, LibraryDocument, WorkbenchStore, PRIVATE_DIRECTORY,
};

#[test]
fn v1_documents_upgrade_in_memory_without_mutating_bytes_or_revisions() {
    let temp = TempDir::new().unwrap();
    let store = WorkbenchStore::open(temp.path()).unwrap();
    let cases = [
        (
            "library.json",
            serde_json::to_value(LibraryDocument::default()).unwrap(),
        ),
        (
            "downloads.json",
            serde_json::to_value(DownloadsDocument::default()).unwrap(),
        ),
        (
            "discovery.json",
            serde_json::to_value(DiscoveryDocument::default()).unwrap(),
        ),
    ];
    for (file, mut value) in cases {
        value["version"] = json!(1);
        value.as_object_mut().unwrap().remove("scanBaseline");
        let bytes =
            serde_json::to_vec(&json!({"schemaVersion":1,"revision":7,"value":value})).unwrap();
        let path = temp.path().join(PRIVATE_DIRECTORY).join(file);
        fs::write(&path, &bytes).unwrap();
        let (revision, version) = match file {
            "library.json" => {
                let d = store.read_library().unwrap();
                (d.revision, d.value.version)
            }
            "downloads.json" => {
                let d = store.read_downloads().unwrap();
                (d.revision, d.value.version)
            }
            _ => {
                let d = store.read_discovery().unwrap();
                (d.revision, d.value.version)
            }
        };
        assert_eq!((revision, version), (7, 2));
        assert_eq!(
            fs::read(&path).unwrap(),
            bytes,
            "reads never publish a migration"
        );
    }
    let library = store.read_library().unwrap();
    store
        .write_library(library.revision, library.value)
        .unwrap();
    let downloads = store.read_downloads().unwrap();
    store
        .write_downloads(downloads.revision, downloads.value)
        .unwrap();
    let discovery = store.read_discovery().unwrap();
    store
        .write_discovery(discovery.revision, discovery.value)
        .unwrap();
    for file in ["library.json", "downloads.json", "discovery.json"] {
        let value: Value = serde_json::from_slice(
            &fs::read(temp.path().join(PRIVATE_DIRECTORY).join(file)).unwrap(),
        )
        .unwrap();
        assert_eq!(value["revision"], 8);
        assert_eq!(value["value"]["version"], 2);
        // v1.0.1's pre-deserialization guard (store.rs) must reject before
        // its deny_unknown_fields/old enum decoder ever sees new fields.
        assert!(
            value["schemaVersion"].as_u64().unwrap() > 1
                || value["value"]["version"].as_u64().unwrap() > 1
        );
    }
}

#[test]
fn future_or_corrupt_library_is_not_silently_migrated_or_overwritten() {
    let temp = TempDir::new().unwrap();
    let store = WorkbenchStore::open(temp.path()).unwrap();
    let path = temp.path().join(PRIVATE_DIRECTORY).join("library.json");
    for (version, extra, code) in [
        (3, false, "UNSUPPORTED_SCHEMA"),
        (2, true, "DOCUMENT_CORRUPT"),
        (0, false, "DOCUMENT_CORRUPT"),
    ] {
        let mut value = serde_json::to_value(LibraryDocument::default()).unwrap();
        value["version"] = json!(version);
        if extra {
            value["unknownFutureField"] = json!(true);
        }
        let bytes =
            serde_json::to_vec(&json!({"schemaVersion":1,"revision":7,"value":value})).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert_eq!(store.read_library().unwrap_err().code, code);
        assert_eq!(
            store
                .write_library(7, LibraryDocument::default())
                .unwrap_err()
                .code,
            code
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}
