//! Synthetic archives and reversible temporary-directory moves only. No Shell
//! deletion API, network/account action, or user-selected library is used here.
use image::{DynamicImage, ImageFormat};
use std::{fs, io::{Cursor, Write}, path::Path};
use tempfile::TempDir;
use workbench_library::{
    LibraryEvidence, LibraryItemState, LibraryPhase, LibraryRecycleRequest, LibraryService,
    LibrarySnapshot, ScanAction,
};
use workbench_storage::{
    DownloadPhase, DownloadRecord, DownloadsDocument, JmDownloadMetadata, LibraryLinkEvidence,
    LibraryReference, LibrarySourceLink, Source, StoreError, WorkbenchStore, PRIVATE_DIRECTORY,
};
use zip::{write::SimpleFileOptions, ZipWriter};

struct Fixture {
    app: TempDir,
    media: TempDir,
    bin: TempDir,
    store: WorkbenchStore,
    service: LibraryService,
    ready: LibrarySnapshot,
}

fn archive(path: &Path) {
    let mut png = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(2, 3).write_to(&mut png, ImageFormat::Png).unwrap();
    let mut zip = ZipWriter::new(fs::File::create(path).unwrap());
    zip.start_file("1.png", SimpleFileOptions::default()).unwrap();
    zip.write_all(png.get_ref()).unwrap();
    zip.finish().unwrap();
}

fn finish(service: &mut LibraryService, store: &WorkbenchStore, mut snapshot: LibrarySnapshot) -> LibrarySnapshot {
    for _ in 0..30 {
        if snapshot.phase != LibraryPhase::Reading { return snapshot; }
        snapshot = service.scan(store, snapshot.root_id.as_deref().unwrap(), snapshot.generation, ScanAction::Next).unwrap();
    }
    panic!("small synthetic library did not finish");
}

fn fixture() -> Fixture {
    let app = TempDir::new().unwrap();
    let media = TempDir::new().unwrap();
    let bin = TempDir::new().unwrap();
    archive(&media.path().join("合成 Selected.CBZ"));
    archive(&media.path().join("unrelated.zip"));
    let store = WorkbenchStore::open(app.path()).unwrap();
    let mut service = LibraryService::new();
    let started = service.choose(&store, media.path()).unwrap();
    let ready = finish(&mut service, &store, started);
    assert_eq!(ready.phase, LibraryPhase::Complete);
    Fixture { app, media, bin, store, service, ready }
}

fn selection_request(snapshot: &LibrarySnapshot) -> LibraryRecycleRequest {
    LibraryRecycleRequest {
        root_id: snapshot.root_id.clone().unwrap(),
        generation: snapshot.generation,
        entry_id: snapshot.items.iter().find(|item| item.file_name == "合成 Selected.CBZ").unwrap().id.clone(),
        expected_revision: snapshot.revision,
    }
}

fn selected(snapshot: &LibrarySnapshot) -> &workbench_storage::LibraryItem {
    snapshot.items.iter().find(|item| item.file_name == "合成 Selected.CBZ").unwrap()
}

#[test]
fn cancellation_and_host_failure_leave_media_and_registration_byte_identical() {
    let mut f = fixture();
    let request = selection_request(&f.ready);
    let before = f.store.read_library().unwrap();
    let bytes = fs::read(f.media.path().join("合成 Selected.CBZ")).unwrap();
    let cancelled = f.service.recycle_confirmed(&f.store, &request, |preview| {
        assert_eq!(preview.relative_path, "合成 Selected.CBZ");
        assert!(preview.bytes > 0);
        false
    }, |_| panic!("cancelled operation reached host")).unwrap();
    assert!(cancelled.is_none());
    assert_eq!(f.store.read_library().unwrap(), before);
    let problem = f.service.recycle_confirmed(&f.store, &request, |_| true, |_| Err(StoreError { code: "LIBRARY_RECYCLE_UNAVAILABLE" })).unwrap_err();
    assert_eq!(problem.code, "LIBRARY_RECYCLE_UNAVAILABLE");
    assert_eq!(f.store.read_library().unwrap(), before);
    assert_eq!(fs::read(f.media.path().join("合成 Selected.CBZ")).unwrap(), bytes);
    let problem = f.service.recycle_confirmed(&f.store, &request, |_| true, |_| Ok(())).unwrap_err();
    assert_eq!(problem.code, "LIBRARY_RECYCLE_NOT_COMPLETED");
    assert_eq!(f.store.read_library().unwrap(), before);
}

