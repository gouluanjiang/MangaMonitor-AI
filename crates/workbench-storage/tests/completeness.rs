use std::fs;
use tempfile::TempDir;
use workbench_storage::*;

const PICA: &str = "0123456789abcdef01234567";

fn reference(source: Source, id: &str) -> LibraryReference {
    LibraryReference {
        source,
        work_id: id.into(),
    }
}
fn member(source: Source, id: &str) -> CompletenessMember {
    CompletenessMember::Source {
        reference: reference(source, id),
    }
}
fn phone_member(name: &str) -> CompletenessMember {
    CompletenessMember::Phone { name: name.into() }
}

fn record(source: Source, id: &str, title: &str, tags: &[&str]) -> DiscoveryRecord {
    DiscoveryRecord {
        work: DiscoveryWork {
            source,
            work_id: id.into(),
            title: title.into(),
            authors: vec!["Writer".into()],
            description: None,
            tags: tags.iter().map(|tag| (*tag).into()).collect(),
            favorite: None,
            chapter_count: Some(1),
            page_count: Some(20),
            cover_available: false,
        },
        matched_authors: vec!["Writer".into()],
        author_verified: true,
        observed_at: 1,
        scan_id: "1".repeat(64),
    }
}

fn fixture() -> (TempDir, WorkbenchStore) {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path().join("app")).unwrap();
    fs::create_dir(directory.path().join("pc")).unwrap();
    store
        .write_library(
            0,
            LibraryDocument {
                root: Some(LibraryRoot {
                    id: "a".repeat(64),
                    file_key: "b".repeat(64),
                    path: directory.path().join("pc").to_string_lossy().into_owned(),
                }),
                generation: 1,
                phase: LibraryPhase::Complete,
                updated_at: Some(1),
                ..LibraryDocument::default()
            },
        )
        .unwrap();
    (directory, store)
}

fn mark(store: &WorkbenchStore, name: &str, source_ref: Option<LibraryReference>) {
    let revision = store.read_phone_library().unwrap().revision;
    phone_library_mark(store, revision, name.into(), source_ref).unwrap();
}

fn pc_record(id: char, name: &str, source_ref: LibraryReference) -> LibraryRecord {
    LibraryRecord {
        item: LibraryItem {
            id: id.to_string().repeat(64),
            relative_path: name.into(),
            file_name: name.into(),
            format: LibraryFormat::Zip,
            title: name.trim_end_matches(".zip").into(),
            authors: vec!["Writer".into()],
            description: None,
            tags: vec![],
            bytes: 7,
            modified_at: Some(1),
            page_count: Some(1),
            cover_available: false,
            state: LibraryItemState::Indexed,
            error_code: None,
            source_ref: Some(source_ref),
            identity_evidence: Some(LibraryEvidence::Metadata),
        },
        identity: Some(LibraryFileIdentity {
            file_key: "f".repeat(64),
            bytes: 7,
            modified: "1234".into(),
        }),
        manual_override: false,
        cover: None,
    }
}

fn set_pc(store: &WorkbenchStore, records: Vec<LibraryRecord>) {
    let mut current = store.read_library().unwrap();
    current.value.visited = records.len() as u64;
    current.value.records = records;
    store
        .write_library(current.revision, current.value)
        .unwrap();
}

fn pair(store: &WorkbenchStore) {
    source_matches_confirm(
        store,
        0,
        SourceMatchWork {
            source: Source::Jm,
            work_id: "123".into(),
            title: "Japanese original".into(),
        },
        SourceMatchWork {
            source: Source::Pica,
            work_id: PICA.into(),
            title: "Chinese version".into(),
        },
    )
    .unwrap();
}

#[test]
fn omissions_survive_scan_generations_and_do_not_become_automatic_translation_tasks() {
    let (directory, store) = fixture();
    let records = vec![record(Source::Jm, "123", "Older omission [Chinese]", &[])];
    let no_phone = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(no_phone.groups[0].status, CompletenessStatus::Unknown);
    mark(&store, "Unrelated phone copy [Chinese]", None);
    let path = directory.path().join("app").join(PRIVATE_DIRECTORY);
    let phone_before = fs::read(path.join("phone-library.json")).unwrap();
    let pc_before = fs::read(path.join("library.json")).unwrap();
    let old = completeness_project(&store, 1, &records).unwrap();
    let subsequent = completeness_project(&store, 2, &records).unwrap();
    assert_eq!(old.groups[0].status, CompletenessStatus::Missing);
    assert_eq!(subsequent.groups[0].status, CompletenessStatus::Missing);
    assert_eq!(
        subsequent.groups[0].eligible.as_ref().unwrap().kind,
        CompletenessCandidateKind::Missing
    );
    assert_ne!(old.evidence_hash, subsequent.evidence_hash);
    assert_eq!(old.groups[0].group_id, subsequent.groups[0].group_id);
    assert_eq!(
        fs::read(path.join("phone-library.json")).unwrap(),
        phone_before
    );
    assert_eq!(fs::read(path.join("library.json")).unwrap(), pc_before);
    assert!(!path.join("downloads.json").exists());
    assert!(!path.join("completeness.json").exists());
}

