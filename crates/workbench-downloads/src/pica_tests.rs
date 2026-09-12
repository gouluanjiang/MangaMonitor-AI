//! Synthetic Pica pagination, unchanged media bytes, and desktop approval tests.
use super::*;
use cloud_monitor::source_completion::PaginationProof;

const PICA_ID: &str = "0123456789abcdef01234567";
const TOKEN: &str = "synthetic-pica-session-for-offline-tests";

fn pica_fixture() -> Fixture {
    let mut f = fixture();
    let library = f.store.read_library().unwrap();
    let metadata: crate::DownloadMetadata = JmDownloadMetadata {
        work_id: PICA_ID.into(),
        title: "Offline Pica work".into(),
        authors: vec!["Pica fixture author".into()],
        tags: vec!["fixture".into()],
        description: Some("Synthetic paginated source".into()),
    };
    let plan = f
        .service
        .prepare_for_source(
            &f.store,
            &library.value.root.as_ref().unwrap().id,
            library.value.generation,
            Source::Pica,
            metadata,
        )
        .unwrap();
    assert_eq!(plan.source, Source::Pica);
    f.service
        .confirm(&f.store, &plan.plan_id, plan.revision)
        .unwrap();
    f.id = plan.plan_id;
    f
}
fn pica_record(f: &Fixture) -> DownloadRecord {
    f.store
        .read_downloads()
        .unwrap()
        .value
        .tasks
        .into_iter()
        .find(|t| t.id == f.id)
        .unwrap()
}
fn save_task(f: &Fixture, value: DownloadRecord) {
    let mut document = f.store.read_downloads().unwrap();
    let task = document
        .value
        .tasks
        .iter_mut()
        .find(|task| task.id == value.id)
        .unwrap();
    *task = value;
    f.store
        .write_downloads(document.revision, document.value)
        .unwrap();
}
fn pagination() -> PaginationProof {
    PaginationProof {
        total_pages: 2,
        successful_pages: vec![1, 2],
        failed_pages: vec![],
    }
}
fn pica_core(record: &DownloadRecord) -> Core {
    assert_eq!(record.source, Source::Pica);
    let chapters = [vec!["jpg", "gif"], vec!["webp", "png", "jpeg"]].into_iter()
        .enumerate().map(|(chapter_index, formats)| {
            let order = chapter_index as u64 + 1;
            let id = format!("{order:024x}");
            let media = formats.into_iter().enumerate().map(|(index, format)| {
                let number = index as u64 + 1;
                MediaDescriptor {
                    image_index: number, source_media_id: format!("{:024x}", order * 100 + number),
                    request_url: format!("https://storage.example.invalid/static/pages/{order}-{number}.{format}"),
                    source_format: format.into(), transform: "NONE".into(), transform_parameter: 0,
                    relative_path: format!("chapters/{order:06}-{id}/{number:06}.{format}"),
                }
            }).collect();
            MediaChapterDescriptors { chapter_id: id, chapter_order: order, jm_scramble_id: None, media }
        }).collect();
    core_for_chapters(record, chapters, Some(pagination()), Some(pagination()))
}
fn pica_bytes(format: &str) -> Vec<u8> {
    match format {
        "jpg" | "jpeg" => static_image(image::ImageFormat::Jpeg),
        "png" => static_image(image::ImageFormat::Png),
        "webp" => static_image(image::ImageFormat::WebP),
        "gif" => gif(),
        _ => panic!("unsupported synthetic format"),
    }
}
async fn seed_pica(f: &Fixture) -> Core {
    let mut value = pica_record(f);
    let c = pica_core(&value);
    let workspace = f.store.open_download_workspace().unwrap();
    let execution = isolated_staging_execution::execute_resumable_with_fetcher(
        context(workspace.path(), &c),
        None,
        |d| async move {
            let bytes = pica_bytes(&d.source_format);
            Ok(ProcessedMedia {
                source_media_id: d.source_media_id,
                request_url: d.request_url,
                source_format: d.source_format,
                applied_transform: d.transform,
                applied_transform_parameter: d.transform_parameter,
                bytes,
            })
        },
        || Ok(c.authorization.clone()),
        |_| Ok(()),
    )
    .await
    .unwrap();
    let (state, ledger, command) = adapter::current(&value).unwrap();
    let receipt =
        verified_execution_receipt::build(&command, &c.plan, &execution, "2026-09-12T12:00:00Z")
            .unwrap();
    let receipt_view = executor_handoff::receipt_view(&state, &ledger, &receipt).unwrap();
    let report = LocalExecutionReport {
        schema_version: 2,
        command_id: command.command_id,
        task_id: command.task_id,
        work_id: command.work_id,
        task_revision: command.task_revision,
        target_hash: command.target_hash,
        source: command.source,
        source_work_id: command.source_work_id,
        staging_subdir: c.plan.staging_subdir.clone(),
        preflight_hash: execution.preflight_hash,
        staging_execution_completed: execution.staging_execution_completed,
        receipt,
        source_completion: execution.source_completion,
        filesystem_verification: execution.filesystem_verification,
        receipt_view,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
        production_enablement_authorized: false,
    };
    assert_eq!(report.source_completion.manifest.backend, "PICA");
    value.files_done = c.proof.expected_content_units;
    value.files_total = Some(c.proof.expected_content_units);
    value.bytes_done = report.filesystem_verification.total_bytes;
    value.staging_report_json = Some(serde_json::to_string(&report).unwrap());
    save_task(f, value);
    c
}

