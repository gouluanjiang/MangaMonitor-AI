use std::fs;
use tempfile::TempDir;
use workbench_storage::{
    AccountFollowing, DiscoveryAccount, DiscoveryAuthorRange, DiscoveryBaseline, DiscoveryDocument,
    DiscoveryMode, DiscoveryPagePatch, DiscoveryRangeState, DiscoveryRecord, DiscoveryWork,
    FollowedAccount, Source, WorkbenchStore, MAX_DISCOVERY_RECORDS, PRIVATE_DIRECTORY,
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
            source_updated_at: None,
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
                last_checked_at: None,
                last_check_mode: None,
                baseline: None,
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
fn legacy_ranges_load_without_claiming_an_incremental_checkpoint() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let value = serde_json::to_value(document()).unwrap();
    assert!(value["accounts"][0]["authors"][0].get("baseline").is_none());
    assert!(value["accounts"][0]["records"][0]["work"]
        .get("sourceUpdatedAt")
        .is_none());
    fs::write(
        directory
            .path()
            .join(PRIVATE_DIRECTORY)
            .join("discovery.json"),
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion":1,"revision":7,"value":value
        }))
        .unwrap(),
    )
    .unwrap();
    let saved = store.read_discovery().unwrap();
    assert_eq!(saved.revision, 7);
    assert_eq!(saved.value, document());
    assert_eq!(saved.value.accounts[0].authors[0].baseline, None);
    assert_eq!(
        saved.value.accounts[0].records[0].work.source_updated_at,
        None
    );
}

#[test]
fn source_dates_survive_page_journal_and_checkpoint_without_observation_substitution() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let mut original = document();
    original.accounts[0].records[0].work.source_updated_at = Some("2026-09-15".into());
    store.write_discovery(0, original.clone()).unwrap();
    let legacy = directory
        .path()
        .join(PRIVATE_DIRECTORY)
        .join("discovery.json");
    let before = fs::read(&legacy).unwrap();
    let mut incoming = original.accounts[0].records[0].clone();
    incoming.work.source_updated_at = Some("2026-09-20T01:30:00.123Z".into());
    incoming.observed_at = 900;
    store
        .apply_discovery_patch_for_following(1, 0, patch(vec![incoming.clone()]))
        .unwrap();
    assert_eq!(fs::read(&legacy).unwrap(), before);
    let reopened = WorkbenchStore::open(directory.path()).unwrap();
    assert_eq!(
        reopened.read_discovery().unwrap().value.accounts[0].records[0],
        incoming
    );
    reopened.checkpoint_discovery_for_following(2, 0).unwrap();
    let saved = WorkbenchStore::open(directory.path())
        .unwrap()
        .read_discovery()
        .unwrap();
    assert_eq!(saved.value.accounts[0].records[0], incoming);
    assert_eq!(
        saved.value.accounts[0].authors[0],
        original.accounts[0].authors[0]
    );
}

#[test]
fn invalid_source_dates_do_not_replace_saved_discovery_metadata() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    store.write_discovery(0, document()).unwrap();
    for date in [
        "yesterday",
        "1970-01-01",
        "2026-02-30",
        "2026-09-15T09:30:00+08:00",
    ] {
        let mut invalid = document();
        invalid.accounts[0].records[0].work.source_updated_at = Some(date.into());
        assert_eq!(
            store.write_discovery(1, invalid).unwrap_err().code,
            "VALIDATION_FAILED"
        );
        assert_eq!(store.read_discovery().unwrap().value, document());
    }
}

fn checkpoint_document() -> DiscoveryDocument {
    let mut value = document();
    let range = &mut value.accounts[0].authors[0];
    range.last_checked_at = Some(20);
    range.last_check_mode = Some(DiscoveryMode::Incremental);
    range.baseline = Some(DiscoveryBaseline {
        query_version: 1,
        head_ids: vec!["123".into()],
        total: 1,
        established_at: 10,
    });
    value
}

#[test]
fn checkpoint_roundtrip_preserves_the_separate_full_scan_time() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    store.write_discovery(0, checkpoint_document()).unwrap();
    let reopened = WorkbenchStore::open(directory.path()).unwrap();
    let read = reopened.read_discovery().unwrap();
    assert_eq!(read.value, checkpoint_document());
    assert_eq!(read.value.accounts[0].authors[0].last_complete_at, Some(10));
    assert_eq!(read.value.accounts[0].authors[0].last_checked_at, Some(20));
}

