//! Isolated synthetic queue/housekeeping regressions; never uses account or media transports.
use super::*;

fn synthetic_record(base: &DownloadRecord, index: usize, phase: DownloadPhase) -> DownloadRecord {
    let mut value = base.clone();
    value.id = hash(format!("maintenance-{index}").as_bytes());
    value.metadata.work_id = (900000 + index).to_string();
    value.metadata.title = format!("Synthetic {index}");
    value.destination = format!("Synthetic {index}.zip");
    value.phase = phase;
    value.target_hash = binding(&value).unwrap();
    value
}

#[test]
fn abandonment_releases_capacity_and_explicit_empty_cleanup_removes_only_its_record() {
    let f = zip_fixture();
    let base = record(&f);
    let mut document = f.store.read_downloads().unwrap();
    document.value.tasks = (0..500)
        .map(|n| synthetic_record(&base, n, DownloadPhase::Paused))
        .collect();
    let first = document.value.tasks[0].clone();
    f.store
        .write_downloads(document.revision, document.value)
        .unwrap();
    let prepare = || {
        f.service.prepare(
            &f.store,
            &base.root.id,
            base.generation,
            base.metadata.clone(),
        )
    };
    assert_eq!(prepare().unwrap_err().code, "DOWNLOAD_LIMIT_REACHED");
    f.service
        .control(&f.store, &first.id, first.revision, Control::Abandon)
        .unwrap();
    let plan = prepare().unwrap();
    f.service
        .confirm(&f.store, &plan.plan_id, plan.revision)
        .unwrap();
    assert_eq!(f.store.read_downloads().unwrap().value.tasks.len(), 501);
    assert_eq!(prepare().unwrap_err().code, "DOWNLOAD_LIMIT_REACHED");
    assert_eq!(
        f.service
            .control(&f.store, &first.id, first.revision, Control::Cleanup)
            .unwrap_err()
            .code,
        "DOWNLOAD_TASK_STALE"
    );
    let library = f.store.read_library().unwrap();
    let result = f
        .service
        .control(&f.store, &first.id, first.revision + 1, Control::Cleanup)
        .unwrap();
    assert_eq!(result.tasks.len(), 500);
    assert!(result.tasks.iter().all(|task| task.id != first.id));
    assert_eq!(f.store.read_library().unwrap(), library);
}

#[test]
fn abandoned_retention_is_bounded_and_cleaning_one_reopens_an_abandonment_slot() {
    let f = zip_fixture();
    let base = record(&f);
    let mut document = f.store.read_downloads().unwrap();
    document.value.tasks = (0..500)
        .map(|n| synthetic_record(&base, n, DownloadPhase::Abandoned))
        .collect();
    let first = document.value.tasks[0].clone();
    let pending = synthetic_record(&base, 501, DownloadPhase::Paused);
    document.value.tasks.push(pending.clone());
    f.store
        .write_downloads(document.revision, document.value)
        .unwrap();
    assert_eq!(
        f.service
            .control(&f.store, &pending.id, pending.revision, Control::Abandon)
            .unwrap_err()
            .code,
        "DOWNLOAD_ABANDONED_LIMIT_REACHED"
    );
    f.service
        .control(&f.store, &first.id, first.revision, Control::Cleanup)
        .unwrap();
    f.service
        .control(&f.store, &pending.id, pending.revision, Control::Abandon)
        .unwrap();
    assert_eq!(f.store.read_downloads().unwrap().value.tasks.len(), 500);
}

#[test]
fn cleanup_preflights_all_staging_and_preserves_unknown_or_changed_contents() {
    for changed in [false, true] {
        let f = zip_fixture();
        let mut value = record(&f);
        let (stage, report) = report(&f, &value);
        let command = stage.join(&report.staging_subdir);
        let first = command.join(&report.source_completion.manifest.artifacts[0].relative_path);
        let second = command.join(&report.source_completion.manifest.artifacts[1].relative_path);
        let bytes = fs::read(&first).unwrap();
        let unknown = first.parent().unwrap().join("unknown.txt");
        if changed {
            fs::write(&second, b"changed").unwrap();
        } else {
            fs::write(&unknown, b"unrelated").unwrap();
        }
        value.staging_report_json = Some(serde_json::to_string(&report).unwrap());
        value.phase = DownloadPhase::Error;
        put(&f, value.clone());
        f.service
            .control(&f.store, &value.id, value.revision, Control::Abandon)
            .unwrap();
        let before = f.store.read_downloads().unwrap();
        assert!(f
            .service
            .control(&f.store, &value.id, value.revision + 1, Control::Cleanup)
            .is_err());
        assert_eq!(f.store.read_downloads().unwrap(), before);
        assert_eq!(fs::read(&first).unwrap(), bytes);
        if !changed {
            assert_eq!(fs::read(&unknown).unwrap(), b"unrelated");
            fs::remove_file(&unknown).unwrap();
            f.service
                .control(&f.store, &value.id, value.revision + 1, Control::Cleanup)
                .unwrap();
            assert!(!command.exists());
        }
    }
}