#[test]
fn any_confirmed_chinese_phone_version_satisfies_both_sources() {
    let (_directory, store) = fixture();
    mark(
        &store,
        "A translated edition [Chinese]",
        Some(reference(Source::Pica, PICA)),
    );
    pair(&store);
    let records = vec![
        record(Source::Jm, "123", "Original [Japanese]", &[]),
        record(Source::Pica, PICA, "Different Chinese title", &["中文"]),
    ];
    let snapshot = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(snapshot.groups.len(), 1);
    assert_eq!(snapshot.groups[0].sources.len(), 2);
    assert_eq!(snapshot.groups[0].status, CompletenessStatus::OwnedChinese);
    assert!(snapshot.groups[0].eligible.is_none());
    assert_eq!(
        store
            .read_phone_library()
            .unwrap()
            .value
            .manual_entries
            .len(),
        1
    );
}

#[test]
fn explicit_japanese_phone_relation_and_verified_chinese_source_create_bound_upgrade_evidence() {
    let (_directory, store) = fixture();
    mark(
        &store,
        "Phone original [Japanese]",
        Some(reference(Source::Jm, "123")),
    );
    let records = vec![
        record(Source::Jm, "123", "Original [Japanese]", &[]),
        record(Source::Pica, PICA, "Different translation [Chinese]", &[]),
    ];
    let before_relation = completeness_project(&store, 1, &records).unwrap();
    assert!(before_relation.groups.iter().all(|group| group
        .eligible
        .as_ref()
        .is_none_or(|candidate| candidate.kind != CompletenessCandidateKind::Translation)));
    completeness_family_confirm(
        &store,
        0,
        vec![member(Source::Jm, "123"), member(Source::Pica, PICA)],
    )
    .unwrap();
    let snapshot = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(snapshot.groups.len(), 1);
    assert_eq!(
        snapshot.groups[0].status,
        CompletenessStatus::TranslationAvailable
    );
    let candidate = snapshot.groups[0].eligible.as_ref().unwrap();
    assert_eq!(candidate.kind, CompletenessCandidateKind::Translation);
    assert_eq!(candidate.reference, reference(Source::Pica, PICA));
    assert_eq!(candidate.evidence_hash, snapshot.evidence_hash);
    assert_eq!(
        snapshot.groups[0].phone[0].language,
        CompletenessLanguage::Japanese
    );
    assert!(store.read_phone_library().unwrap().value.manual_entries[0]
        .reference
        .is_some());
    // A later explicit language correction invalidates that eligibility instead
    // of rewriting the original phone inventory entry or deleting its copy.
    completeness_language_set(
        &store,
        snapshot.revision,
        phone_member("Phone original [Japanese]"),
        Some(CompletenessLanguage::Chinese),
    )
    .unwrap();
    let corrected = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(corrected.groups[0].status, CompletenessStatus::OwnedChinese);
    assert!(corrected.groups[0].eligible.is_none());
    assert_ne!(snapshot.evidence_hash, corrected.evidence_hash);
}

#[test]
fn existing_chinese_pc_copy_is_reused_and_invalidated_catalog_entry_is_not_presence() {
    let (_directory, store) = fixture();
    mark(
        &store,
        "Original [Japanese]",
        Some(reference(Source::Jm, "123")),
    );
    pair(&store);
    let records = vec![
        record(Source::Jm, "123", "Original [Japanese]", &[]),
        record(Source::Pica, PICA, "Translated [Chinese]", &[]),
    ];
    let pc = pc_record(
        'c',
        "Translated [Chinese].zip",
        reference(Source::Pica, PICA),
    );
    set_pc(&store, vec![pc.clone()]);
    let downloaded = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(
        downloaded.groups[0].status,
        CompletenessStatus::TranslationDownloaded
    );
    assert!(downloaded.groups[0].eligible.is_none());
    let mut unavailable = pc;
    unavailable.item.state = LibraryItemState::Unreadable;
    unavailable.item.error_code = Some("LIBRARY_FILE_MISSING".into());
    set_pc(&store, vec![unavailable]);
    let missing_file = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(
        missing_file.groups[0].status,
        CompletenessStatus::TranslationAvailable
    );
    assert!(missing_file.groups[0].computer.is_empty());
    assert_ne!(downloaded.evidence_hash, missing_file.evidence_hash);
}

