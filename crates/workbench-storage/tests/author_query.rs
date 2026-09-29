use tempfile::TempDir;
use workbench_storage::{
    AccountFollowing, AuthorQueryAccount, AuthorQueryDocument, AuthorQueryProfile,
    AuthorWorkCredit, DiscoveryPagePatch, FollowedAccount, Source, WorkbenchStore,
};

fn policies() -> AuthorQueryDocument {
    AuthorQueryDocument {
        version: 1,
        accounts: vec![AuthorQueryAccount {
            source: Source::Jm,
            account_key: "a".repeat(64),
            work_credits: vec![],
            profiles: vec![AuthorQueryProfile {
                author: "Writer Name".into(),
                queries: vec!["Writer~Name".into(), "Writer～Name".into()],
                verified_aliases: vec!["WriterName".into()],
                exact_credits: vec!["WriterName Coauthor".into()],
            }],
        }],
    }
}

fn work_credit(id: &str, expected: &[&str], corrected: &[&str]) -> AuthorWorkCredit {
    AuthorWorkCredit {
        work_id: id.into(),
        expected_authors: expected.iter().map(|name| (*name).into()).collect(),
        expected_author_variants: vec![],
        corrected_authors: corrected.iter().map(|name| (*name).into()).collect(),
    }
}

#[test]
fn work_credit_guard_is_exact_scoped_and_preserves_raw_authors_and_query_fingerprint() {
    let mut document = policies();
    let before = document.resolve(Source::Jm, &"a".repeat(64), "Writer Name");
    document.accounts[0].work_credits = vec![
        work_credit("100", &["Wrong Name", "Support"], &["WriterName"]),
        work_credit("101", &["Unrelated"], &["SomebodyElse"]),
    ];
    let policy = document.resolve(Source::Jm, &"a".repeat(64), "Writer Name");
    assert_eq!(policy.work_credits.len(), 1);
    assert_eq!(policy.query_fingerprint, before.query_fingerprint);
    let raw = vec![" SUPPORT ".into(), "ＷＲＯＮＧ　ＮＡＭＥ".into()];
    let saved_raw = raw.clone();
    assert!(policy.matches_work_credits("100", &raw));
    assert_eq!(policy.effective_work_credits("100", &raw), ["WriterName"]);
    assert!(!policy.matches_work_credits("102", &raw));
    assert!(!policy.matches_work_credits("100", &["Wrong Name".into()]));
    assert!(!policy.matches_work_credits(
        "100",
        &["Wrong Name".into(), "Support".into(), "Additional".into()]
    ));
    assert!(!policy.matches_work_credits("100", &["Wrong Name (Support)".into()]));
    assert!(!policy.matches_work_credits("100", &[]));
    assert!(policy.matches_work_credits("100", &["WriterName".into()])); // Source already fixed.
    assert_eq!(raw, saved_raw);
    let wrong_author = document.resolve(Source::Jm, &"a".repeat(64), "Wrong Name");
    assert_eq!(wrong_author.work_credits.len(), 1);
    assert!(!wrong_author.matches_work_credits("100", &raw));
    assert!(wrong_author.matches_work_credits("102", &raw));
    for (source, account) in [(Source::Pica, "a".repeat(64)), (Source::Jm, "b".repeat(64))] {
        let other = document.resolve(source, &account, "Writer Name");
        assert!(other.work_credits.is_empty());
        assert!(!other.matches_work_credits("100", &raw));
    }
    let unrelated = document.resolve(Source::Jm, &"a".repeat(64), "Nobody");
    assert!(unrelated.work_credits.is_empty());
    // Previously reviewed complete collaboration fields can select a rule without
    // turning the member's name into an inferred alias.
    document.accounts[0].work_credits[0].corrected_authors = vec!["WriterName Coauthor".into()];
    let exact = document.resolve(Source::Jm, &"a".repeat(64), "Writer Name");
    assert_eq!(exact.work_credits.len(), 1);
    assert!(exact.matches_work_credits("100", &raw));
    assert!(document
        .resolve(Source::Jm, &"a".repeat(64), "Coauthor")
        .work_credits
        .is_empty());
}

#[test]
fn reviewed_listing_credit_variants_require_the_entire_credit_set_for_one_work() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let mut document = policies();
    let mut rule = work_credit("100", &["Wrong", "Guest"], &["WriterName", "Guest"]);
    rule.expected_author_variants = vec![
        vec!["Wrong Guest".into()],
        vec!["Other spelling".into(), "Guest".into()],
    ];
    document.accounts[0].work_credits = vec![rule.clone()];
    let stored = store.write_author_query_policies(0, document).unwrap();
    assert_eq!(store.read_author_query_policies().unwrap(), stored);
    let policy = stored
        .value
        .resolve(Source::Jm, &"a".repeat(64), "Writer Name");
    for names in std::iter::once(&rule.expected_authors).chain(&rule.expected_author_variants) {
        assert_eq!(
            policy.effective_work_credits("100", names),
            rule.corrected_authors
        );
        assert_eq!(policy.effective_work_credits("101", names), *names);
    }
    let wrong = stored
        .value
        .resolve(Source::Jm, &"a".repeat(64), "Wrong Guest");
    assert_eq!(wrong.work_credits.len(), 1);
    assert!(!wrong.matches_work_credits("100", &["Wrong Guest".into()]));
    for names in [
        vec!["Other spelling".into()],
        vec![
            "Other spelling".into(),
            "Guest".into(),
            "New contributor".into(),
        ],
    ] {
        assert_eq!(policy.effective_work_credits("100", &names), names);
    }
    let legacy: AuthorWorkCredit = serde_json::from_value(serde_json::json!({
        "workId":"101", "expectedAuthors":["Old"], "correctedAuthors":["New"]
    }))
    .unwrap();
    assert!(legacy.expected_author_variants.is_empty());
}