#[tokio::test]
async fn abandoned_index_failure_can_clean_staging_without_touching_final_zip_or_library() {
    let f = zip_fixture();
    seed_report(&f);
    let receipt = f
        .service
        .run(&f.store, &f.id, || Ok(()))
        .await
        .unwrap()
        .unwrap();
    let bytes = fs::read(f.library.join(&receipt.relative_path)).unwrap();
    f.service
        .index_failed(&f.store, &receipt, "LIBRARY_LIMIT_REACHED")
        .unwrap();
    let value = record(&f);
    let library = f.store.read_library().unwrap();
    f.service
        .control(&f.store, &value.id, value.revision, Control::Abandon)
        .unwrap();
    f.service
        .control(&f.store, &value.id, value.revision + 1, Control::Cleanup)
        .unwrap();
    assert!(f.store.read_downloads().unwrap().value.tasks.is_empty());
    assert_eq!(
        fs::read(f.library.join(&receipt.relative_path)).unwrap(),
        bytes
    );
    assert_eq!(f.store.read_library().unwrap(), library);
}

#[test]
fn active_task_cannot_be_abandoned_and_cleanup_requires_exclusive_worker_workspace() {
    let f = zip_fixture();
    set_active(&f);
    let before = f.store.read_downloads().unwrap();
    assert!(f
        .service
        .control(&f.store, &f.id, 1, Control::Abandon)
        .is_err());
    assert_eq!(f.store.read_downloads().unwrap(), before);
    f.service.clear(&f.id, 1);
    f.service
        .control(&f.store, &f.id, 1, Control::Abandon)
        .unwrap();
    let worker = f.store.open_download_workspace().unwrap();
    assert_eq!(
        f.service
            .control(&f.store, &f.id, 2, Control::Cleanup)
            .unwrap_err()
            .code,
        "DOWNLOAD_WORKER_BUSY"
    );
    drop(worker);
    assert_eq!(record(&f).phase, DownloadPhase::Abandoned);
}

fn selection(task: &DownloadRecord) -> TaskSelection {
    TaskSelection {
        task_id: task.id.clone(),
        expected_revision: task.revision,
    }
}

fn staged_command(f: &Fixture, task: &DownloadRecord) -> PathBuf {
    let report: LocalExecutionReport =
        serde_json::from_str(task.staging_report_json.as_deref().unwrap()).unwrap();
    downloads_path(f)
        .parent()
        .unwrap()
        .join("download-staging-v1")
        .join(report.staging_subdir)
}

#[test]
fn completed_before_cleanup_reopens_with_proof_until_explicit_cleanup_then_history_removal() {
    for zip in [false, true] {
        let f = if zip { zip_fixture() } else { fixture() };
        // Simulate a process exiting after the durable completion write and
        // before it could attempt cleanup or record a cleanup error.
        let completed = complete_before_cleanup(&f, record(&f));
        let command = staged_command(&f, &completed);
        let before = fs::read(downloads_path(&f)).unwrap();
        let library = f.store.read_library().unwrap();
        let phone = f.store.read_phone_library().unwrap();
        let reopened = WorkbenchStore::open(f._temp.path().join("private")).unwrap();
        let service = DownloadService::new();
        let restored = service.read(&reopened).unwrap();
        assert_eq!(restored.tasks[0].phase, DownloadPhase::Downloaded);
        assert!(restored.tasks[0]
            .allowed_actions
            .contains(&Control::Cleanup));
        assert!(
            command.is_dir(),
            "reading must not automatically delete staging"
        );
        assert_eq!(
            service
                .remove_history(&reopened, &[selection(&completed)])
                .unwrap_err()
                .code,
            "DOWNLOAD_HISTORY_CLEANUP_REQUIRED"
        );
        assert_eq!(fs::read(downloads_path(&f)).unwrap(), before);
        for _ in 0..2 {
            let cleaned = service
                .control(
                    &reopened,
                    &completed.id,
                    completed.revision,
                    Control::Cleanup,
                )
                .unwrap();
            assert_eq!(cleaned.tasks[0].phase, DownloadPhase::Downloaded);
            assert_eq!(cleaned.tasks[0].revision, completed.revision);
            assert_eq!(cleaned.tasks[0].updated_at, completed.updated_at);
            assert_eq!(fs::read(downloads_path(&f)).unwrap(), before);
            assert!(!command.exists());
            materialize::verify_output(&completed).unwrap();
        }
        assert!(service
            .remove_history(&reopened, &[selection(&completed)])
            .unwrap()
            .tasks
            .is_empty());
        assert_eq!(
            reopened
                .read_downloads()
                .unwrap()
                .value
                .history_evidence
                .len(),
            1
        );
        assert_eq!(reopened.read_library().unwrap(), library);
        assert_eq!(reopened.read_phone_library().unwrap(), phone);
        materialize::verify_output(&completed).unwrap();
    }
}

