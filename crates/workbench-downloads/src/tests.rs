use super::*;
use crate::{fs::Directory, materialize};
use cloud_monitor::{
    executor_handoff, filesystem_verifier,
    image_download_authorization::ImageDownloadAuthorization,
    isolated_staging_execution::{
        self, IsolatedStagingExecutionContext, IsolatedStagingExecutionResult, ProcessedMedia,
    },
    local_executor::{self, LocalExecutionPlan},
    monitor, source_bridge_request,
    source_completion::{SourceCompletionProof, JM_UPSTREAM_COMMIT},
    source_media_descriptors::{
        MediaChapterDescriptors, MediaDescriptor, SourceMediaDescriptorSet,
    },
    source_preflight::{self, PreflightChapter, SourcePreflightEvidence, SourcePreflightProof},
    staging_manifest::{StagedArtifact, StagingManifest},
    verified_execution_receipt,
};
use std::{
    cell::{Cell, RefCell},
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

#[path = "pica_tests.rs"]
mod pica_tests;

struct Fixture {
    _temp: TempDir,
    store: WorkbenchStore,
    service: DownloadService,
    library: PathBuf,
    id: String,
}
fn fixture() -> Fixture {
    let temp = TempDir::new().unwrap();
    let library = temp.path().join("library");
    fs::create_dir(&library).unwrap();
    let store = WorkbenchStore::open(temp.path().join("private")).unwrap();
    let mut indexer = workbench_library::LibraryService::new();
    indexer.choose(&store, &library).unwrap();
    let mut lib = store.read_library().unwrap();
    lib.value.phase = LibraryPhase::Complete;
    let lib = store.write_library(lib.revision, lib.value).unwrap();
    let root = lib.value.root.unwrap();
    let service = DownloadService::new();
    let plan = service
        .prepare(
            &store,
            &root.id,
            lib.value.generation,
            JmDownloadMetadata {
                work_id: "123456".into(),
                title: "Offline example".into(),
                authors: vec!["Example author".into()],
                tags: vec!["test".into()],
                description: None,
            },
        )
        .unwrap();
    service
        .confirm(&store, &plan.plan_id, plan.revision)
        .unwrap();
    Fixture {
        _temp: temp,
        store,
        service,
        library,
        id: plan.plan_id,
    }
}
fn record(f: &Fixture) -> DownloadRecord {
    f.store
        .read_downloads()
        .unwrap()
        .value
        .tasks
        .into_iter()
        .next()
        .unwrap()
}
fn put(f: &Fixture, record: DownloadRecord) {
    let mut doc = f.store.read_downloads().unwrap();
    doc.value.tasks[0] = record;
    f.store.write_downloads(doc.revision, doc.value).unwrap();
}

// Materialize and index only synthetic, already verified staging. No source
// request or real-account state is involved in these presence regressions.
fn complete_for_presence(f: &Fixture, mut value: DownloadRecord) -> DownloadRecord {
    let (stage, report) = report(f, &value);
    value.files_done = 2;
    value.files_total = Some(2);
    value.bytes_done = report.filesystem_verification.total_bytes;
    value.staging_report_json = Some(serde_json::to_string(&report).unwrap());
    materialize::save(&mut value, &report, &stage, &|| Ok(()), &mut |_| Ok(())).unwrap();
    let indexed = workbench_library::LibraryService::new()
        .register_completed(
            &f.store,
            &value.root.id,
            value.generation,
            &value.destination,
            &workbench_storage::LibraryReference {
                source: Source::Jm,
                work_id: value.metadata.work_id.clone(),
            },
            2,
        )
        .unwrap();
    value.library_entry_id = Some(
        indexed
            .items
            .iter()
            .find(|item| item.relative_path == value.destination)
            .unwrap()
            .id
            .clone(),
    );
    value.phase = DownloadPhase::Downloaded;
    value.revision += 1;
    let mut document = f.store.read_downloads().unwrap();
    *document
        .value
        .tasks
        .iter_mut()
        .find(|task| task.id == value.id)
        .unwrap() = value.clone();
    f.store
        .write_downloads(document.revision, document.value)
        .unwrap();
    value
}

fn prepare_same(f: &Fixture) -> Result<DownloadPlan> {
    let library = f.store.read_library().unwrap();
    f.service.prepare(
        &f.store,
        &library.value.root.as_ref().unwrap().id,
        library.value.generation,
        record(f).metadata,
    )
}

#[test]
fn file_status_is_advisory_read_only_and_polling_reuses_only_the_same_revision() {
    let f = fixture();
    assert_eq!(f.service.read(&f.store).unwrap().tasks[0].local_files, None);
    let mut completed = complete_for_presence(&f, record(&f));
    let view = f.service.read(&f.store).unwrap();
    assert_eq!(view.tasks[0].local_files, Some(LocalFiles::Present));
    assert_eq!(
        serde_json::to_value(&view).unwrap()["tasks"][0]["localFiles"],
        "present"
    );
    let page = f
        .library
        .join(&completed.destination)
        .join("0001-123456/0001.gif");
    fs::remove_file(&page).unwrap();
    let before = fs::read(downloads_path(&f)).unwrap();
    let index_before = f.store.read_library().unwrap();
    assert_eq!(
        f.service
            .read_with_file_check(&f.store, false)
            .unwrap()
            .tasks[0]
            .local_files,
        Some(LocalFiles::Present),
        "ordinary polling does not stat media again"
    );
    assert_eq!(
        DownloadService::new()
            .read_with_file_check(&f.store, false)
            .unwrap()
            .tasks[0]
            .local_files,
        Some(LocalFiles::Incomplete),
        "a new completed task is checked even without an explicit refresh"
    );
    assert_eq!(
        f.service.read(&f.store).unwrap().tasks[0].local_files,
        Some(LocalFiles::Incomplete)
    );
    assert_eq!(fs::read(downloads_path(&f)).unwrap(), before);
    assert_eq!(f.store.read_library().unwrap(), index_before);
    fs::write(&page, gif()).unwrap();
    completed.revision += 1;
    put(&f, completed.clone());
    assert_eq!(
        f.service
            .read_with_file_check(&f.store, false)
            .unwrap()
            .tasks[0]
            .local_files,
        Some(LocalFiles::Present),
        "a changed task revision invalidates its cached status"
    );
    fs::remove_dir_all(f.library.join(&completed.destination)).unwrap();
    let before = fs::read(downloads_path(&f)).unwrap();
    let view = f.service.read(&f.store).unwrap();
    assert_eq!(view.tasks[0].phase, DownloadPhase::Downloaded);
    assert_eq!(view.tasks[0].local_files, Some(LocalFiles::Missing));
    assert!(view.tasks[0].allowed_actions.is_empty());
    assert_eq!(fs::read(downloads_path(&f)).unwrap(), before);
    assert_eq!(f.store.read_library().unwrap(), index_before);
}

#[tokio::test]
async fn deleted_completed_work_requires_a_new_confirmation_then_registers_without_old_identity() {
    let f = fixture();
    let old = complete_for_presence(&f, record(&f));
    fs::remove_dir_all(f.library.join(&old.destination)).unwrap();
    let index_before = f.store.read_library().unwrap();
    let downloads_before = f.store.read_downloads().unwrap();
    let phone_before = f.store.read_phone_library().unwrap();
    let plan = prepare_same(&f).unwrap();
    assert_ne!(plan.plan_id, old.id);
    assert_eq!(
        f.store.read_library().unwrap(),
        index_before,
        "prepare is read-only"
    );
    assert_eq!(f.store.read_downloads().unwrap(), downloads_before);
    assert!(!f.library.join(&old.destination).exists());
    assert_eq!(
        f.service
            .control(&f.store, &old.id, old.revision, Control::Retry)
            .unwrap_err()
            .code,
        "DOWNLOAD_CONTROL_INVALID",
        "a historical completion cannot reuse its old execution proof"
    );
    let queued = f
        .service
        .confirm(&f.store, &plan.plan_id, plan.revision)
        .unwrap();
    assert_eq!(queued.tasks.len(), 2);
    assert_eq!(queued.tasks[0].phase, DownloadPhase::Downloaded);
    assert_eq!(queued.tasks[1].local_files, None);
    assert!(f.store.read_library().unwrap().value.records.is_empty());
    let document = f.store.read_downloads().unwrap();
    assert_eq!(document.value.tasks[0], old);
    let fresh = document.value.tasks[1].clone();
    let (_, _, old_command) = adapter::current(&old).unwrap();
    let (_, _, fresh_command) = adapter::current(&fresh).unwrap();
    assert_ne!(fresh_command.task_id, old_command.task_id);
    assert_ne!(fresh_command.command_id, old_command.command_id);
    assert_ne!(
        local_executor::plan(&fresh_command).unwrap().staging_subdir,
        local_executor::plan(&old_command).unwrap().staging_subdir
    );
    let old_report: LocalExecutionReport =
        serde_json::from_str(old.staging_report_json.as_deref().unwrap()).unwrap();
    let staging = f
        .store
        .open_download_workspace()
        .unwrap()
        .path()
        .to_path_buf();
    assert_eq!(
        materialize::validate_staging(&fresh, &old_report, &staging)
            .unwrap_err()
            .code,
        "DOWNLOAD_PROOF_INVALID"
    );
    assert_eq!(fresh.phase, DownloadPhase::Queued);
    assert_eq!(fresh.output_identity, None);
    assert_eq!(fresh.checkpoint_json, None);
    assert_eq!(fresh.staging_report_json, None);
    assert!(fresh.output_files.is_empty());
    assert!(
        !f.library.join(&fresh.destination).exists(),
        "confirm never executes"
    );
    let mut seeded = fresh.clone();
    let (_, report) = report(&f, &seeded);
    seeded.files_done = 2;
    seeded.files_total = Some(2);
    seeded.bytes_done = report.filesystem_verification.total_bytes;
    seeded.staging_report_json = Some(serde_json::to_string(&report).unwrap());
    let mut document = f.store.read_downloads().unwrap();
    document.value.tasks[1] = seeded;
    f.store
        .write_downloads(document.revision, document.value)
        .unwrap();
    let receipt = f
        .service
        .run(&f.store, &fresh.id, || Ok(()))
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
    let done = f
        .service
        .mark_indexed(&f.store, &receipt, &entry.id)
        .unwrap();
    assert_eq!(done.tasks[1].phase, DownloadPhase::Downloaded);
    assert_eq!(done.tasks[1].local_files, Some(LocalFiles::Present));
    let new = f.store.read_downloads().unwrap().value.tasks[1].clone();
    let index = f.store.read_library().unwrap();
    assert_eq!(index.value.records.len(), 1);
    assert_eq!(
        index.value.records[0].identity.as_ref().unwrap().file_key,
        new.output_identity.unwrap()
    );
    assert_eq!(f.store.read_downloads().unwrap().value.tasks[0], old);
    assert_eq!(f.store.read_phone_library().unwrap(), phone_before);
}

#[test]
fn existing_incomplete_and_reappearing_destinations_never_authorize_replacement() {
    let f = fixture();
    let old = complete_for_presence(&f, record(&f));
    assert_eq!(
        prepare_same(&f).unwrap_err().code,
        "DOWNLOAD_ALREADY_PRESENT"
    );
    let page = f
        .library
        .join(&old.destination)
        .join("0001-123456/0001.gif");
    fs::write(&page, b"changed-size").unwrap();
    assert_eq!(
        f.service.read(&f.store).unwrap().tasks[0].local_files,
        Some(LocalFiles::Incomplete)
    );
    assert_eq!(
        prepare_same(&f).unwrap_err().code,
        "DOWNLOAD_LOCAL_FILES_INCOMPLETE"
    );
    fs::remove_dir_all(f.library.join(&old.destination)).unwrap();
    let plan = prepare_same(&f).unwrap();
    let index_before = f.store.read_library().unwrap();
    let downloads_before = f.store.read_downloads().unwrap();
    fs::create_dir(f.library.join(&old.destination)).unwrap();
    let sentinel = f.library.join(&old.destination).join("KEEP");
    fs::write(&sentinel, b"user file").unwrap();
    assert!(f
        .service
        .confirm(&f.store, &plan.plan_id, plan.revision)
        .is_err());
    assert_eq!(f.store.read_library().unwrap(), index_before);
    assert_eq!(f.store.read_downloads().unwrap(), downloads_before);
    assert_eq!(fs::read(sentinel).unwrap(), b"user file");
}

#[test]
fn an_existing_indexed_alias_still_blocks_when_the_historical_destination_is_missing() {
    let f = fixture();
    let old = complete_for_presence(&f, record(&f));
    let alias = "user renamed work";
    fs::rename(f.library.join(&old.destination), f.library.join(alias)).unwrap();
    let mut library = f.store.read_library().unwrap();
    let row = &mut library.value.records[0];
    row.item.relative_path = alias.into();
    row.item.file_name = alias.into();
    row.item.id = hash(format!("{}\0{alias}", old.root.id).as_bytes());
    if let Some(cover) = &mut row.cover {
        cover.relative_path = cover.relative_path.replacen(&old.destination, alias, 1);
    }
    f.store
        .write_library(library.revision, library.value)
        .unwrap();
    assert_eq!(
        f.service.read(&f.store).unwrap().tasks[0].local_files,
        Some(LocalFiles::Missing)
    );
    assert_eq!(
        prepare_same(&f).unwrap_err().code,
        "DOWNLOAD_ALREADY_PRESENT"
    );
    assert!(f.library.join(alias).join("0001-123456/0001.gif").is_file());
}

#[test]
fn an_unavailable_or_recreated_root_needs_a_new_picker_identity_before_confirmation() {
    let f = fixture();
    let old = complete_for_presence(&f, record(&f));
    let preserved = f._temp.path().join("preserved old root");
    fs::rename(&f.library, &preserved).unwrap();
    let before = f.store.read_downloads().unwrap();
    assert_eq!(
        f.service.read(&f.store).unwrap().tasks[0].local_files,
        Some(LocalFiles::Unavailable)
    );
    assert!(prepare_same(&f).is_err());
    fs::create_dir(&f.library).unwrap();
    assert_eq!(
        f.service.read(&f.store).unwrap().tasks[0].local_files,
        Some(LocalFiles::Unavailable)
    );
    assert_eq!(prepare_same(&f).unwrap_err().code, "DOWNLOAD_ROOT_CHANGED");
    assert_eq!(f.store.read_downloads().unwrap(), before);
    let mut indexer = workbench_library::LibraryService::new();
    indexer.choose(&f.store, &f.library).unwrap();
    let mut library = f.store.read_library().unwrap();
    library.value.phase = LibraryPhase::Complete;
    assert_eq!(library.value.root.as_ref().unwrap().id, old.root.id);
    assert_ne!(
        library.value.root.as_ref().unwrap().file_key,
        old.root.file_key
    );
    f.store
        .write_library(library.revision, library.value)
        .unwrap();
    let plan = prepare_same(&f).unwrap();
    let view = f
        .service
        .confirm(&f.store, &plan.plan_id, plan.revision)
        .unwrap();
    assert_eq!(view.tasks.len(), 2);
    assert_eq!(view.tasks[0].local_files, Some(LocalFiles::Unavailable));
    assert_eq!(f.store.read_downloads().unwrap().value.tasks[0], old);
    assert!(preserved
        .join(&old.destination)
        .join("0001-123456/0001.gif")
        .is_file());
}

#[test]
fn manual_unlink_or_reassociation_is_not_removed_by_a_later_download_confirmation() {
    for replacement in [None, Some("999999")] {
        let f = fixture();
        let old = complete_for_presence(&f, record(&f));
        workbench_library::LibraryService::new()
            .link(
                &f.store,
                &old.root.id,
                old.generation,
                old.library_entry_id.as_deref().unwrap(),
                replacement.map(|id| workbench_storage::LibraryReference {
                    source: Source::Jm,
                    work_id: id.into(),
                }),
            )
            .unwrap();
        fs::remove_dir_all(f.library.join(&old.destination)).unwrap();
        let before = f.store.read_library().unwrap();
        let mut metadata = old.metadata.clone();
        metadata.title = "A different new destination".into();
        assert_eq!(
            f.service
                .prepare(&f.store, &old.root.id, old.generation, metadata)
                .unwrap_err()
                .code,
            "LIBRARY_IDENTITY_CONFLICT"
        );
        assert_eq!(f.store.read_library().unwrap(), before);
    }
}

#[cfg(unix)]
#[test]
fn redirected_completed_paths_are_unavailable_instead_of_missing() {
    use std::os::unix::fs::symlink;
    let f = fixture();
    let old = complete_for_presence(&f, record(&f));
    fs::remove_dir_all(f.library.join(&old.destination)).unwrap();
    let outside = f._temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("KEEP"), b"keep").unwrap();
    symlink(&outside, f.library.join(&old.destination)).unwrap();
    let before = f.store.read_library().unwrap();
    assert_eq!(
        f.service.read(&f.store).unwrap().tasks[0].local_files,
        Some(LocalFiles::Unavailable)
    );
    assert_eq!(
        prepare_same(&f).unwrap_err().code,
        "DOWNLOAD_LOCAL_FILES_UNAVAILABLE"
    );
    assert_eq!(f.store.read_library().unwrap(), before);
    assert_eq!(fs::read(outside.join("KEEP")).unwrap(), b"keep");
}