#[test]
fn ambiguous_titles_unknown_language_and_unverified_authors_never_create_upgrade_proof() {
    let (_directory, store) = fixture();
    mark(&store, "[Writer] Work 2 [Japanese]", None);
    let records = vec![record(Source::Pica, PICA, "[Writer] Work 2 [Chinese]", &[])];
    let ambiguous = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(
        ambiguous.groups[0].status,
        CompletenessStatus::ReviewRequired
    );
    assert_eq!(
        ambiguous.groups[0].reasons,
        vec!["VERSION_IDENTITY_UNCONFIRMED"]
    );
    assert!(ambiguous.groups[0].phone.is_empty());
    assert!(ambiguous.groups[0].eligible.is_none());
    let unmarked_language = vec![record(Source::Jm, "456", "只有中文标题不证明语言", &[])];
    let unknown = completeness_project(&store, 1, &unmarked_language).unwrap();
    assert_eq!(
        unknown.groups[0].sources[0].language,
        CompletenessLanguage::Unknown
    );
    assert!(unknown.groups[0].eligible.is_none());
    let mut false_author = record(Source::Jm, "456", "A translation [Chinese]", &[]);
    false_author.author_verified = false;
    let unverified = completeness_project(&store, 1, &[false_author]).unwrap();
    assert_eq!(
        unverified.groups[0].status,
        CompletenessStatus::ReviewRequired
    );
    assert!(unverified.groups[0].eligible.is_none());
    let untranslated = vec![record(Source::Jm, "456", "Untitled [untranslated]", &[])];
    assert_eq!(
        completeness_project(&store, 1, &untranslated)
            .unwrap()
            .groups[0]
            .sources[0]
            .language,
        CompletenessLanguage::Unknown
    );
}

#[test]
fn language_conflicts_and_incomplete_pc_catalog_block_automatic_translation() {
    let (_directory, store) = fixture();
    mark(
        &store,
        "Original [Japanese]",
        Some(reference(Source::Jm, "123")),
    );
    pair(&store);
    let records = vec![
        record(Source::Jm, "123", "Original [Japanese]", &[]),
        record(Source::Pica, PICA, "Translated [Chinese]", &["Japanese"]),
    ];
    let conflicting = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(
        conflicting.groups[0].status,
        CompletenessStatus::WaitingTranslation
    );
    assert!(conflicting.groups[0].eligible.is_none());
    let corrected = completeness_language_set(
        &store,
        0,
        member(Source::Pica, PICA),
        Some(CompletenessLanguage::Chinese),
    )
    .unwrap();
    let mut library = store.read_library().unwrap();
    library.value.phase = LibraryPhase::Paused;
    store
        .write_library(library.revision, library.value)
        .unwrap();
    let incomplete = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(incomplete.revision, corrected.revision);
    assert_eq!(
        incomplete.groups[0].status,
        CompletenessStatus::TranslationAvailable
    );
    assert_eq!(
        incomplete.groups[0].reasons,
        vec!["COMPUTER_CATALOG_INCOMPLETE"]
    );
    assert!(incomplete.groups[0].eligible.is_none());
}

