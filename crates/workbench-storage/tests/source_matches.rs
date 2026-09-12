use std::fs;
use tempfile::TempDir;
use workbench_storage::{
    phone_library_mark, source_matches_confirm, source_matches_read, source_matches_unlink,
    LibraryDocument, LibraryReference, Source, SourceMatchWork, WorkbenchStore, PRIVATE_DIRECTORY,
};

// All identities, titles and files here are synthetic. No media requests are made.
const PICA: &str = "0123456789abcdef01234567";
const OTHER_PICA: &str = "fedcba9876543210fedcba98";

fn work(source: Source, id: &str) -> SourceMatchWork {
    SourceMatchWork {
        source,
        work_id: id.to_owned(),
        title: "Synthetic work [edition A]".into(),
    }
}

#[test]
fn manual_pair_survives_restart_and_unlink_preserves_both_libraries_and_media() {
    let fixture = TempDir::new().unwrap();
    let store = WorkbenchStore::open(fixture.path()).unwrap();
    let private = fixture.path().join(PRIVATE_DIRECTORY);
    let media = fixture.path().join("synthetic-original.zip");
    fs::write(&media, b"untouched existing bytes").unwrap();
    store.write_library(0, LibraryDocument::default()).unwrap();
    phone_library_mark(
        &store,
        0,
        "Phone edition [A]".into(),
        Some(LibraryReference {
            source: Source::Jm,
            work_id: "123".into(),
        }),
    )
    .unwrap();
    let phone_bytes = fs::read(private.join("phone-library.json")).unwrap();
    let library_bytes = fs::read(private.join("library.json")).unwrap();
    assert!(source_matches_read(&store).unwrap().pairs.is_empty());
    let confirmed = source_matches_confirm(
        &store,
        0,
        work(Source::Jm, "123"),
        SourceMatchWork {
            title: "Different source title".into(),
            ..work(Source::Pica, PICA)
        },
    )
    .unwrap();
    assert_eq!(confirmed.pairs.len(), 1);
    assert_eq!(confirmed.pairs[0].jm.work_id, "123");
    assert_eq!(confirmed.pairs[0].pica.work_id, PICA);
    assert_eq!(
        fs::read(private.join("phone-library.json")).unwrap(),
        phone_bytes
    );
    assert_eq!(
        fs::read(private.join("library.json")).unwrap(),
        library_bytes
    );
    drop(store);
    let reopened = WorkbenchStore::open(fixture.path()).unwrap();
    assert_eq!(source_matches_read(&reopened).unwrap(), confirmed);
    let unlinked =
        source_matches_unlink(&reopened, confirmed.revision, &confirmed.pairs[0].id).unwrap();
    assert!(unlinked.pairs.is_empty());
    assert_eq!(unlinked.revision, confirmed.revision + 1);
    assert_eq!(
        fs::read(private.join("phone-library.json")).unwrap(),
        phone_bytes
    );
    assert_eq!(
        fs::read(private.join("library.json")).unwrap(),
        library_bytes
    );
    assert_eq!(fs::read(media).unwrap(), b"untouched existing bytes");
}