#[test]
fn completed_cleanup_resumes_after_the_last_file_or_chapters_directory_was_removed() {
    for empty_command in [false, true] {
        let f = zip_fixture();
        let completed = complete_before_cleanup(&f, record(&f));
        let command = staged_command(&f, &completed);
        let report: LocalExecutionReport =
            serde_json::from_str(completed.staging_report_json.as_deref().unwrap()).unwrap();
        let mut chapter_paths = std::collections::BTreeSet::new();
        for artifact in &report.source_completion.manifest.artifacts {
            let relative = Path::new(&artifact.relative_path);
            fs::remove_file(command.join(relative)).unwrap();
            chapter_paths.insert(relative.parent().unwrap().to_path_buf());
        }
        for chapter in chapter_paths {
            fs::remove_dir(command.join(chapter)).unwrap();
        }
        if empty_command {
            fs::remove_dir(command.join("chapters")).unwrap();
        }
        let before = f.store.read_downloads().unwrap();
        let library = f.store.read_library().unwrap();
        let reopened = WorkbenchStore::open(f._temp.path().join("private")).unwrap();
        let service = DownloadService::new();
        assert_eq!(
            service
                .remove_history(&reopened, &[selection(&completed)])
                .unwrap_err()
                .code,
            "DOWNLOAD_HISTORY_CLEANUP_REQUIRED"
        );
        let cleaned = service
            .control(
                &reopened,
                &completed.id,
                completed.revision,
                Control::Cleanup,
            )
            .unwrap();
        assert_eq!(cleaned.tasks[0].phase, DownloadPhase::Downloaded);
        assert!(!command.exists());
        assert_eq!(f.store.read_downloads().unwrap(), before);
        assert_eq!(f.store.read_library().unwrap(), library);
        materialize::verify_output(&completed).unwrap();
    }
}

#[test]
fn missing_private_staging_tree_does_not_permanently_block_completed_history() {
    for missing_root in [false, true] {
        let f = zip_fixture();
        let completed = complete_for_presence(&f, record(&f));
        let staging = downloads_path(&f)
            .parent()
            .unwrap()
            .join("download-staging-v1");
        // Only known, generated empty directories and the released fixture lock
        // are removed. Opening a workspace safely recreates these ancestors.
        fs::remove_dir(staging.join("commands")).unwrap();
        if missing_root {
            fs::remove_file(staging.join("worker.lock")).unwrap();
            fs::remove_dir(&staging).unwrap();
        }
        let library = f.store.read_library().unwrap();
        let reopened = WorkbenchStore::open(f._temp.path().join("private")).unwrap();
        let service = DownloadService::new();
        assert!(service
            .remove_history(&reopened, &[selection(&completed)])
            .unwrap()
            .tasks
            .is_empty());
        assert_eq!(
            reopened
                .read_downloads()
                .unwrap()
                .value
                .history_evidence
                .len(),
            1
        );
        assert!(staging.join("commands").is_dir());
        assert_eq!(reopened.read_library().unwrap(), library);
        materialize::verify_output(&completed).unwrap();
    }
}