#[test]
fn malformed_checkpoints_cannot_replace_the_previous_catalog() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let original = checkpoint_document();
    store.write_discovery(0, original.clone()).unwrap();
    let path = directory
        .path()
        .join(PRIVATE_DIRECTORY)
        .join("discovery.json");
    let before = fs::read(&path).unwrap();
    let invalid_heads = [
        vec![],
        vec!["../bad".to_owned()],
        vec!["123".to_owned(), "123".to_owned()],
        (1..=21).map(|number| number.to_string()).collect(),
    ];
    for (index, head_ids) in invalid_heads.into_iter().enumerate() {
        let mut value = original.clone();
        let baseline = value.accounts[0].authors[0].baseline.as_mut().unwrap();
        baseline.total = if index == 0 { 1 } else { head_ids.len() as u64 };
        baseline.head_ids = head_ids;
        assert_eq!(
            store.write_discovery(1, value).unwrap_err().code,
            "VALIDATION_FAILED"
        );
        assert_eq!(fs::read(&path).unwrap(), before);
    }
    let mut future_clock = original;
    future_clock.accounts[0].authors[0].last_checked_at = Some(u64::MAX);
    assert_eq!(
        store.write_discovery(1, future_clock).unwrap_err().code,
        "VALIDATION_FAILED"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
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

fn patch(records: Vec<DiscoveryRecord>) -> DiscoveryPagePatch {
    DiscoveryPagePatch {
        account_key: "b".repeat(64),
        authors: vec![],
        records,
        retain_authors: None,
    }
}

fn manifest(directory: &TempDir) -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(
            directory
                .path()
                .join(PRIVATE_DIRECTORY)
                .join("discovery-journal.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

fn page_path(directory: &TempDir) -> std::path::PathBuf {
    directory.path().join(PRIVATE_DIRECTORY).join(format!(
        "discovery-page-{}.json",
        manifest(directory)["headSha256"].as_str().unwrap()
    ))
}

#[test]
fn first_page_adopts_legacy_without_rewriting_or_losing_cold_history() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let mut original = checkpoint_document();
    let mut cold = record(Source::Jm, "124");
    cold.work.authors = vec!["别的作者".into()];
    cold.author_verified = false;
    original.accounts[0].records.push(cold.clone());
    store.write_discovery(0, original.clone()).unwrap();
    let legacy = directory
        .path()
        .join(PRIVATE_DIRECTORY)
        .join("discovery.json");
    let before = fs::read(&legacy).unwrap();
    let mut update = patch(vec![record(Source::Jm, "125")]);
    let mut range = original.accounts[0].authors[0].clone();
    range.pages_read = 2;
    range.observed_count = 3;
    range.state = DiscoveryRangeState::Partial;
    update.authors.push(range.clone());
    assert_eq!(
        store
            .apply_discovery_patch_for_following(1, 0, update)
            .unwrap(),
        2
    );
    assert_eq!(fs::read(&legacy).unwrap(), before);
    let reopened = WorkbenchStore::open(directory.path()).unwrap();
    let saved = reopened.read_discovery().unwrap();
    assert_eq!(saved.revision, 2);
    assert_eq!(
        saved.value.accounts[0].records,
        vec![
            original.accounts[0].records[0].clone(),
            cold,
            record(Source::Jm, "125")
        ]
    );
    assert_eq!(saved.value.accounts[0].authors, vec![range]);
    assert_eq!(reopened.read_library().unwrap().revision, 0);
    assert_eq!(reopened.read_following().unwrap().revision, 0);
    assert_eq!(
        store
            .write_discovery(2, DiscoveryDocument::default())
            .unwrap_err()
            .code,
        "DISCOVERY_JOURNAL_ACTIVE"
    );
}

#[test]
fn page_write_is_small_after_one_hundred_thousand_raw_keyword_records() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let mut original = document();
    original.accounts[0].records = (1..=MAX_DISCOVERY_RECORDS)
        .map(|id| {
            let mut value = record(Source::Jm, &id.to_string());
            value.work.authors = vec!["别的作者".into()];
            value.author_verified = false;
            value
        })
        .collect();
    store.write_discovery(0, original).unwrap();
    let legacy = directory
        .path()
        .join(PRIVATE_DIRECTORY)
        .join("discovery.json");
    let legacy_bytes = fs::read(&legacy).unwrap();
    // Warm the native writer's compact key/size index once, as the scanner does.
    assert_eq!(
        store.read_discovery().unwrap().value.accounts[0]
            .records
            .len(),
        MAX_DISCOVERY_RECORDS
    );
    store
        .apply_discovery_patch_for_following(1, 0, patch(vec![record(Source::Jm, "100001")]))
        .unwrap();
    let page = page_path(&directory);
    assert!(fs::metadata(&page).unwrap().len() < 4096);
    assert!(
        fs::metadata(
            directory
                .path()
                .join(PRIVATE_DIRECTORY)
                .join("discovery-journal.json")
        )
        .unwrap()
        .len()
            < 4096
    );
    assert_eq!(fs::read(&legacy).unwrap(), legacy_bytes);
    let mut replacement = record(Source::Jm, "100001");
    replacement.work.title = "更新的合成元数据".into();
    store
        .apply_discovery_patch_for_following(2, 0, patch(vec![replacement.clone()]))
        .unwrap();
    assert!(fs::metadata(page_path(&directory)).unwrap().len() < 4096);
    let saved = WorkbenchStore::open(directory.path())
        .unwrap()
        .read_discovery()
        .unwrap();
    assert_eq!(saved.revision, 3);
    assert_eq!(
        saved.value.accounts[0].records.len(),
        MAX_DISCOVERY_RECORDS + 1
    );
    assert_eq!(saved.value.accounts[0].records.last(), Some(&replacement));
    assert!(!saved.value.accounts[0].records[0].author_verified);
}

#[test]
fn rejected_page_never_advances_manifest_or_replaces_committed_results() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    store.write_discovery(0, document()).unwrap();
    store
        .apply_discovery_patch_for_following(1, 0, patch(vec![record(Source::Jm, "124")]))
        .unwrap();
    let before = store.read_discovery().unwrap();
    let manifest_before = manifest(&directory);
    assert_eq!(
        store
            .apply_discovery_patch_for_following(1, 0, patch(vec![record(Source::Jm, "125")]))
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
    store
        .write_following(0, AccountFollowing::default())
        .unwrap();
    assert_eq!(
        store
            .apply_discovery_patch_for_following(2, 0, patch(vec![record(Source::Jm, "125")]))
            .unwrap_err()
            .code,
        "DISCOVERY_FOLLOWING_CHANGED"
    );
    let duplicate = record(Source::Jm, "125");
    assert_eq!(
        store
            .apply_discovery_patch_for_following(2, 1, patch(vec![duplicate.clone(), duplicate]))
            .unwrap_err()
            .code,
        "VALIDATION_FAILED"
    );
    assert_eq!(manifest(&directory), manifest_before);
    assert_eq!(store.read_discovery().unwrap(), before);
    assert_eq!(
        store
            .checkpoint_discovery_for_following(2, 0)
            .unwrap_err()
            .code,
        "DISCOVERY_FOLLOWING_CHANGED"
    );
    assert_eq!(manifest(&directory), manifest_before);
}

#[test]
fn missing_corrupt_pages_and_a_missing_adopted_manifest_fail_closed() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    store.write_discovery(0, document()).unwrap();
    store
        .apply_discovery_patch_for_following(1, 0, patch(vec![record(Source::Jm, "124")]))
        .unwrap();
    let page = page_path(&directory);
    let bytes = fs::read(&page).unwrap();
    fs::remove_file(&page).unwrap();
    assert!(store.read_discovery().is_err());
    fs::write(&page, b"corrupt").unwrap();
    assert_eq!(store.read_discovery().unwrap_err().code, "DOCUMENT_CORRUPT");
    fs::write(&page, bytes).unwrap();
    assert_eq!(store.read_discovery().unwrap().revision, 2);
    fs::remove_file(
        directory
            .path()
            .join(PRIVATE_DIRECTORY)
            .join("discovery-journal.json"),
    )
    .unwrap();
    assert_eq!(store.read_discovery().unwrap_err().code, "DOCUMENT_CORRUPT");
    assert_eq!(
        store
            .apply_discovery_patch_for_following(1, 0, patch(vec![]))
            .unwrap_err()
            .code,
        "DOCUMENT_CORRUPT"
    );
}

