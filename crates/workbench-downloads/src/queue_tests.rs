//! Batch admission and history regressions use only synthetic metadata/media.
use super::*;

fn prepare_id(f: &Fixture, id: &str) -> DownloadPlan {
    let library = f.store.read_library().unwrap();
    f.service
        .prepare(
            &f.store,
            &library.value.root.as_ref().unwrap().id,
            library.value.generation,
            JmDownloadMetadata {
                work_id: id.into(),
                title: format!("Synthetic queue work {id}"),
                authors: Vec::new(),
                tags: Vec::new(),
                description: None,
            },
        )
        .unwrap()
}
fn selected(plan: &DownloadPlan) -> PreparedSelection {
    PreparedSelection {
        plan_id: plan.plan_id.clone(),
        expected_revision: plan.revision,
    }
}
fn task_selected(task: &DownloadRecord) -> TaskSelection {
    TaskSelection {
        task_id: task.id.clone(),
        expected_revision: task.revision,
    }
}

#[test]
fn prepared_batch_rebases_unrelated_active_progress_and_queues_without_a_second_worker() {
    let f = fixture();
    let first = prepare_id(&f, "1001");
    let second = prepare_id(&f, "1002");
    set_active(&f);
    // A per-image checkpoint changes the document revision, not the approval
    // represented by either reviewed plan.
    f.service
        .update_run(&f.store, &f.id, 1, |task| {
            task.phase = DownloadPhase::Downloading;
            task.files_done = 1;
            task.files_total = Some(2);
            task.bytes_done = 100;
            Ok(())
        })
        .unwrap();
    let before = f.store.read_downloads().unwrap();
    assert!(before.revision > first.revision);
    let view = f
        .service
        .confirm_many(&f.store, &[selected(&first), selected(&second)])
        .unwrap();
    assert_eq!(view.revision, before.revision + 1, "one batch queue write");
    assert_eq!(view.tasks.len(), 3);
    assert_eq!(view.tasks[0].phase, DownloadPhase::Downloading);
    let reread = f.service.read_with_file_check(&f.store, false).unwrap();
    assert_eq!(reread.tasks[1].phase, DownloadPhase::Queued);
    assert_eq!(reread.tasks[2].phase, DownloadPhase::Queued);
    assert_eq!(
        f.service.lock().unwrap().active.as_ref().unwrap().task_id,
        f.id
    );
    assert_eq!(
        f.store.read_downloads().unwrap().value.tasks[0],
        before.value.tasks[0]
    );
    let untouched = f.store.read_downloads().unwrap();
    assert!(f
        .service
        .confirm_many(&f.store, &[selected(&first)])
        .is_err());
    assert_eq!(f.store.read_downloads().unwrap(), untouched);
    assert!(fs::read_dir(&f.library).unwrap().next().is_none());
}

#[test]
fn invalid_batch_member_or_new_duplicate_never_partially_confirms_the_other_plans() {
    let f = fixture();
    let first = prepare_id(&f, "1001");
    let duplicate = prepare_id(&f, "1001");
    let second = prepare_id(&f, "1002");
    let before = f.store.read_downloads().unwrap();
    assert_eq!(
        f.service
            .confirm_many(&f.store, &[selected(&first), selected(&duplicate)])
            .unwrap_err()
            .code,
        "DOWNLOAD_ALREADY_PRESENT"
    );
    assert_eq!(f.store.read_downloads().unwrap(), before);
    let mut stale = selected(&second);
    stale.expected_revision += 1;
    assert_eq!(
        f.service
            .confirm_many(&f.store, &[selected(&first), stale])
            .unwrap_err()
            .code,
        "DOWNLOAD_PLAN_STALE"
    );
    assert_eq!(f.store.read_downloads().unwrap(), before);
    f.service
        .confirm(&f.store, &duplicate.plan_id, duplicate.revision)
        .unwrap();
    let after_other_confirmation = f.store.read_downloads().unwrap();
    assert_eq!(
        f.service
            .confirm_many(&f.store, &[selected(&first), selected(&second)])
            .unwrap_err()
            .code,
        "DOWNLOAD_ALREADY_PRESENT"
    );
    assert_eq!(f.store.read_downloads().unwrap(), after_other_confirmation);
    assert!(fs::read_dir(&f.library).unwrap().next().is_none());
}

