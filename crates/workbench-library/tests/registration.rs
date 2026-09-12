use image::{DynamicImage, ImageFormat};
use std::{fs, io::Cursor, path::Path};
use tempfile::TempDir;
use workbench_library::{
    LibraryEvidence, LibraryPhase, LibraryReference, LibraryService, LibrarySnapshot, ScanAction,
};
use workbench_storage::{Source, WorkbenchStore};

// Synthetic, already finalized trees only. These tests exercise private index
// registration; they never run a downloader or touch a user/media directory.
struct Fixture {
    _app: TempDir,
    media: TempDir,
    store: WorkbenchStore,
    service: LibraryService,
    initial: LibrarySnapshot,
}

fn image_bytes() -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(2, 3)
        .write_to(&mut buffer, ImageFormat::Png)
        .unwrap();
    buffer.into_inner()
}

fn reference(id: &str) -> LibraryReference {
    LibraryReference {
        source: Source::Jm,
        work_id: id.into(),
    }
}

fn work(root: &Path, name: &str, id: &str, pages: u64) {
    let directory = root.join(name);
    fs::create_dir_all(directory.join("chapter-01")).unwrap();
    let xml = format!("<ComicInfo><Title>Synthetic complete work</Title><Source>JM</Source><WorkId>{id}</WorkId></ComicInfo>");
    fs::write(directory.join("ComicInfo.xml"), xml).unwrap();
    for page in 1..=pages {
        fs::write(
            directory.join(format!("chapter-01/{page:04}.png")),
            image_bytes(),
        )
        .unwrap();
    }
}

fn finish(
    service: &mut LibraryService,
    store: &WorkbenchStore,
    mut state: LibrarySnapshot,
) -> LibrarySnapshot {
    for _ in 0..100 {
        if state.phase != LibraryPhase::Reading {
            return state;
        }
        state = service
            .scan(
                store,
                state.root_id.as_deref().unwrap(),
                state.generation,
                ScanAction::Next,
            )
            .unwrap();
    }
    panic!("small synthetic library did not finish");
}

fn fixture() -> Fixture {
    let app = TempDir::new().unwrap();
    let media = TempDir::new().unwrap();
    let store = WorkbenchStore::open(app.path()).unwrap();
    let mut service = LibraryService::new();
    let selected = service.choose(&store, media.path()).unwrap();
    let initial = finish(&mut service, &store, selected);
    Fixture {
        _app: app,
        media,
        store,
        service,
        initial,
    }
}

fn register(
    fixture: &mut Fixture,
    name: &str,
    id: &str,
    pages: u64,
) -> workbench_library::Result<LibrarySnapshot> {
    fixture.service.register_completed(
        &fixture.store,
        fixture.initial.root_id.as_deref().unwrap(),
        fixture.initial.generation,
        name,
        &reference(id),
        pages,
    )
}

fn persisted(fixture: &Fixture) -> Vec<u8> {
    serde_json::to_vec(&fixture.store.read_library().unwrap()).unwrap()
}

#[test]
fn registers_only_the_named_finalized_work_and_survives_restart_without_media_or_phone_changes() {
    let mut fixture = fixture();
    work(fixture.media.path(), "new work", "123", 2);
    fs::create_dir(fixture.media.path().join("unrelated malformed work")).unwrap();
    fs::write(
        fixture
            .media
            .path()
            .join("unrelated malformed work/ComicInfo.xml"),
        b"bad metadata",
    )
    .unwrap();
    let page = fixture.media.path().join("new work/chapter-01/0001.png");
    let before = fs::read(&page).unwrap();
    let phone_before = fixture.store.read_phone_library().unwrap();
    let result = register(&mut fixture, "new work", "123", 2).unwrap();
    assert_eq!(result.phase, LibraryPhase::Complete);
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].relative_path, "new work");
    assert_eq!(result.items[0].source_ref, Some(reference("123")));
    assert_eq!(
        result.items[0].identity_evidence,
        Some(LibraryEvidence::Metadata)
    );
    assert_eq!(result.items[0].page_count, Some(2));
    assert_eq!(fs::read(page).unwrap(), before);
    assert_eq!(fixture.store.read_phone_library().unwrap(), phone_before);
    let saved = persisted(&fixture);
    let repeat = register(&mut fixture, "new work", "123", 2).unwrap();
    assert_eq!(
        repeat.revision, result.revision,
        "same identity is idempotent"
    );
    assert_eq!(persisted(&fixture), saved);
    let reopened = LibraryService::new().read(&fixture.store).unwrap();
    assert_eq!(reopened.items, result.items);
    assert_eq!(fs::read_dir(fixture.media.path()).unwrap().count(), 2);
}

