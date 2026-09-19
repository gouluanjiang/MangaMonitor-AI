//! Synthetic receipts and files only; no live source or user media.
use super::*;

#[test]
fn ownership_requires_a_completed_same_source_receipt_and_current_files() {
    let f = fixture();
    assert!(f.service.inventory(&f.store).unwrap().items.is_empty());
    let completed = complete_for_presence(&f, record(&f));
    let before = f.store.read_downloads().unwrap();
    let index_before = f.store.read_library().unwrap();
    let owned = f.service.inventory(&f.store).unwrap();
    assert_eq!(owned.items.len(), 1);
    assert_eq!(owned.items[0].source, Source::Jm);
    assert_eq!(owned.items[0].work_id, completed.metadata.work_id);
    assert_eq!(owned.items[0].local_files, LocalFiles::Present);
    assert_eq!(f.store.read_downloads().unwrap(), before);
    assert_eq!(f.store.read_library().unwrap(), index_before);
    fs::remove_file(
        f.library
            .join(&completed.destination)
            .join("0001-123456/0001.gif"),
    )
    .unwrap();
    assert_eq!(
        f.service.inventory(&f.store).unwrap().items[0].local_files,
        LocalFiles::Incomplete
    );
    fs::remove_dir_all(f.library.join(&completed.destination)).unwrap();
    assert_eq!(
        f.service.inventory(&f.store).unwrap().items[0].local_files,
        LocalFiles::Missing
    );
}

#[test]
fn library_metadata_and_legacy_associations_never_become_receipts() {
    let f = fixture();
    let completed = complete_for_presence(&f, record(&f));
    let mut downloads = f.store.read_downloads().unwrap();
    downloads.value.tasks.clear();
    f.store
        .write_downloads(downloads.revision, downloads.value)
        .unwrap();
    let mut library = f.store.read_library().unwrap();
    assert_eq!(
        library.value.records[0]
            .item
            .source_ref
            .as_ref()
            .unwrap()
            .work_id,
        completed.metadata.work_id
    );
    library.value.records[0].manual_override = true;
    library.value.records[0].item.identity_evidence =
        Some(workbench_storage::LibraryEvidence::Manual);
    f.store
        .write_library(library.revision, library.value)
        .unwrap();
    assert!(f.service.inventory(&f.store).unwrap().items.is_empty());
}

#[test]
fn clearing_history_preserves_zip_ownership_without_accepting_missing_or_replaced_files() {
    let f = zip_fixture();
    let completed = complete_for_presence(&f, record(&f));
    f.service
        .remove_history(
            &f.store,
            &[TaskSelection {
                task_id: completed.id.clone(),
                expected_revision: completed.revision,
            }],
        )
        .unwrap();
    let saved = f.store.read_downloads().unwrap();
    assert!(saved.value.tasks.is_empty());
    assert_eq!(
        f.service.inventory(&f.store).unwrap().items[0].local_files,
        LocalFiles::Present
    );
    fs::remove_file(f.library.join(&completed.destination)).unwrap();
    assert_eq!(
        f.service.inventory(&f.store).unwrap().items[0].local_files,
        LocalFiles::Missing
    );
    fs::write(
        f.library.join(&completed.destination),
        b"replacement is not the recorded ZIP",
    )
    .unwrap();
    assert_eq!(
        f.service.inventory(&f.store).unwrap().items[0].local_files,
        LocalFiles::Incomplete
    );
    assert_eq!(f.store.read_downloads().unwrap(), saved);
}

#[test]
fn a_new_library_root_does_not_inherit_old_receipts() {
    let f = fixture();
    complete_for_presence(&f, record(&f));
    let other = f._temp.path().join("other-root");
    fs::create_dir(&other).unwrap();
    workbench_library::LibraryService::new()
        .choose(&f.store, &other)
        .unwrap();
    assert!(f.service.inventory(&f.store).unwrap().items.is_empty());
}

#[test]
fn reviewed_old_library_is_visible_without_creating_a_download_receipt() {
    use sha2::{Digest, Sha256};
    let f = zip_fixture();
    let completed = complete_for_presence(&f, record(&f));
    let mut downloads = f.store.read_downloads().unwrap();
    downloads.value.tasks.clear();
    let downloads = f
        .store
        .write_downloads(downloads.revision, downloads.value)
        .unwrap();
    assert!(f.service.inventory(&f.store).unwrap().items.is_empty());
    let library = f.store.read_library().unwrap();
    let bytes = fs::read(f.library.join(&completed.destination)).unwrap();
    let manifest = serde_json::to_vec(&serde_json::json!({
        "schemaVersion":1,"root":library.value.root,
        "items":[{"relativePath":completed.destination,"bytes":bytes.len(),
            "sha256":format!("{:x}",Sha256::digest(&bytes)),
            "references":[{"source":"Pica","workId":"0123456789abcdef01234567"}]}]
    }))
    .unwrap();
    workbench_library::import_reviewed_library(
        &f.store,
        &manifest,
        library.revision,
        &format!("{:x}", Sha256::digest(&manifest)),
    )
    .unwrap();
    let inventory = f.service.inventory(&f.store).unwrap();
    assert_eq!(inventory.items.len(), 1);
    assert_eq!(inventory.items[0].source, Source::Pica);
    assert_eq!(inventory.items[0].local_files, LocalFiles::Present);
    assert_eq!(f.store.read_downloads().unwrap(), downloads);
    fs::remove_file(f.library.join(&completed.destination)).unwrap();
    assert_eq!(
        f.service.inventory(&f.store).unwrap().items[0].local_files,
        LocalFiles::Missing
    );
}
