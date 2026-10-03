//! Linux diagnostic probes, explicitly armed by the isolated ENOSPC driver.
//! Uses generated GIFs and previously verified synthetic staging only.
use super::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

struct FaultControl {
    control: PathBuf,
    trace: PathBuf,
    trace_before: u64,
}
impl FaultControl {
    fn arm(f: &Fixture, prefix: &Path, permitted_bytes: u64) -> Self {
        let root = PathBuf::from(
            std::env::var_os("MANGAMONITOR_FAULT_ROOT").expect("fault driver required"),
        );
        assert!(f._temp.path().starts_with(&root));
        assert!(prefix.starts_with(f._temp.path()));
        assert_eq!(
            fs::read_to_string(root.join(".synthetic-fault-root")).unwrap(),
            "isolated synthetic ENOSPC probe\n"
        );
        assert_eq!(
            std::env::var("CI").as_deref(),
            Ok("true"),
            "live source execution must remain forbidden"
        );
        let control = PathBuf::from(std::env::var_os("MANGAMONITOR_FAULT_CONTROL").unwrap());
        let trace = PathBuf::from(std::env::var_os("MANGAMONITOR_FAULT_TRACE").unwrap());
        let trace_before = fs::metadata(&trace).map_or(0, |m| m.len());
        fs::write(
            &control,
            format!("{}/\n{}\n", prefix.display(), permitted_bytes),
        )
        .unwrap();
        Self {
            control,
            trace,
            trace_before,
        }
    }
    fn disarm(&self) {
        if self.control.exists() {
            fs::remove_file(&self.control).unwrap();
        }
    }
    fn assert_fired(&self) {
        assert!(
            fs::metadata(&self.trace).unwrap().len() > self.trace_before,
            "no actual ENOSPC injection was observed"
        );
    }
}
impl Drop for FaultControl {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.control);
    }
}

fn preserve_on_failure(f: Fixture, result: std::thread::Result<()>) {
    if let Err(problem) = result {
        eprintln!(
            "PRESERVED_SYNTHETIC_FAULT_ROOT={}",
            f._temp.path().display()
        );
        std::mem::forget(f);
        std::panic::resume_unwind(problem);
    }
}

fn download_probe(zip: bool, permitted_bytes: u64) {
    let f = if zip { zip_fixture() } else { fixture() };
    seed_report(&f);
    let before = record(&f);
    let library_before = f.store.read_library().unwrap();
    let unrelated = f.library.join("unrelated-synthetic.txt");
    fs::write(&unrelated, b"unrelated synthetic bytes must survive").unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = catch_unwind(AssertUnwindSafe(|| {
        runtime.block_on(async {
            let fault = FaultControl::arm(&f, &f.library, permitted_bytes);
            let failure = f.service.run(&f.store, &f.id, || Ok(())).await;
            fault.disarm();
            fault.assert_fired();
            assert!(
                failure.is_err(),
                "a short/failed output write was reported successful"
            );
            let failed = record(&f);
            assert_eq!(failed.phase, DownloadPhase::Error);
            assert!(failed.error_code.is_some());
            assert!(failed.output_manifest_hash.is_none());
            assert_eq!(failed.staging_report_json, before.staging_report_json);
            assert_eq!(f.store.read_library().unwrap(), library_before);
            assert_eq!(
                fs::read(&unrelated).unwrap(),
                b"unrelated synthetic bytes must survive"
            );
            assert!(f.service.read(&f.store).unwrap().tasks[0]
                .allowed_actions
                .contains(&Control::Retry));
            drop(
                f.store
                    .open_download_workspace()
                    .expect("failed worker did not release its workspace"),
            );
            f.service
                .control(&f.store, &f.id, failed.revision, Control::Retry)
                .unwrap();
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
                .find(|entry| entry.relative_path == receipt.relative_path)
                .unwrap();
            let completed = f
                .service
                .mark_indexed(&f.store, &receipt, &entry.id)
                .unwrap();
            assert_eq!(completed.tasks[0].phase, DownloadPhase::Downloaded);
            assert_eq!(
                fs::read(&unrelated).unwrap(),
                b"unrelated synthetic bytes must survive"
            );
            assert_eq!(f.store.read_phone_library().unwrap().revision, 0);
            println!("SYNTHETIC_ENOSPC_RECOVERED zip={zip} permitted_bytes={permitted_bytes}");
        })
    }));
    preserve_on_failure(f, result);
}

#[test]
#[ignore = "requires the isolated Linux ENOSPC driver; no live source execution"]
fn synthetic_enospc_zip_zero_bytes_recovers_only_on_explicit_retry() {
    download_probe(true, 0);
}

#[test]
#[ignore = "requires the isolated Linux ENOSPC driver; no live source execution"]
fn synthetic_enospc_zip_partial_write_recovers_only_on_explicit_retry() {
    download_probe(true, 32);
}

#[test]
#[ignore = "requires the isolated Linux ENOSPC driver; no live source execution"]
fn synthetic_enospc_directory_zero_bytes_recovers_only_on_explicit_retry() {
    download_probe(false, 0);
}

#[test]
#[ignore = "requires the isolated Linux ENOSPC driver; no live source execution"]
fn synthetic_enospc_directory_partial_write_recovers_only_on_explicit_retry() {
    download_probe(false, 32);
}

#[test]
#[ignore = "requires the isolated Linux ENOSPC driver; no live source execution"]
fn synthetic_enospc_preferences_preserve_committed_bytes_and_reopen() {
    let f = zip_fixture();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let initial = f
            .store
            .write_preferences(0, workbench_storage::WorkbenchPreferences::default())
            .unwrap();
        let private_root = f._temp.path().join("private");
        let private = private_root.join(workbench_storage::PRIVATE_DIRECTORY);
        let prefs_path = private.join("preferences.json");
        let before = fs::read(&prefs_path).unwrap();
        let library_before = fs::read(private.join("library.json")).unwrap();
        let downloads_before = fs::read(private.join("downloads.json")).unwrap();
        let fault = FaultControl::arm(&f, &private, 16);
        let failed = f
            .store
            .write_preferences(initial.revision, initial.value.clone());
        fault.disarm();
        fault.assert_fired();
        assert!(failed.is_err());
        assert_eq!(fs::read(&prefs_path).unwrap(), before);
        assert_eq!(
            fs::read(private.join("library.json")).unwrap(),
            library_before
        );
        assert_eq!(
            fs::read(private.join("downloads.json")).unwrap(),
            downloads_before
        );
        let reopened = WorkbenchStore::open(&private_root).unwrap();
        assert_eq!(reopened.read_preferences().unwrap(), initial);
        let saved = reopened
            .write_preferences(initial.revision, initial.value.clone())
            .unwrap();
        assert_eq!(saved.revision, initial.revision + 1);
        assert_eq!(saved.value, initial.value);
        println!("SYNTHETIC_ENOSPC_STORAGE_PRESERVED_AND_RECOVERED");
    }));
    preserve_on_failure(f, result);
}