#[test]
fn rejects_a_second_path_for_the_same_source_without_replacing_the_existing_record() {
    let mut fixture = fixture();
    work(fixture.media.path(), "first", "123", 1);
    register(&mut fixture, "first", "123", 1).unwrap();
    work(fixture.media.path(), "second", "123", 1);
    let before = persisted(&fixture);
    assert_eq!(
        register(&mut fixture, "second", "123", 1).unwrap_err().code,
        "LIBRARY_IDENTITY_CONFLICT"
    );
    assert_eq!(persisted(&fixture), before);
    assert!(fixture
        .media
        .path()
        .join("first/chapter-01/0001.png")
        .is_file());
    assert!(fixture
        .media
        .path()
        .join("second/chapter-01/0001.png")
        .is_file());
}

#[test]
fn missing_pages_bad_metadata_empty_images_and_partial_trees_do_not_change_the_index() {
    let mut fixture = fixture();
    work(fixture.media.path(), "existing", "111", 1);
    register(&mut fixture, "existing", "111", 1).unwrap();
    for name in [
        "missing page",
        "bad metadata",
        "empty image",
        "partial directory",
        "partial file",
    ] {
        work(fixture.media.path(), name, "123", 2);
    }
    fs::remove_file(
        fixture
            .media
            .path()
            .join("missing page/chapter-01/0002.png"),
    )
    .unwrap();
    fs::write(
        fixture.media.path().join("bad metadata/ComicInfo.xml"),
        b"<ComicInfo><broken>",
    )
    .unwrap();
    fs::write(
        fixture.media.path().join("empty image/chapter-01/0002.png"),
        b"",
    )
    .unwrap();
    fs::create_dir(
        fixture
            .media
            .path()
            .join("partial directory/.下载中-chapter"),
    )
    .unwrap();
    fs::write(
        fixture
            .media
            .path()
            .join("partial file/chapter-01/0003.png.part"),
        b"partial",
    )
    .unwrap();
    let before = persisted(&fixture);
    for name in [
        "missing page",
        "bad metadata",
        "empty image",
        "partial directory",
        "partial file",
    ] {
        assert!(register(&mut fixture, name, "123", 2).is_err(), "{name}");
        assert_eq!(persisted(&fixture), before, "{name}");
    }
}

#[test]
fn requires_exact_metadata_identity_and_page_count_not_a_filename_hint() {
    let mut fixture = fixture();
    work(fixture.media.path(), "valid", "123", 2);
    work(fixture.media.path(), "[JM-123] filename only", "123", 2);
    fs::remove_file(
        fixture
            .media
            .path()
            .join("[JM-123] filename only/ComicInfo.xml"),
    )
    .unwrap();
    let before = persisted(&fixture);
    for (name, id, pages) in [
        ("valid", "456", 2),
        ("valid", "123", 1),
        ("[JM-123] filename only", "123", 2),
    ] {
        assert_eq!(
            register(&mut fixture, name, id, pages).unwrap_err().code,
            "LIBRARY_IDENTITY_CONFLICT"
        );
        assert_eq!(persisted(&fixture), before);
    }
    for pages in [0, 10_001] {
        assert_eq!(
            register(&mut fixture, "valid", "123", pages)
                .unwrap_err()
                .code,
            "VALIDATION_FAILED"
        );
        assert_eq!(persisted(&fixture), before);
    }
}

