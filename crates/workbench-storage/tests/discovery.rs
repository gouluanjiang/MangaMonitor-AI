use std::fs;
use tempfile::TempDir;
use workbench_storage::{
    AccountFollowing, DiscoveryAccount, DiscoveryAuthorRange, DiscoveryDocument,
    DiscoveryRangeState, DiscoveryRecord, DiscoveryWork, FollowedAccount, Source, WorkbenchStore,
    PRIVATE_DIRECTORY,
};

fn record(source: Source, id: &str) -> DiscoveryRecord {
    DiscoveryRecord {
        work: DiscoveryWork {
            source,
            work_id: id.into(),
            title: "合成作品 [中文]".into(),
            authors: vec!["作者".into()],
            description: None,
            tags: vec!["中文".into()],
            favorite: None,
            chapter_count: Some(1),
            page_count: Some(20),
            cover_available: true,
        },
        matched_authors: vec!["作者".into()],
        author_verified: true,
        observed_at: 10,
        scan_id: "a".repeat(64),
    }
}

fn document() -> DiscoveryDocument {
    DiscoveryDocument {
        version: 1,
        accounts: vec![DiscoveryAccount {
            account_key: "b".repeat(64),
            authors: vec![DiscoveryAuthorRange {
                author: "作者".into(),
                source: Source::Jm,
                state: DiscoveryRangeState::Complete,
                last_attempt_at: Some(10),
                last_complete_at: Some(10),
                observed_count: 1,
                pages_read: 1,
                error_code: None,
            }],
            records: vec![record(Source::Jm, "123")],
        }],
    }
}

#[test]
fn discovery_roundtrip_is_independent_of_library_phone_and_following() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    assert_eq!(store.read_discovery().unwrap().revision, 0);
    let saved = store
        .write_discovery_for_following(0, 0, document())
        .unwrap();
    assert_eq!(saved.value, document());
    let reopened = WorkbenchStore::open(directory.path()).unwrap();
    assert_eq!(reopened.read_discovery().unwrap(), saved);
    assert_eq!(reopened.read_following().unwrap().revision, 0);
    assert_eq!(reopened.read_library().unwrap().revision, 0);
    assert_eq!(reopened.read_phone_library().unwrap().revision, 0);
    let bytes = fs::read_to_string(
        directory
            .path()
            .join(PRIVATE_DIRECTORY)
            .join("discovery.json"),
    )
    .unwrap();
    for secret_field in ["sessionId", "token", "cookie", "accountId", "coverUrl"] {
        assert!(!bytes.contains(secret_field));
    }
}

#[test]
fn current_following_and_discovery_revisions_are_checked_together() {
    let directory = TempDir::new().unwrap();
    let first = WorkbenchStore::open(directory.path()).unwrap();
    let second = WorkbenchStore::open(directory.path()).unwrap();
    first
        .write_discovery_for_following(0, 0, document())
        .unwrap();
    second
        .write_following(0, AccountFollowing::default())
        .unwrap();
    assert_eq!(
        first
            .write_discovery_for_following(1, 0, DiscoveryDocument::default())
            .unwrap_err()
            .code,
        "DISCOVERY_FOLLOWING_CHANGED"
    );
    assert_eq!(first.read_discovery().unwrap().value, document());
    assert_eq!(
        first
            .write_discovery_for_following(0, 1, DiscoveryDocument::default())
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
}

#[test]
fn identity_evidence_and_source_keys_cannot_be_forged_by_corrupt_metadata() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    store.write_discovery(0, document()).unwrap();
    let mut bad_author = document();
    bad_author.accounts[0].records[0].matched_authors = vec!["不同作者".into()];
    let mut duplicate = document();
    let repeated = duplicate.accounts[0].records[0].clone();
    duplicate.accounts[0].records.push(repeated);
    let mut wrong_id = document();
    wrong_id.accounts[0].records[0].work.work_id = "../escape".into();
    let mut account_key = document();
    account_key.accounts[0].account_key = "renderer-picked-account".into();
    let mut scan = document();
    scan.accounts[0].records[0].scan_id = "not-native-id".into();
    for value in [bad_author, duplicate, wrong_id, account_key, scan] {
        assert_eq!(
            store.write_discovery(1, value).unwrap_err().code,
            "VALIDATION_FAILED"
        );
        assert_eq!(store.read_discovery().unwrap().value, document());
    }
    let mut uncertain = document();
    uncertain.accounts[0].records[0].work.authors.clear();
    uncertain.accounts[0].records[0].author_verified = false;
    store.write_discovery(1, uncertain).unwrap();
}

#[test]
fn malformed_or_future_document_is_not_replaced() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let path = directory
        .path()
        .join(PRIVATE_DIRECTORY)
        .join("discovery.json");
    for (contents, code) in [
        ("broken".to_owned(), "DOCUMENT_CORRUPT"),
        (
            serde_json::json!({"schemaVersion":2,"revision":1,"value":document()}).to_string(),
            "UNSUPPORTED_SCHEMA",
        ),
    ] {
        fs::write(&path, &contents).unwrap();
        assert_eq!(store.write_discovery(0, document()).unwrap_err().code, code);
        assert_eq!(fs::read_to_string(&path).unwrap(), contents);
    }
}

#[test]
fn current_three_hundred_author_following_fits_without_truncation() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let following = AccountFollowing {
        version: 1,
        accounts: vec![FollowedAccount {
            source: Source::Jm,
            account_key: "c".repeat(64),
            works: vec![],
            authors: (0..1000).map(|number| format!("作者{number}")).collect(),
        }],
    };
    let saved = store.write_following(0, following.clone()).unwrap();
    assert_eq!(saved.value, following);
    assert_eq!(saved.value.accounts[0].authors.len(), 1000);
}

#[cfg(unix)]
#[test]
fn discovery_rejects_symlink_without_touching_external_target() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let outside = directory.path().join("external.json");
    fs::write(&outside, b"unchanged").unwrap();
    std::os::unix::fs::symlink(
        &outside,
        directory
            .path()
            .join(PRIVATE_DIRECTORY)
            .join("discovery.json"),
    )
    .unwrap();
    assert_eq!(
        store.write_discovery(0, document()).unwrap_err().code,
        "UNSAFE_PATH"
    );
    assert_eq!(fs::read(outside).unwrap(), b"unchanged");
}