#[test]
fn confirmed_move_affects_one_file_preserves_metadata_and_survives_absent_scan_and_restore() {
    let mut f = fixture();
    let mut document = f.store.read_library().unwrap();
    let record = document.value.records.iter_mut().find(|r| r.item.file_name == "合成 Selected.CBZ").unwrap();
    record.manual_override = true;
    record.item.source_ref = Some(LibraryReference { source: Source::Jm, work_id: "123".into() });
    record.item.identity_evidence = Some(LibraryEvidence::Manual);
    record.item.version_updated_at = Some("2026-09-01".into());
    record.item.links = vec![LibrarySourceLink {
        reference: LibraryReference { source: Source::Pica, work_id: "a".repeat(24) },
        evidence: LibraryLinkEvidence::Manual,
        linked_at: 123,
    }];
    let original_record = record.clone();
    f.store.write_library(document.revision, document.value).unwrap();
    let ready = f.service.read(&f.store).unwrap();
    let request = selection_request(&ready);
    let unrelated = fs::read(f.media.path().join("unrelated.zip")).unwrap();
    let destination = f.bin.path().join("recoverable.cbz");
    let result = f.service.recycle_confirmed(&f.store, &request, |_| true, |target| {
        target.verify().unwrap();
        fs::rename(target.path(), &destination).unwrap();
        Ok(())
    }).unwrap().unwrap();
    assert!(result.recycled);
    assert_eq!(result.error_code, None);
    assert_eq!(selected(&result.snapshot).state, LibraryItemState::Unreadable);
    assert_eq!(selected(&result.snapshot).error_code.as_deref(), Some("LIBRARY_RECYCLED"));
    assert_eq!(fs::read(f.media.path().join("unrelated.zip")).unwrap(), unrelated);
    let stored = f.store.read_library().unwrap();
    let record = stored.value.records.iter().find(|r| r.item.id == request.entry_id).unwrap();
    assert_eq!(record.identity, original_record.identity);
    assert_eq!(record.item.links, original_record.item.links);
    assert_eq!(record.item.source_ref, original_record.item.source_ref);
    assert_eq!(record.item.added_at, original_record.item.added_at);
    assert_eq!(record.item.authors, original_record.item.authors);
    assert_eq!(record.item.tags, original_record.item.tags);

    let started = f.service.scan(&f.store, &request.root_id, request.generation, ScanAction::Start).unwrap();
    let absent = finish(&mut f.service, &f.store, started);
    assert_eq!(selected(&absent).error_code.as_deref(), Some("LIBRARY_RECYCLED"));
    // A cold service also preserves the tombstone and its manual metadata.
    f.service = LibraryService::new();
    fs::rename(&destination, f.media.path().join("合成 Selected.CBZ")).unwrap();
    let started = f.service.scan(&f.store, &request.root_id, absent.generation, ScanAction::Start).unwrap();
    let restored = finish(&mut f.service, &f.store, started);
    assert_eq!(restored.items.len(), 2);
    let item = selected(&restored);
    assert_eq!(item.state, LibraryItemState::Indexed);
    assert_eq!(item.error_code, None);
    assert_eq!(item.links, original_record.item.links);
    assert_eq!(item.source_ref, original_record.item.source_ref);
    assert_eq!(item.added_at, original_record.item.added_at);
    assert_eq!(item.version_updated_at, original_record.item.version_updated_at);
}

#[test]
fn failed_host_with_absent_path_is_uncertain_not_claimed_recycled() {
    let mut f = fixture();
    let request = selection_request(&f.ready);
    let result = f.service.recycle_confirmed(&f.store, &request, |_| true, |target| {
        fs::rename(target.path(), f.bin.path().join("recoverable.cbz")).unwrap();
        Err(StoreError { code: "LIBRARY_RECYCLE_RESULT_UNCERTAIN" })
    }).unwrap().unwrap();
    assert!(!result.recycled);
    assert_eq!(result.error_code.as_deref(), Some("LIBRARY_RECYCLE_RESULT_UNCERTAIN"));
    assert_eq!(selected(&result.snapshot).state, LibraryItemState::Unreadable);
    assert_ne!(selected(&result.snapshot).error_code.as_deref(), Some("LIBRARY_RECYCLED"));
    let started = f.service.scan(&f.store, &request.root_id, request.generation, ScanAction::Start).unwrap();
    let absent = finish(&mut f.service, &f.store, started);
    assert_eq!(selected(&absent).error_code.as_deref(), Some("LIBRARY_RECYCLE_RESULT_UNCERTAIN"));
}

