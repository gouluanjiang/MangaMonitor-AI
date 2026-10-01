use crate::{
    downloads::DesktopDownloads,
    library::{with_library, DesktopLibrary},
    require_main, DesktopStore,
};
use std::sync::Arc;
use tauri::{AppHandle, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use workbench_library::{LibraryRecycleRequest, LibraryRecycleResult, VerifiedRecycleTarget};
use workbench_storage::StoreError;

fn error(code: &'static str) -> StoreError {
    StoreError { code }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn library_recycle<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    library: State<'_, Arc<DesktopLibrary>>,
    store: State<'_, Arc<DesktopStore>>,
    downloads: State<'_, Arc<DesktopDownloads>>,
    root_id: String,
    generation: u64,
    entry_id: String,
    expected_revision: u64,
) -> Result<Option<LibraryRecycleResult>, StoreError> {
    require_main(window.label())?;
    let downloads = Arc::clone(downloads.inner());
    with_library(
        Arc::clone(library.inner()),
        Arc::clone(store.inner()),
        move |service, store| {
            // Try, never wait while holding the library mutex: a download driver
            // may need that mutex to finish registration. The owned guard spans
            // confirmation, the Shell thread and the final metadata write.
            let _exclusive = downloads.try_library_recycle_guard()?;
            service.recycle_confirmed(
                store,
                &LibraryRecycleRequest { root_id, generation, entry_id, expected_revision },
                |preview| app.dialog()
                    .message(format!(
                        "将这一本漫画移到 Windows 回收站？\n\n{}\n{}\n\n可以在回收站恢复；不支持回收时会停止。阅读进度和漫画登记资料将保留。",
                        preview.title, preview.relative_path
                    ))
                    .title("删除漫画")
                    .parent(&window)
                    .kind(MessageDialogKind::Warning)
                    .buttons(MessageDialogButtons::OkCancel)
                    .blocking_show(),
                native_recycle,
            )
        },
    ).await
}

#[cfg(not(windows))]
fn native_recycle(_target: Arc<VerifiedRecycleTarget>) -> Result<(), StoreError> {
    Err(error("LIBRARY_RECYCLE_UNSUPPORTED"))
}

#[cfg(windows)]
fn native_recycle(target: Arc<VerifiedRecycleTarget>) -> Result<(), StoreError> {
    // IFileOperation requires STA. A dedicated thread cannot inherit a Tokio
    // blocking worker's incompatible apartment. COM objects stay on this thread.
    std::thread::Builder::new()
        .name("library-recycle".into())
        .spawn(move || shell::recycle(target))
        .map_err(|_| error("LIBRARY_RECYCLE_UNAVAILABLE"))?
        .join()
        .map_err(|_| error("LIBRARY_RECYCLE_RESULT_UNCERTAIN"))?
}

#[cfg(windows)]
mod shell {
    use super::*;
    use std::{os::windows::ffi::OsStrExt, path::PathBuf, sync::Mutex};
    use windows::{
        core::{implement, Ref, HRESULT, PCWSTR},
        Win32::{
            Foundation::E_ABORT,
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
            },
            UI::Shell::{
                FileOperation, IFileOperation, IFileOperationProgressSink,
                IFileOperationProgressSink_Impl, IShellItem, SHCreateItemFromParsingName,
                FOFX_ADDUNDORECORD, FOFX_EARLYFAILURE, FOFX_NOCOPYHOOKS, FOFX_RECYCLEONDELETE,
                FOF_NORECURSEREPARSE, FOF_NORECURSION, FOF_NO_CONNECTED_ELEMENTS, FOF_NO_UI,
                SIGDN_FILESYSPATH, TSF_DELETE_RECYCLE_IF_POSSIBLE,
            },
        },
    };

    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            // Created only after successful initialization on this STA thread.
            unsafe { CoUninitialize() };
        }
    }

    #[derive(Default)]
    struct Proof {
        pre_delete: bool,
        post_delete: bool,
        recycled: bool,
        error: Option<&'static str>,
    }

    impl Proof {
        fn before(&mut self, flags: u32, matches: bool) -> Result<(), StoreError> {
            let problem = if flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 == 0 {
                Some("LIBRARY_RECYCLE_UNAVAILABLE")
            } else if !matches || self.pre_delete {
                Some("LIBRARY_FILE_CHANGED")
            } else {
                None
            };
            if let Some(code) = problem {
                self.error = Some(code);
                return Err(error(code));
            }
            self.pre_delete = true;
            Ok(())
        }

        fn after(
            &mut self,
            flags: u32,
            succeeded: bool,
            same_recycled_file: bool,
        ) -> Result<(), StoreError> {
            self.post_delete = true;
            self.recycled = self.pre_delete
                && self.error.is_none()
                && succeeded
                && flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 != 0
                && same_recycled_file;
            if !self.recycled {
                self.error.get_or_insert("LIBRARY_RECYCLE_RESULT_UNCERTAIN");
                return Err(error(
                    self.error.unwrap_or("LIBRARY_RECYCLE_RESULT_UNCERTAIN"),
                ));
            }
            Ok(())
        }

        fn outcome(&self, completed: bool, aborted: bool) -> Result<(), StoreError> {
            if let Some(code) = self.error {
                return Err(error(code));
            }
            if completed && !aborted && self.pre_delete && self.post_delete && self.recycled {
                Ok(())
            } else {
                Err(error("LIBRARY_RECYCLE_NOT_COMPLETED"))
            }
        }
    }

    #[implement(IFileOperationProgressSink)]
    struct RecycleSink {
        target: Arc<VerifiedRecycleTarget>,
        proof: Arc<Mutex<Proof>>,
    }

    fn abort() -> windows::core::Error {
        windows::core::Error::from_hresult(E_ABORT)
    }

    fn shell_path(item: &IShellItem) -> windows::core::Result<PathBuf> {
        // GetDisplayName allocates with the COM task allocator. Copy while the
        // pointer is live, then free on both valid and invalid UTF-16 paths.
        let value = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH)? };
        let copied = unsafe { value.to_string() };
        unsafe { CoTaskMemFree(Some(value.0.cast())) };
        copied.map(PathBuf::from)
    }

    #[allow(non_snake_case)]
    impl IFileOperationProgressSink_Impl for RecycleSink_Impl {
        fn StartOperations(&self) -> windows::core::Result<()> {
            Ok(())
        }
        fn FinishOperations(&self, _result: HRESULT) -> windows::core::Result<()> {
            Ok(())
        }
        fn PreDeleteItem(
            &self,
            flags: u32,
            item: Ref<'_, IShellItem>,
        ) -> windows::core::Result<()> {
            let matches = item
                .as_ref()
                .and_then(|item| shell_path(item).ok())
                .and_then(|path| std::fs::canonicalize(path).ok())
                .is_some_and(|path| path == self.target.path())
                && self.target.verify().is_ok();
            self.proof
                .lock()
                .map_err(|_| abort())?
                .before(flags, matches)
                .map_err(|_| abort())
        }
        fn PostDeleteItem(
            &self,
            flags: u32,
            _item: Ref<'_, IShellItem>,
            result: HRESULT,
            recycled: Ref<'_, IShellItem>,
        ) -> windows::core::Result<()> {
            // Microsoft documents this pointer as the Recycle Bin item; NULL
            // means permanent deletion. Never accept NULL or a different file.
            let same_file = recycled
                .as_ref()
                .and_then(|item| shell_path(item).ok())
                .is_some_and(|path| self.target.matches_recycled_file(&path).unwrap_or(false));
            self.proof
                .lock()
                .map_err(|_| abort())?
                .after(flags, result.is_ok(), same_file)
                .map_err(|_| abort())
        }
        fn PreRenameItem(
            &self,
            _: u32,
            _: Ref<'_, IShellItem>,
            _: &PCWSTR,
        ) -> windows::core::Result<()> {
            Err(abort())
        }
        fn PostRenameItem(
            &self,
            _: u32,
            _: Ref<'_, IShellItem>,
            _: &PCWSTR,
            _: HRESULT,
            _: Ref<'_, IShellItem>,
        ) -> windows::core::Result<()> {
            Err(abort())
        }
        fn PreMoveItem(
            &self,
            _: u32,
            _: Ref<'_, IShellItem>,
            _: Ref<'_, IShellItem>,
            _: &PCWSTR,
        ) -> windows::core::Result<()> {
            Err(abort())
        }
        fn PostMoveItem(
            &self,
            _: u32,
            _: Ref<'_, IShellItem>,
            _: Ref<'_, IShellItem>,
            _: &PCWSTR,
            _: HRESULT,
            _: Ref<'_, IShellItem>,
        ) -> windows::core::Result<()> {
            Err(abort())
        }
        fn PreCopyItem(
            &self,
            _: u32,
            _: Ref<'_, IShellItem>,
            _: Ref<'_, IShellItem>,
            _: &PCWSTR,
        ) -> windows::core::Result<()> {
            Err(abort())
        }
        fn PostCopyItem(
            &self,
            _: u32,
            _: Ref<'_, IShellItem>,
            _: Ref<'_, IShellItem>,
            _: &PCWSTR,
            _: HRESULT,
            _: Ref<'_, IShellItem>,
        ) -> windows::core::Result<()> {
            Err(abort())
        }
        fn PreNewItem(
            &self,
            _: u32,
            _: Ref<'_, IShellItem>,
            _: &PCWSTR,
        ) -> windows::core::Result<()> {
            Err(abort())
        }
        fn PostNewItem(
            &self,
            _: u32,
            _: Ref<'_, IShellItem>,
            _: &PCWSTR,
            _: &PCWSTR,
            _: u32,
            _: HRESULT,
            _: Ref<'_, IShellItem>,
        ) -> windows::core::Result<()> {
            Err(abort())
        }
        fn UpdateProgress(&self, _: u32, _: u32) -> windows::core::Result<()> {
            Ok(())
        }
        fn ResetTimer(&self) -> windows::core::Result<()> {
            Ok(())
        }
        fn PauseTimer(&self) -> windows::core::Result<()> {
            Ok(())
        }
        fn ResumeTimer(&self) -> windows::core::Result<()> {
            Ok(())
        }
    }

    pub(super) fn recycle(target: Arc<VerifiedRecycleTarget>) -> Result<(), StoreError> {
        target.verify()?;
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|_| error("LIBRARY_RECYCLE_UNAVAILABLE"))?;
        let _apartment = Apartment;
        let wide: Vec<u16> = dunce::simplified(target.path())
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let proof = Arc::new(Mutex::new(Proof::default()));
        let sink: IFileOperationProgressSink = RecycleSink {
            target: Arc::clone(&target),
            proof: Arc::clone(&proof),
        }
        .into();
        // No raw COM vtables, no legacy ALLOWUNDO fallback, no recursive or
        // connected-item deletion, and no silent permanent-delete fallback.
        let operation: IFileOperation =
            unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER) }
                .map_err(|_| error("LIBRARY_RECYCLE_UNAVAILABLE"))?;
        unsafe {
            operation
                .SetOperationFlags(
                    FOFX_RECYCLEONDELETE
                        | FOFX_ADDUNDORECORD
                        | FOFX_EARLYFAILURE
                        | FOFX_NOCOPYHOOKS
                        | FOF_NO_UI
                        | FOF_NORECURSION
                        | FOF_NORECURSEREPARSE
                        | FOF_NO_CONNECTED_ELEMENTS,
                )
                .map_err(|_| error("LIBRARY_RECYCLE_UNAVAILABLE"))?;
        }
        let item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }
            .map_err(|_| error("LIBRARY_RECYCLE_UNAVAILABLE"))?;
        target.verify()?;
        unsafe { operation.DeleteItem(&item, &sink) }
            .map_err(|_| error("LIBRARY_RECYCLE_UNAVAILABLE"))?;
        let completed = unsafe { operation.PerformOperations() }.is_ok();
        let aborted = unsafe { operation.GetAnyOperationsAborted() }
            .map(|value| value.as_bool())
            .unwrap_or(true);
        let proof = proof
            .lock()
            .map_err(|_| error("LIBRARY_RECYCLE_RESULT_UNCERTAIN"))?;
        proof.outcome(completed, aborted)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn native_shell_recycles_only_a_generated_temporary_zip_and_proves_bin_identity() {
            // Windows CI exercises the actual adapter with an independently
            // created tiny ZIP. No real library path or account is accepted.
            // The small synthetic ZIP may remain in the runner's Recycle Bin.
            use std::io::Write;
            let temp = tempfile::tempdir().unwrap();
            let media = temp.path().join("media");
            std::fs::create_dir(&media).unwrap();
            let file = media.join("synthetic-native-recycle.zip");
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&file).unwrap());
            zip.start_file("1.jpg", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"synthetic-index-only-page").unwrap();
            zip.finish().unwrap();
            let store = workbench_storage::WorkbenchStore::open(temp.path().join("app")).unwrap();
            let mut service = workbench_library::LibraryService::new();
            let mut ready = service.choose(&store, &media).unwrap();
            for _ in 0..30 {
                if ready.phase != workbench_library::LibraryPhase::Reading {
                    break;
                }
                ready = service
                    .scan(
                        &store,
                        ready.root_id.as_deref().unwrap(),
                        ready.generation,
                        workbench_library::ScanAction::Next,
                    )
                    .unwrap();
            }
            assert_eq!(ready.phase, workbench_library::LibraryPhase::Complete);
            assert_eq!(ready.items.len(), 1);
            let unrelated = media.join("untouched.txt");
            std::fs::write(&unrelated, b"synthetic unrelated bytes").unwrap();
            let request = LibraryRecycleRequest {
                root_id: ready.root_id.clone().unwrap(),
                generation: ready.generation,
                entry_id: ready.items[0].id.clone(),
                expected_revision: ready.revision,
            };
            let result = service
                .recycle_confirmed(&store, &request, |_| true, native_recycle)
                .unwrap()
                .unwrap();
            assert!(
                result.recycled,
                "Shell must report a matching, non-null Recycle Bin destination"
            );
            assert_eq!(result.error_code, None);
            assert!(!file.exists());
            assert_eq!(
                std::fs::read(unrelated).unwrap(),
                b"synthetic unrelated bytes"
            );
            let item = &result.snapshot.items[0];
            assert_eq!(item.state, workbench_library::LibraryItemState::Unreadable);
            assert_eq!(item.error_code.as_deref(), Some("LIBRARY_RECYCLED"));
        }

        #[test]
        fn typed_sink_checks_the_single_shell_item_and_rejects_a_null_recycle_destination() {
            // Exercise the actual COM sink without calling DeleteItem or
            // PerformOperations. Every file is an isolated synthetic archive.
            std::thread::spawn(|| {
                use std::io::Write;
                use windows::Win32::Foundation::S_OK;
                unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
                    .ok()
                    .unwrap();
                let _apartment = Apartment;
                let temp = tempfile::tempdir().unwrap();
                let media = temp.path().join("media");
                std::fs::create_dir(&media).unwrap();
                let mut zip =
                    zip::ZipWriter::new(std::fs::File::create(media.join("合成.zip")).unwrap());
                zip.start_file("1.jpg", zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(b"synthetic-index-only-page").unwrap();
                zip.finish().unwrap();
                let store =
                    workbench_storage::WorkbenchStore::open(temp.path().join("app")).unwrap();
                let mut service = workbench_library::LibraryService::new();
                let mut ready = service.choose(&store, &media).unwrap();
                while ready.phase == workbench_library::LibraryPhase::Reading {
                    ready = service
                        .scan(
                            &store,
                            ready.root_id.as_deref().unwrap(),
                            ready.generation,
                            workbench_library::ScanAction::Next,
                        )
                        .unwrap();
                }
                let request = LibraryRecycleRequest {
                    root_id: ready.root_id.clone().unwrap(),
                    generation: ready.generation,
                    entry_id: ready.items[0].id.clone(),
                    expected_revision: ready.revision,
                };
                let result = service
                    .recycle_confirmed(
                        &store,
                        &request,
                        |_| true,
                        |target| {
                            let wide: Vec<_> = dunce::simplified(target.path())
                                .as_os_str()
                                .encode_wide()
                                .chain(Some(0))
                                .collect();
                            let item: IShellItem =
                                unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }
                                    .unwrap();
                            let proof = Arc::new(Mutex::new(Proof::default()));
                            let sink: IFileOperationProgressSink = RecycleSink {
                                target,
                                proof: Arc::clone(&proof),
                            }
                            .into();
                            let flags = TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32;
                            unsafe { sink.PreDeleteItem(flags, &item) }.unwrap();
                            assert!(unsafe {
                                sink.PostDeleteItem(flags, &item, S_OK, None::<&IShellItem>)
                            }
                            .is_err());
                            assert!(!proof.lock().unwrap().recycled);
                            Err(error("SYNTHETIC_STOP"))
                        },
                    )
                    .unwrap_err();
                assert_eq!(result.code, "SYNTHETIC_STOP");
                assert!(media.join("合成.zip").is_file());
                assert_eq!(store.read_library().unwrap().revision, ready.revision);
            })
            .join()
            .unwrap();
        }

        #[test]
        fn delete_proof_vetoes_permanent_fallback_changed_identity_and_repeated_items() {
            let recycle = TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32;
            for (flags, matches) in [(0, true), (recycle, false)] {
                let mut proof = Proof::default();
                assert!(proof.before(flags, matches).is_err());
                assert!(proof.outcome(true, false).is_err());
            }
            let mut proof = Proof::default();
            proof.before(recycle, true).unwrap();
            assert!(proof.before(recycle, true).is_err());
        }

        #[test]
        fn success_requires_nonnull_matching_recycled_item_and_all_shell_completion_signals() {
            let recycle = TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32;
            for (flags, succeeded, matches) in [
                (0, true, true),
                (recycle, false, true),
                (recycle, true, false),
            ] {
                let mut proof = Proof::default();
                proof.before(recycle, true).unwrap();
                assert!(proof.after(flags, succeeded, matches).is_err());
                assert!(proof.outcome(true, false).is_err());
            }
            let mut proof = Proof::default();
            proof.before(recycle, true).unwrap();
            assert!(proof.outcome(true, false).is_err());
            proof.after(recycle, true, true).unwrap();
            assert!(proof.outcome(false, false).is_err());
            assert!(proof.outcome(true, true).is_err());
            proof.outcome(true, false).unwrap();
        }
    }
}