#[test]
fn explicit_family_links_merge_incrementally_and_unlink_only_changes_corrections() {
    let (directory, store) = fixture();
    mark(&store, "Phone [Japanese]", None);
    let first = completeness_family_confirm(
        &store,
        0,
        vec![
            member(Source::Jm, "123"),
            phone_member("Phone [Japanese].zip"),
        ],
    )
    .unwrap();
    let second = completeness_family_confirm(
        &store,
        first.revision,
        vec![member(Source::Pica, PICA), member(Source::Jm, "123")],
    )
    .unwrap();
    assert_eq!(second.families.len(), 1);
    assert_eq!(second.families[0].members.len(), 3);
    let duplicate = completeness_family_confirm(
        &store,
        second.revision,
        vec![member(Source::Jm, "123"), member(Source::Pica, PICA)],
    )
    .unwrap();
    assert_eq!(duplicate, second);
    assert_eq!(
        completeness_family_confirm(
            &store,
            first.revision,
            vec![member(Source::Jm, "123"), member(Source::Jm, "456")]
        )
        .unwrap_err()
        .code,
        "REVISION_CONFLICT"
    );
    let private = directory.path().join("app").join(PRIVATE_DIRECTORY);
    let phone = fs::read(private.join("phone-library.json")).unwrap();
    let pc = fs::read(private.join("library.json")).unwrap();
    let removed =
        completeness_family_unlink(&store, second.revision, &second.families[0].id).unwrap();
    assert!(removed.families.is_empty());
    assert_eq!(fs::read(private.join("phone-library.json")).unwrap(), phone);
    assert_eq!(fs::read(private.join("library.json")).unwrap(), pc);
    assert_eq!(completeness_settings_read(&store).unwrap(), removed);
}

#[test]
fn txt_refresh_and_exact_pc_filename_bridge_update_phone_translation_without_removing_pc() {
    let (directory, store) = fixture();
    let txt = directory.path().join("phone.txt");
    fs::write(&txt, "Phone original [Japanese].zip\n").unwrap();
    phone_library_from_path(&store, &txt, 0).unwrap();
    completeness_family_confirm(
        &store,
        0,
        vec![
            member(Source::Pica, PICA),
            phone_member("Phone original [Japanese]"),
        ],
    )
    .unwrap();
    let records = vec![record(Source::Pica, PICA, "Translated [Chinese]", &[])];
    assert_eq!(
        completeness_project(&store, 1, &records).unwrap().groups[0].status,
        CompletenessStatus::TranslationAvailable
    );
    set_pc(
        &store,
        vec![pc_record(
            'c',
            "Translated [Chinese].zip",
            reference(Source::Pica, PICA),
        )],
    );
    let before = store.read_library().unwrap();
    fs::write(&txt, "Translated [Chinese].zip\n").unwrap();
    let phone_revision = store.read_phone_library().unwrap().revision;
    phone_library_from_path(&store, &txt, phone_revision).unwrap();
    let refreshed = completeness_project(&store, 2, &records).unwrap();
    assert_eq!(refreshed.groups[0].status, CompletenessStatus::OwnedChinese);
    assert_eq!(refreshed.groups[0].computer.len(), 1);
    assert_eq!(refreshed.groups[0].phone.len(), 1);
    assert_eq!(store.read_library().unwrap(), before);
    assert!(refreshed.groups[0].eligible.is_none());
}

#[test]
fn unknown_phone_language_needs_explicit_correction_not_a_family_wide_language_guess() {
    let (_directory, store) = fixture();
    mark(&store, "Phone edition", None);
    let family = completeness_family_confirm(
        &store,
        0,
        vec![member(Source::Pica, PICA), phone_member("Phone edition")],
    )
    .unwrap();
    let records = vec![record(Source::Pica, PICA, "Translated [Chinese]", &[])];
    let unknown = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(unknown.groups[0].status, CompletenessStatus::ReviewRequired);
    assert_eq!(
        unknown.groups[0].phone[0].language,
        CompletenessLanguage::Unknown
    );
    let japanese = completeness_language_set(
        &store,
        family.revision,
        phone_member("Phone edition"),
        Some(CompletenessLanguage::Japanese),
    )
    .unwrap();
    assert_eq!(
        completeness_project(&store, 1, &records).unwrap().groups[0].status,
        CompletenessStatus::TranslationAvailable
    );
    completeness_language_set(
        &store,
        japanese.revision,
        phone_member("Phone edition"),
        None,
    )
    .unwrap();
    assert_eq!(
        completeness_project(&store, 1, &records).unwrap().groups[0].status,
        CompletenessStatus::ReviewRequired
    );
}