#[test]
fn all_fifty_plans_survive_prepare_and_the_queue_is_not_limited_to_fifty_lifetime_downloads() {
    let f = fixture();
    let plans: Vec<_> = (2000..2050)
        .map(|id| prepare_id(&f, &id.to_string()))
        .collect();
    let selections: Vec<_> = plans.iter().map(selected).collect();
    let view = f.service.confirm_many(&f.store, &selections).unwrap();
    assert_eq!(view.tasks.len(), 51);
    assert!(view
        .tasks
        .iter()
        .all(|task| task.phase == DownloadPhase::Queued));
    assert_eq!(f.service.lock().unwrap().queued.len(), 51);
    let before = f.store.read_downloads().unwrap();
    let too_many = vec![selected(&plans[0]); 51];
    assert_eq!(
        f.service
            .confirm_many(&f.store, &too_many)
            .unwrap_err()
            .code,
        "DOWNLOAD_BATCH_LIMIT"
    );
    assert_eq!(f.store.read_downloads().unwrap(), before);
}

#[tokio::test]
async fn restart_requires_exact_source_bound_explicit_resume_and_old_scheduler_epochs_fail() {
    let f = fixture();
    let second = prepare_id(&f, "1002");
    f.service
        .confirm(&f.store, &second.plan_id, second.revision)
        .unwrap();
    let document = f.store.read_downloads().unwrap();
    let selections: Vec<_> = document.value.tasks.iter().map(task_selected).collect();
    let restored = DownloadService::new();
    assert!(restored
        .read(&f.store)
        .unwrap()
        .tasks
        .iter()
        .all(|t| t.phase == DownloadPhase::Paused));
    assert_eq!(
        restored
            .run(&f.store, &f.id, || Ok(()))
            .await
            .unwrap_err()
            .code,
        "DOWNLOAD_RESUME_REQUIRED"
    );
    assert_eq!(
        restored
            .resume_many(&f.store, Source::Pica, &selections)
            .unwrap_err()
            .code,
        "DOWNLOAD_SOURCE_MISMATCH"
    );
    assert_eq!(f.store.read_downloads().unwrap(), document);
    let resumed = restored
        .resume_many(&f.store, Source::Jm, &selections)
        .unwrap();
    assert!(resumed
        .tasks
        .iter()
        .all(|t| t.phase == DownloadPhase::Queued && t.revision == 2));
    assert_eq!(
        restored
            .run_selected_with_token(&f.store, &f.id, 1, None, || Ok(()))
            .await
            .unwrap_err()
            .code,
        "DOWNLOAD_TASK_STALE"
    );
    let after = f.store.read_downloads().unwrap();
    restored
        .fail_queued(&f.store, &f.id, 1, "SESSION_CHANGED")
        .unwrap();
    assert_eq!(
        f.store.read_downloads().unwrap(),
        after,
        "an old driver cannot fail a new epoch"
    );
    restored
        .fail_queued(&f.store, &f.id, 2, "SESSION_CHANGED")
        .unwrap();
    let failed = restored.read(&f.store).unwrap();
    assert_eq!(failed.tasks[0].phase, DownloadPhase::Error);
    assert_eq!(
        failed.tasks[0].error_code.as_deref(),
        Some("SESSION_CHANGED")
    );
    assert_eq!(
        failed.tasks[1].phase,
        DownloadPhase::Queued,
        "one failure leaves later work admitted"
    );
    assert!(fs::read_dir(&f.library).unwrap().next().is_none());
}

#[test]
fn confirming_a_new_work_never_projects_older_unadmitted_restart_tasks_as_queued() {
    let f = fixture();
    let restored = DownloadService::new();
    let task = record(&f);
    let plan = restored
        .prepare(
            &f.store,
            &task.root.id,
            task.generation,
            JmDownloadMetadata {
                work_id: "1002".into(),
                title: "New explicit confirmation".into(),
                authors: vec![],
                tags: vec![],
                description: None,
            },
        )
        .unwrap();
    let view = restored
        .confirm(&f.store, &plan.plan_id, plan.revision)
        .unwrap();
    assert_eq!(view.tasks[0].phase, DownloadPhase::Paused);
    assert_eq!(view.tasks[1].phase, DownloadPhase::Queued);
}