#[tokio::test]
async fn pica_mixed_pages_materialize_register_and_redownload_without_touching_jm_or_phone() {
    let f = pica_fixture();
    let jm = complete_for_presence(&f, record(&f));
    let phone = f.store.read_phone_library().unwrap();
    let initial = pica_record(&f);
    assert!(!initial.jpeg_output);
    assert!(initial
        .destination
        .starts_with(&format!("[Pica{PICA_ID}] ")));
    let c = seed_pica(&f).await;
    let receipt = f
        .service
        .run_with_token(&f.store, &f.id, Some(TOKEN), || Ok(()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(receipt.source, Source::Pica);
    assert_eq!(receipt.expected_pages, 5);
    let output = f.library.join(&receipt.relative_path);
    for chapter in &c.descriptors.chapters {
        let directory = format!("{:03}-{}", chapter.chapter_order, chapter.chapter_id);
        for media in &chapter.media {
            let path = output
                .join(&directory)
                .join(format!("{:03}.{}", media.image_index, media.source_format));
            assert_eq!(fs::read(path).unwrap(), pica_bytes(&media.source_format));
        }
        let metadata: serde_json::Value = serde_json::from_slice(
            &fs::read(output.join(directory).join("章节元数据.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(metadata["chapterId"], chapter.chapter_id);
    }
    let metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("元数据.json")).unwrap()).unwrap();
    assert_eq!(metadata["id"], PICA_ID);
    assert!(metadata["author"].is_string());
    assert_eq!(metadata["pagesCount"], 5);
    assert_eq!(metadata["chapterCount"], 2);
    assert_eq!(metadata["downloaded"], true);
    assert_eq!(metadata["thumb"]["path"], "cover.jpg");
    assert_eq!(
        image::guess_format(&fs::read(output.join("cover.jpg")).unwrap()).unwrap(),
        image::ImageFormat::Jpeg
    );
    let marker: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("_mangamonitor-layout.json")).unwrap())
            .unwrap();
    assert_eq!(marker["source"], "Pica");
    assert_eq!(marker["expectedPages"], 5);
    let mut wrong_source = receipt.clone();
    wrong_source.source = Source::Jm;
    assert_eq!(
        f.service
            .validate_receipt(&f.store, &wrong_source)
            .unwrap_err()
            .code,
        "DOWNLOAD_TASK_STALE"
    );
    let indexed = workbench_library::LibraryService::new()
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
        .find(|entry| entry.relative_path == receipt.relative_path)
        .unwrap();
    let done = f
        .service
        .mark_indexed(&f.store, &receipt, &entry.id)
        .unwrap();
    assert_eq!(done.tasks[1].phase, DownloadPhase::Downloaded);
    assert_eq!(done.tasks[1].source, Source::Pica);
    assert_eq!(done.tasks[1].local_files, Some(LocalFiles::Present));
    assert_eq!(record(&f), jm);
    assert_eq!(f.store.read_phone_library().unwrap(), phone);
    assert!(!serde_json::to_string(&f.store.read_downloads().unwrap())
        .unwrap()
        .contains(TOKEN));
    let completed = pica_record(&f);
    fs::remove_dir_all(&output).unwrap();
    assert_eq!(
        f.service.read(&f.store).unwrap().tasks[1].local_files,
        Some(LocalFiles::Missing)
    );
    let plan = f
        .service
        .prepare_for_source(
            &f.store,
            &completed.root.id,
            completed.generation,
            Source::Pica,
            completed.metadata.clone(),
        )
        .unwrap();
    f.service
        .confirm(&f.store, &plan.plan_id, plan.revision)
        .unwrap();
    let library = f.store.read_library().unwrap();
    assert_eq!(library.value.records.len(), 1);
    assert_eq!(
        library.value.records[0]
            .item
            .source_ref
            .as_ref()
            .unwrap()
            .source,
        Source::Jm
    );
    let history = f.store.read_downloads().unwrap();
    assert_eq!(history.value.tasks[1], completed);
    assert_eq!(history.value.tasks[2].source, Source::Pica);
    assert_eq!(record(&f), jm);
    assert!(
        !output.exists(),
        "confirmation must not execute or create media"
    );
}

#[tokio::test]
async fn pica_token_and_source_revision_fail_closed_before_worker_or_queue_mutation() {
    let f = pica_fixture();
    let pica = pica_record(&f);
    let jm = record(&f);
    let before = f.store.read_downloads().unwrap();
    assert_eq!(
        f.service
            .task_source(&f.store, &pica.id, pica.revision)
            .unwrap(),
        Source::Pica
    );
    assert_eq!(
        f.service
            .task_source(&f.store, &pica.id, pica.revision + 1)
            .unwrap_err()
            .code,
        "DOWNLOAD_TASK_STALE"
    );
    assert_eq!(
        f.service
            .run(&f.store, &pica.id, || Ok(()))
            .await
            .unwrap_err()
            .code,
        "DOWNLOAD_SOURCE_TOKEN_REQUIRED"
    );
    assert_eq!(
        f.service
            .run_with_token(&f.store, &jm.id, Some(TOKEN), || Ok(()))
            .await
            .unwrap_err()
            .code,
        "DOWNLOAD_SOURCE_MISMATCH"
    );
    for token in ["", "bad\r\nheader", " padded "] {
        assert_eq!(
            f.service
                .run_with_token(&f.store, &pica.id, Some(token), || Ok(()))
                .await
                .unwrap_err()
                .code,
            "DOWNLOAD_SOURCE_TOKEN_INVALID"
        );
    }
    let mut invalid = before.value.clone();
    invalid.tasks[1].jpeg_output = true;
    assert!(f.store.write_downloads(before.revision, invalid).is_err());
    assert_eq!(f.store.read_downloads().unwrap(), before);
    assert!(f.service.lock().unwrap().active.is_none());
    assert!(!f.library.join(&pica.destination).exists());
}

#[tokio::test]
async fn missing_pica_pagination_never_fetches_or_creates_a_completed_directory() {
    let f = pica_fixture();
    let value = pica_record(&f);
    let before = f.store.read_downloads().unwrap();
    let workspace = f.store.open_download_workspace().unwrap();
    for image_pages in [false, true] {
        let mut c = pica_core(&value);
        if image_pages {
            c.evidence.chapters[0]
                .image_pagination
                .as_mut()
                .unwrap()
                .successful_pages
                .pop();
        } else {
            c.evidence
                .chapter_pagination
                .as_mut()
                .unwrap()
                .successful_pages
                .pop();
        }
        let calls = Cell::new(0);
        let result = isolated_staging_execution::execute_resumable_with_fetcher(
            context(workspace.path(), &c),
            None,
            |_| {
                calls.set(calls.get() + 1);
                async { Err("UNEXPECTED_FETCH".into()) }
            },
            || Ok(c.authorization.clone()),
            |_| Ok(()),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(calls.get(), 0);
    }
    assert_eq!(f.store.read_downloads().unwrap(), before);
    assert!(!f.library.join(&value.destination).exists());
}

#[test]
fn legacy_jm_source_defaults_keep_both_historical_binding_profiles_unchanged() {
    let f = fixture();
    for jpeg in [false, true] {
        let mut old = record(&f);
        old.jpeg_output = jpeg;
        let historical = serde_json::to_vec(&(
            if jpeg {
                "manual-JM-layout-v1-jpeg"
            } else {
                "manual-JM-layout-v1"
            },
            &old.root,
            old.generation,
            &old.metadata,
            &old.destination,
            old.approval_revision,
        ))
        .unwrap();
        old.target_hash = hash(&historical);
        assert_eq!(binding(&old).unwrap(), old.target_hash);
        let mut encoded = serde_json::to_value(&old).unwrap();
        assert!(encoded.get("source").is_none());
        if !jpeg {
            encoded.as_object_mut().unwrap().remove("jpegOutput");
        }
        let restored: DownloadRecord = serde_json::from_value(encoded).unwrap();
        assert_eq!(restored.source, Source::Jm);
        assert_eq!(restored, old);
        assert_eq!(
            adapter::current(&restored).unwrap().2.command_id,
            adapter::current(&old).unwrap().2.command_id
        );
    }
}

#[test]
fn source_failures_keep_actionable_codes_without_returning_raw_errors() {
    for (incoming, expected) in [
        ("HTTP_401", "SESSION_EXPIRED"),
        ("API_CODE_401", "SESSION_EXPIRED"),
        ("SESSION_EXPIRED", "SESSION_EXPIRED"),
        ("SESSION_CHANGED", "SESSION_CHANGED"),
        ("PICA_PAGINATION_CHANGED", "DOWNLOAD_SOURCE_CHANGED"),
        ("PICA_PAGINATION_INVALID", "DOWNLOAD_SOURCE_INCOMPLETE"),
        ("PICA_PAGINATION_INCOMPLETE", "DOWNLOAD_SOURCE_INCOMPLETE"),
        (
            "PICA_PREFLIGHT_IMAGE_PAGINATION_INCOMPLETE",
            "DOWNLOAD_SOURCE_INCOMPLETE",
        ),
        (
            "upstream response included a private token",
            "DOWNLOAD_FAILED",
        ),
        ("HTTP_401: raw response body", "DOWNLOAD_FAILED"),
    ] {
        assert_eq!(classify(incoming).code, expected);
    }
}