#[tokio::test]
async fn failed_post_registration_cleanup_keeps_completion_and_reopens_for_explicit_retry() {
    let f = zip_fixture();
    seed_report(&f);
    let receipt = f
        .service
        .run(&f.store, &f.id, || Ok(()))
        .await
        .unwrap()
        .unwrap();
    let indexed = workbench_library::LibraryService::new()
        .register_completed(
            &f.store,
            &receipt.root_id,
            receipt.generation,
            &receipt.relative_path,
            &workbench_storage::LibraryReference {
                source: Source::Jm,
                work_id: receipt.work_id.clone(),
            },
            receipt.expected_pages,
        )
        .unwrap();
    let entry = indexed
        .items
        .iter()
        .find(|item| item.relative_path == receipt.relative_path)
        .unwrap();
    let command = staged_command(&f, &record(&f));
    let unknown = command.join("unrecognized-synthetic.txt");
    fs::write(&unknown, b"retain this unrecognized file").unwrap();
    let finished = f
        .service
        .mark_indexed(&f.store, &receipt, &entry.id)
        .unwrap();
    assert_eq!(finished.tasks[0].phase, DownloadPhase::Downloaded);
    assert_eq!(finished.tasks[0].error_code, None);
    let completed = record(&f);
    let before = f.store.read_downloads().unwrap();
    let service = DownloadService::new();
    assert_eq!(
        service
            .remove_history(&f.store, &[selection(&completed)])
            .unwrap_err()
            .code,
        "DOWNLOAD_HISTORY_CLEANUP_REQUIRED"
    );
    assert!(service
        .control(
            &f.store,
            &completed.id,
            completed.revision,
            Control::Cleanup
        )
        .is_err());
    assert_eq!(f.store.read_downloads().unwrap(), before);
    assert_eq!(
        fs::read(&unknown).unwrap(),
        b"retain this unrecognized file"
    );
    // Remove only the deliberately injected synthetic blocker, then use the
    // same explicit service action a restarted desktop exposes.
    fs::remove_file(unknown).unwrap();
    service
        .control(
            &f.store,
            &completed.id,
            completed.revision,
            Control::Cleanup,
        )
        .unwrap();
    assert!(!command.exists());
    assert_eq!(f.store.read_downloads().unwrap(), before);
    materialize::verify_output(&completed).unwrap();
}

#[test]
fn completed_cleanup_preflights_changed_staging_and_final_output_without_losing_proof() {
    for change_final in [false, true] {
        let f = zip_fixture();
        let completed = complete_before_cleanup(&f, record(&f));
        let command = staged_command(&f, &completed);
        let report: LocalExecutionReport =
            serde_json::from_str(completed.staging_report_json.as_deref().unwrap()).unwrap();
        let artifacts = &report.source_completion.manifest.artifacts;
        let first = command.join(&artifacts[0].relative_path);
        let second = command.join(&artifacts[1].relative_path);
        let first_bytes = fs::read(&first).unwrap();
        let target = if change_final {
            f.library.join(&completed.destination)
        } else {
            second.clone()
        };
        fs::write(&target, b"changed synthetic content").unwrap();
        let before = f.store.read_downloads().unwrap();
        let service = DownloadService::new();
        assert!(service
            .control(
                &f.store,
                &completed.id,
                completed.revision,
                Control::Cleanup
            )
            .is_err());
        assert_eq!(fs::read(&first).unwrap(), first_bytes);
        assert!(second.is_file());
        assert_eq!(fs::read(&target).unwrap(), b"changed synthetic content");
        assert_eq!(f.store.read_downloads().unwrap(), before);
        assert!(service
            .remove_history(&f.store, &[selection(&completed)])
            .is_err());
        assert_eq!(f.store.read_downloads().unwrap(), before);
    }
}

#[test]
fn completed_cleanup_and_history_checks_require_workspace_and_current_task_revision() {
    let f = zip_fixture();
    let completed = complete_before_cleanup(&f, record(&f));
    let before = f.store.read_downloads().unwrap();
    let service = DownloadService::new();
    let worker = f.store.open_download_workspace().unwrap();
    assert_eq!(
        service
            .control(
                &f.store,
                &completed.id,
                completed.revision,
                Control::Cleanup
            )
            .unwrap_err()
            .code,
        "DOWNLOAD_WORKER_BUSY"
    );
    assert_eq!(
        service
            .remove_history(&f.store, &[selection(&completed)])
            .unwrap_err()
            .code,
        "DOWNLOAD_WORKER_BUSY"
    );
    drop(worker);
    assert_eq!(
        service
            .control(
                &f.store,
                &completed.id,
                completed.revision + 1,
                Control::Cleanup
            )
            .unwrap_err()
            .code,
        "DOWNLOAD_TASK_STALE"
    );
    assert_eq!(f.store.read_downloads().unwrap(), before);
    assert!(staged_command(&f, &completed).is_dir());
    materialize::verify_output(&completed).unwrap();
}