#[test]
fn already_admitted_waiting_work_cannot_be_resumed_again_to_change_its_queue_epoch() {
    let f = fixture();
    let before = f.store.read_downloads().unwrap();
    assert_eq!(
        f.service
            .control(&f.store, &f.id, 1, Control::Resume)
            .unwrap_err()
            .code,
        "DOWNLOAD_CONTROL_INVALID"
    );
    assert_eq!(
        f.service
            .resume_many(&f.store, Source::Jm, &[task_selected(&record(&f))])
            .unwrap_err()
            .code,
        "DOWNLOAD_CONTROL_INVALID"
    );
    assert_eq!(f.store.read_downloads().unwrap(), before);
}

#[test]
fn pause_all_invalidates_active_and_waiting_work_and_resume_selection_is_atomic() {
    let f = fixture();
    let second = prepare_id(&f, "1002");
    f.service
        .confirm(&f.store, &second.plan_id, second.revision)
        .unwrap();
    set_active(&f);
    let paused = f.service.pause_all(&f.store).unwrap();
    assert!(paused
        .tasks
        .iter()
        .all(|t| t.phase == DownloadPhase::Paused && t.revision == 2));
    assert!(f.service.lock().unwrap().queued.is_empty());
    assert_eq!(
        f.service.require_run(&f.store, &f.id, 1).unwrap_err().code,
        "DOWNLOAD_PAUSED"
    );
    let before = f.store.read_downloads().unwrap();
    let selections: Vec<_> = before.value.tasks.iter().map(task_selected).collect();
    assert_eq!(
        f.service
            .resume_many(&f.store, Source::Jm, &selections)
            .unwrap_err()
            .code,
        "DOWNLOAD_WORKER_BUSY"
    );
    assert_eq!(f.store.read_downloads().unwrap(), before);
    // Waiting work can be explicitly queued while the old active task unwinds.
    f.service
        .resume_many(&f.store, Source::Jm, &selections[1..])
        .unwrap();
    assert_eq!(
        f.service.read(&f.store).unwrap().tasks[1].phase,
        DownloadPhase::Queued
    );
    f.service.clear(&f.id, 1);
    f.service
        .resume_many(&f.store, Source::Jm, &selections[..1])
        .unwrap();
    assert_eq!(
        f.service.read(&f.store).unwrap().tasks[0].phase,
        DownloadPhase::Queued
    );
}

#[tokio::test]
async fn pause_all_leaves_current_final_save_valid_but_stops_the_next_work() {
    let f = fixture();
    seed_report(&f);
    let second = prepare_id(&f, "1002");
    f.service
        .confirm(&f.store, &second.plan_id, second.revision)
        .unwrap();
    let receipt = f
        .service
        .run(&f.store, &f.id, || Ok(()))
        .await
        .unwrap()
        .unwrap();
    let paused = f.service.pause_all(&f.store).unwrap();
    assert_eq!(paused.tasks[0].phase, DownloadPhase::Saving);
    assert_eq!(paused.tasks[0].revision, receipt.task_revision);
    assert_eq!(paused.tasks[1].phase, DownloadPhase::Paused);
    assert!(f.service.validate_receipt(&f.store, &receipt).is_ok());
    assert!(f.service.lock().unwrap().queued.is_empty());
    f.service.clear(&receipt.task_id, receipt.task_revision);
}

