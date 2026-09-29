//! Synthetic receipt identity and destination reservation regressions.
use super::*;
use std::io::Write;

fn clear_history(f: &Fixture, task: &DownloadRecord) {
    f.service
        .remove_history(
            &f.store,
            &[TaskSelection {
                task_id: task.id.clone(),
                expected_revision: task.revision,
            }],
        )
        .unwrap();
}

fn legacy_history(f: &Fixture) {
    let mut saved = f.store.read_downloads().unwrap();
    for receipt in &mut saved.value.history_evidence {
        receipt.output_identity = None;
    }
    let json = serde_json::to_value(&saved.value).unwrap();
    assert!(json["historyEvidence"][0].get("outputIdentity").is_none());
    let value = serde_json::from_value::<workbench_storage::DownloadsDocument>(json).unwrap();
    f.store.write_downloads(saved.revision, value).unwrap();
}

fn prepare_metadata(f: &Fixture, metadata: JmDownloadMetadata) -> Result<DownloadPlan> {
    let library = f.store.read_library().unwrap();
    f.service.prepare(
        &f.store,
        &library.value.root.as_ref().unwrap().id,
        library.value.generation,
        metadata,
    )
}

fn preview(f: &Fixture, id: &str, title: &str) -> DownloadPlan {
    let mut metadata = record(f).metadata;
    metadata.work_id = id.into();
    metadata.title = title.into();
    prepare_metadata(f, metadata).unwrap()
}

fn selection(plan: &DownloadPlan) -> PreparedSelection {
    PreparedSelection {
        plan_id: plan.plan_id.clone(),
        expected_revision: plan.revision,
    }
}

fn replace_with_other_work(f: &Fixture, task: &DownloadRecord) {
    let replacement = f._temp.path().join("replacement.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&replacement).unwrap());
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);
    zip.start_file("0001.gif", options).unwrap();
    zip.write_all(&gif()).unwrap();
    zip.start_file("ComicInfo.xml", options).unwrap();
    zip.write_all(b"<ComicInfo><Title>Other work</Title><Source>JM</Source><WorkId>999999</WorkId></ComicInfo>")
        .unwrap();
    zip.finish().unwrap();
    // Keep the original inode/file ID alive, so this never depends on whether
    // the filesystem immediately reuses a deleted file's numeric identity.
    fs::rename(
        f.library.join(&task.destination),
        f._temp.path().join("original.zip"),
    )
    .unwrap();
    fs::rename(replacement, f.library.join(&task.destination)).unwrap();
}

#[test]
fn cleared_history_never_adopts_a_same_path_replacement_after_rescan() {
    // Retained task, history cleared before replacement, and history cleared
    // after the replacement was scanned must all preserve the original key.
    for clearing in [0, 1, 2] {
        let f = zip_fixture();
        let completed = complete_for_presence(&f, record(&f));
        if clearing == 1 {
            clear_history(&f, &completed);
        }
        replace_with_other_work(&f, &completed);
        rescan_tests::finish_rescan(&f);
        let library = f.store.read_library().unwrap();
        let item = &library.value.records[0];
        assert_eq!(Some(&item.item.id), completed.library_entry_id.as_ref());
        assert_eq!(item.item.source_ref.as_ref().unwrap().work_id, "999999");
        assert_eq!(item.item.state, workbench_storage::LibraryItemState::Indexed);
        assert_ne!(
            Some(&item.identity.as_ref().unwrap().file_key),
            completed.output_identity.as_ref()
        );
        if clearing == 2 {
            clear_history(&f, &completed);
        }
        let before = f.store.read_downloads().unwrap();
        if clearing != 0 {
            assert_eq!(
                before.value.history_evidence[0].output_identity,
                completed.output_identity
            );
        }
        let inventory = f.service.inventory(&f.store).unwrap();
        assert_eq!(inventory.items.len(), 1);
        assert_eq!(inventory.items[0].work_id, completed.metadata.work_id);
        assert_eq!(inventory.items[0].local_files, LocalFiles::Incomplete);
        let mut same_work = completed.metadata.clone();
        same_work.title = "A different destination for the old work".into();
        assert_eq!(
            prepare_metadata(&f, same_work).unwrap_err().code,
            "DOWNLOAD_LOCAL_FILES_INCOMPLETE"
        );
        let mut same_destination = completed.metadata.clone();
        same_destination.work_id = "999998".into();
        assert_eq!(
            prepare_metadata(&f, same_destination).unwrap_err().code,
            "DOWNLOAD_DESTINATION_EXISTS"
        );
        assert_eq!(f.store.read_downloads().unwrap(), before);
        assert_eq!(f.store.read_library().unwrap(), library);
    }
}

