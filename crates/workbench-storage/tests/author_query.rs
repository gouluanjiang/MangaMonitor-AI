use tempfile::TempDir;
use workbench_storage::{
    AccountFollowing, AuthorQueryAccount, AuthorQueryDocument, AuthorQueryProfile,
    DiscoveryPagePatch, FollowedAccount, Source, WorkbenchStore,
};

fn policies() -> AuthorQueryDocument {
    AuthorQueryDocument {
        version: 1,
        accounts: vec![AuthorQueryAccount {
            source: Source::Jm,
            account_key: "a".repeat(64),
            profiles: vec![AuthorQueryProfile {
                author: "Writer Name".into(),
                queries: vec!["Writer~Name".into(), "Writer～Name".into()],
                verified_aliases: vec!["WriterName".into()],
                exact_credits: vec!["WriterName Coauthor".into()],
            }],
        }],
    }
}

#[test]
fn exact_queries_are_preserved_while_only_explicit_credits_expand_attribution() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let saved = store.write_author_query_policies(0, policies()).unwrap();
    assert_eq!(store.read_author_query_policies().unwrap(), saved);
    let policy = saved
        .value
        .resolve(Source::Jm, &"a".repeat(64), "Writer Name");
    assert_eq!(policy.queries, ["Writer~Name", "Writer～Name"]);
    assert!(policy.matches_credits(&["Circle (WriterName)".into()]));
    assert!(policy.matches_credits(&["WriterName Coauthor".into()]));
    assert!(!policy.matches_credits(&["Coauthor".into()]));
    assert!(!policy.matches_credits(&["Unrelated (WriterName Coauthor)".into()]));
    assert!(!policy.matches_credits(&["Writer～Name".into()]));
    let mut edited = saved.value.clone();
    edited.accounts[0].profiles[0]
        .verified_aliases
        .push("OtherSignature".into());
    assert_eq!(
        edited
            .resolve(Source::Jm, &"a".repeat(64), "Writer Name")
            .query_fingerprint,
        policy.query_fingerprint
    );
    edited.accounts[0].profiles[0].queries.reverse();
    assert_ne!(
        edited
            .resolve(Source::Jm, &"a".repeat(64), "Writer Name")
            .query_fingerprint,
        policy.query_fingerprint
    );
    for (source, account) in [(Source::Pica, "a".repeat(64)), (Source::Jm, "b".repeat(64))] {
        let other = saved.value.resolve(source, &account, "Writer Name");
        assert_eq!(other.queries, ["Writer Name"]);
        assert!(other.verified_aliases.is_empty());
        assert!(other.exact_credits.is_empty());
    }
}

#[test]
fn policies_are_bounded_revisioned_and_do_not_rewrite_unrelated_documents() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let mut invalid = policies();
    invalid.accounts[0].profiles[0]
        .queries
        .push("Writer~Name".into());
    assert_eq!(
        store
            .write_author_query_policies(0, invalid)
            .unwrap_err()
            .code,
        "VALIDATION_FAILED"
    );
    let following = AccountFollowing {
        version: 1,
        accounts: vec![FollowedAccount {
            source: Source::Jm,
            account_key: "a".repeat(64),
            works: vec![],
            authors: vec!["Writer Name".into()],
        }],
    };
    let following = store.write_following(0, following).unwrap();
    assert_eq!(
        store
            .import_author_query_policies(0, following.revision, 1, policies())
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
    let applied = store
        .import_author_query_policies(0, following.revision, 0, policies())
        .unwrap();
    assert_eq!(applied.revision, 1);
    assert_eq!(store.read_following().unwrap(), following);
    assert_eq!(store.read_library().unwrap().revision, 0);
    assert_eq!(store.read_downloads().unwrap().revision, 0);
    let patch = DiscoveryPagePatch {
        account_key: "b".repeat(64),
        authors: vec![],
        records: vec![],
        retain_authors: None,
    };
    assert_eq!(
        store
            .apply_discovery_patch_for_policy(0, following.revision, Some(0), patch)
            .unwrap_err()
            .code,
        "DISCOVERY_POLICY_CHANGED"
    );
    assert_eq!(
        store
            .checkpoint_discovery_for_policy(0, following.revision, Some(0))
            .unwrap_err()
            .code,
        "DISCOVERY_POLICY_CHANGED"
    );
    assert_eq!(store.read_discovery().unwrap().revision, 0);
    let mut bad = policies();
    bad.accounts[0].profiles[0].author = "Unfollowed".into();
    assert_eq!(
        store
            .import_author_query_policies(1, following.revision, 0, bad)
            .unwrap_err()
            .code,
        "AUTHOR_POLICY_AUTHOR_UNKNOWN"
    );

    let checking: workbench_storage::DiscoveryDocument =
        serde_json::from_value(serde_json::json!({
            "version": 1, "accounts": [{ "accountKey": "b".repeat(64), "records": [], "authors": [{
                "author": "Writer Name", "source": "JM", "state": "checking", "lastAttemptAt": 1,
                "lastCompleteAt": null, "observedCount": 0, "pagesRead": 0, "errorCode": null
            }]}]
        }))
        .unwrap();
    let checking = store
        .write_discovery_for_following(0, following.revision, checking)
        .unwrap();
    assert_eq!(
        store
            .import_author_query_policies(1, following.revision, checking.revision, policies())
            .unwrap_err()
            .code,
        "DISCOVERY_BUSY"
    );
    assert_eq!(store.read_author_query_policies().unwrap(), applied);
}