#[test]
fn changed_scope_and_active_or_paused_full_scan_cannot_register() {
    let mut fixture = fixture();
    work(fixture.media.path(), "valid", "123", 1);
    let before = persisted(&fixture);
    assert_eq!(
        fixture
            .service
            .register_completed(
                &fixture.store,
                &"b".repeat(64),
                fixture.initial.generation,
                "valid",
                &reference("123"),
                1
            )
            .unwrap_err()
            .code,
        "LIBRARY_STALE_SNAPSHOT"
    );
    assert_eq!(
        fixture
            .service
            .register_completed(
                &fixture.store,
                fixture.initial.root_id.as_deref().unwrap(),
                fixture.initial.generation + 1,
                "valid",
                &reference("123"),
                1
            )
            .unwrap_err()
            .code,
        "LIBRARY_STALE_SNAPSHOT"
    );
    assert_eq!(persisted(&fixture), before);
    let refresh = fixture
        .service
        .scan(
            &fixture.store,
            fixture.initial.root_id.as_deref().unwrap(),
            fixture.initial.generation,
            ScanAction::Start,
        )
        .unwrap();
    fixture.initial = refresh;
    for action in [None, Some(ScanAction::Pause)] {
        if let Some(action) = action {
            fixture
                .service
                .scan(
                    &fixture.store,
                    fixture.initial.root_id.as_deref().unwrap(),
                    fixture.initial.generation,
                    action,
                )
                .unwrap();
        }
        let before = persisted(&fixture);
        assert_eq!(
            register(&mut fixture, "valid", "123", 1).unwrap_err().code,
            "LIBRARY_BUSY"
        );
        assert_eq!(persisted(&fixture), before);
    }
    // Registration rejection must not consume or discard the full-scan cursor.
    let resumed = fixture
        .service
        .scan(
            &fixture.store,
            fixture.initial.root_id.as_deref().unwrap(),
            fixture.initial.generation,
            ScanAction::Resume,
        )
        .unwrap();
    let done = finish(&mut fixture.service, &fixture.store, resumed);
    assert_eq!(done.phase, LibraryPhase::Complete);
    assert_eq!(done.items.len(), 1);
}

#[test]
fn replacing_a_registered_directory_or_manually_unlinking_it_cannot_be_silently_overwritten() {
    let mut fixture = fixture();
    work(fixture.media.path(), "valid", "123", 1);
    let registered = register(&mut fixture, "valid", "123", 1).unwrap();
    fixture
        .service
        .link(
            &fixture.store,
            registered.root_id.as_deref().unwrap(),
            registered.generation,
            &registered.items[0].id,
            None,
        )
        .unwrap();
    let before = persisted(&fixture);
    assert_eq!(
        register(&mut fixture, "valid", "123", 1).unwrap_err().code,
        "LIBRARY_IDENTITY_CONFLICT"
    );
    assert_eq!(persisted(&fixture), before);
    fixture
        .service
        .link(
            &fixture.store,
            registered.root_id.as_deref().unwrap(),
            registered.generation,
            &registered.items[0].id,
            Some(reference("123")),
        )
        .unwrap();
    fs::rename(
        fixture.media.path().join("valid"),
        fixture.media.path().join("preserved original"),
    )
    .unwrap();
    work(fixture.media.path(), "valid", "123", 1);
    let before = persisted(&fixture);
    assert_eq!(
        register(&mut fixture, "valid", "123", 1).unwrap_err().code,
        "LIBRARY_FILE_CHANGED"
    );
    assert_eq!(persisted(&fixture), before);
    assert!(fixture
        .media
        .path()
        .join("preserved original/chapter-01/0001.png")
        .exists());
}

#[test]
fn rejected_paths_and_non_directory_targets_never_change_the_index() {
    let mut fixture = fixture();
    fs::write(fixture.media.path().join("file.zip"), b"not a directory").unwrap();
    let before = persisted(&fixture);
    for name in [
        "../outside",
        "/absolute",
        "nested\\escape",
        "C:relative",
        ".下载中-work",
        "file.zip",
    ] {
        assert!(register(&mut fixture, name, "123", 1).is_err());
        assert_eq!(persisted(&fixture), before);
    }
}

#[cfg(unix)]
#[test]
fn redirected_children_do_not_turn_a_partial_work_into_a_completed_record() {
    use std::os::unix::fs::symlink;
    let mut fixture = fixture();
    work(fixture.media.path(), "valid", "123", 1);
    symlink(
        "0001.png",
        fixture.media.path().join("valid/chapter-01/0002.png"),
    )
    .unwrap();
    let before = persisted(&fixture);
    assert_eq!(
        register(&mut fixture, "valid", "123", 1).unwrap_err().code,
        "LIBRARY_REGISTER_INCOMPLETE"
    );
    assert_eq!(persisted(&fixture), before);
}