#[test]
fn altered_legacy_base_is_not_accepted_under_a_new_journal() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    store.write_discovery(0, document()).unwrap();
    store
        .apply_discovery_patch_for_following(1, 0, patch(vec![record(Source::Jm, "124")]))
        .unwrap();
    let path = directory
        .path()
        .join(PRIVATE_DIRECTORY)
        .join("discovery.json");
    let mut legacy: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    legacy["value"]["accounts"][0]["records"][0]["work"]["title"] = "外部修改".into();
    fs::write(path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert_eq!(store.read_discovery().unwrap_err().code, "DOCUMENT_CORRUPT");
}

#[test]
fn uncommitted_orphan_page_is_preserved_without_becoming_current() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    store.write_discovery(0, document()).unwrap();
    store
        .apply_discovery_patch_for_following(1, 0, patch(vec![record(Source::Jm, "124")]))
        .unwrap();
    let before = store.read_discovery().unwrap();
    let orphan = directory
        .path()
        .join(PRIVATE_DIRECTORY)
        .join(format!("discovery-page-{}.json", "d".repeat(64)));
    fs::write(&orphan, b"an interrupted, unreferenced write").unwrap();
    assert_eq!(store.read_discovery().unwrap(), before);
    assert!(orphan.exists());
}

#[test]
fn checkpoint_preserves_revision_and_base_then_reclaims_only_retired_pages() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    store.write_discovery(0, checkpoint_document()).unwrap();
    let legacy = directory
        .path()
        .join(PRIVATE_DIRECTORY)
        .join("discovery.json");
    let legacy_bytes = fs::read(&legacy).unwrap();
    store
        .apply_discovery_patch_for_following(1, 0, patch(vec![record(Source::Jm, "124")]))
        .unwrap();
    let first_page = page_path(&directory);
    store
        .apply_discovery_patch_for_following(2, 0, patch(vec![record(Source::Jm, "125")]))
        .unwrap();
    let second_page = page_path(&directory);
    let before = store.read_discovery().unwrap();
    store.checkpoint_discovery_for_following(3, 0).unwrap();
    assert_eq!(manifest(&directory)["patchCount"], 0);
    assert_eq!(manifest(&directory)["revision"], 3);
    assert!(!first_page.exists());
    assert!(!second_page.exists());
    assert_eq!(fs::read(&legacy).unwrap(), legacy_bytes);
    assert_eq!(
        WorkbenchStore::open(directory.path())
            .unwrap()
            .read_discovery()
            .unwrap(),
        before
    );
    let first_checkpoint = directory.path().join(PRIVATE_DIRECTORY).join(format!(
        "discovery-checkpoint-{}.json",
        manifest(&directory)["checkpoint"]["sha256"]
            .as_str()
            .unwrap()
    ));
    store
        .apply_discovery_patch_for_following(3, 0, patch(vec![record(Source::Jm, "126")]))
        .unwrap();
    assert_eq!(
        store.read_discovery().unwrap().value.accounts[0]
            .records
            .len(),
        4
    );
    store.checkpoint_discovery_for_following(4, 0).unwrap();
    assert!(!first_checkpoint.exists());
    let current_checkpoint = directory.path().join(PRIVATE_DIRECTORY).join(format!(
        "discovery-checkpoint-{}.json",
        manifest(&directory)["checkpoint"]["sha256"]
            .as_str()
            .unwrap()
    ));
    fs::remove_file(current_checkpoint).unwrap();
    assert!(store.read_discovery().is_err());
}