#[test]
fn new_history_keeps_unchanged_zip_and_directory_ownership_across_rescan() {
    for zip in [false, true] {
        let f = if zip { zip_fixture() } else { fixture() };
        let completed = complete_for_presence(&f, record(&f));
        clear_history(&f, &completed);
        let before = f.store.read_downloads().unwrap();
        assert_eq!(
            before.value.history_evidence[0].output_identity,
            completed.output_identity
        );
        rescan_tests::finish_rescan(&f);
        assert_eq!(
            f.service.inventory(&f.store).unwrap().items[0].local_files,
            LocalFiles::Present
        );
        assert_eq!(f.store.read_downloads().unwrap(), before);
    }
}

#[test]
fn legacy_history_stays_unknown_but_reviewed_registration_remains_independent() {
    let f = zip_fixture();
    let completed = complete_for_presence(&f, record(&f));
    clear_history(&f, &completed);
    legacy_history(&f);
    rescan_tests::finish_rescan(&f);
    let before = f.store.read_downloads().unwrap();
    assert_eq!(
        f.service.inventory(&f.store).unwrap().items[0].local_files,
        LocalFiles::Unavailable
    );
    let mut metadata = completed.metadata.clone();
    metadata.title = "Another destination".into();
    assert_eq!(
        prepare_metadata(&f, metadata).unwrap_err().code,
        "DOWNLOAD_LOCAL_FILES_UNAVAILABLE"
    );
    let library = f.store.read_library().unwrap();
    let bytes = fs::read(f.library.join(&completed.destination)).unwrap();
    let manifest = serde_json::to_vec(&serde_json::json!({
        "schemaVersion":1,"root":library.value.root,
        "items":[{"relativePath":completed.destination,"bytes":bytes.len(),
            "sha256":hash(&bytes),"references":[{"source":"JM","workId":"123456"}]}]
    }))
    .unwrap();
    workbench_library::import_reviewed_library(
        &f.store,
        &manifest,
        library.revision,
        &hash(&manifest),
    )
    .unwrap();
    assert_eq!(
        f.service.inventory(&f.store).unwrap().items[0].local_files,
        LocalFiles::Present
    );
    fs::remove_file(f.library.join(&completed.destination)).unwrap();
    assert_eq!(
        f.service.inventory(&f.store).unwrap().items[0].local_files,
        LocalFiles::Missing
    );
    assert_eq!(f.store.read_downloads().unwrap(), before);
}

