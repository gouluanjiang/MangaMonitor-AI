//! Ignored, CI-compiled Windows probes. Synthetic media and explicitly marked
//! private test roots only; no account client, real request, or real-library path.
use super::*;
use crate::{windows_validation_io, windows_validation_support as support};
use std::{
    os::windows::fs::OpenOptionsExt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

fn synthetic_fixture() -> Fixture {
    let temp = support::new_case("download");
    support::verify_case(temp.path());
    let library = temp.path().join("合成 日本語 library");
    fs::create_dir(&library).unwrap();
    let store = WorkbenchStore::open(temp.path().join("private")).unwrap();
    let mut indexer = workbench_library::LibraryService::new();
    indexer.choose(&store, &library).unwrap();
    let mut lib = store.read_library().unwrap();
    lib.value.phase = LibraryPhase::Complete;
    let lib = store.write_library(lib.revision, lib.value).unwrap();
    let service = DownloadService::new();
    let plan = service
        .prepare(
            &store,
            &lib.value.root.unwrap().id,
            lib.value.generation,
            JmDownloadMetadata {
                work_id: "123456".into(),
                title: "SYNTHETIC Windows validation".into(),
                authors: vec!["Synthetic author".into()],
                tags: vec!["test".into()],
                description: None,
                version_updated_at: None,
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

fn assert_not_downloaded(f: &Fixture) {
    assert_ne!(record(f).phase, DownloadPhase::Downloaded);
    assert!(f.store.read_library().unwrap().value.records.is_empty());
    assert_eq!(f.store.read_phone_library().unwrap().revision, 0);
}

fn index_synthetic(f: &Fixture, receipt: &AwaitingIndexReceipt) {
    let snapshot = workbench_library::LibraryService::new()
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
    let entry = snapshot
        .items
        .iter()
        .find(|v| v.relative_path == receipt.relative_path)
        .unwrap();
    assert_eq!(
        f.service
            .mark_indexed(&f.store, receipt, &entry.id)
            .unwrap()
            .tasks[0]
            .phase,
        DownloadPhase::Downloaded
    );
    assert!(f.library.join(&receipt.relative_path).is_file());
}

#[tokio::test]
#[ignore = "requires an explicitly marked synthetic Windows validation root"]
async fn pause_cancel_retry_and_registration_are_separate() {
    let f = synthetic_fixture();
    seed_report(&f);
    // Pause/cancel the admitted media attempt before its worker starts. Reopening
    // the ledger must not restart it, and resume is revision-bound and explicit.
    let paused = f
        .service
        .control(&f.store, &f.id, 1, Control::Pause)
        .unwrap();
    assert_eq!(paused.tasks[0].phase, DownloadPhase::Paused);
    assert_not_downloaded(&f);
    let before = fs::read(downloads_path(&f)).unwrap();
    assert_eq!(
        DownloadService::new().read(&f.store).unwrap().tasks[0].phase,
        DownloadPhase::Paused
    );
    assert_eq!(fs::read(downloads_path(&f)).unwrap(), before);
    f.service
        .control(&f.store, &f.id, record(&f).revision, Control::Resume)
        .unwrap();
    let receipt = f
        .service
        .run(&f.store, &f.id, || Ok(()))
        .await
        .unwrap()
        .unwrap();
    assert_not_downloaded(&f);
    // A failed registration retains the finished synthetic ZIP; retry reuses it
    // without a media client. This is the same production DownloadService path.
    let output = f.library.join(&receipt.relative_path);
    let bytes = fs::read(&output).unwrap();
    f.service.index_failed(&f.store, &receipt, "BUSY").unwrap();
    assert_not_downloaded(&f);
    f.service
        .control(&f.store, &f.id, record(&f).revision, Control::Retry)
        .unwrap();
    let retry = f
        .service
        .run(&f.store, &f.id, || Ok(()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fs::read(&output).unwrap(), bytes);
    index_synthetic(&f, &retry);
    println!("WINDOWS_VALIDATION download-pause-retry: no network; completion only after exact library registration");
}

#[tokio::test]
#[ignore = "requires an explicitly marked synthetic Windows validation root"]
async fn write_denied_and_disk_full_keep_partial_output_and_retry() {
    for (code, prefix) in [(5, 0), (112, 64)] {
        let f = synthetic_fixture();
        seed_report(&f);
        let fault = Mutex::new(None);
        let armed = AtomicBool::new(false);
        let result = f
            .service
            .run(&f.store, &f.id, || {
                if !armed.load(Ordering::SeqCst) {
                    if let Some(identity) = record(&f).output_identity {
                        *fault.lock().unwrap() = Some(windows_validation_io::arm(
                            f._temp.path(),
                            identity,
                            prefix,
                            code,
                        ));
                        armed.store(true, Ordering::SeqCst);
                    }
                }
                Ok(())
            })
            .await;
        assert!(
            armed.load(Ordering::SeqCst),
            "fault must target the final synthetic output handle"
        );
        assert_eq!(result.unwrap_err().code, "DOWNLOAD_WRITE_FAILED");
        drop(fault.into_inner().unwrap());
        let failed = record(&f);
        assert_eq!(failed.phase, DownloadPhase::Error);
        assert!(failed.output_manifest_hash.is_none());
        let output = f.library.join(&failed.destination);
        assert_eq!(fs::metadata(&output).unwrap().len(), prefix as u64);
        assert_not_downloaded(&f);
        f.service
            .control(&f.store, &f.id, failed.revision, Control::Retry)
            .unwrap();
        let receipt = f
            .service
            .run(&f.store, &f.id, || Ok(()))
            .await
            .unwrap()
            .unwrap();
        assert_not_downloaded(&f);
        index_synthetic(&f, &receipt);
        println!("WINDOWS_VALIDATION download-write code={code} partial_bytes={prefix}: retained, explicit retry and registration succeeded");
    }
}

#[test]
#[ignore = "requires an explicitly marked synthetic Windows validation root"]
fn existing_name_and_locked_partial_file_are_preserved() {
    let f = synthetic_fixture();
    let mut task = record(&f);
    let (stage, proof) = report(&f, &task);
    let output = f.library.join(&task.destination);
    fs::write(&output, b"synthetic unrelated pre-existing output").unwrap();
    let before = fs::read(&output).unwrap();
    assert!(materialize::save(&mut task, &proof, &stage, &|| Ok(()), &mut |_| Ok(())).is_err());
    assert_eq!(fs::read(&output).unwrap(), before);
    assert_not_downloaded(&f);
    // Separate fixture: do not remove or overwrite the conflicting file.
    let f = synthetic_fixture();
    let mut task = record(&f);
    let (stage, proof) = report(&f, &task);
    let pause = Cell::new(false);
    assert!(materialize::save(
        &mut task,
        &proof,
        &stage,
        &|| if pause.get() {
            Err(error("DOWNLOAD_PAUSED"))
        } else {
            Ok(())
        },
        &mut |v| {
            if v.output_identity.is_some() {
                pause.set(true);
            }
            Ok(())
        }
    )
    .is_err());
    let output = f.library.join(&task.destination);
    assert!(output.is_file());
    assert_not_downloaded(&f);
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&output)
        .unwrap();
    let error =
        materialize::save(&mut task, &proof, &stage, &|| Ok(()), &mut |_| Ok(())).unwrap_err();
    assert_eq!(error.code, "DOWNLOAD_FILE_UNAVAILABLE");
    assert!(task.output_manifest_hash.is_none());
    assert_not_downloaded(&f);
    drop(lock);
    materialize::save(&mut task, &proof, &stage, &|| Ok(()), &mut |_| Ok(())).unwrap();
    materialize::verify_output(&task).unwrap();
    assert_not_downloaded(&f);
    println!("WINDOWS_VALIDATION download-conflict-lock: unrelated bytes unchanged, locked partial retained, unlock recovers without automatic admission");
}

#[tokio::test]
#[ignore = "requires an explicitly marked synthetic Windows validation root"]
async fn partial_fetch_cancel_and_invalid_bytes_never_complete() {
    let f = synthetic_fixture();
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
                return Err("SYNTHETIC_CANCEL".into());
            }
            *saved.borrow_mut() = Some(cp.clone());
            Ok(())
        },
    )
    .await
    .unwrap_err();
    assert_eq!(failure, "SYNTHETIC_CANCEL");
    assert_not_downloaded(&f);
    let checkpoint = saved.borrow().clone().unwrap();
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
    assert_not_downloaded(&f);
    // New private roots for each invalid response; an HTML error body or a
    // truncated image cannot be promoted merely because metadata said GIF.
    for bytes in [
        b"<html>synthetic failure</html>".to_vec(),
        gif()[..5].to_vec(),
    ] {
        let f = synthetic_fixture();
        let c = core(&record(&f));
        let workspace = f.store.open_download_workspace().unwrap();
        let result = isolated_staging_execution::execute_resumable_with_fetcher(
            context(workspace.path(), &c),
            None,
            |d| {
                let bytes = bytes.clone();
                async move {
                    let mut value = processed(d);
                    value.bytes = bytes;
                    Ok(value)
                }
            },
            || Ok(c.authorization.clone()),
            |_| Ok(()),
        )
        .await;
        assert!(result.is_err());
        assert_not_downloaded(&f);
        assert!(fs::read_dir(&f.library).unwrap().next().is_none());
    }
    for (incoming, expected) in [
        ("HTTP_401", "SESSION_EXPIRED"),
        ("PICA_MEDIA_HTTP_401", "DOWNLOAD_MEDIA_ACCESS_DENIED"),
        ("PICA_MEDIA_HTTP_429", "DOWNLOAD_MEDIA_RATE_LIMITED"),
    ] {
        assert_eq!(classify(incoming).code, expected);
    }
    println!("WINDOWS_VALIDATION download-fetch: injected fetcher cancellation, truncated/invalid bytes, error classification; HTTP transport and Retry-After are not tested");
}
