//! Synthetic container tests. No network, real source or user-library access.
use super::*;
use std::io::{Read, Write};

#[test]
fn new_jm_zip_contains_original_jpeg_gif_metadata_and_registers_without_prefix() {
    let f = zip_fixture();
    let mut task = record(&f);
    assert!(task.zip_output);
    assert_eq!(task.destination, "[Example author] Offline example.zip");
    let jpg = static_image(image::ImageFormat::Jpeg);
    let gif = gif();
    let (stage, report) =
        report_with_images(&f, &task, &[("jpg", jpg.clone()), ("gif", gif.clone())]);
    materialize::save(&mut task, &report, &stage, &|| Ok(()), &mut |_| Ok(())).unwrap();
    let output = f.library.join(&task.destination);
    assert!(output.is_file());
    let mut zip = zip::ZipArchive::new(fs::File::open(&output).unwrap()).unwrap();
    for (name, expected) in [("0001-123456/0001.jpg", jpg), ("0001-123456/0002.gif", gif)] {
        let mut actual = vec![];
        let mut entry = zip.by_name(name).unwrap();
        assert_eq!(entry.compression(), zip::CompressionMethod::Stored);
        entry.read_to_end(&mut actual).unwrap();
        assert_eq!(actual, expected);
    }
    let metadata: serde_json::Value =
        serde_json::from_reader(zip.by_name("元数据.json").unwrap()).unwrap();
    assert_eq!(metadata["id"], 123456);
    assert_eq!(metadata["name"], task.metadata.title);
    drop(zip);
    let indexed = workbench_library::LibraryService::new()
        .register_completed(
            &f.store,
            &task.root.id,
            task.generation,
            &task.destination,
            &workbench_storage::LibraryReference {
                source: Source::Jm,
                work_id: "123456".into(),
            },
            2,
        )
        .unwrap();
    assert_eq!(indexed.items[0].page_count, Some(2));
    assert_eq!(
        indexed.items[0].format,
        workbench_storage::LibraryFormat::Zip
    );
    assert_eq!(indexed.items[0].error_code, None);
    assert!(indexed.items[0].cover_available);
    materialize::verify_output(&task).unwrap();
    assert_eq!(fs::read_dir(&f.library).unwrap().count(), 1);
}

#[test]
fn completed_zip_presence_deletion_and_new_confirmation_preserve_history() {
    let f = zip_fixture();
    let completed = complete_for_presence(&f, record(&f));
    assert_eq!(
        f.service.read(&f.store).unwrap().tasks[0].local_files,
        Some(LocalFiles::Present)
    );
    assert!(prepare_same(&f).is_err());
    fs::remove_file(f.library.join(&completed.destination)).unwrap();
    assert_eq!(
        f.service.read(&f.store).unwrap().tasks[0].local_files,
        Some(LocalFiles::Missing)
    );
    let next = prepare_same(&f).unwrap();
    f.service
        .confirm(&f.store, &next.plan_id, next.revision)
        .unwrap();
    assert_eq!(f.store.read_downloads().unwrap().value.tasks[0], completed);
    assert!(f.store.read_library().unwrap().value.records.is_empty());
    assert!(!f.library.join(&completed.destination).exists());
}

#[test]
fn existing_destination_and_changed_zip_never_become_verified_outputs() {
    let f = zip_fixture();
    let mut task = record(&f);
    let (stage, report) = report(&f, &task);
    let output = f.library.join(&task.destination);
    fs::write(&output, b"unrelated existing file").unwrap();
    assert!(materialize::save(&mut task, &report, &stage, &|| Ok(()), &mut |_| Ok(())).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"unrelated existing file");
    fs::remove_file(&output).unwrap();
    materialize::save(&mut task, &report, &stage, &|| Ok(()), &mut |_| Ok(())).unwrap();
    let mut changed = fs::OpenOptions::new().write(true).open(&output).unwrap();
    changed.write_all(b"not a valid ZIP header").unwrap();
    drop(changed);
    assert!(materialize::verify_output(&task).is_err());
}