#[test]
fn batch_history_keeps_every_record_when_one_completed_command_still_has_staging() {
    let f = zip_fixture();
    let first = complete_for_presence(&f, record(&f));
    let mut next = first.metadata.clone();
    next.work_id = "1002".into();
    let plan = f
        .service
        .prepare(&f.store, &first.root.id, first.generation, next)
        .unwrap();
    f.service
        .confirm(&f.store, &plan.plan_id, plan.revision)
        .unwrap();
    let pending = f
        .store
        .read_downloads()
        .unwrap()
        .value
        .tasks
        .into_iter()
        .find(|task| task.id == plan.plan_id)
        .unwrap();
    let second = complete_before_cleanup(&f, pending);
    let before = f.store.read_downloads().unwrap();
    assert_eq!(
        f.service
            .remove_history(&f.store, &[selection(&first), selection(&second)])
            .unwrap_err()
            .code,
        "DOWNLOAD_HISTORY_CLEANUP_REQUIRED"
    );
    assert_eq!(f.store.read_downloads().unwrap(), before);
    assert!(before.value.history_evidence.is_empty());
    materialize::verify_output(&first).unwrap();
    materialize::verify_output(&second).unwrap();
}

#[cfg(unix)]
#[test]
fn redirected_completed_staging_blocks_history_forgetting_and_cleanup_without_following_links() {
    let f = zip_fixture();
    let completed = complete_before_cleanup(&f, record(&f));
    let command = staged_command(&f, &completed);
    let saved = command.with_extension("saved-synthetic");
    fs::rename(&command, &saved).unwrap();
    std::os::unix::fs::symlink(&saved, &command).unwrap();
    let before = f.store.read_downloads().unwrap();
    let service = DownloadService::new();
    assert_eq!(
        service
            .remove_history(&f.store, &[selection(&completed)])
            .unwrap_err()
            .code,
        "DOWNLOAD_HISTORY_CLEANUP_UNCONFIRMED"
    );
    assert!(service
        .control(
            &f.store,
            &completed.id,
            completed.revision,
            Control::Cleanup
        )
        .is_err());
    assert_eq!(f.store.read_downloads().unwrap(), before);
    assert!(saved.join("chapters").is_dir());
    materialize::verify_output(&completed).unwrap();
}

#[test]
fn v1_download_migration_preserves_revision_and_bytes_until_a_write() {
    let f = zip_fixture();
    let path = downloads_path(&f);
    let mut envelope: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    envelope["value"]["version"] = 1.into();
    let bytes = serde_json::to_vec(&envelope).unwrap();
    fs::write(&path, &bytes).unwrap();
    let migrated = f.store.read_downloads().unwrap();
    assert_eq!(migrated.value.version, 2);
    assert_eq!(migrated.revision, envelope["revision"].as_u64().unwrap());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let saved = f
        .store
        .write_downloads(migrated.revision, migrated.value)
        .unwrap();
    assert_eq!(saved.value.version, 2);
    assert_eq!(saved.revision, migrated.revision + 1);
    let mut invalid = serde_json::to_value(&saved).unwrap();
    invalid["value"]["version"] = 1.into();
    invalid["value"]["tasks"][0]["phase"] = "abandoned".into();
    let invalid_bytes = serde_json::to_vec(&invalid).unwrap();
    fs::write(&path, &invalid_bytes).unwrap();
    assert!(f.store.read_downloads().is_err());
    assert_eq!(fs::read(&path).unwrap(), invalid_bytes);
}

