//! Offline regression for a completed media download interrupted by a PC rescan.
use super::*;
use workbench_library::{LibraryService, ScanAction};

pub(super) fn finish_rescan(f: &Fixture) -> u64 {
    let mut indexer = LibraryService::new();
    let library = f.store.read_library().unwrap();
    let root = library.value.root.unwrap();
    let mut scan = indexer
        .scan(
            &f.store,
            &root.id,
            library.value.generation,
            ScanAction::Start,
        )
        .unwrap();
    for _ in 0..10 {
        if scan.phase == LibraryPhase::Complete {
            return scan.generation;
        }
        scan = indexer
            .scan(&f.store, &root.id, scan.generation, ScanAction::Next)
            .unwrap();
    }
    panic!("synthetic rescan did not complete");
}

pub(super) fn mark_retryable(f: &Fixture) -> DownloadRecord {
    let mut document = f.store.read_downloads().unwrap();
    let task = document
        .value
        .tasks
        .iter_mut()
        .find(|t| t.id == f.id)
        .unwrap();
    task.phase = DownloadPhase::Error;
    task.error_code = Some("LIBRARY_BUSY".into());
    let task = task.clone();
    f.store
        .write_downloads(document.revision, document.value)
        .unwrap();
    task
}

pub(super) fn register(f: &Fixture, receipt: &AwaitingIndexReceipt) {
    let indexed = LibraryService::new()
        .register_completed(
            &f.store,
            &receipt.root_id,
            receipt.generation,
            &receipt.relative_path,
            &workbench_storage::LibraryReference {
                source: receipt.source,
                work_id: receipt.work_id.clone(),
            },
            receipt.expected_pages,
        )
        .unwrap();
    let entry = indexed
        .items
        .iter()
        .find(|v| v.relative_path == receipt.relative_path)
        .unwrap();
    f.service
        .mark_indexed(&f.store, receipt, &entry.id)
        .unwrap();
}