fn managed_work(root: &Path, name: &str, id: &str, pages: u64) -> serde_json::Value {
    use sha2::{Digest, Sha256};
    let directory = root.join(name);
    let chapter = format!("0001-{id}");
    fs::create_dir_all(directory.join(&chapter)).unwrap();
    let marker = serde_json::json!({"version":1,"source":"JM","workId":id,"expectedPages":pages,"layoutVersion":1,"taskId":"a".repeat(64)});
    fs::write(
        directory.join("_mangamonitor-layout.json"),
        serde_json::to_vec(&marker).unwrap(),
    )
    .unwrap();
    let metadata = serde_json::json!({"id":id.parse::<u64>().unwrap(),"name":"Synthetic managed work","author":[],"tags":[],"chapterInfos":[{"chapterId":id.parse::<u64>().unwrap(),"imageCount":pages}]});
    fs::write(
        directory.join("元数据.json"),
        serde_json::to_vec(&metadata).unwrap(),
    )
    .unwrap();
    let mut image = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(2, 3)
        .write_to(&mut image, ImageFormat::WebP)
        .unwrap();
    let mut cover = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(2, 3)
        .write_to(&mut cover, ImageFormat::Jpeg)
        .unwrap();
    fs::write(directory.join("cover.jpg"), cover.into_inner()).unwrap();
    fs::write(directory.join(format!("{chapter}/章节元数据.json")), b"{}").unwrap();
    let mut paths = vec![
        "_mangamonitor-layout.json".to_owned(),
        "元数据.json".into(),
        "cover.jpg".into(),
        format!("{chapter}/章节元数据.json"),
    ];
    for page in 1..=pages {
        let path = format!("{chapter}/{page:04}.webp");
        fs::write(directory.join(&path), image.get_ref()).unwrap();
        paths.push(path);
    }
    let files: Vec<_> = paths.into_iter().map(|path| {
        let bytes = fs::read(directory.join(&path)).unwrap();
        serde_json::json!({"relativePath":path,"sizeBytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))})
    }).collect();
    serde_json::json!({"version":1,"origin":"manual","taskId":"a".repeat(64),"approvalRevision":1,"targetHash":"b".repeat(64),"source":"JM","workId":id,"rootId":"c".repeat(64),"generation":1,"layoutVersion":1,"files":files})
}

fn write_final(root: &Path, name: &str, manifest: &serde_json::Value) {
    fs::write(
        root.join(name).join("_mangamonitor.json"),
        serde_json::to_vec(manifest).unwrap(),
    )
    .unwrap();
}

fn rescan(fixture: &mut Fixture) -> LibrarySnapshot {
    let started = fixture
        .service
        .scan(
            &fixture.store,
            fixture.initial.root_id.as_deref().unwrap(),
            fixture.initial.generation,
            ScanAction::Start,
        )
        .unwrap();
    let state = finish(&mut fixture.service, &fixture.store, started);
    fixture.initial = state.clone();
    state
}

#[test]
fn ordinary_scan_only_publishes_managed_work_after_the_final_manifest_and_preserves_external_layouts(
) {
    let mut fixture = fixture();
    let manifest = managed_work(fixture.media.path(), "managed", "123", 2);
    work(fixture.media.path(), "external", "456", 1);
    fs::write(
        fixture.media.path().join("external/_mangamonitor.json"),
        b"unrelated external file",
    )
    .unwrap();
    let partial = rescan(&mut fixture);
    assert_eq!(partial.phase, LibraryPhase::Error);
    assert_eq!(partial.items.len(), 1);
    assert_eq!(partial.items[0].source_ref, Some(reference("456")));
    write_final(fixture.media.path(), "managed", &manifest);
    let proof_before = fs::read(fixture.media.path().join("managed/_mangamonitor.json")).unwrap();
    let complete = rescan(&mut fixture);
    assert_eq!(complete.phase, LibraryPhase::Complete);
    assert_eq!(complete.items.len(), 2);
    let item = complete
        .items
        .iter()
        .find(|item| item.relative_path == "managed")
        .unwrap();
    assert_eq!(item.source_ref, Some(reference("123")));
    assert_eq!(item.page_count, Some(2));
    assert_eq!(item.error_code, None);
    assert_eq!(
        fs::read(fixture.media.path().join("managed/_mangamonitor.json")).unwrap(),
        proof_before
    );
}

#[test]
fn malformed_or_partial_managed_manifests_never_produce_downloaded_library_evidence() {
    for variant in [
        "wrong id",
        "wrong task",
        "wrong page count",
        "missing page",
        "extra file",
        "extra directory",
        "duplicate path",
        "unsafe path",
        "changed size",
        "truncated manifest",
    ] {
        let mut fixture = fixture();
        let mut manifest = managed_work(fixture.media.path(), "managed", "123", 2);
        let directory = fixture.media.path().join("managed");
        match variant {
            "wrong id" => manifest["workId"] = "456".into(),
            "wrong task" => manifest["taskId"] = "d".repeat(64).into(),
            "wrong page count" => {
                let marker = directory.join("_mangamonitor-layout.json");
                let mut value: serde_json::Value =
                    serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
                value["expectedPages"] = 3.into();
                fs::write(marker, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            "missing page" => fs::remove_file(directory.join("0001-123/0002.webp")).unwrap(),
            "extra file" => fs::write(directory.join("0001-123/0003.webp"), image_bytes()).unwrap(),
            "extra directory" => fs::create_dir(directory.join("unplanned")).unwrap(),
            "duplicate path" => {
                let duplicate = manifest["files"][0].clone();
                manifest["files"].as_array_mut().unwrap().push(duplicate);
            }
            "unsafe path" => manifest["files"][0]["relativePath"] = "../outside.webp".into(),
            "changed size" => fs::write(directory.join("0001-123/0001.webp"), b"short").unwrap(),
            "truncated manifest" => {}
            _ => unreachable!(),
        }
        write_final(fixture.media.path(), "managed", &manifest);
        if variant == "truncated manifest" {
            fs::write(directory.join("_mangamonitor.json"), b"{\"files\":").unwrap();
        }
        let state = rescan(&mut fixture);
        assert!(
            state.items.is_empty(),
            "{variant} must not appear as downloaded"
        );
        assert_eq!(state.phase, LibraryPhase::Error, "{variant}");
    }
}

#[test]
fn targeted_registration_also_requires_the_managed_completion_marker() {
    let mut fixture = fixture();
    let manifest = managed_work(fixture.media.path(), "managed", "123", 2);
    let before = persisted(&fixture);
    assert!(register(&mut fixture, "managed", "123", 2).is_err());
    assert_eq!(persisted(&fixture), before);
    write_final(fixture.media.path(), "managed", &manifest);
    let state = register(&mut fixture, "managed", "123", 2).unwrap();
    assert_eq!(state.items.len(), 1);
    assert_eq!(state.items[0].source_ref, Some(reference("123")));
    assert_eq!(state.items[0].page_count, Some(2));
}

fn managed_pica_work(root: &Path) -> serde_json::Value {
    use sha2::{Digest, Sha256};
    let id = "0123456789abcdef01234567";
    let chapter_id = "fedcba987654321001234567";
    let directory = root.join("pica work");
    let chapter = format!("0001-{chapter_id}");
    fs::create_dir_all(directory.join(&chapter)).unwrap();
    let marker = serde_json::json!({"version":1,"source":"Pica","workId":id,"expectedPages":4,"layoutVersion":1,"taskId":"a".repeat(64)});
    let metadata = serde_json::json!({"id":id,"title":"Synthetic Pica work","author":"Synthetic author","pagesCount":4,"tags":[],"chapterInfos":[{"chapterId":chapter_id,"imageCount":4}]});
    fs::write(
        directory.join("_mangamonitor-layout.json"),
        serde_json::to_vec(&marker).unwrap(),
    )
    .unwrap();
    fs::write(
        directory.join("元数据.json"),
        serde_json::to_vec(&metadata).unwrap(),
    )
    .unwrap();
    fs::write(directory.join(format!("{chapter}/章节元数据.json")), b"{}").unwrap();
    let mut cover = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(2, 3)
        .write_to(&mut cover, ImageFormat::Jpeg)
        .unwrap();
    fs::write(directory.join("cover.jpg"), cover.into_inner()).unwrap();
    let mut paths = vec![
        "_mangamonitor-layout.json".to_owned(),
        "元数据.json".into(),
        "cover.jpg".into(),
        format!("{chapter}/章节元数据.json"),
    ];
    for (index, (extension, format)) in [
        ("jpeg", ImageFormat::Jpeg),
        ("png", ImageFormat::Png),
        ("webp", ImageFormat::WebP),
        ("gif", ImageFormat::Gif),
    ]
    .into_iter()
    .enumerate()
    {
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(2, 3)
            .write_to(&mut bytes, format)
            .unwrap();
        let path = format!("{chapter}/{:03}.{extension}", index + 1);
        fs::write(directory.join(&path), bytes.into_inner()).unwrap();
        paths.push(path);
    }
    let files: Vec<_> = paths.into_iter().map(|path| {
        let bytes = fs::read(directory.join(&path)).unwrap();
        serde_json::json!({"relativePath":path,"sizeBytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))})
    }).collect();
    serde_json::json!({"version":1,"origin":"manual","taskId":"a".repeat(64),"approvalRevision":1,"targetHash":"b".repeat(64),"source":"Pica","workId":id,"rootId":"c".repeat(64),"generation":1,"layoutVersion":1,"files":files})
}

#[test]
fn pica_managed_original_formats_register_and_rescan_without_changing_phone_or_files() {
    let mut fixture = fixture();
    let manifest = managed_pica_work(fixture.media.path());
    write_final(fixture.media.path(), "pica work", &manifest);
    let phone_before = fixture.store.read_phone_library().unwrap();
    let reference = LibraryReference {
        source: Source::Pica,
        work_id: "0123456789abcdef01234567".into(),
    };
    let state = fixture
        .service
        .register_completed(
            &fixture.store,
            fixture.initial.root_id.as_deref().unwrap(),
            fixture.initial.generation,
            "pica work",
            &reference,
            4,
        )
        .unwrap();
    assert_eq!(state.items.len(), 1);
    assert_eq!(state.items[0].source_ref, Some(reference.clone()));
    assert_eq!(state.items[0].page_count, Some(4));
    let rescanned = rescan(&mut fixture);
    assert_eq!(rescanned.phase, LibraryPhase::Complete);
    assert_eq!(rescanned.items[0].source_ref, Some(reference));
    assert_eq!(fixture.store.read_phone_library().unwrap(), phone_before);
    use sha2::{Digest, Sha256};
    for file in manifest["files"].as_array().unwrap() {
        let bytes = fs::read(
            fixture
                .media
                .path()
                .join("pica work")
                .join(file["relativePath"].as_str().unwrap()),
        )
        .unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            file["sha256"].as_str().unwrap()
        );
    }
}

#[test]
fn pica_managed_partial_or_cross_source_completion_is_not_indexed() {
    for variant in ["missing page", "wrong source", "wrong chapter id"] {
        let mut fixture = fixture();
        let mut manifest = managed_pica_work(fixture.media.path());
        match variant {
            "missing page" => fs::remove_file(
                fixture
                    .media
                    .path()
                    .join("pica work/0001-fedcba987654321001234567/002.png"),
            )
            .unwrap(),
            "wrong source" => manifest["source"] = "JM".into(),
            "wrong chapter id" => {
                let directory = fixture.media.path().join("pica work");
                fs::rename(
                    directory.join("0001-fedcba987654321001234567"),
                    directory.join("0001-123"),
                )
                .unwrap();
                for file in manifest["files"].as_array_mut().unwrap() {
                    let path = file["relativePath"]
                        .as_str()
                        .unwrap()
                        .replace("0001-fedcba987654321001234567", "0001-123");
                    file["relativePath"] = path.into();
                }
            }
            _ => unreachable!(),
        }
        write_final(fixture.media.path(), "pica work", &manifest);
        let reference = LibraryReference {
            source: Source::Pica,
            work_id: "0123456789abcdef01234567".into(),
        };
        let before = persisted(&fixture);
        assert!(
            fixture
                .service
                .register_completed(
                    &fixture.store,
                    fixture.initial.root_id.as_deref().unwrap(),
                    fixture.initial.generation,
                    "pica work",
                    &reference,
                    4
                )
                .is_err(),
            "{variant}"
        );
        assert_eq!(persisted(&fixture), before);
        let state = rescan(&mut fixture);
        assert!(state.items.is_empty(), "{variant}");
        assert_eq!(state.phase, LibraryPhase::Error, "{variant}");
    }
}
