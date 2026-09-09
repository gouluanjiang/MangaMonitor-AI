use serde_json::json;
use std::{
    fs,
    sync::{Arc, Barrier},
    thread,
};
use tempfile::TempDir;
use workbench_storage::{
    AccountFollowing, Booklists, FollowedAccount, FollowedWork, Source, WorkbenchPreferences,
    WorkbenchStore, MAX_FOLLOWED_ACCOUNTS, MAX_FOLLOWED_AUTHORS_PER_ACCOUNT,
    MAX_FOLLOWED_WORKS_PER_ACCOUNT, MAX_SAFE_INTEGER, PRIVATE_DIRECTORY,
};

fn scope(source: Source, account: u64) -> FollowedAccount {
    FollowedAccount {
        source,
        account_key: format!("{account:064x}"),
        works: vec![FollowedWork {
            work_id: "same-id".into(),
            title: "作品 😀".into(),
        }],
        authors: vec!["作者".into()],
    }
}

fn following() -> AccountFollowing {
    AccountFollowing {
        version: 1,
        accounts: vec![scope(Source::Jm, 1)],
    }
}

#[test]
fn following_defaults_persists_and_is_independent_of_other_documents() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let initial = store.read_following().unwrap();
    assert_eq!(initial.revision, 0);
    assert_eq!(initial.value, AccountFollowing::default());
    let mut value = following();
    // Identical work IDs and account hashes are valid across sources, and work
    // IDs are also independent between verified accounts on the same source.
    value.accounts.push(scope(Source::Pica, 1));
    value.accounts.push(scope(Source::Jm, 2));
    store.write_following(0, value.clone()).unwrap();
    store.write_booklists(0, Booklists::default()).unwrap();
    store
        .write_preferences(0, WorkbenchPreferences::default())
        .unwrap();
    drop(store);
    let reopened = WorkbenchStore::open(directory.path()).unwrap();
    let persisted = reopened.read_following().unwrap();
    assert_eq!(persisted.revision, 1);
    assert_eq!(persisted.value, value);
    assert_eq!(reopened.read_booklists().unwrap().revision, 1);
    assert_eq!(reopened.read_preferences().unwrap().revision, 1);
    assert!(!directory.path().join("following.json").exists());
}

#[test]
fn stale_following_write_cannot_erase_another_account_scope() {
    let directory = TempDir::new().unwrap();
    let first = WorkbenchStore::open(directory.path()).unwrap();
    let second = WorkbenchStore::open(directory.path()).unwrap();
    first.write_following(0, following()).unwrap();
    let stale = first.read_following().unwrap();
    let mut current = second.read_following().unwrap();
    current.value.accounts.push(scope(Source::Pica, 2));
    second
        .write_following(current.revision, current.value.clone())
        .unwrap();
    assert_eq!(
        first
            .write_following(stale.revision, stale.value)
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
    assert_eq!(first.read_following().unwrap().value, current.value);
}

#[test]
fn competing_instances_do_not_silently_lose_a_following_write() {
    let directory = TempDir::new().unwrap();
    let first = WorkbenchStore::open(directory.path()).unwrap();
    let second = WorkbenchStore::open(directory.path()).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = [first, second]
        .into_iter()
        .enumerate()
        .map(|(index, store)| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let mut value = following();
                value.accounts[0].authors = vec![format!("writer-{index}")];
                barrier.wait();
                store.write_following(0, value)
            })
        })
        .collect();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let failed = results
        .iter()
        .find_map(|result| result.as_ref().err())
        .unwrap();
    assert!(matches!(failed.code, "BUSY" | "REVISION_CONFLICT"));
    let saved = results.into_iter().find_map(Result::ok).unwrap();
    assert_eq!(
        WorkbenchStore::open(directory.path())
            .unwrap()
            .read_following()
            .unwrap(),
        saved
    );
}

#[test]
fn corrupt_and_future_following_originals_block_overwrite() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let path = directory
        .path()
        .join(PRIVATE_DIRECTORY)
        .join("following.json");
    let valid = json!({"schemaVersion":1,"revision":1,"value":following()});
    let mut unknown_account_field = valid.clone();
    unknown_account_field["value"]["accounts"][0]["unknown"] = json!(true);
    let mut unknown_work_field = valid.clone();
    unknown_work_field["value"]["accounts"][0]["works"][0]["unknown"] = json!(true);
    let mut malformed_key = valid.clone();
    malformed_key["value"]["accounts"][0]["accountKey"] = json!("not-a-hash");
    let cases = [
        ("broken-json".into(), "DOCUMENT_CORRUPT"),
        (unknown_account_field.to_string(), "DOCUMENT_CORRUPT"),
        (unknown_work_field.to_string(), "DOCUMENT_CORRUPT"),
        (malformed_key.to_string(), "DOCUMENT_CORRUPT"),
        (json!({"schemaVersion":2,"revision":1,"value":{"version":1,"accounts":[]}}).to_string(), "UNSUPPORTED_SCHEMA"),
        (json!({"schemaVersion":1,"revision":1,"value":{"version":2,"accounts":[]}}).to_string(), "UNSUPPORTED_SCHEMA"),
        (json!({"schemaVersion":1,"revision":0,"value":{"version":1,"accounts":[]}}).to_string(), "DOCUMENT_CORRUPT"),
        (json!({"schemaVersion":1,"revision":MAX_SAFE_INTEGER+1,"value":{"version":1,"accounts":[]}}).to_string(), "DOCUMENT_CORRUPT"),
    ];
    for (bytes, expected) in cases {
        fs::write(&path, bytes.as_bytes()).unwrap();
        assert_eq!(store.read_following().unwrap_err().code, expected);
        assert_eq!(
            store.write_following(0, following()).unwrap_err().code,
            expected
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), bytes);
    }
    // A damaged following document does not prevent an independent preference write.
    store
        .write_preferences(0, WorkbenchPreferences::default())
        .unwrap();
}