#[test]
fn shared_semantic_validation_reuses_unchanged_values_but_rejects_changed_proof() {
    let f = fixture();
    let first = f.service.load_shared(&f.store).unwrap();
    assert!(Arc::ptr_eq(
        &first,
        &f.service.load_shared(&f.store).unwrap()
    ));

    let mut altered = first.value.clone();
    // Valid JSON at the storage layer, but not a valid typed staging checkpoint.
    altered.tasks[0].checkpoint_json = Some("{}".into());
    let saved = f.store.write_downloads(first.revision, altered).unwrap();
    assert_eq!(
        f.service.read(&f.store).unwrap_err().code,
        "DOWNLOAD_DOCUMENT_INVALID"
    );
    f.store
        .write_downloads(saved.revision, first.value.clone())
        .unwrap();
    let restored = f.service.load_shared(&f.store).unwrap();
    assert!(!Arc::ptr_eq(&first, &restored));
    assert_eq!(restored.value, first.value);

    let mut altered = restored.value.clone();
    altered.tasks[0].metadata.title = "Changed after authorization".into();
    f.store.write_downloads(restored.revision, altered).unwrap();
    assert_eq!(
        f.service.read(&f.store).unwrap_err().code,
        "DOWNLOAD_DOCUMENT_INVALID"
    );
}
fn gif() -> Vec<u8> {
    let image =
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2, 2, image::Rgb([30, 60, 90])));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Gif).unwrap();
    bytes.into_inner()
}
fn report(f: &Fixture, record: &DownloadRecord) -> (PathBuf, LocalExecutionReport) {
    report_with_images(f, record, &[("gif", gif()), ("gif", gif())])
}
fn report_with_images(
    f: &Fixture,
    record: &DownloadRecord,
    pages: &[(&str, Vec<u8>)],
) -> (PathBuf, LocalExecutionReport) {
    let workspace = f.store.open_download_workspace().unwrap();
    let staging = workspace.path().to_path_buf();
    let (state, ledger, command) = adapter::current(record).unwrap();
    let plan = local_executor::plan(&command).unwrap();
    let root = staging.join(&plan.staging_subdir);
    fs::create_dir_all(root.join("chapters/000001-123456")).unwrap();
    let mut artifacts = Vec::new();
    for (index, (extension, bytes)) in pages.iter().enumerate() {
        let number = index + 1;
        let path = format!("chapters/000001-123456/{number:06}.{extension}");
        fs::write(root.join(&path), bytes).unwrap();
        artifacts.push(StagedArtifact {
            relative_path: path,
            size_bytes: bytes.len() as u64,
            sha256: hash(bytes),
        });
    }
    let source_completion = SourceCompletionProof {
        schema_version: 1,
        source: "jm".into(),
        upstream_commit: JM_UPSTREAM_COMMIT.into(),
        source_contract_verified: true,
        execution_supported: false,
        manifest: StagingManifest {
            schema_version: 1,
            command_id: command.command_id.clone(),
            task_id: command.task_id.clone(),
            work_id: command.work_id.clone(),
            task_revision: command.task_revision,
            target_hash: command.target_hash.clone(),
            backend: "JM".into(),
            source_work_id: command.source_work_id.clone(),
            staging_subdir: plan.staging_subdir.clone(),
            source_enumeration_complete: true,
            all_scheduled_downloads_joined: true,
            downloader_reported_full_completion: true,
            expected_content_units: pages.len() as u64,
            completed_content_units: pages.len() as u64,
            failed_content_units: 0,
            artifacts,
        },
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    };
    let filesystem_verification =
        filesystem_verifier::verify(&staging, &plan, &source_completion.manifest).unwrap();
    let execution = IsolatedStagingExecutionResult {
        schema_version: 1,
        command_id: command.command_id.clone(),
        task_id: command.task_id.clone(),
        work_id: command.work_id.clone(),
        task_revision: command.task_revision,
        target_hash: command.target_hash.clone(),
        source: "jm".into(),
        source_work_id: command.source_work_id.clone(),
        preflight_hash: "a".repeat(64),
        staging_execution_completed: true,
        source_completion: source_completion.clone(),
        filesystem_verification: filesystem_verification.clone(),
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    };
    let receipt =
        verified_execution_receipt::build(&command, &plan, &execution, "2026-09-10T12:00:00Z")
            .unwrap();
    let receipt_view = executor_handoff::receipt_view(&state, &ledger, &receipt).unwrap();
    (
        staging,
        LocalExecutionReport {
            schema_version: 2,
            command_id: command.command_id,
            task_id: command.task_id,
            work_id: command.work_id,
            task_revision: command.task_revision,
            target_hash: command.target_hash,
            source: "jm".into(),
            source_work_id: command.source_work_id,
            staging_subdir: plan.staging_subdir,
            preflight_hash: execution.preflight_hash,
            staging_execution_completed: true,
            receipt,
            source_completion,
            filesystem_verification,
            receipt_view,
            inventory_mutation_authorized: false,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
            production_enablement_authorized: false,
        },
    )
}
fn seed_report(f: &Fixture) {
    let mut value = record(f);
    let (_, report) = report(f, &value);
    value.files_done = 2;
    value.files_total = Some(2);
    value.bytes_done = report.filesystem_verification.total_bytes;
    value.staging_report_json = Some(serde_json::to_string(&report).unwrap());
    put(f, value);
}
fn set_active(f: &Fixture) {
    let value = record(f);
    let workspace = f.store.open_download_workspace().unwrap();
    f.service.lock().unwrap().active = Some(Active {
        task_id: value.id,
        revision: value.revision,
        _workspace: workspace,
    });
}

