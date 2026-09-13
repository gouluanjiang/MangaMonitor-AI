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

fn computer(id: char) -> CompletenessMember {
    CompletenessMember::Computer {
        item_id: id.to_string().repeat(64),
    }
}

#[test]
fn omissions_persist_across_scans_and_retired_phone_lists_are_not_ownership() {
    let (directory, store) = fixture();
    let records = vec![record(Source::Jm, "123", "Older omission [Chinese]", &[])];
    let first = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(first.groups[0].status, CompletenessStatus::Missing);
    mark(
        &store,
        "Older omission [Chinese]",
        Some(reference(Source::Jm, "123")),
    );
    let private = directory.path().join("app").join(PRIVATE_DIRECTORY);
    let before = fs::read(private.join("phone-library.json")).unwrap();
    let second = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(
        second, first,
        "retired phone state cannot alter projection or authority hash"
    );
    let later = completeness_project(&store, 2, &records).unwrap();
    assert_eq!(later.groups[0].status, CompletenessStatus::Missing);
    assert_eq!(
        later.groups[0].eligible.as_ref().unwrap().kind,
        CompletenessCandidateKind::Missing
    );
    assert_eq!(first.groups[0].group_id, later.groups[0].group_id);
    assert_ne!(first.evidence_hash, later.evidence_hash);
    assert_eq!(
        fs::read(private.join("phone-library.json")).unwrap(),
        before
    );
    assert!(!private.join("downloads.json").exists());
}

#[test]
fn chinese_pc_version_satisfies_both_confirmed_sources_without_phone_evidence() {
    let (_directory, store) = fixture();
    set_pc(
        &store,
        vec![pc_record(
            'c',
            "Translated [Chinese].zip",
            reference(Source::Pica, PICA),
        )],
    );
    pair(&store);
    let records = vec![
        record(Source::Jm, "123", "Original [Japanese]", &[]),
        record(Source::Pica, PICA, "Translation [Chinese]", &[]),
    ];
    let view = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(view.groups.len(), 1);
    assert_eq!(view.groups[0].status, CompletenessStatus::OwnedChinese);
    assert_eq!(view.groups[0].sources.len(), 2);
    assert_eq!(view.groups[0].computer.len(), 1);
    assert!(view.groups[0].phone.is_empty());
    assert!(view.groups[0].eligible.is_none());
}

#[test]
fn japanese_copy_waits_for_a_confirmed_translation_then_chinese_zip_completes_it() {
    let (_directory, store) = fixture();
    let original = pc_record('c', "Original [Japanese].zip", reference(Source::Jm, "123"));
    set_pc(&store, vec![original.clone()]);
    let mut records = vec![record(Source::Jm, "123", "Original [Japanese]", &[])];
    assert_eq!(
        completeness_project(&store, 1, &records).unwrap().groups[0].status,
        CompletenessStatus::WaitingTranslation
    );
    records.push(record(
        Source::Pica,
        PICA,
        "Different translation [Chinese]",
        &[],
    ));
    let unlinked = completeness_project(&store, 2, &records).unwrap();
    assert!(unlinked.groups.iter().all(|g| g
        .eligible
        .as_ref()
        .is_none_or(|c| c.kind != CompletenessCandidateKind::Translation)));
    pair(&store);
    let ready = completeness_project(&store, 2, &records).unwrap();
    assert_eq!(
        ready.groups[0].status,
        CompletenessStatus::TranslationAvailable
    );
    assert_eq!(
        ready.groups[0].eligible.as_ref().unwrap().kind,
        CompletenessCandidateKind::Translation
    );
    set_pc(
        &store,
        vec![
            original.clone(),
            pc_record(
                'd',
                "Translated [Chinese].zip",
                reference(Source::Pica, PICA),
            ),
        ],
    );
    let done = completeness_project(&store, 3, &records).unwrap();
    assert_eq!(done.groups[0].status, CompletenessStatus::OwnedChinese);
    assert!(done.groups[0].eligible.is_none());
    assert_eq!(store.read_library().unwrap().value.records[0], original);
}

#[test]
fn unreadable_changed_and_zero_page_files_require_review_instead_of_presence_or_absence() {
    let (_directory, store) = fixture();
    let records = vec![record(Source::Jm, "123", "Translated [Chinese]", &[])];
    for state in 0..3 {
        let mut item = pc_record(
            'c',
            "Translated [Chinese].zip",
            reference(Source::Jm, "123"),
        );
        if state == 0 {
            item.item.state = LibraryItemState::Unreadable;
            item.item.error_code = Some("LIBRARY_FILE_CHANGED".into());
        } else if state == 1 {
            item.item.error_code = Some("LIBRARY_METADATA_INVALID".into());
        } else {
            item.item.page_count = Some(0);
        }
        set_pc(&store, vec![item]);
        let view = completeness_project(&store, 1, &records).unwrap();
        assert_eq!(view.groups[0].status, CompletenessStatus::ReviewRequired);
        assert!(view.groups[0].eligible.is_none());
        assert!(view.groups[0].computer.is_empty());
    }
}