#[test]
fn clearing_completed_history_preserves_files_library_and_compact_duplicate_evidence() {
    let f = fixture();
    let completed = complete_for_presence(&f, record(&f));
    let page = f
        .library
        .join(&completed.destination)
        .join("0001-123456/0001.gif");
    let bytes = fs::read(&page).unwrap();
    let library_before = f.store.read_library().unwrap();
    let phone_before = f.store.read_phone_library().unwrap();
    let view = f
        .service
        .remove_history(&f.store, &[task_selected(&completed)])
        .unwrap();
    assert!(view.tasks.is_empty());
    let saved = f.store.read_downloads().unwrap();
    assert_eq!(saved.value.history_evidence.len(), 1);
    assert_eq!(fs::read(&page).unwrap(), bytes);
    assert_eq!(f.store.read_library().unwrap(), library_before);
    assert_eq!(f.store.read_phone_library().unwrap(), phone_before);
    let mut metadata = completed.metadata.clone();
    metadata.title = "Changed remote title".into();
    assert_eq!(
        f.service
            .prepare(&f.store, &completed.root.id, completed.generation, metadata)
            .unwrap_err()
            .code,
        "DOWNLOAD_ALREADY_PRESENT"
    );
    assert_eq!(f.store.read_downloads().unwrap(), saved);
    let serialized = serde_json::to_string(&saved.value.history_evidence).unwrap();
    assert!(!serialized.contains("checkpoint"));
    assert!(!serialized.contains("outputFiles"));
}

#[test]
fn history_clear_retains_manual_unlink_and_reassociation_authority_after_original_files_disappear()
{
    for replacement in [None, Some("999999")] {
        let f = fixture();
        let completed = complete_for_presence(&f, record(&f));
        workbench_library::LibraryService::new()
            .link(
                &f.store,
                &completed.root.id,
                completed.generation,
                completed.library_entry_id.as_deref().unwrap(),
                replacement.map(|id| workbench_storage::LibraryReference {
                    source: Source::Jm,
                    work_id: id.into(),
                }),
            )
            .unwrap();
        f.service
            .remove_history(&f.store, &[task_selected(&completed)])
            .unwrap();
        fs::remove_dir_all(f.library.join(&completed.destination)).unwrap();
        let library_before = f.store.read_library().unwrap();
        let mut metadata = completed.metadata.clone();
        metadata.title = "A different new destination".into();
        assert_eq!(
            f.service
                .prepare(&f.store, &completed.root.id, completed.generation, metadata)
                .unwrap_err()
                .code,
            "LIBRARY_IDENTITY_CONFLICT"
        );
        assert_eq!(f.store.read_library().unwrap(), library_before);
    }
}

#[test]
fn history_clear_is_revision_bound_and_never_removes_unfinished_tasks_in_a_partial_selection() {
    let f = fixture();
    let completed = complete_for_presence(&f, record(&f));
    let next = prepare_id(&f, "1002");
    f.service
        .confirm(&f.store, &next.plan_id, next.revision)
        .unwrap();
    let before = f.store.read_downloads().unwrap();
    let selections: Vec<_> = before.value.tasks.iter().map(task_selected).collect();
    assert_eq!(
        f.service
            .remove_history(&f.store, &selections)
            .unwrap_err()
            .code,
        "DOWNLOAD_HISTORY_NOT_COMPLETED"
    );
    assert_eq!(f.store.read_downloads().unwrap(), before);
    let mut stale = task_selected(&completed);
    stale.expected_revision += 1;
    assert_eq!(
        f.service
            .remove_history(&f.store, &[stale])
            .unwrap_err()
            .code,
        "DOWNLOAD_TASK_STALE"
    );
    assert_eq!(f.store.read_downloads().unwrap(), before);
}

#[test]
fn legacy_empty_history_serialization_stays_unchanged_and_archive_paths_remain_validated() {
    let f = fixture();
    let original = f.store.read_downloads().unwrap();
    let serialized = serde_json::to_value(&original.value).unwrap();
    assert!(serialized.get("historyEvidence").is_none());
    let completed = complete_for_presence(&f, record(&f));
    f.service
        .remove_history(&f.store, &[task_selected(&completed)])
        .unwrap();
    let archived = f.store.read_downloads().unwrap();
    let mut bad = archived.value.clone();
    bad.history_evidence[0].destination = "../outside".into();
    assert!(f.store.write_downloads(archived.revision, bad).is_err());
    assert_eq!(f.store.read_downloads().unwrap(), archived);
}