#[test]
fn persistence_failure_after_move_returns_corrected_snapshot_and_never_overwrites_bad_store() {
    let mut f = fixture();
    let request = selection_request(&f.ready);
    let file = f.app.path().join(PRIVATE_DIRECTORY).join("library.json");
    let original = fs::read(&file).unwrap();
    let result = f.service.recycle_confirmed(&f.store, &request, |_| true, |target| {
        fs::rename(target.path(), f.bin.path().join("recoverable.cbz")).unwrap();
        fs::write(&file, b"synthetic-corruption").unwrap();
        Ok(())
    }).unwrap().unwrap();
    assert!(result.recycled);
    assert_eq!(result.error_code.as_deref(), Some("LIBRARY_RECYCLE_SAVE_FAILED"));
    assert_eq!(result.snapshot.root_id.as_deref(), Some(request.root_id.as_str()));
    assert_eq!(result.snapshot.generation, request.generation);
    assert_eq!(selected(&result.snapshot).state, LibraryItemState::Unreadable);
    assert_eq!(fs::read(&file).unwrap(), b"synthetic-corruption");
    fs::write(file, original).unwrap();
}

#[test]
fn foreign_revision_after_move_is_preserved_and_result_is_still_bound_to_selected_scope() {
    let mut f = fixture();
    let request = selection_request(&f.ready);
    let result = f.service.recycle_confirmed(&f.store, &request, |_| true, |target| {
        fs::rename(target.path(), f.bin.path().join("recoverable.cbz")).unwrap();
        let mut changed = f.store.read_library().unwrap();
        changed.value.generation += 1;
        f.store.write_library(changed.revision, changed.value).unwrap();
        Ok(())
    }).unwrap().unwrap();
    assert!(result.recycled);
    assert_eq!(result.error_code.as_deref(), Some("LIBRARY_RECYCLE_REFRESH_REQUIRED"));
    assert_eq!(result.snapshot.generation, request.generation);
    assert_eq!(f.store.read_library().unwrap().value.generation, request.generation + 1);
    assert_eq!(selected(&result.snapshot).state, LibraryItemState::Unreadable);
}

#[test]
fn stale_scope_revision_unknown_entry_and_active_scan_never_reach_confirmation() {
    let mut f = fixture();
    let original = selection_request(&f.ready);
    let mut cases = Vec::new();
    let mut changed = original.clone(); changed.root_id = "a".repeat(64); cases.push(changed);
    let mut changed = original.clone(); changed.generation += 1; cases.push(changed);
    let mut changed = original.clone(); changed.expected_revision += 1; cases.push(changed);
    let mut changed = original.clone(); changed.entry_id = "a".repeat(64); cases.push(changed);
    let mut changed = original.clone(); changed.entry_id = "../escape.zip".into(); cases.push(changed);
    for request in cases {
        assert!(f.service.recycle_confirmed(&f.store, &request, |_| panic!("invalid selection prompted"), |_| panic!("invalid selection reached host")).is_err());
    }
    let mut changed = f.store.read_library().unwrap();
    changed.value.phase = LibraryPhase::Reading;
    f.store.write_library(changed.revision, changed.value).unwrap();
    let request = selection_request(&f.service.read(&f.store).unwrap());
    assert_eq!(f.service.recycle_confirmed(&f.store, &request, |_| panic!("scan prompted"), |_| panic!("scan reached host")).unwrap_err().code, "LIBRARY_BUSY");
}

#[test]
fn changed_revision_during_confirmation_aborts_before_host() {
    let mut f = fixture();
    let request = selection_request(&f.ready);
    let problem = f.service.recycle_confirmed(&f.store, &request, |_| {
        let mut changed = f.store.read_library().unwrap();
        changed.value.updated_at = Some(1);
        f.store.write_library(changed.revision, changed.value).unwrap();
        true
    }, |_| panic!("stale confirmation reached host")).unwrap_err();
    assert_eq!(problem.code, "LIBRARY_STALE_SNAPSHOT");
    assert!(f.media.path().join("合成 Selected.CBZ").is_file());
}