#[test]
fn read_reopens_as_paused_without_changing_disk_or_starting_work() {
    let f = fixture();
    let before = f.store.read_downloads().unwrap();
    let new = DownloadService::new();
    let view = new.read(&f.store).unwrap();
    assert_eq!(view.tasks[0].phase, DownloadPhase::Paused);
    assert_eq!(before, f.store.read_downloads().unwrap());
    assert!(fs::read_dir(&f.library).unwrap().next().is_none());
}
#[test]
fn active_read_is_not_recovered_as_paused_and_resume_waits_for_old_worker() {
    let f = fixture();
    set_active(&f);
    assert_eq!(
        f.service.read(&f.store).unwrap().tasks[0].phase,
        DownloadPhase::Queued
    );
    let paused = f
        .service
        .control(&f.store, &f.id, 1, Control::Pause)
        .unwrap();
    assert_eq!(paused.tasks[0].revision, 2);
    assert_eq!(
        f.service
            .control(&f.store, &f.id, 2, Control::Resume)
            .unwrap_err()
            .code,
        "DOWNLOAD_WORKER_BUSY"
    );
    assert_eq!(record(&f).phase, DownloadPhase::Paused);
    f.service.clear(&f.id, 1);
    assert_eq!(
        f.service
            .control(&f.store, &f.id, 2, Control::Resume)
            .unwrap()
            .tasks[0]
            .phase,
        DownloadPhase::Queued
    );
}
#[test]
fn stale_control_and_changed_root_are_refused_without_writes() {
    let f = fixture();
    let before = f.store.read_downloads().unwrap();
    assert_eq!(
        f.service
            .control(&f.store, &f.id, 99, Control::Pause)
            .unwrap_err()
            .code,
        "DOWNLOAD_TASK_STALE"
    );
    assert_eq!(before, f.store.read_downloads().unwrap());
    let mut library = f.store.read_library().unwrap();
    library.value.generation += 1;
    f.store
        .write_library(library.revision, library.value)
        .unwrap();
    assert_eq!(
        f.service
            .control(&f.store, &f.id, 1, Control::Resume)
            .unwrap_err()
            .code,
        "DOWNLOAD_ROOT_CHANGED"
    );
}
#[test]
fn document_corruption_and_future_schema_preserve_original_bytes() {
    let f = fixture();
    let path = f
        ._temp
        .path()
        .join("private")
        .join(workbench_storage::PRIVATE_DIRECTORY)
        .join("downloads.json");
    for bytes in [
        b"{corrupt".as_slice(),
        b"{\"schemaVersion\":2,\"revision\":1,\"value\":{\"version\":2}}".as_slice(),
    ] {
        fs::write(&path, bytes).unwrap();
        assert!(f.service.read(&f.store).is_err());
        assert!(f
            .store
            .write_downloads(0, DownloadsDocument::default())
            .is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}
#[test]
fn worker_file_lock_is_cross_instance_and_released_on_drop() {
    let f = fixture();
    let other = WorkbenchStore::open(f._temp.path().join("private")).unwrap();
    let worker = f.store.open_download_workspace().unwrap();
    assert!(other.open_download_workspace().is_err());
    drop(worker);
    assert!(other.open_download_workspace().is_ok());
}
#[test]
fn pause_after_authorized_write_preserves_new_revision_and_completed_checkpoint() {
    let f = fixture();
    set_active(&f);
    let initial = record(&f);
    let page = StagedArtifact {
        relative_path: "chapters/000001-123456/000001.gif".into(),
        size_bytes: gif().len() as u64,
        sha256: hash(&gif()),
    };
    let mut checkpoint = StagingCheckpoint {
        descriptor_hash: "a".repeat(64),
        expected_files: 2,
        artifacts: vec![],
        pending: Some(page.clone()),
    };
    f.service
        .checkpoint(&f.store, &initial, &checkpoint)
        .unwrap();
    f.service
        .control(&f.store, &f.id, 1, Control::Pause)
        .unwrap();
    checkpoint.pending = None;
    checkpoint.artifacts.push(page);
    f.service
        .checkpoint(&f.store, &initial, &checkpoint)
        .unwrap();
    let saved = record(&f);
    assert_eq!(saved.revision, 2);
    assert_eq!(saved.phase, DownloadPhase::Paused);
    assert_eq!(saved.files_done, 1);
}
#[test]
fn materialization_is_exact_add_only_and_compatible_metadata_is_complete() {
    let f = fixture();
    let mut record = record(&f);
    let (stage, report) = report(&f, &record);
    materialize::save(&mut record, &report, &stage, &|| Ok(()), &mut |_| Ok(())).unwrap();
    materialize::verify_output(&record).unwrap();
    let root = f.library.join(&record.destination);
    let metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("元数据.json")).unwrap()).unwrap();
    for key in [
        "id",
        "name",
        "addtime",
        "description",
        "total_views",
        "likes",
        "chapterInfos",
        "series_id",
        "comment_total",
        "author",
        "tags",
        "works",
        "actors",
        "related_list",
        "liked",
        "is_favorite",
        "is_aids",
    ] {
        assert!(metadata.get(key).is_some(), "{key}");
    }
    assert_eq!(metadata["description"], "");
    assert!(root.join("_mangamonitor-layout.json").is_file());
    assert!(root.join("0001-123456/0001.gif").is_file());
    let mut unrelated = record.clone();
    unrelated.output_identity = None;
    unrelated.output_manifest_hash = None;
    assert_eq!(
        materialize::save(&mut unrelated, &report, &stage, &|| Ok(()), &mut |_| Ok(()))
            .unwrap_err()
            .code,
        "DOWNLOAD_DESTINATION_EXISTS"
    );
}
#[test]
fn interrupted_partial_output_resumes_exact_prefix_and_never_replaces_wrong_bytes() {
    let f = fixture();
    let mut record = record(&f);
    let (stage, report) = report(&f, &record);
    let first_page = f
        .library
        .join(&record.destination)
        .join("0001-123456/0001.gif");
    let interrupted = Cell::new(false);
    let persisted = RefCell::new(None);
    let result = materialize::save(
        &mut record,
        &report,
        &stage,
        &|| {
            if first_page.is_file() && !interrupted.replace(true) {
                Err(error("DOWNLOAD_PAUSED"))
            } else {
                Ok(())
            }
        },
        &mut |v| {
            *persisted.borrow_mut() = Some(v.clone());
            Ok(())
        },
    );
    assert!(result.is_err());
    record = persisted.borrow().clone().unwrap();
    let path = f
        .library
        .join(&record.destination)
        .join("0001-123456/0001.gif");
    if path.exists() {
        fs::write(&path, &gif()[..5]).unwrap();
    }
    materialize::save(&mut record, &report, &stage, &|| Ok(()), &mut |_| Ok(())).unwrap();
    assert_eq!(fs::read(&path).unwrap(), gif());
    record.output_manifest_hash = None;
    fs::write(&path, b"WRONG").unwrap();
    assert_eq!(
        materialize::save(&mut record, &report, &stage, &|| Ok(()), &mut |_| Ok(()))
            .unwrap_err()
            .code,
        "DOWNLOAD_OUTPUT_CHANGED"
    );
    assert_eq!(fs::read(&path).unwrap(), b"WRONG");
}
#[tokio::test]
async fn completed_staging_runs_offline_and_index_retry_does_not_download_again() {
    let f = fixture();
    seed_report(&f);
    let receipt = f
        .service
        .run(&f.store, &f.id, || Ok(()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        f.service.read(&f.store).unwrap().tasks[0].phase,
        DownloadPhase::Saving
    );
    assert_eq!(
        f.service
            .control(&f.store, &f.id, 1, Control::Pause)
            .unwrap_err()
            .code,
        "DOWNLOAD_CONTROL_INVALID"
    );
    f.service.index_failed(&f.store, &receipt, "BUSY").unwrap();
    f.service
        .control(&f.store, &f.id, 1, Control::Retry)
        .unwrap();
    let retry = f
        .service
        .run(&f.store, &f.id, || Ok(()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retry.manifest_hash, receipt.manifest_hash);
    assert_eq!(retry.expected_pages, 2);
    let mut library = workbench_library::LibraryService::new();
    let snapshot = library
        .register_completed(
            &f.store,
            &retry.root_id,
            retry.generation,
            &retry.relative_path,
            &workbench_storage::LibraryReference {
                source: Source::Jm,
                work_id: retry.work_id.clone(),
            },
            retry.expected_pages,
        )
        .unwrap();
    let entry = snapshot
        .items
        .iter()
        .find(|item| item.relative_path == retry.relative_path)
        .unwrap();
    let done = f.service.mark_indexed(&f.store, &retry, &entry.id).unwrap();
    assert_eq!(done.tasks[0].phase, DownloadPhase::Downloaded);
    assert_eq!(f.store.read_phone_library().unwrap().revision, 0);
    let workspace = f.store.open_download_workspace().unwrap();
    let (_, _, command) = adapter::current(&record(&f)).unwrap();
    assert!(!workspace
        .path()
        .join("commands")
        .join(command.command_id)
        .exists());
    assert!(f
        .library
        .join(&retry.relative_path)
        .join("0001-123456/0001.gif")
        .is_file());
}
#[tokio::test]
async fn invalid_finish_receipt_releases_worker_and_preserves_complete_files() {
    let f = fixture();
    seed_report(&f);
    let mut receipt = f
        .service
        .run(&f.store, &f.id, || Ok(()))
        .await
        .unwrap()
        .unwrap();
    receipt.manifest_hash = "f".repeat(64);
    assert!(f
        .service
        .mark_indexed(&f.store, &receipt, &"e".repeat(64))
        .is_err());
    assert!(f.service.lock().unwrap().active.is_none());
    assert_eq!(
        f.service.read(&f.store).unwrap().tasks[0].phase,
        DownloadPhase::Paused
    );
    assert!(f
        .library
        .join(&receipt.relative_path)
        .join("_mangamonitor.json")
        .is_file());
}

struct Core {
    plan: LocalExecutionPlan,
    authorization: ImageDownloadAuthorization,
    evidence: SourcePreflightEvidence,
    proof: SourcePreflightProof,
    descriptors: SourceMediaDescriptorSet,
}
fn core(record: &DownloadRecord) -> Core {
    let media = (1..=2)
        .map(|n| MediaDescriptor {
            image_index: n,
            source_media_id: format!("{n:03}.gif"),
            request_url: format!("https://cdn-msp2.jmapiproxy2.cc/media/photos/123456/{n:03}.gif"),
            source_format: "gif".into(),
            transform: "NONE".into(),
            transform_parameter: 0,
            relative_path: format!("chapters/000001-123456/{n:06}.gif"),
        })
        .collect();
    core_for_chapters(
        record,
        vec![MediaChapterDescriptors {
            chapter_id: "123456".into(),
            chapter_order: 1,
            jm_scramble_id: Some(200_000),
            media,
        }],
        None,
        None,
    )
}
fn core_for_chapters(
    record: &DownloadRecord,
    chapters: Vec<MediaChapterDescriptors>,
    chapter_pagination: Option<cloud_monitor::source_completion::PaginationProof>,
    image_pagination: Option<cloud_monitor::source_completion::PaginationProof>,
) -> Core {
    let (_, _, command) = adapter::current(record).unwrap();
    let plan = local_executor::plan(&command).unwrap();
    let request = source_bridge_request::build(&plan).unwrap();
    let evidence = SourcePreflightEvidence {
        schema_version: 1,
        command_id: request.command_id.clone(),
        task_id: request.task_id.clone(),
        work_id: request.work_id.clone(),
        task_revision: request.task_revision,
        target_hash: request.target_hash.clone(),
        source: request.source.clone(),
        source_work_id: request.source_work_id.clone(),
        upstream_commit: request.upstream_commit.clone(),
        completion_contract_version: request.completion_contract_version,
        scope: request.scope.clone(),
        source_enumeration_complete: true,
        chapter_pagination,
        expected_chapter_count: chapters.len() as u64,
        chapters: chapters
            .iter()
            .map(|chapter| PreflightChapter {
                chapter_id: chapter.chapter_id.clone(),
                chapter_order: chapter.chapter_order,
                expected_images: chapter.media.len() as u64,
                image_pagination: image_pagination.clone(),
            })
            .collect(),
        image_bytes_downloaded: false,
        staging_written: false,
    };
    let proof = source_preflight::validate(&plan, &request, &evidence).unwrap();
    let authorization = ImageDownloadAuthorization {
        schema_version: 1,
        command_id: request.command_id.clone(),
        task_id: request.task_id.clone(),
        work_id: request.work_id.clone(),
        task_revision: request.task_revision,
        target_hash: request.target_hash.clone(),
        source: request.source.clone(),
        source_work_id: request.source_work_id.clone(),
        preflight_hash: proof.preflight_hash.clone(),
        expected_chapter_count: proof.expected_chapter_count,
        expected_content_units: proof.expected_content_units,
        staging_subdir: plan.staging_subdir.clone(),
        write_scope: "COMMAND_OWNED_STAGING_ONLY".into(),
        current_state_binding_hash: "state-generation".into(),
        current_gate_ledger_hash: "gate-generation".into(),
        live_preflight_generation_verified: true,
        image_download_authorized: true,
        staging_write_authorized: true,
        reusable_permit: false,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    };
    let descriptors = SourceMediaDescriptorSet {
        schema_version: 1,
        command_id: authorization.command_id.clone(),
        task_id: authorization.task_id.clone(),
        work_id: authorization.work_id.clone(),
        task_revision: authorization.task_revision,
        target_hash: authorization.target_hash.clone(),
        source: authorization.source.clone(),
        source_work_id: authorization.source_work_id.clone(),
        preflight_hash: authorization.preflight_hash.clone(),
        expected_chapter_count: proof.expected_chapter_count,
        expected_content_units: proof.expected_content_units,
        staging_subdir: authorization.staging_subdir.clone(),
        write_scope: authorization.write_scope.clone(),
        chapters,
        image_download_authorized: true,
        staging_write_authorized: true,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    };
    Core {
        plan,
        authorization,
        evidence,
        proof,
        descriptors,
    }
}
fn context<'a>(stage: &'a Path, core: &'a Core) -> IsolatedStagingExecutionContext<'a> {
    IsolatedStagingExecutionContext {
        staging_root: stage,
        plan: &core.plan,
        authorization: &core.authorization,
        evidence: &core.evidence,
        preflight: &core.proof,
        descriptors: &core.descriptors,
    }
}
fn processed(d: MediaDescriptor) -> ProcessedMedia {
    ProcessedMedia {
        source_media_id: d.source_media_id,
        request_url: d.request_url,
        source_format: d.source_format,
        applied_transform: d.transform,
        applied_transform_parameter: d.transform_parameter,
        bytes: gif(),
    }
}
#[tokio::test]
async fn full_pending_page_after_checkpoint_interruption_is_reused_without_fetch() {
    let f = fixture();
    let c = core(&record(&f));
    let workspace = f.store.open_download_workspace().unwrap();
    let saved = RefCell::new(None);
    let failure = isolated_staging_execution::execute_resumable_with_fetcher(
        context(workspace.path(), &c),
        None,
        |d| async { Ok(processed(d)) },
        || Ok(c.authorization.clone()),
        |cp| {
            if cp.pending.is_none() && cp.artifacts.len() == 1 {
                return Err("SIMULATED_EXIT".into());
            }
            *saved.borrow_mut() = Some(cp.clone());
            Ok(())
        },
    )
    .await
    .unwrap_err();
    assert_eq!(failure, "SIMULATED_EXIT");
    let checkpoint = saved.borrow().clone().unwrap();
    assert!(checkpoint.pending.is_some());
    let requested = RefCell::new(Vec::new());
    let result = isolated_staging_execution::execute_resumable_with_fetcher(
        context(workspace.path(), &c),
        Some(&checkpoint),
        |d| {
            requested.borrow_mut().push(d.image_index);
            async { Ok(processed(d)) }
        },
        || Ok(c.authorization.clone()),
        |_| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(*requested.borrow(), vec![2]);
    assert!(result.staging_execution_completed);
}
#[tokio::test]
async fn partial_pending_page_resumes_only_matching_prefix_and_descriptor_generation() {
    let f = fixture();
    let c = core(&record(&f));
    let workspace = f.store.open_download_workspace().unwrap();
    let page = &c.descriptors.chapters[0].media[0];
    let root = workspace.path().join(&c.plan.staging_subdir);
    fs::create_dir_all(root.join("chapters/000001-123456")).unwrap();
    fs::write(root.join(&page.relative_path), &gif()[..5]).unwrap();
    let mut cp = StagingCheckpoint {
        descriptor_hash: monitor::hash(&c.descriptors),
        expected_files: 2,
        artifacts: Vec::new(),
        pending: Some(StagedArtifact {
            relative_path: page.relative_path.clone(),
            size_bytes: gif().len() as u64,
            sha256: hash(&gif()),
        }),
    };
    let original = fs::read(root.join(&page.relative_path)).unwrap();
    cp.descriptor_hash = "f".repeat(64);
    assert!(isolated_staging_execution::execute_resumable_with_fetcher(
        context(workspace.path(), &c),
        Some(&cp),
        |_| async { panic!("must not fetch stale generation") },
        || Ok(c.authorization.clone()),
        |_| Ok(())
    )
    .await
    .is_err());
    assert_eq!(original, fs::read(root.join(&page.relative_path)).unwrap());
    cp.descriptor_hash = monitor::hash(&c.descriptors);
    isolated_staging_execution::execute_resumable_with_fetcher(
        context(workspace.path(), &c),
        Some(&cp),
        |d| async { Ok(processed(d)) },
        || Ok(c.authorization.clone()),
        |_| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(gif(), fs::read(root.join(&page.relative_path)).unwrap());
}
#[cfg(unix)]
#[test]
fn redirected_destination_is_rejected_and_unrelated_bytes_survive() {
    use std::os::unix::fs::symlink;
    let f = fixture();
    let mut r = record(&f);
    let (stage, report) = report(&f, &r);
    let outside = f._temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("KEEP"), b"keep").unwrap();
    symlink(&outside, f.library.join(&r.destination)).unwrap();
    assert!(materialize::save(&mut r, &report, &stage, &|| Ok(()), &mut |_| Ok(())).is_err());
    assert_eq!(fs::read(outside.join("KEEP")).unwrap(), b"keep");
}
#[test]
fn safe_directory_does_not_follow_file_symlinks_or_overwrite_existing_files() {
    let temp = TempDir::new().unwrap();
    let directory = Directory::open(temp.path()).unwrap();
    fs::write(temp.path().join("sentinel"), b"keep").unwrap();
    assert!(directory.create_file("sentinel").is_err());
    assert!(directory.create_file("../escape").is_err());
    assert_eq!(fs::read(temp.path().join("sentinel")).unwrap(), b"keep");
}

#[test]
fn completed_temporary_cleanup_is_exact_idempotent_and_leaves_final_work() {
    let f = fixture();
    let mut value = record(&f);
    let (stage, report) = report(&f, &value);
    value.files_total = Some(2);
    value.staging_report_json = Some(serde_json::to_string(&report).unwrap());
    materialize::save(&mut value, &report, &stage, &|| Ok(()), &mut |_| Ok(())).unwrap();
    assert!(materialize::cleanup_completed(&value, &stage).is_err());
    let staged_root = stage.join(&report.staging_subdir);
    let foreign = staged_root.join("unknown.bin");
    fs::write(&foreign, b"keep").unwrap();
    value.phase = DownloadPhase::Downloaded;
    value.library_entry_id = Some("e".repeat(64));
    assert!(materialize::cleanup_completed(&value, &stage).is_err());
    assert_eq!(fs::read(&foreign).unwrap(), b"keep");
    assert!(staged_root
        .join("chapters/000001-123456/000001.gif")
        .is_file());
    fs::remove_file(&foreign).unwrap();
    materialize::cleanup_completed(&value, &stage).unwrap();
    materialize::cleanup_completed(&value, &stage).unwrap();
    assert!(!staged_root.exists());
    materialize::verify_output(&value).unwrap();
    assert!(f
        .library
        .join(&value.destination)
        .join("0001-123456/0001.gif")
        .is_file());
}

fn static_image(format: image::ImageFormat) -> Vec<u8> {
    let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(3, 4, |x, y| {
        image::Rgb([(x * 70) as u8, (y * 50) as u8, 90])
    }));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, format).unwrap();
    let bytes = bytes.into_inner();
    assert_eq!(image::guess_format(&bytes).unwrap(), format);
    assert_eq!(image::load_from_memory(&bytes).unwrap().width(), 3);
    bytes
}

fn downloads_path(f: &Fixture) -> PathBuf {
    f._temp
        .path()
        .join("private")
        .join(workbench_storage::PRIVATE_DIRECTORY)
        .join("downloads.json")
}

#[test]
fn new_jpeg_and_unchanged_gif_pages_materialize_and_register_as_one_complete_work() {
    let f = fixture();
    let mut value = record(&f);
    assert!(
        value.jpeg_output,
        "new approvals select the JPEG output policy"
    );
    let jpeg = static_image(image::ImageFormat::Jpeg);
    let original_gif = gif();
    let pages = [("jpg", jpeg.clone()), ("gif", original_gif.clone())];
    let (stage, report) = report_with_images(&f, &value, &pages);
    let phone_before = f.store.read_phone_library().unwrap();
    materialize::save(&mut value, &report, &stage, &|| Ok(()), &mut |_| Ok(())).unwrap();
    materialize::verify_output(&value).unwrap();
    let root = f.library.join(&value.destination);
    assert_eq!(fs::read(root.join("0001-123456/0001.jpg")).unwrap(), jpeg);
    assert_eq!(
        fs::read(root.join("0001-123456/0002.gif")).unwrap(),
        original_gif
    );
    assert!(!root.join("0001-123456/0001.webp").exists());
    assert!(!root.join("0001-123456/0002.jpg").exists());
    let mut library = workbench_library::LibraryService::new();
    let snapshot = library
        .register_completed(
            &f.store,
            &value.root.id,
            value.generation,
            &value.destination,
            &workbench_storage::LibraryReference {
                source: Source::Jm,
                work_id: value.metadata.work_id.clone(),
            },
            2,
        )
        .unwrap();
    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(snapshot.items[0].page_count, Some(2));
    assert_eq!(
        snapshot.items[0].source_ref.as_ref().unwrap().work_id,
        "123456"
    );
    assert_eq!(snapshot.items[0].error_code, None);
    assert!(snapshot.items[0].cover_available);
    assert_eq!(f.store.read_phone_library().unwrap(), phone_before);
}

#[tokio::test]
async fn legacy_webp_task_without_profile_field_reopens_and_finishes_its_saved_report() {
    let f = fixture();
    let mut value = record(&f);
    value.jpeg_output = false;
    // Freeze the pre-JPEG binding contract independently of the current helper.
    value.target_hash = hash(
        &serde_json::to_vec(&(
            "manual-JM-layout-v1",
            &value.root,
            value.generation,
            &value.metadata,
            &value.destination,
            value.approval_revision,
        ))
        .unwrap(),
    );
    let original_target = value.target_hash.clone();
    assert_eq!(binding(&value).unwrap(), original_target);
    let webp = static_image(image::ImageFormat::WebP);
    let (stage, report) = report_with_images(&f, &value, &[("webp", webp.clone()), ("gif", gif())]);
    value.files_done = 2;
    value.files_total = Some(2);
    value.bytes_done = report.filesystem_verification.total_bytes;
    value.staging_report_json = Some(serde_json::to_string(&report).unwrap());
    value.phase = DownloadPhase::Paused;
    put(&f, value);
    let path = downloads_path(&f);
    let mut old_document: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    old_document["value"]["tasks"][0]
        .as_object_mut()
        .unwrap()
        .remove("jpegOutput");
    fs::write(&path, serde_json::to_vec(&old_document).unwrap()).unwrap();
    let before_read = fs::read(&path).unwrap();
    let restarted = DownloadService::new();
    assert_eq!(
        restarted.read(&f.store).unwrap().tasks[0].phase,
        DownloadPhase::Paused
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        before_read,
        "opening does not migrate or execute an old task"
    );
    assert!(!record(&f).jpeg_output);
    assert_eq!(record(&f).target_hash, original_target);
    assert!(fs::read_dir(&f.library).unwrap().next().is_none());
    restarted
        .control(&f.store, &f.id, 1, Control::Resume)
        .unwrap();
    let receipt = restarted
        .run(&f.store, &f.id, || Ok(()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(receipt.expected_pages, 2);
    let root = f.library.join(&receipt.relative_path);
    assert_eq!(fs::read(root.join("0001-123456/0001.webp")).unwrap(), webp);
    assert!(!root.join("0001-123456/0001.jpg").exists());
    let mut library = workbench_library::LibraryService::new();
    let snapshot = library
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
    let done = restarted
        .mark_indexed(&f.store, &receipt, &snapshot.items[0].id)
        .unwrap();
    assert_eq!(done.tasks[0].phase, DownloadPhase::Downloaded);
    assert!(!record(&f).jpeg_output);
    assert_eq!(record(&f).target_hash, original_target);
    assert_eq!(fs::read(root.join("0001-123456/0001.webp")).unwrap(), webp);
    assert!(!stage.join(report.staging_subdir).exists());
}

#[test]
fn changing_a_saved_output_profile_without_its_approval_binding_is_rejected_without_writes() {
    for original_profile in [false, true] {
        let f = fixture();
        let mut value = record(&f);
        value.jpeg_output = original_profile;
        value.target_hash = binding(&value).unwrap();
        put(&f, value.clone());
        value.jpeg_output = !original_profile;
        put(&f, value);
        let before = fs::read(downloads_path(&f)).unwrap();
        assert_eq!(
            f.service.read(&f.store).unwrap_err().code,
            "DOWNLOAD_DOCUMENT_INVALID"
        );
        assert_eq!(
            f.service
                .control(&f.store, &f.id, 1, Control::Pause)
                .unwrap_err()
                .code,
            "DOWNLOAD_DOCUMENT_INVALID"
        );
        assert_eq!(fs::read(downloads_path(&f)).unwrap(), before);
        assert!(fs::read_dir(&f.library).unwrap().next().is_none());
    }
}

#[test]
fn materialization_rejects_static_formats_outside_the_approved_profile() {
    for jpeg_output in [false, true] {
        let f = fixture();
        let mut value = record(&f);
        value.jpeg_output = jpeg_output;
        value.target_hash = binding(&value).unwrap();
        let (extension, format) = if jpeg_output {
            ("webp", image::ImageFormat::WebP)
        } else {
            ("jpg", image::ImageFormat::Jpeg)
        };
        let bytes = static_image(format);
        let (stage, report) =
            report_with_images(&f, &value, &[(extension, bytes.clone()), ("gif", gif())]);
        let staged_page = stage
            .join(&report.staging_subdir)
            .join(format!("chapters/000001-123456/000001.{extension}"));
        let before = f.store.read_downloads().unwrap();
        assert_eq!(
            materialize::save(&mut value, &report, &stage, &|| Ok(()), &mut |_| Ok(()))
                .unwrap_err()
                .code,
            "DOWNLOAD_PROOF_INVALID"
        );
        assert_eq!(f.store.read_downloads().unwrap(), before);
        assert_eq!(fs::read(staged_page).unwrap(), bytes);
        assert!(fs::read_dir(&f.library).unwrap().next().is_none());
    }
}

#[test]
fn approval_for_another_output_profile_cannot_reuse_an_old_staging_receipt() {
    let f = fixture();
    let mut value = record(&f);
    let original_private_hash = value.target_hash.clone();
    let (stage, report) = report_with_images(
        &f,
        &value,
        &[
            ("jpg", static_image(image::ImageFormat::Jpeg)),
            ("gif", gif()),
        ],
    );
    value.jpeg_output = false;
    value.target_hash = binding(&value).unwrap();
    assert_ne!(value.target_hash, original_private_hash);
    assert_eq!(
        materialize::save(&mut value, &report, &stage, &|| Ok(()), &mut |_| Ok(()))
            .unwrap_err()
            .code,
        "DOWNLOAD_PROOF_INVALID"
    );
    assert!(fs::read_dir(&f.library).unwrap().next().is_none());
    assert!(stage
        .join(report.staging_subdir)
        .join("chapters/000001-123456/000001.jpg")
        .is_file());
}