#[test]
fn unknown_language_requires_explicit_file_correction_not_family_language_inheritance() {
    let (_directory, store) = fixture();
    set_pc(
        &store,
        vec![pc_record(
            'c',
            "Different edition.zip",
            reference(Source::Jm, "123"),
        )],
    );
    let records = vec![record(Source::Pica, PICA, "Translated [Chinese]", &[])];
    let family =
        completeness_family_confirm(&store, 0, vec![member(Source::Pica, PICA), computer('c')])
            .unwrap();
    assert_eq!(
        completeness_project(&store, 1, &records).unwrap().groups[0].status,
        CompletenessStatus::ReviewRequired
    );
    let corrected = completeness_language_set(
        &store,
        family.revision,
        computer('c'),
        Some(CompletenessLanguage::Japanese),
    )
    .unwrap();
    assert_eq!(
        completeness_project(&store, 1, &records).unwrap().groups[0].status,
        CompletenessStatus::TranslationAvailable
    );
    completeness_language_set(&store, corrected.revision, computer('c'), None).unwrap();
    assert_eq!(
        completeness_project(&store, 1, &records).unwrap().groups[0].status,
        CompletenessStatus::ReviewRequired
    );
}

#[test]
fn title_candidates_unverified_author_language_conflicts_and_partial_catalog_block_execution() {
    let (_directory, store) = fixture();
    let mut pc = pc_record(
        'c',
        "[Writer] Work 2 [Japanese]",
        reference(Source::Jm, "123"),
    );
    pc.item.source_ref = None;
    pc.item.identity_evidence = None;
    set_pc(&store, vec![pc]);
    let source = record(Source::Pica, PICA, "[Writer] Work 2 [Chinese]", &[]);
    let ambiguous = completeness_project(&store, 1, &[source]).unwrap();
    assert_eq!(
        ambiguous.groups[0].reasons,
        vec!["VERSION_IDENTITY_UNCONFIRMED"]
    );
    assert!(ambiguous.groups[0].eligible.is_none());
    let other_volume = record(Source::Pica, PICA, "[Writer] Work 3 [Chinese]", &[]);
    assert_eq!(
        completeness_project(&store, 1, std::slice::from_ref(&other_volume))
            .unwrap()
            .groups[0]
            .status,
        CompletenessStatus::Missing
    );
    let mut wrong_author = other_volume.clone();
    wrong_author.author_verified = false;
    assert!(completeness_project(&store, 1, &[wrong_author])
        .unwrap()
        .groups[0]
        .eligible
        .is_none());
    let mut conflicting = other_volume.clone();
    conflicting.work.tags = vec!["Japanese".into()];
    assert_eq!(
        completeness_project(&store, 1, &[conflicting])
            .unwrap()
            .groups[0]
            .sources[0]
            .language,
        CompletenessLanguage::Unknown
    );
    let mut library = store.read_library().unwrap();
    library.value.phase = LibraryPhase::Paused;
    store
        .write_library(library.revision, library.value)
        .unwrap();
    assert_eq!(
        completeness_project(&store, 1, &[other_volume])
            .unwrap()
            .groups[0]
            .status,
        CompletenessStatus::Unknown
    );
}

#[test]
fn migrated_file_keeps_prior_family_and_language_corrections_only_for_same_identity() {
    let (_directory, store) = fixture();
    let new = pc_record('d', "Renamed.zip", reference(Source::Jm, "123"));
    set_pc(&store, vec![new.clone()]);
    let family =
        completeness_family_confirm(&store, 0, vec![member(Source::Pica, PICA), computer('c')])
            .unwrap();
    completeness_language_set(
        &store,
        family.revision,
        computer('c'),
        Some(CompletenessLanguage::Japanese),
    )
    .unwrap();
    let mut library = store.read_library().unwrap();
    library.value.relocations.push(LibraryRelocation {
        old_path: "Old name".into(),
        old_item_id: "c".repeat(64),
        new_path: new.item.relative_path.clone(),
        new_item_id: new.item.id.clone(),
        identity: new.identity.clone().unwrap(),
        sha256: "a".repeat(64),
    });
    store
        .write_library(library.revision, library.value)
        .unwrap();
    let records = vec![record(Source::Pica, PICA, "Translation [Chinese]", &[])];
    let view = completeness_project(&store, 1, &records).unwrap();
    assert_eq!(
        view.groups[0].status,
        CompletenessStatus::TranslationAvailable
    );
    assert_eq!(view.groups[0].computer[0].member, computer('d'));
    let mut library = store.read_library().unwrap();
    library.value.records[0].identity.as_mut().unwrap().modified = "999".into();
    store
        .write_library(library.revision, library.value)
        .unwrap();
    let changed = completeness_project(&store, 1, &records).unwrap();
    assert!(changed.groups[0]
        .eligible
        .as_ref()
        .is_none_or(|c| c.kind != CompletenessCandidateKind::Translation));
}

#[test]
fn family_merge_and_unlink_are_revision_bound_and_do_not_mutate_library_files() {
    let (_directory, store) = fixture();
    let first =
        completeness_family_confirm(&store, 0, vec![member(Source::Jm, "123"), computer('c')])
            .unwrap();
    let next = completeness_family_confirm(
        &store,
        first.revision,
        vec![member(Source::Pica, PICA), member(Source::Jm, "123")],
    )
    .unwrap();
    assert_eq!(next.families.len(), 1);
    assert_eq!(next.families[0].members.len(), 3);
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
    let before = store.read_library().unwrap();
    completeness_family_unlink(&store, next.revision, &next.families[0].id).unwrap();
    assert_eq!(store.read_library().unwrap(), before);
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