#[test]
fn changed_file_unsupported_format_and_missing_file_never_reach_host() {
    let mut f = fixture();
    let request = selection_request(&f.ready);
    let path = f.media.path().join("合成 Selected.CBZ");
    fs::write(&path, b"different synthetic bytes").unwrap();
    assert_eq!(f.service.recycle_confirmed(&f.store, &request, |_| panic!("changed file prompted"), |_| panic!("changed file reached host")).unwrap_err().code, "LIBRARY_FILE_CHANGED");
    fs::rename(&path, f.bin.path().join("kept.cbz")).unwrap();
    assert_eq!(f.service.recycle_confirmed(&f.store, &request, |_| panic!("missing file prompted"), |_| panic!("missing file reached host")).unwrap_err().code, "LIBRARY_ENTRY_MISSING");

    let mut f = fixture();
    let mut changed = f.store.read_library().unwrap();
    changed.value.records.iter_mut().find(|r| r.item.file_name == "合成 Selected.CBZ").unwrap().item.format = workbench_library::LibraryFormat::Directory;
    f.store.write_library(changed.revision, changed.value).unwrap();
    let request = selection_request(&f.service.read(&f.store).unwrap());
    assert_eq!(f.service.recycle_confirmed(&f.store, &request, |_| panic!("unsupported file prompted"), |_| panic!("unsupported file reached host")).unwrap_err().code, "LIBRARY_RECYCLE_FORMAT_UNSUPPORTED");
}

fn queue(f: &Fixture, phase: DownloadPhase) {
    let library = f.store.read_library().unwrap();
    f.store.write_downloads(0, DownloadsDocument {
        tasks: vec![DownloadRecord {
            id: "b".repeat(64), source: Source::Jm, origin: "manual".into(), revision: 1,
            approval_revision: 1, target_hash: "c".repeat(64), root: library.value.root.unwrap(),
            generation: library.value.generation, metadata: JmDownloadMetadata {
                work_id: "456".into(), title: "Synthetic pending".into(), authors: Vec::new(),
                tags: Vec::new(), description: None, version_updated_at: None,
            }, destination: "pending.zip".into(), jpeg_output: false, zip_output: true,
            archive_file: None, phase, files_done: 0, files_total: None, bytes_done: 0,
            error_code: None, library_entry_id: None, updated_at: 1, checkpoint_json: None,
            staging_report_json: None, output_identity: None, output_files: Vec::new(), output_manifest_hash: None,
        }], ..DownloadsDocument::default()
    }).unwrap();
}

#[test]
fn persisted_queue_and_every_running_download_phase_block_recycle() {
    for phase in [DownloadPhase::Queued, DownloadPhase::Downloading, DownloadPhase::Verifying, DownloadPhase::Saving] {
        let mut f = fixture();
        queue(&f, phase);
        let request = selection_request(&f.ready);
        assert_eq!(f.service.recycle_confirmed(&f.store, &request, |_| panic!("active task prompted"), |_| panic!("active task reached host")).unwrap_err().code, "LIBRARY_ITEM_BUSY");
    }
}

#[cfg(windows)]
#[test]
fn reader_handle_blocks_recycle_and_target_lease_blocks_new_reader_and_content_writer() {
    let mut f = fixture();
    let request = selection_request(&f.ready);
    let reader = workbench_library::LocalReader::open(&f.store, &request.root_id, request.generation, &request.entry_id).unwrap();
    assert_eq!(f.service.recycle_confirmed(&f.store, &request, |_| panic!("active reader prompted"), |_| panic!("active reader reached host")).unwrap_err().code, "LIBRARY_ITEM_BUSY");
    drop(reader);
    let problem = f.service.recycle_confirmed(&f.store, &request, |_| true, |target| {
        assert!(workbench_library::LocalReader::open(&f.store, &request.root_id, request.generation, &request.entry_id).is_err());
        assert!(fs::OpenOptions::new().write(true).open(target.path()).is_err());
        assert!(fs::rename(f.media.path(), f.bin.path().join("moved-parent")).is_err());
        Err(StoreError { code: "SYNTHETIC_STOP" })
    }).unwrap_err();
    assert_eq!(problem.code, "SYNTHETIC_STOP");
}