#[test]
fn work_credit_import_merges_only_explicit_ids_and_supports_unfollowed_corrected_authors() {
    let mut original = policies();
    original.accounts[0].work_credits = vec![
        work_credit("100", &["Old"], &["First"]),
        work_credit("101", &["Old"], &["Second"]),
    ];
    let mut incoming = policies();
    incoming.accounts[0].profiles.clear();
    incoming.accounts[0].work_credits = vec![
        work_credit("100", &["Old"], &["Unfollowed Writer"]),
        work_credit("102", &["Old"], &["Third"]),
    ];
    let following = AccountFollowing {
        version: 1,
        accounts: vec![FollowedAccount {
            source: Source::Jm,
            account_key: "a".repeat(64),
            works: vec![],
            authors: vec!["Writer Name".into()],
        }],
    };
    let merged = original.merged_import(&incoming, &following).unwrap();
    assert_eq!(merged.accounts[0].profiles, original.accounts[0].profiles);
    assert_eq!(merged.accounts[0].work_credits.len(), 3);
    assert_eq!(
        merged.accounts[0].work_credits[1],
        original.accounts[0].work_credits[1]
    );
    let arbitrary = merged.resolve(Source::Jm, &"a".repeat(64), "Unfollowed Writer");
    assert_eq!(arbitrary.queries, ["Unfollowed Writer"]);
    assert_eq!(arbitrary.work_credits.len(), 1);
    assert!(arbitrary.matches_work_credits("100", &["Old".into()]));
    incoming.accounts[0].work_credits.clear();
    assert_eq!(merged.merged_import(&incoming, &following).unwrap(), merged);
    incoming.accounts[0].account_key = "b".repeat(64);
    assert_eq!(
        merged
            .merged_import(&incoming, &following)
            .unwrap_err()
            .code,
        "AUTHOR_POLICY_SCOPE_UNKNOWN"
    );
    let old_json = serde_json::to_value(policies()).unwrap();
    assert!(old_json["accounts"][0].get("workCredits").is_none());
    let decoded: AuthorQueryDocument = serde_json::from_value(old_json).unwrap();
    assert!(decoded.accounts[0].work_credits.is_empty());
}

#[test]
fn work_credit_validation_rejects_ambiguous_ids_credit_sets_and_unbounded_rules() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let valid_rule = work_credit("100", &["Old"], &["New"]);
    for variation in 0..15 {
        let mut invalid = policies();
        invalid.accounts[0].work_credits.push(valid_rule.clone());
        match variation {
            0 => invalid.accounts[0].work_credits.push(valid_rule.clone()),
            1 => invalid.accounts[0].work_credits[0].work_id = "0100".into(),
            2 => invalid.accounts[0].work_credits[0].expected_authors.clear(),
            3 => invalid.accounts[0].work_credits[0].corrected_authors = vec!["\u{feff}".into()],
            4 => {
                invalid.accounts[0].work_credits[0].expected_authors =
                    vec!["Old".into(), "ＯＬＤ".into()]
            }
            5 => invalid.accounts[0].work_credits[0].corrected_authors = vec!["New\nName".into()],
            6 => invalid.accounts[0].work_credits[0].expected_authors = vec!["x".repeat(2001)],
            7 => {
                invalid.accounts[0].work_credits[0].corrected_authors =
                    (0..65).map(|i| format!("Author {i}")).collect()
            }
            8 => {
                invalid.accounts[0].work_credits = (1..=501)
                    .map(|id| work_credit(&id.to_string(), &["Old"], &["New"]))
                    .collect()
            }
            9 => invalid.accounts[0].source = Source::Pica,
            10 => invalid.accounts[0].work_credits[0].expected_author_variants = vec![vec![]],
            11 => {
                invalid.accounts[0].work_credits[0].expected_author_variants =
                    vec![vec!["ＯＬＤ".into()]]
            }
            12 => {
                invalid.accounts[0].work_credits[0].expected_author_variants =
                    vec![vec!["Other".into(), "ＯＴＨＥＲ".into()]]
            }
            13 => {
                invalid.accounts[0].work_credits[0].expected_author_variants =
                    (0..5).map(|i| vec![format!("Other {i}")]).collect()
            }
            _ => {
                invalid.accounts[0].work_credits[0].expected_author_variants =
                    vec![vec!["Other\nCredit".into()]]
            }
        }
        assert_eq!(
            store
                .write_author_query_policies(0, invalid)
                .unwrap_err()
                .code,
            "VALIDATION_FAILED",
            "variation {variation}"
        );
    }
    let mut pica = policies();
    pica.accounts[0].source = Source::Pica;
    pica.accounts[0].work_credits =
        vec![work_credit("abcdefabcdefabcdefabcdef", &["Old"], &["New"])];
    store.write_author_query_policies(0, pica).unwrap();
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
        last_check: None,
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