#[test]
fn range_reconciliation_never_deletes_raw_history_or_other_accounts() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let mut initial = document();
    let mut other = initial.accounts[0].clone();
    other.account_key = "c".repeat(64);
    initial.accounts.push(other.clone());
    store.write_discovery(0, initial).unwrap();
    let mut update = patch(vec![]);
    update.retain_authors = Some(vec![]);
    store
        .apply_discovery_patch_for_following(1, 0, update)
        .unwrap();
    let saved = store.read_discovery().unwrap();
    assert!(saved.value.accounts[0].authors.is_empty());
    assert_eq!(
        saved.value.accounts[0].records,
        vec![record(Source::Jm, "123")]
    );
    assert_eq!(saved.value.accounts[1], other);
}

#[test]
fn fresh_profile_journal_and_checkpoint_do_not_require_a_legacy_file() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    assert_eq!(
        store
            .apply_discovery_patch_for_following(0, 0, patch(vec![record(Source::Jm, "123")]))
            .unwrap(),
        1
    );
    let legacy = directory
        .path()
        .join(PRIVATE_DIRECTORY)
        .join("discovery.json");
    assert!(!legacy.exists());
    let before = store.read_discovery().unwrap();
    assert_eq!(before.revision, 1);
    store.checkpoint_discovery_for_following(1, 0).unwrap();
    assert_eq!(
        WorkbenchStore::open(directory.path())
            .unwrap()
            .read_discovery()
            .unwrap(),
        before
    );
    assert!(!legacy.exists());
}

#[test]
fn future_or_inconsistent_manifest_cannot_be_loaded_or_overwritten() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    store
        .apply_discovery_patch_for_following(0, 0, patch(vec![record(Source::Jm, "123")]))
        .unwrap();
    let path = directory
        .path()
        .join(PRIVATE_DIRECTORY)
        .join("discovery-journal.json");
    let original = manifest(&directory);
    let mut future = original.clone();
    future["version"] = 2.into();
    let mut broken_chain = original;
    broken_chain["revision"] = 2.into();
    broken_chain["patchCount"] = 2.into();
    for (value, expected) in [
        (future, "UNSUPPORTED_SCHEMA"),
        (broken_chain, "DOCUMENT_CORRUPT"),
    ] {
        let bytes = serde_json::to_vec(&value).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert_eq!(store.read_discovery().unwrap_err().code, expected);
        assert_eq!(
            store
                .apply_discovery_patch_for_following(1, 0, patch(vec![]))
                .unwrap_err()
                .code,
            expected
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}