#[test]
fn verified_relocation_keeps_new_and_legacy_history_bound_to_its_original_zip() {
    let f = fixture();
    let completed = complete_for_presence(&f, record(&f));
    clear_history(&f, &completed);
    let destination = "[Example author] Relocated.zip";
    let output = f.library.join(destination);
    let mut zip = zip::ZipWriter::new(fs::File::create(&output).unwrap());
    for name in completed
        .output_files
        .iter()
        .map(|file| file.relative_path.as_str())
        .chain(std::iter::once("_mangamonitor.json"))
    {
        zip.start_file(
            name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(&fs::read(f.library.join(&completed.destination).join(name)).unwrap())
            .unwrap();
    }
    zip.finish().unwrap();
    let mapping = serde_json::to_vec(&serde_json::json!({
        "schema_version":1,"library":f.library,
        "items":[{"old_source":f.library.join(&completed.destination),
            "zip":output,"output_sha256":hash(&fs::read(&output).unwrap())}]
    }))
    .unwrap();
    fs::remove_dir_all(f.library.join(&completed.destination)).unwrap();
    workbench_library::LibraryService::new()
        .import_paths(&f.store, &completed.root.id, completed.generation, &mapping)
        .unwrap();
    rescan_tests::finish_rescan(&f);
    assert_eq!(
        f.service.inventory(&f.store).unwrap().items[0].local_files,
        LocalFiles::Present
    );
    legacy_history(&f);
    let before = f.store.read_downloads().unwrap();
    assert_eq!(
        f.service.inventory(&f.store).unwrap().items[0].local_files,
        LocalFiles::Present
    );
    let mut relocated = completed.clone();
    relocated.destination = destination.into();
    replace_with_other_work(&f, &relocated);
    rescan_tests::finish_rescan(&f);
    assert_eq!(
        f.service.inventory(&f.store).unwrap().items[0].local_files,
        LocalFiles::Incomplete
    );
    assert_eq!(f.store.read_downloads().unwrap(), before);
}

#[test]
fn history_identity_validation_rejects_bad_keys_without_collapsing_new_receipts() {
    let f = zip_fixture();
    let completed = complete_for_presence(&f, record(&f));
    clear_history(&f, &completed);
    let before = f.store.read_downloads().unwrap();
    let mut bad = before.value.clone();
    bad.history_evidence[0].output_identity = Some("invalid".into());
    assert!(f.store.write_downloads(before.revision, bad).is_err());
    assert_eq!(f.store.read_downloads().unwrap(), before);
    let mut distinct = before.value.clone();
    let mut later = distinct.history_evidence[0].clone();
    later.output_identity = Some("a".repeat(64));
    distinct.history_evidence.push(later);
    f.store.write_downloads(before.revision, distinct).unwrap();
}

#[test]
fn pending_tasks_reserve_case_equivalent_destinations_across_work_ids() {
    for phase in [
        DownloadPhase::Queued,
        DownloadPhase::Downloading,
        DownloadPhase::Verifying,
        DownloadPhase::Saving,
        DownloadPhase::Paused,
        DownloadPhase::Error,
    ] {
        let f = zip_fixture();
        let mut task = record(&f);
        task.phase = phase;
        task.error_code = (phase == DownloadPhase::Error).then(|| "DOWNLOAD_FAILED".into());
        put(&f, task.clone());
        let mut metadata = task.metadata;
        metadata.work_id = "999999".into();
        metadata.title = metadata.title.to_uppercase();
        let before = f.store.read_downloads().unwrap();
        assert_eq!(
            prepare_metadata(&f, metadata).unwrap_err().code,
            "DOWNLOAD_DESTINATION_EXISTS"
        );
        assert_eq!(f.store.read_downloads().unwrap(), before);
        assert!(fs::read_dir(&f.library).unwrap().next().is_none());
    }
}

#[test]
fn same_selection_name_collision_is_rejected_without_losing_the_other_preview() {
    let f = zip_fixture();
    let first = preview(&f, "1001", "Collision");
    let second = preview(&f, "1002", "COLLISION");
    let before = f.store.read_downloads().unwrap();
    assert_eq!(
        f.service
            .confirm_many(&f.store, &[selection(&first), selection(&second)])
            .unwrap_err()
            .code,
        "DOWNLOAD_DESTINATION_EXISTS"
    );
    assert_eq!(f.store.read_downloads().unwrap(), before);
    assert_eq!(
        f.service
            .check_prepared_destinations(&second.plan_id, std::slice::from_ref(&first.plan_id))
            .unwrap_err()
            .code,
        "DOWNLOAD_DESTINATION_EXISTS"
    );
    assert_eq!(
        f.service
            .confirm(&f.store, &second.plan_id, second.revision)
            .unwrap_err()
            .code,
        "DOWNLOAD_PLAN_STALE"
    );
    f.service
        .confirm(&f.store, &first.plan_id, first.revision)
        .unwrap();
}

#[test]
fn mixed_source_selection_reserves_shared_names_but_accepts_distinct_destinations() {
    let f = zip_fixture();
    let jm = preview(&f, "1001", "Shared destination");
    let library = f.store.read_library().unwrap();
    let root_id = &library.value.root.as_ref().unwrap().id;
    let mut metadata = record(&f).metadata;
    metadata.work_id = "0123456789abcdef01234567".into();
    metadata.title = "SHARED DESTINATION".into();
    let prepare_pica = |metadata| {
        f.service.prepare_for_source(
            &f.store,
            root_id,
            library.value.generation,
            Source::Pica,
            metadata,
        )
    };
    let pica = prepare_pica(metadata.clone()).unwrap();
    let before = f.store.read_downloads().unwrap();
    assert_eq!(
        f.service
            .confirm_selection(&f.store, &[selection(&jm), selection(&pica)])
            .unwrap_err()
            .code,
        "DOWNLOAD_DESTINATION_EXISTS"
    );
    assert_eq!(f.store.read_downloads().unwrap(), before);
    assert_eq!(
        f.service
            .check_prepared_destinations(&pica.plan_id, std::slice::from_ref(&jm.plan_id))
            .unwrap_err()
            .code,
        "DOWNLOAD_DESTINATION_EXISTS"
    );
    metadata.title = "Pica distinct destination".into();
    let distinct = prepare_pica(metadata.clone()).unwrap();
    f.service
        .check_prepared_destinations(&distinct.plan_id, std::slice::from_ref(&jm.plan_id))
        .unwrap();
    let confirmed = f
        .service
        .confirm_selection(&f.store, &[selection(&jm), selection(&distinct)])
        .unwrap();
    assert_eq!(confirmed.tasks.len(), 3);
    assert_eq!(confirmed.tasks[1].source, Source::Jm);
    assert_eq!(confirmed.tasks[2].source, Source::Pica);
    metadata.work_id = "1123456789abcdef01234567".into();
    metadata.title = "Shared destination".into();
    assert_eq!(
        prepare_pica(metadata).unwrap_err().code,
        "DOWNLOAD_DESTINATION_EXISTS"
    );
}

#[test]
fn preview_reservations_are_explicit_and_stale_selections_discard_only_the_new_plan() {
    let f = zip_fixture();
    let cancelled = preview(&f, "1001", "Same preview");
    let current = preview(&f, "1001", "Same preview");
    f.service
        .check_prepared_destinations(&current.plan_id, &[])
        .unwrap();
    f.service
        .check_prepared_destinations(
            &current.plan_id,
            &[current.plan_id.clone(), current.plan_id.clone()],
        )
        .unwrap();
    assert_eq!(
        f.service
            .check_prepared_destinations(&current.plan_id, &["missing".into()])
            .unwrap_err()
            .code,
        "DOWNLOAD_PLAN_STALE"
    );
    f.service
        .confirm(&f.store, &cancelled.plan_id, cancelled.revision)
        .unwrap();
}

#[test]
fn unicode_case_equivalent_existing_file_is_rejected_during_prepare_and_confirm() {
    let f = zip_fixture();
    let plan = preview(&f, "1001", "Ä example");
    let existing = f.library.join("[EXAMPLE AUTHOR] ä EXAMPLE.ZIP");
    fs::write(&existing, b"unrelated existing file").unwrap();
    let before = f.store.read_downloads().unwrap();
    assert_eq!(
        f.service
            .confirm(&f.store, &plan.plan_id, plan.revision)
            .unwrap_err()
            .code,
        "DOWNLOAD_DESTINATION_EXISTS"
    );
    let mut metadata = record(&f).metadata;
    metadata.work_id = "1002".into();
    metadata.title = "Ä example".into();
    assert_eq!(
        prepare_metadata(&f, metadata).unwrap_err().code,
        "DOWNLOAD_DESTINATION_EXISTS"
    );
    assert_eq!(fs::read(existing).unwrap(), b"unrelated existing file");
    assert_eq!(f.store.read_downloads().unwrap(), before);
}

#[tokio::test]
async fn a_destination_occupied_after_confirmation_is_rejected_before_media_work() {
    let f = zip_fixture();
    let original = record(&f);
    let existing = f.library.join(original.destination.to_uppercase());
    fs::write(&existing, b"unrelated existing file").unwrap();
    let before = f.store.read_downloads().unwrap();
    assert_eq!(
        f.service
            .run(&f.store, &f.id, || Ok(()))
            .await
            .unwrap_err()
            .code,
        "DOWNLOAD_DESTINATION_EXISTS"
    );
    assert_eq!(f.store.read_downloads().unwrap(), before);
    assert!(!f._temp.path().join("private/download-staging-v1").exists());
    assert_eq!(fs::read(&existing).unwrap(), b"unrelated existing file");
    f.service
        .fail_queued(
            &f.store,
            &f.id,
            original.revision,
            "DOWNLOAD_DESTINATION_EXISTS",
        )
        .unwrap();
    fs::remove_file(existing).unwrap();
    seed_report(&f);
    f.service
        .control(&f.store, &f.id, original.revision, Control::Retry)
        .unwrap();
    let receipt = f
        .service
        .run(&f.store, &f.id, || Ok(()))
        .await
        .unwrap()
        .unwrap();
    rescan_tests::register(&f, &receipt);
    assert_eq!(record(&f).phase, DownloadPhase::Downloaded);
    assert_eq!(record(&f).target_hash, original.target_hash);
}

#[tokio::test]
async fn destination_preflight_allows_the_same_tasks_partial_zip_and_directory() {
    for zip in [false, true] {
        let f = if zip { zip_fixture() } else { fixture() };
        seed_report(&f);
        let mut task = record(&f);
        let original_hash = task.target_hash.clone();
        let report: LocalExecutionReport =
            serde_json::from_str(task.staging_report_json.as_deref().unwrap()).unwrap();
        let stage = f
            .store
            .open_download_workspace()
            .unwrap()
            .path()
            .to_path_buf();
        let paused = Cell::new(false);
        assert_eq!(
            materialize::save(
                &mut task,
                &report,
                &stage,
                &|| {
                    if paused.get() {
                        Err(error("DOWNLOAD_PAUSED"))
                    } else {
                        Ok(())
                    }
                },
                &mut |saved| {
                    if saved.output_identity.is_some() {
                        paused.set(true);
                    }
                    Ok(())
                },
            )
            .unwrap_err()
            .code,
            "DOWNLOAD_PAUSED"
        );
        assert!(task.output_identity.is_some());
        assert!(task.output_manifest_hash.is_none());
        task.phase = DownloadPhase::Error;
        task.error_code = Some("DOWNLOAD_PAUSED".into());
        put(&f, task.clone());
        f.service
            .control(&f.store, &f.id, task.revision, Control::Retry)
            .unwrap();
        let receipt = f
            .service
            .run(&f.store, &f.id, || Ok(()))
            .await
            .unwrap()
            .unwrap();
        rescan_tests::register(&f, &receipt);
        let completed = record(&f);
        assert_eq!(completed.phase, DownloadPhase::Downloaded);
        assert_eq!(completed.target_hash, original_hash);
        assert_eq!(completed.output_identity, task.output_identity);
        materialize::verify_output(&completed).unwrap();
    }
}