#[tokio::test]
async fn explicit_retry_after_same_root_rescan_reuses_verified_media_and_registers() {
    for zip in [false, true] {
        let f = if zip { zip_fixture() } else { fixture() };
        seed_report(&f);
        let original = mark_retryable(&f);
        let generation = finish_rescan(&f);
        assert!(generation > original.generation);
        let before = f.store.read_downloads().unwrap();
        assert_eq!(
            DownloadService::new().read(&f.store).unwrap().tasks[0].phase,
            DownloadPhase::Error
        );
        assert_eq!(before, f.store.read_downloads().unwrap());
        assert!(!f.library.join(&original.destination).exists());
        f.service
            .control(&f.store, &f.id, original.revision, Control::Retry)
            .unwrap();
        let admitted = record(&f);
        assert_eq!(admitted.generation, original.generation);
        assert_eq!(admitted.approval_revision, original.approval_revision);
        assert_eq!(admitted.target_hash, original.target_hash);
        assert_eq!(admitted.staging_report_json, original.staging_report_json);
        let receipt = f
            .service
            .run(&f.store, &f.id, || Ok(()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(receipt.generation, generation);
        assert_eq!(receipt.expected_pages, 2);
        let mut stale = receipt.clone();
        stale.generation = original.generation;
        assert_eq!(
            f.service
                .validate_receipt(&f.store, &stale)
                .unwrap_err()
                .code,
            "DOWNLOAD_TASK_STALE"
        );
        register(&f, &receipt);
        assert_eq!(record(&f).phase, DownloadPhase::Downloaded);
        assert_eq!(record(&f).target_hash, original.target_hash);
        materialize::verify_output(&record(&f)).unwrap();
        assert_eq!(
            f.service.read(&f.store).unwrap().tasks[0].local_files,
            Some(LocalFiles::Present)
        );
    }
}

#[tokio::test]
async fn rescan_between_save_and_index_reuses_exact_existing_zip_on_retry() {
    let f = zip_fixture();
    seed_report(&f);
    let original = record(&f);
    let first = f
        .service
        .run(&f.store, &f.id, || Ok(()))
        .await
        .unwrap()
        .unwrap();
    let zip_before = fs::read(f.library.join(&original.destination)).unwrap();
    let generation = finish_rescan(&f);
    assert_eq!(
        f.service
            .validate_receipt(&f.store, &first)
            .unwrap_err()
            .code,
        "DOWNLOAD_ROOT_CHANGED"
    );
    f.service
        .index_failed(&f.store, &first, "DOWNLOAD_ROOT_CHANGED")
        .unwrap();
    let failed = record(&f);
    assert_eq!(failed.error_code.as_deref(), Some("DOWNLOAD_INDEX_FAILED"));
    f.service
        .control(&f.store, &f.id, failed.revision, Control::Retry)
        .unwrap();
    let next = f
        .service
        .run(&f.store, &f.id, || Ok(()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next.generation, generation);
    register(&f, &next);
    assert_eq!(
        fs::read(f.library.join(&original.destination)).unwrap(),
        zip_before
    );
    assert_eq!(record(&f).phase, DownloadPhase::Downloaded);
}

#[tokio::test]
async fn retry_admission_is_not_reused_after_another_scan_or_process_restart() {
    let f = zip_fixture();
    seed_report(&f);
    let original = mark_retryable(&f);
    finish_rescan(&f);
    f.service
        .control(&f.store, &f.id, original.revision, Control::Retry)
        .unwrap();
    let before = f.store.read_downloads().unwrap();
    let reopened = DownloadService::new();
    assert_eq!(
        reopened.read(&f.store).unwrap().tasks[0].phase,
        DownloadPhase::Paused
    );
    assert_eq!(
        reopened
            .run(&f.store, &f.id, || Ok(()))
            .await
            .unwrap_err()
            .code,
        "DOWNLOAD_RESUME_REQUIRED"
    );
    assert_eq!(before, f.store.read_downloads().unwrap());
    finish_rescan(&f);
    assert_eq!(
        f.service
            .run(&f.store, &f.id, || Ok(()))
            .await
            .unwrap_err()
            .code,
        "DOWNLOAD_ROOT_CHANGED"
    );
    assert_eq!(before, f.store.read_downloads().unwrap());
    assert!(!f.library.join(&original.destination).exists());
    f.service.pause_all(&f.store).unwrap();
    let paused = record(&f);
    f.service
        .resume_many(
            &f.store,
            Source::Jm,
            &[TaskSelection {
                task_id: f.id.clone(),
                expected_revision: paused.revision,
            }],
        )
        .unwrap();
    let receipt = f
        .service
        .run(&f.store, &f.id, || Ok(()))
        .await
        .unwrap()
        .unwrap();
    register(&f, &receipt);
}

#[test]
fn retry_rejects_reading_paused_incomplete_and_different_directory_without_writes() {
    for phase in [
        LibraryPhase::Reading,
        LibraryPhase::Paused,
        LibraryPhase::Error,
    ] {
        let f = fixture();
        let original = mark_retryable(&f);
        let mut library = f.store.read_library().unwrap();
        library.value.generation += 1;
        library.value.phase = phase;
        f.store
            .write_library(library.revision, library.value)
            .unwrap();
        let before = f.store.read_downloads().unwrap();
        assert!(f
            .service
            .control(&f.store, &f.id, original.revision, Control::Retry)
            .is_err());
        assert_eq!(before, f.store.read_downloads().unwrap());
    }
    let f = fixture();
    let original = mark_retryable(&f);
    let other = f._temp.path().join("different-library");
    fs::create_dir(&other).unwrap();
    LibraryService::new().choose(&f.store, &other).unwrap();
    finish_rescan(&f);
    let before = f.store.read_downloads().unwrap();
    assert_eq!(
        f.service
            .control(&f.store, &f.id, original.revision, Control::Retry)
            .unwrap_err()
            .code,
        "DOWNLOAD_ROOT_CHANGED"
    );
    assert_eq!(before, f.store.read_downloads().unwrap());
}