#[test]
fn invalid_members_overlap_corruption_and_future_documents_are_not_replaced() {
    let (directory, store) = fixture();
    for invalid in [
        member(Source::Jm, "0123"),
        phone_member("../outside"),
        CompletenessMember::Computer {
            item_id: "file-path".into(),
        },
    ] {
        assert_eq!(
            completeness_family_confirm(&store, 0, vec![member(Source::Jm, "123"), invalid])
                .unwrap_err()
                .code,
            "COMPLETENESS_INVALID"
        );
    }
    let current = completeness_family_confirm(
        &store,
        0,
        vec![member(Source::Jm, "123"), member(Source::Pica, PICA)],
    )
    .unwrap();
    let private = directory
        .path()
        .join("app")
        .join(PRIVATE_DIRECTORY)
        .join("completeness.json");
    let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(&private).unwrap()).unwrap();
    let duplicate = raw["value"]["families"][0].clone();
    raw["value"]["families"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    let corrupt = serde_json::to_vec(&raw).unwrap();
    fs::write(&private, &corrupt).unwrap();
    assert!(completeness_settings_read(&store).is_err());
    assert!(completeness_family_unlink(&store, current.revision, &current.families[0].id).is_err());
    assert_eq!(fs::read(&private).unwrap(), corrupt);
    let future = br#"{"schemaVersion":2,"revision":1,"value":{"version":2}}"#;
    fs::write(&private, future).unwrap();
    assert_eq!(
        completeness_settings_read(&store).unwrap_err().code,
        "UNSUPPORTED_SCHEMA"
    );
    assert_eq!(fs::read(&private).unwrap(), future);
}

#[test]
fn exact_language_brackets_keep_volume_and_edition_qualifiers_distinct() {
    let (_directory, store) = fixture();
    mark(&store, "[Writer] Work 2 [Japanese] [Limited edition]", None);
    let records = vec![record(
        Source::Jm,
        "123",
        "[Writer] Work 3 [Chinese] [Limited edition]",
        &[],
    )];
    let different_volume = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(
        different_volume.groups[0].status,
        CompletenessStatus::Missing
    );
    assert!(different_volume.groups[0].phone.is_empty());
    let records = vec![record(
        Source::Jm,
        "123",
        "[Writer] Work 2 [Chinese] [Standard edition]",
        &[],
    )];
    assert_eq!(
        completeness_project(&store, 1, &records).unwrap().groups[0].status,
        CompletenessStatus::Missing
    );
}

#[test]
fn explicit_phone_source_tokens_bind_versions_but_conflicting_ids_do_not() {
    let (_directory, store) = fixture();
    mark(&store, "[JM123] Phone original [Japanese]", None);
    pair(&store);
    let records = vec![
        record(Source::Jm, "123", "Original [Japanese]", &[]),
        record(Source::Pica, PICA, "Translated [Chinese]", &[]),
    ];
    let snapshot = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(snapshot.groups.len(), 1);
    assert_eq!(
        snapshot.groups[0].status,
        CompletenessStatus::TranslationAvailable
    );
    assert_eq!(
        snapshot.groups[0].eligible.as_ref().unwrap().kind,
        CompletenessCandidateKind::Translation
    );
    let (_other_directory, other) = fixture();
    mark(&other, "[JM123] [JM456] Ambiguous phone [Japanese]", None);
    pair(&other);
    let ambiguous = completeness_project(&other, 1, &records).unwrap();
    assert!(ambiguous.groups[0].phone.is_empty());
    assert!(ambiguous.groups[0]
        .eligible
        .as_ref()
        .is_none_or(|candidate| candidate.kind != CompletenessCandidateKind::Translation));
}

#[test]
fn phone_member_round_trips_multiple_archive_suffixes_without_repeated_normalization() {
    let (_directory, store) = fixture();
    mark(&store, "Phone.zip.rar", None);
    mark(&store, "Phone.zip", None);
    let family = completeness_family_confirm(
        &store,
        0,
        vec![member(Source::Pica, PICA), phone_member("Phone.zip.rar")],
    )
    .unwrap();
    let japanese = completeness_language_set(
        &store,
        family.revision,
        phone_member("Phone.zip.rar"),
        Some(CompletenessLanguage::Japanese),
    )
    .unwrap();
    let records = vec![record(Source::Pica, PICA, "Translation [Chinese]", &[])];
    let snapshot = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(
        snapshot.groups[0].status,
        CompletenessStatus::TranslationAvailable
    );
    assert_eq!(snapshot.groups[0].phone.len(), 1);
    assert_eq!(
        snapshot.groups[0].phone[0].member,
        phone_member("Phone.zip.rar")
    );
    completeness_language_set(
        &store,
        japanese.revision,
        snapshot.groups[0].phone[0].member.clone(),
        Some(CompletenessLanguage::Chinese),
    )
    .unwrap();
    assert_eq!(
        completeness_project(&store, 1, &records).unwrap().groups[0].status,
        CompletenessStatus::OwnedChinese
    );
    assert_eq!(
        store
            .read_phone_library()
            .unwrap()
            .value
            .manual_entries
            .len(),
        2
    );
}