#[test]
fn abandoned_following_temporary_is_never_adopted() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let path = directory
        .path()
        .join(PRIVATE_DIRECTORY)
        .join(".following.json.123.1.tmp");
    let bytes = json!({"schemaVersion":1,"revision":7,"value":following()}).to_string();
    fs::write(&path, &bytes).unwrap();
    assert_eq!(store.read_following().unwrap().revision, 0);
    assert_eq!(
        store
            .write_following(0, AccountFollowing::default())
            .unwrap()
            .revision,
        1
    );
    assert_eq!(fs::read_to_string(path).unwrap(), bytes);
}

#[test]
fn invalid_scopes_and_members_preserve_the_last_valid_document() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let valid = following();
    store.write_following(0, valid.clone()).unwrap();
    let mut cases = Vec::new();
    let mut value = valid.clone();
    value.accounts.push(value.accounts[0].clone());
    cases.push(value);
    for key in [
        "a".repeat(63),
        "a".repeat(65),
        "A".repeat(64),
        "g".repeat(64),
    ] {
        let mut value = valid.clone();
        value.accounts[0].account_key = key;
        cases.push(value);
    }
    let mut value = valid.clone();
    let duplicate_work = value.accounts[0].works[0].clone();
    value.accounts[0].works.push(duplicate_work);
    cases.push(value);
    let mut value = valid.clone();
    value.accounts[0].authors.push("作者".into());
    cases.push(value);
    for name in [
        "".into(),
        "  ".into(),
        "a\r\nb".into(),
        "a\u{85}b".into(),
        "界".repeat(201),
    ] {
        let mut value = valid.clone();
        value.accounts[0].works[0].title = name.clone();
        cases.push(value);
        let mut value = valid.clone();
        value.accounts[0].authors[0] = name;
        cases.push(value);
    }
    for work_id in [
        "".into(),
        "../escape".into(),
        "not a key".into(),
        "x".repeat(161),
    ] {
        let mut value = valid.clone();
        value.accounts[0].works[0].work_id = work_id;
        cases.push(value);
    }
    let mut value = valid.clone();
    value.accounts = (0..=MAX_FOLLOWED_ACCOUNTS)
        .map(|index| scope(Source::Jm, index as u64))
        .collect();
    cases.push(value);
    let mut value = valid.clone();
    value.accounts[0].works = (0..=MAX_FOLLOWED_WORKS_PER_ACCOUNT)
        .map(|index| FollowedWork {
            work_id: format!("w{index}"),
            title: "work".into(),
        })
        .collect();
    cases.push(value);
    let mut value = valid.clone();
    value.accounts[0].authors = (0..=MAX_FOLLOWED_AUTHORS_PER_ACCOUNT)
        .map(|index| format!("author-{index}"))
        .collect();
    cases.push(value);
    for value in cases {
        assert_eq!(
            store.write_following(1, value).unwrap_err().code,
            "VALIDATION_FAILED"
        );
    }
    let saved = store.read_following().unwrap();
    assert_eq!(saved.revision, 1);
    assert_eq!(saved.value, valid);
}

#[test]
fn legal_maximum_unicode_document_roundtrips_without_truncation() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let mut value = AccountFollowing::default();
    for index in 0..MAX_FOLLOWED_ACCOUNTS {
        let mut account = scope(Source::Jm, index as u64);
        account.works = (0..MAX_FOLLOWED_WORKS_PER_ACCOUNT)
            .map(|work| FollowedWork {
                work_id: format!("{work:0160}"),
                title: "😀".repeat(200),
            })
            .collect();
        account.authors = (0..MAX_FOLLOWED_AUTHORS_PER_ACCOUNT)
            .map(|author| format!("{author:03}{}", "😀".repeat(197)))
            .collect();
        value.accounts.push(account);
    }
    let written = store.write_following(0, value.clone()).unwrap();
    assert_eq!(written.value, value);
    assert_eq!(store.read_following().unwrap(), written);
}

#[cfg(unix)]
#[test]
fn following_symlink_is_rejected_without_modifying_the_external_file() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let other = TempDir::new().unwrap();
    let outside = other.path().join("outside.json");
    fs::write(&outside, b"preserve external fixture").unwrap();
    std::os::unix::fs::symlink(
        &outside,
        directory
            .path()
            .join(PRIVATE_DIRECTORY)
            .join("following.json"),
    )
    .unwrap();
    assert_eq!(store.read_following().unwrap_err().code, "UNSAFE_PATH");
    assert_eq!(
        store.write_following(0, following()).unwrap_err().code,
        "UNSAFE_PATH"
    );
    assert_eq!(fs::read(&outside).unwrap(), b"preserve external fixture");
}