#[tokio::test]
async fn index_failures_preserve_only_controlled_reasons_across_reopen() {
    for (code, expected) in [
        ("LIBRARY_BUSY", "DOWNLOAD_INDEX_LIBRARY_BUSY"),
        (
            "LIBRARY_LIMIT_REACHED",
            "DOWNLOAD_INDEX_LIBRARY_LIMIT_REACHED",
        ),
        (
            "LIBRARY_IDENTITY_CONFLICT",
            "DOWNLOAD_INDEX_LIBRARY_IDENTITY_CONFLICT",
        ),
        (
            "DOWNLOAD_OUTPUT_CHANGED",
            "DOWNLOAD_INDEX_DOWNLOAD_OUTPUT_CHANGED",
        ),
        ("PRIVATE_PATH_OR_TOKEN", "DOWNLOAD_INDEX_FAILED"),
    ] {
        let f = zip_fixture();
        seed_report(&f);
        let receipt = f
            .service
            .run(&f.store, &f.id, || Ok(()))
            .await
            .unwrap()
            .unwrap();
        f.service.index_failed(&f.store, &receipt, code).unwrap();
        let reopened = DownloadService::new().read(&f.store).unwrap();
        assert_eq!(reopened.tasks[0].error_code.as_deref(), Some(expected));
        assert_eq!(reopened.tasks[0].phase, DownloadPhase::Error);
        assert!(f.library.join(&receipt.relative_path).is_file());
    }
}

#[test]
fn worker_recovery_replaces_poisoned_transient_admissions_and_requires_explicit_retry() {
    let f = zip_fixture();
    let before = record(&f);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = f.service.runtime.lock().unwrap();
        panic!("synthetic driver unwind");
    }))
    .is_err());
    f.service.worker_interrupted(&f.store).unwrap();
    let stopped = record(&f);
    assert_eq!(stopped.phase, DownloadPhase::Error);
    assert_eq!(stopped.revision, before.revision + 1);
    assert_eq!(
        stopped.error_code.as_deref(),
        Some("DOWNLOAD_WORKER_INTERRUPTED")
    );
    assert!(f.service.lock().unwrap().queued.is_empty());
    assert!(f.service.lock().unwrap().active.is_none());
    assert_eq!(
        f.service
            .control(&f.store, &f.id, before.revision, Control::Retry)
            .unwrap_err()
            .code,
        "DOWNLOAD_TASK_STALE"
    );
    f.service
        .control(&f.store, &f.id, stopped.revision, Control::Retry)
        .unwrap();
    assert_eq!(record(&f).phase, DownloadPhase::Queued);
}

#[tokio::test]
async fn four_receipt_boundaries_keep_full_zip_hashes_but_reuse_identical_entry_proof() {
    let f = zip_fixture();
    seed_report(&f);
    materialize::reset_verify_metrics();
    let receipt = f
        .service
        .run(&f.store, &f.id, || Ok(()))
        .await
        .unwrap()
        .unwrap();
    f.service.validate_receipt(&f.store, &receipt).unwrap();
    let indexed = workbench_library::LibraryService::new()
        .register_completed(
            &f.store,
            &receipt.root_id,
            receipt.generation,
            &receipt.relative_path,
            &workbench_storage::LibraryReference {
                source: Source::Jm,
                work_id: receipt.work_id.clone(),
            },
            receipt.expected_pages,
        )
        .unwrap();
    let entry = indexed
        .items
        .iter()
        .find(|item| item.relative_path == receipt.relative_path)
        .unwrap();
    f.service
        .mark_indexed(&f.store, &receipt, &entry.id)
        .unwrap();
    let reused = materialize::verify_metrics();
    assert_eq!(reused.whole_passes, 4);
    assert_eq!(reused.entry_passes, 1);
    let completed = record(&f);
    assert_eq!(
        reused.whole_bytes,
        completed.archive_file.as_ref().unwrap().size_bytes * 4
    );
    materialize::reset_verify_metrics();
    for _ in 0..4 {
        materialize::forget_verified_layout();
        materialize::verify_output(&completed).unwrap();
    }
    let cold = materialize::verify_metrics();
    assert_eq!(cold.whole_passes, 4);
    assert_eq!(cold.entry_passes, 4);
    assert_eq!(cold.entry_bytes, reused.entry_bytes * 4);
    eprintln!("synthetic ZIP receipt reads: reused={reused:?}; cold={cold:?}");
    let mut changed_proof = completed.clone();
    changed_proof.output_manifest_hash = Some("f".repeat(64));
    assert!(materialize::verify_output(&changed_proof).is_err());
    let output = f.library.join(&completed.destination);
    let original = fs::read(&output).unwrap();
    let mut changed = original.clone();
    changed[0] ^= 1;
    fs::write(&output, &changed).unwrap();
    assert_eq!(
        materialize::verify_output(&completed).unwrap_err().code,
        "DOWNLOAD_OUTPUT_CHANGED"
    );
    fs::write(&output, &original).unwrap();
    materialize::verify_output(&completed).unwrap();
    fs::rename(&output, output.with_extension("preserved")).unwrap();
    fs::write(&output, &original).unwrap();
    assert_eq!(
        materialize::verify_output(&completed).unwrap_err().code,
        "DOWNLOAD_OUTPUT_CHANGED"
    );
}