#[test]
fn zip_pause_keeps_resumable_images_and_resumes_only_its_own_exact_output() {
    let f = zip_fixture();
    let mut task = record(&f);
    let (stage, report) = report(&f, &task);
    let paused = Cell::new(false);
    let require = || {
        if paused.get() {
            Err(error("DOWNLOAD_PAUSED"))
        } else {
            Ok(())
        }
    };
    let result = materialize::save(&mut task, &report, &stage, &require, &mut |value| {
        if value.output_identity.is_some() {
            paused.set(true);
        }
        Ok(())
    });
    assert!(result.is_err());
    let proof = task.archive_file.clone();
    paused.set(false);
    materialize::save(&mut task, &report, &stage, &require, &mut |_| Ok(())).unwrap();
    assert_eq!(task.archive_file, proof);
    materialize::verify_output(&task).unwrap();
}

#[test]
fn changing_container_profile_without_new_approval_is_rejected() {
    let f = zip_fixture();
    let mut task = record(&f);
    let original = task.target_hash.clone();
    task.zip_output = false;
    assert_ne!(binding(&task).unwrap(), original);
    put(&f, task);
    assert!(DownloadService::new().read(&f.store).is_err());
}

#[test]
fn explicit_zip_mapping_preserves_old_receipt_and_projects_current_history_path() {
    let f = fixture();
    let completed = complete_for_presence(&f, record(&f));
    let mut indexer = workbench_library::LibraryService::new();
    let original_index = f.store.read_library().unwrap();
    let old_id = original_index.value.records[0].item.id.clone();
    indexer
        .link(
            &f.store,
            &completed.root.id,
            completed.generation,
            &old_id,
            Some(workbench_storage::LibraryReference {
                source: Source::Jm,
                work_id: completed.metadata.work_id.clone(),
            }),
        )
        .unwrap();
    let before = f.store.read_downloads().unwrap();
    let destination = "[Example author] Migrated.zip";
    let output = f.library.join(destination);
    let mut zip = zip::ZipWriter::new(fs::File::create(&output).unwrap());
    let mut names: Vec<_> = completed
        .output_files
        .iter()
        .map(|v| v.relative_path.clone())
        .collect();
    names.push("_mangamonitor.json".into());
    for name in names {
        zip.start_file(
            &name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(&fs::read(f.library.join(&completed.destination).join(&name)).unwrap())
            .unwrap();
    }
    zip.finish().unwrap();
    let bytes = fs::read(&output).unwrap();
    let mapping = serde_json::to_vec(&serde_json::json!({"schema_version":1,"library":f.library,
        "items":[{"old_source":f.library.join(&completed.destination),"zip":output,"output_sha256":hash(&bytes)}]})).unwrap();
    // Originals must really be absent; import never moves or removes them.
    assert!(indexer
        .import_paths(&f.store, &completed.root.id, completed.generation, &mapping)
        .is_err());
    assert_eq!(f.store.read_downloads().unwrap(), before);
    fs::remove_dir_all(f.library.join(&completed.destination)).unwrap();
    let applied = indexer
        .import_paths(&f.store, &completed.root.id, completed.generation, &mapping)
        .unwrap();
    assert_eq!(applied.mapped, 1);
    assert_eq!(applied.snapshot.items.len(), 1);
    assert_eq!(
        applied.snapshot.items[0].identity_evidence,
        Some(workbench_storage::LibraryEvidence::Manual)
    );
    assert_ne!(applied.snapshot.items[0].id, old_id);
    assert_eq!(
        f.store.read_downloads().unwrap(),
        before,
        "immutable old receipt is untouched"
    );
    let view = f.service.read(&f.store).unwrap();
    assert_eq!(view.tasks[0].local_files, Some(LocalFiles::Present));
    assert!(view.tasks[0].destination_display.ends_with(destination));
    assert_eq!(
        view.tasks[0].library_entry_id.as_ref(),
        Some(&applied.snapshot.items[0].id)
    );
    assert!(prepare_same(&f).is_err());
    fs::remove_file(&output).unwrap();
    assert_eq!(
        f.service.read(&f.store).unwrap().tasks[0].local_files,
        Some(LocalFiles::Missing)
    );
    assert_eq!(f.store.read_downloads().unwrap(), before);
}