#[cfg(unix)]
#[test]
fn symlink_replacement_and_root_replacement_are_rejected() {
    use std::os::unix::fs::symlink;
    let mut f = fixture();
    let request = selection_request(&f.ready);
    let file = f.media.path().join("合成 Selected.CBZ");
    let kept = f.bin.path().join("kept.cbz");
    fs::rename(&file, &kept).unwrap();
    symlink(&kept, &file).unwrap();
    assert!(f.service.recycle_confirmed(&f.store, &request, |_| panic!("link prompted"), |_| panic!("link reached host")).is_err());

    let root = f.media.path().to_path_buf();
    let moved = f.bin.path().join("old-root");
    fs::rename(&root, &moved).unwrap();
    fs::create_dir(&root).unwrap();
    assert_eq!(f.service.recycle_confirmed(&f.store, &request, |_| panic!("replaced root prompted"), |_| panic!("replaced root reached host")).unwrap_err().code, "LIBRARY_ROOT_CHANGED");
}


#[test]
fn replacement_at_recycled_path_does_not_inherit_old_manual_identity_or_admission_date() {
    let mut f = fixture();
    let mut document = f.store.read_library().unwrap();
    let original = document.value.records.iter_mut().find(|r| r.item.file_name == "合成 Selected.CBZ").unwrap();
    original.manual_override = true;
    original.item.source_ref = Some(LibraryReference { source: Source::Jm, work_id: "123".into() });
    original.item.identity_evidence = Some(LibraryEvidence::Manual);
    original.item.added_at = Some(1);
    f.store.write_library(document.revision, document.value).unwrap();
    let request = selection_request(&f.service.read(&f.store).unwrap());
    f.service.recycle_confirmed(&f.store, &request, |_| true, |target| {
        fs::rename(target.path(), f.bin.path().join("original.cbz")).unwrap();
        Ok(())
    }).unwrap().unwrap();
    archive(&f.media.path().join("合成 Selected.CBZ"));
    let started = f.service.scan(&f.store, &request.root_id, request.generation, ScanAction::Start).unwrap();
    let ready = finish(&mut f.service, &f.store, started);
    let replaced = selected(&ready);
    assert_eq!(replaced.state, LibraryItemState::Indexed);
    assert_eq!(replaced.source_ref, None);
    assert_ne!(replaced.added_at, Some(1));
    assert!(replaced.links.is_empty());
}

#[cfg(windows)]
#[test]
fn failed_metadata_write_after_recycling_returns_corrected_snapshot() {
    let mut f = fixture();
    let request = selection_request(&f.ready);
    let path = f.app.path().join(PRIVATE_DIRECTORY).join("library.json");
    let original = fs::read(&path).unwrap();
    let permissions = fs::metadata(&path).unwrap().permissions();
    let result = f.service.recycle_confirmed(&f.store, &request, |_| true, |target| {
        fs::rename(target.path(), f.bin.path().join("recoverable.cbz")).unwrap();
        let mut read_only = permissions.clone();
        read_only.set_readonly(true);
        fs::set_permissions(&path, read_only).unwrap();
        Ok(())
    });
    fs::set_permissions(&path, permissions).unwrap();
    let result = result.unwrap().unwrap();
    assert!(result.recycled);
    assert_eq!(result.error_code.as_deref(), Some("LIBRARY_RECYCLE_SAVE_FAILED"));
    assert_eq!(selected(&result.snapshot).error_code.as_deref(), Some("LIBRARY_RECYCLED"));
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn newly_queued_task_during_confirmation_is_rechecked_before_host_dispatch() {
    let mut f = fixture();
    let request = selection_request(&f.ready);
    let store = &f.store;
    let root = store.read_library().unwrap().value.root.unwrap();
    let problem = f.service.recycle_confirmed(store, &request, |_| {
        let value = serde_json::json!({
            "id": "b".repeat(64), "source": "JM", "origin": "manual", "revision": 1,
            "approvalRevision": 1, "targetHash": "c".repeat(64), "root": root,
            "generation": request.generation,
            "metadata": { "workId": "456", "title": "Synthetic queued", "authors": [], "tags": [], "description": null },
            "destination": "pending.zip", "jpegOutput": false, "zipOutput": true, "phase": "queued",
            "filesDone": 0, "filesTotal": null, "bytesDone": 0, "errorCode": null,
            "libraryEntryId": null, "updatedAt": 1, "checkpointJson": null, "stagingReportJson": null,
            "outputIdentity": null, "outputFiles": [], "outputManifestHash": null
        });
        let task: DownloadRecord = serde_json::from_value(value).unwrap();
        store.write_downloads(0, DownloadsDocument { tasks: vec![task], ..DownloadsDocument::default() }).unwrap();
        true
    }, |_| panic!("new queue admission reached host")).unwrap_err();
    assert_eq!(problem.code, "LIBRARY_ITEM_BUSY");
}