#[test]
fn cold_running_records_can_be_abandoned_but_live_saving_cannot() {
    for phase in [
        DownloadPhase::Downloading,
        DownloadPhase::Verifying,
        DownloadPhase::Saving,
    ] {
        let f = zip_fixture();
        let mut value = record(&f);
        value.phase = phase;
        put(&f, value);
        let reopened = DownloadService::new();
        let view = reopened.read(&f.store).unwrap();
        assert_eq!(view.tasks[0].phase, DownloadPhase::Paused);
        assert!(view.tasks[0].allowed_actions.contains(&Control::Abandon));
        reopened
            .control(&f.store, &f.id, 1, Control::Abandon)
            .unwrap();
        assert_eq!(record(&f).phase, DownloadPhase::Abandoned);
    }
    let f = zip_fixture();
    let mut value = record(&f);
    value.phase = DownloadPhase::Saving;
    put(&f, value);
    set_active(&f);
    assert_eq!(
        f.service
            .control(&f.store, &f.id, 1, Control::Abandon)
            .unwrap_err()
            .code,
        "DOWNLOAD_CONTROL_INVALID"
    );
}

#[test]
fn unrecorded_empty_staging_directory_blocks_cleanup_before_known_files_are_removed() {
    let f = zip_fixture();
    let mut value = record(&f);
    let (stage, report) = report(&f, &value);
    let command = stage.join(&report.staging_subdir);
    let unknown = command.join("chapters/unrecorded");
    fs::create_dir(&unknown).unwrap();
    let first = command.join(&report.source_completion.manifest.artifacts[0].relative_path);
    let bytes = fs::read(&first).unwrap();
    value.phase = DownloadPhase::Abandoned;
    value.staging_report_json = Some(serde_json::to_string(&report).unwrap());
    assert_eq!(
        materialize::cleanup_abandoned(&value, &stage)
            .unwrap_err()
            .code,
        "DOWNLOAD_CLEANUP_REVIEW_REQUIRED"
    );
    assert!(unknown.is_dir());
    assert_eq!(fs::read(&first).unwrap(), bytes);
}

#[test]
fn explicitly_abandoned_partial_pending_file_is_cleaned_only_at_its_recorded_staging_path() {
    let f = zip_fixture();
    let mut value = record(&f);
    let (stage, report) = report(&f, &value);
    let command = stage.join(&report.staging_subdir);
    let artifacts = report.source_completion.manifest.artifacts.clone();
    let second = command.join(&artifacts[1].relative_path);
    let bytes = fs::read(&second).unwrap();
    fs::write(&second, &bytes[..bytes.len() / 2]).unwrap();
    value.phase = DownloadPhase::Abandoned;
    value.checkpoint_json = Some(
        serde_json::to_string(&StagingCheckpoint {
            descriptor_hash: "a".repeat(64),
            expected_files: 2,
            artifacts: vec![artifacts[0].clone()],
            pending: Some(artifacts[1].clone()),
        })
        .unwrap(),
    );
    materialize::cleanup_abandoned(&value, &stage).unwrap();
    assert!(!command.exists());
    assert!(fs::read_dir(&f.library).unwrap().next().is_none());
}

#[cfg(unix)]
#[test]
fn abandoned_cleanup_rejects_link_escape_without_touching_the_link_target() {
    let f = zip_fixture();
    let mut value = record(&f);
    let (stage, report) = report(&f, &value);
    let command = stage.join(&report.staging_subdir);
    let first = command.join(&report.source_completion.manifest.artifacts[0].relative_path);
    let outside = f._temp.path().join("outside.gif");
    fs::rename(&first, &outside).unwrap();
    let bytes = fs::read(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, &first).unwrap();
    value.phase = DownloadPhase::Abandoned;
    value.staging_report_json = Some(serde_json::to_string(&report).unwrap());
    assert!(materialize::cleanup_abandoned(&value, &stage).is_err());
    assert_eq!(fs::read(&outside).unwrap(), bytes);
    assert!(fs::symlink_metadata(&first)
        .unwrap()
        .file_type()
        .is_symlink());
}