#[test]
fn either_side_conflict_and_stale_revision_never_replace_a_confirmed_identity() {
    let fixture = TempDir::new().unwrap();
    let first = WorkbenchStore::open(fixture.path()).unwrap();
    let second = WorkbenchStore::open(fixture.path()).unwrap();
    let confirmed =
        source_matches_confirm(&first, 0, work(Source::Jm, "123"), work(Source::Pica, PICA))
            .unwrap();
    let path = fixture
        .path()
        .join(PRIVATE_DIRECTORY)
        .join("source-matches.json");
    let original = fs::read(&path).unwrap();
    for (jm, pica, revision, code) in [
        (
            "123",
            OTHER_PICA,
            confirmed.revision,
            "SOURCE_MATCH_CONFLICT",
        ),
        ("456", PICA, confirmed.revision, "SOURCE_MATCH_CONFLICT"),
        ("123", PICA, 0, "REVISION_CONFLICT"),
    ] {
        assert_eq!(
            source_matches_confirm(
                &second,
                revision,
                work(Source::Jm, jm),
                work(Source::Pica, pica)
            )
            .unwrap_err()
            .code,
            code
        );
        assert_eq!(fs::read(&path).unwrap(), original);
    }
    assert_eq!(
        source_matches_unlink(&second, 0, &confirmed.pairs[0].id)
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    let duplicate = source_matches_confirm(
        &second,
        confirmed.revision,
        work(Source::Jm, "123"),
        work(Source::Pica, PICA),
    )
    .unwrap();
    assert_eq!(duplicate, confirmed);
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn invalid_ids_titles_and_swapped_sources_cannot_create_an_alias() {
    let fixture = TempDir::new().unwrap();
    let store = WorkbenchStore::open(fixture.path()).unwrap();
    for jm in [
        work(Source::Jm, "0"),
        work(Source::Jm, "0123"),
        work(Source::Jm, "../123"),
        work(Source::Jm, "123456789012345678901"),
        work(Source::Pica, PICA),
        SourceMatchWork {
            title: "\n".into(),
            ..work(Source::Jm, "123")
        },
        SourceMatchWork {
            title: "x".repeat(1025),
            ..work(Source::Jm, "123")
        },
    ] {
        assert_eq!(
            source_matches_confirm(&store, 0, jm, work(Source::Pica, PICA))
                .unwrap_err()
                .code,
            "SOURCE_MATCH_INVALID"
        );
    }
    for pica in [PICA.to_uppercase(), "123".into(), format!("../{PICA}")] {
        assert_eq!(
            source_matches_confirm(
                &store,
                0,
                work(Source::Jm, "123"),
                work(Source::Pica, &pica)
            )
            .unwrap_err()
            .code,
            "SOURCE_MATCH_INVALID"
        );
    }
    assert!(source_matches_read(&store).unwrap().pairs.is_empty());
    assert!(!fixture
        .path()
        .join(PRIVATE_DIRECTORY)
        .join("source-matches.json")
        .exists());
}

#[test]
fn malformed_or_future_pair_documents_are_never_silently_replaced() {
    for (bytes, code) in [
        (b"{broken".as_slice(), "DOCUMENT_CORRUPT"),
        (
            br#"{"schemaVersion":2,"revision":1,"value":{"version":2}}"#.as_slice(),
            "UNSUPPORTED_SCHEMA",
        ),
    ] {
        let fixture = TempDir::new().unwrap();
        let store = WorkbenchStore::open(fixture.path()).unwrap();
        let path = fixture
            .path()
            .join(PRIVATE_DIRECTORY)
            .join("source-matches.json");
        fs::write(&path, bytes).unwrap();
        assert_eq!(source_matches_read(&store).unwrap_err().code, code);
        assert_eq!(
            source_matches_confirm(&store, 0, work(Source::Jm, "123"), work(Source::Pica, PICA))
                .unwrap_err()
                .code,
            code
        );
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn stored_identity_and_manual_evidence_are_revalidated_when_read() {
    for mutation in ["identity", "evidence", "conflict"] {
        let fixture = TempDir::new().unwrap();
        let store = WorkbenchStore::open(fixture.path()).unwrap();
        source_matches_confirm(&store, 0, work(Source::Jm, "123"), work(Source::Pica, PICA))
            .unwrap();
        let path = fixture
            .path()
            .join(PRIVATE_DIRECTORY)
            .join("source-matches.json");
        let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        match mutation {
            "identity" => raw["value"]["pairs"][0]["jm"]["workId"] = "456".into(),
            "evidence" => raw["value"]["pairs"][0]["evidence"] = "filename".into(),
            "conflict" => {
                let duplicate = raw["value"]["pairs"][0].clone();
                raw["value"]["pairs"]
                    .as_array_mut()
                    .unwrap()
                    .push(duplicate);
            }
            _ => unreachable!(),
        }
        let original = serde_json::to_vec(&raw).unwrap();
        fs::write(&path, &original).unwrap();
        assert!(source_matches_read(&store).is_err());
        assert_eq!(fs::read(path).unwrap(), original);
    }
}
