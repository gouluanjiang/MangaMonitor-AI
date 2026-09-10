use image::{DynamicImage, ImageFormat};
use std::{
    fs::{self, File},
    io::{Cursor, Write},
    path::Path,
};
use tempfile::TempDir;
use workbench_library::{
    LibraryEvidence, LibraryFormat, LibraryFreshness, LibraryItemState, LibraryPhase,
    LibraryReference, LibraryService, LibrarySnapshot, ScanAction,
};
use workbench_storage::{Source, WorkbenchStore, PRIVATE_DIRECTORY};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

fn image_bytes() -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(24, 32)
        .write_to(&mut output, ImageFormat::Png)
        .unwrap();
    output.into_inner()
}

fn archive(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in entries {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
}

fn finish(
    service: &mut LibraryService,
    store: &WorkbenchStore,
    mut snapshot: LibrarySnapshot,
) -> LibrarySnapshot {
    for _ in 0..2000 {
        if snapshot.phase != LibraryPhase::Reading {
            return snapshot;
        }
        snapshot = service
            .scan(
                store,
                snapshot.root_id.as_deref().unwrap(),
                snapshot.generation,
                ScanAction::Next,
            )
            .unwrap();
    }
    panic!("synthetic scan did not finish within its bounded fixture size");
}

fn named<'a>(snapshot: &'a LibrarySnapshot, name: &str) -> &'a workbench_library::LibraryItem {
    snapshot
        .items
        .iter()
        .find(|item| item.file_name == name)
        .unwrap()
}

fn jm_work(root: &Path, name: &str) {
    let work = root.join(name);
    fs::create_dir_all(work.join("chapter-01")).unwrap();
    fs::write(work.join("cover.jpg"), image_bytes()).unwrap();
    fs::write(work.join("chapter-01/001.jpg"), image_bytes()).unwrap();
    fs::write(work.join("chapter-01/002.jpg"), image_bytes()).unwrap();
    fs::write(work.join("元数据.json"), br#"{"id":123,"name":"Synthetic JM title","author":["Author"],"tags":["tag"],"chapterInfos":[{"chapterId":456,"chapterTitle":"One","order":1}]}"#).unwrap();
}

#[test]
fn directory_metadata_pages_cover_and_reopen_are_read_only() {
    let app = TempDir::new().unwrap();
    let media = TempDir::new().unwrap();
    jm_work(media.path(), "[Author] Synthetic work");
    let cover_path = media.path().join("[Author] Synthetic work/cover.jpg");
    let original = fs::read(&cover_path).unwrap();
    let store = WorkbenchStore::open(app.path()).unwrap();
    let mut service = LibraryService::new();
    assert_eq!(
        service.read(&store).unwrap().freshness,
        LibraryFreshness::None
    );
    let selected = service.choose(&store, media.path()).unwrap();
    assert_eq!(selected.phase, LibraryPhase::Reading);
    assert_eq!(selected.visited, 0);
    assert!(selected.items.is_empty());
    let result = finish(&mut service, &store, selected);
    assert_eq!(result.phase, LibraryPhase::Complete);
    let item = &result.items[0];
    assert_eq!(item.format, LibraryFormat::Directory);
    assert_eq!(item.title, "Synthetic JM title");
    assert_eq!(item.page_count, Some(2));
    assert_eq!(
        item.source_ref,
        Some(LibraryReference {
            source: Source::Jm,
            work_id: "123".into()
        })
    );
    assert_eq!(item.identity_evidence, Some(LibraryEvidence::Metadata));
    let cover = service
        .cover(
            &store,
            result.root_id.as_deref().unwrap(),
            result.generation,
            &item.id,
        )
        .unwrap();
    assert!(cover
        .data_url
        .unwrap()
        .starts_with("data:image/jpeg;base64,"));
    assert_eq!(fs::read(&cover_path).unwrap(), original);
    drop(service);
    let cached = LibraryService::new().read(&store).unwrap();
    assert_eq!(cached.freshness, LibraryFreshness::Cached);
    assert_eq!(cached.items, result.items);
    let private_files: Vec<_> = fs::read_dir(app.path().join(PRIVATE_DIRECTORY))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert!(private_files
        .iter()
        .all(|name| name == "library.json" || name == ".workbench.lock"));
    assert_eq!(fs::read_dir(media.path()).unwrap().count(), 1);
}

#[test]
fn pica_directory_shape_is_distinct_and_arbitrary_id_json_is_not_evidence() {
    let app = TempDir::new().unwrap();
    let media = TempDir::new().unwrap();
    for name in ["Pica work", "Unknown work"] {
        fs::create_dir(media.path().join(name)).unwrap();
        fs::write(media.path().join(name).join("cover.png"), image_bytes()).unwrap();
    }
    fs::write(media.path().join("Pica work/元数据.json"), br#"{"id":"0123456789abcdef01234567","title":"Pica title","author":"Writer","pagesCount":-1,"tags":["tag"],"chapterInfos":[],"thumb":{"path":"../../outside.png","fileServer":"https://invalid.example"}}"#).unwrap();
    fs::write(
        media.path().join("Unknown work/元数据.json"),
        br#"{"id":987,"name":"not a downloader document"}"#,
    )
    .unwrap();
    let store = WorkbenchStore::open(app.path()).unwrap();
    let mut service = LibraryService::new();
    let selected = service.choose(&store, media.path()).unwrap();
    let result = finish(&mut service, &store, selected);
    assert_eq!(
        named(&result, "Pica work")
            .source_ref
            .as_ref()
            .unwrap()
            .source,
        Source::Pica
    );
    let unknown = named(&result, "Unknown work");
    assert!(unknown.source_ref.is_none());
    assert_eq!(
        unknown.error_code.as_deref(),
        Some("LIBRARY_METADATA_INVALID")
    );
    assert!(unknown.cover_available);
}

#[test]
fn zip_cbz_metadata_and_bad_rar_files_are_isolated() {
    let app = TempDir::new().unwrap();
    let media = TempDir::new().unwrap();
    let png = image_bytes();
    archive(&media.path().join("valid.cbz"), &[("001.png", &png), ("ComicInfo.xml", b"<ComicInfo><Title>Title &amp; text</Title><Writer>A, B</Writer><Source>JM</Source><WorkId>123</WorkId></ComicInfo>")]);
    archive(
        &media.path().join("[JM:123] conflicting.zip"),
        &[
            ("001.png", &png),
            (
                "ComicInfo.xml",
                b"<ComicInfo><Source>JM</Source><WorkId>456</WorkId></ComicInfo>",
            ),
        ],
    );
    fs::write(media.path().join("broken.zip"), b"broken").unwrap();
    fs::write(media.path().join("unsupported.rar"), b"not decoded").unwrap();
    fs::write(media.path().join("ignore.txt"), b"ignored").unwrap();
    let store = WorkbenchStore::open(app.path()).unwrap();
    let mut service = LibraryService::new();
    let selected = service.choose(&store, media.path()).unwrap();
    let result = finish(&mut service, &store, selected);
    assert_eq!(result.phase, LibraryPhase::Complete);
    assert_eq!(result.items.len(), 4);
    assert_eq!(named(&result, "valid.cbz").title, "Title & text");
    assert_eq!(named(&result, "valid.cbz").authors, ["A", "B"]);
    assert_eq!(
        named(&result, "broken.zip").state,
        LibraryItemState::Unreadable
    );
    assert_eq!(
        named(&result, "unsupported.rar").state,
        LibraryItemState::Unsupported
    );
    let conflict = named(&result, "[JM:123] conflicting.zip");
    assert!(conflict.source_ref.is_none());
    assert_eq!(
        conflict.error_code.as_deref(),
        Some("LIBRARY_IDENTITY_CONFLICT")
    );
    let item = named(&result, "valid.cbz");
    assert!(service
        .cover(
            &store,
            result.root_id.as_deref().unwrap(),
            result.generation,
            &item.id
        )
        .unwrap()
        .data_url
        .is_some());
}

#[test]
fn large_work_is_incremental_pause_survives_and_restart_requires_refresh() {
    let app = TempDir::new().unwrap();
    let media = TempDir::new().unwrap();
    fs::create_dir(media.path().join("Many pages")).unwrap();
    for index in 0..350 {
        fs::write(
            media.path().join(format!("Many pages/{index:04}.jpg")),
            b"never decoded during scan",
        )
        .unwrap();
    }
    let store = WorkbenchStore::open(app.path()).unwrap();
    let mut service = LibraryService::new();
    let selected = service.choose(&store, media.path()).unwrap();
    let root = selected.root_id.unwrap();
    let first = service
        .scan(&store, &root, selected.generation, ScanAction::Next)
        .unwrap();
    assert_eq!(first.phase, LibraryPhase::Reading);
    assert!(first.visited <= 128);
    let paused = service
        .scan(&store, &root, first.generation, ScanAction::Pause)
        .unwrap();
    assert_eq!(paused.phase, LibraryPhase::Paused);
    let unchanged = service
        .scan(&store, &root, first.generation, ScanAction::Next)
        .unwrap();
    assert_eq!(unchanged.visited, paused.visited);
    drop(service);
    let mut reopened = LibraryService::new();
    let cached = reopened.read(&store).unwrap();
    assert_eq!(cached.phase, LibraryPhase::Paused);
    assert_eq!(cached.freshness, LibraryFreshness::Cached);
    assert_eq!(
        reopened
            .scan(&store, &root, first.generation, ScanAction::Resume)
            .unwrap_err()
            .code,
        "LIBRARY_RESTART_REQUIRED"
    );
    let start = reopened
        .scan(&store, &root, first.generation, ScanAction::Start)
        .unwrap();
    assert!(start.generation > first.generation);
    let finished = finish(&mut reopened, &store, start);
    assert_eq!(finished.items[0].page_count, Some(350));
}

#[test]
fn manual_link_and_unlink_survive_refresh_and_dont_touch_files() {
    let app = TempDir::new().unwrap();
    let media = TempDir::new().unwrap();
    let path = media.path().join("[JM:123] synthetic.zip");
    archive(&path, &[("001.png", &image_bytes())]);
    let original = fs::read(&path).unwrap();
    let store = WorkbenchStore::open(app.path()).unwrap();
    let mut service = LibraryService::new();
    let selected = service.choose(&store, media.path()).unwrap();
    let scanned = finish(&mut service, &store, selected);
    let root = scanned.root_id.as_deref().unwrap();
    let linked = service
        .link(
            &store,
            root,
            scanned.generation,
            &scanned.items[0].id,
            Some(LibraryReference {
                source: Source::Pica,
                work_id: "0123456789ABCDEF01234567".into(),
            }),
        )
        .unwrap();
    assert_eq!(
        linked.items[0].identity_evidence,
        Some(LibraryEvidence::Manual)
    );
    let started = service
        .scan(&store, root, linked.generation, ScanAction::Start)
        .unwrap();
    let refreshed = finish(&mut service, &store, started);
    assert_eq!(refreshed.items[0].source_ref, linked.items[0].source_ref);
    let unlinked = service
        .link(
            &store,
            root,
            refreshed.generation,
            &refreshed.items[0].id,
            None,
        )
        .unwrap();
    let started = service
        .scan(&store, root, unlinked.generation, ScanAction::Start)
        .unwrap();
    let refreshed = finish(&mut service, &store, started);
    assert!(refreshed.items[0].source_ref.is_none());
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn stale_root_or_modified_cover_is_rejected_without_reindexing() {
    let app = TempDir::new().unwrap();
    let media = TempDir::new().unwrap();
    jm_work(media.path(), "Work");
    let store = WorkbenchStore::open(app.path()).unwrap();
    let mut service = LibraryService::new();
    let selected = service.choose(&store, media.path()).unwrap();
    let scanned = finish(&mut service, &store, selected);
    let root = scanned.root_id.as_deref().unwrap();
    assert_eq!(
        service
            .cover(&store, root, scanned.generation + 1, &scanned.items[0].id)
            .unwrap_err()
            .code,
        "LIBRARY_STALE_SNAPSHOT"
    );
    fs::write(media.path().join("Work/cover.jpg"), b"changed bytes").unwrap();
    assert_eq!(
        service
            .cover(&store, root, scanned.generation, &scanned.items[0].id)
            .unwrap_err()
            .code,
        "LIBRARY_FILE_CHANGED"
    );
    assert_eq!(service.read(&store).unwrap().items, scanned.items);
}

#[test]
fn another_instance_refresh_invalidates_the_old_cursor_and_preserves_new_root() {
    let app = TempDir::new().unwrap();
    let one = TempDir::new().unwrap();
    let two = TempDir::new().unwrap();
    let store = WorkbenchStore::open(app.path()).unwrap();
    let observer = WorkbenchStore::open(app.path()).unwrap();
    let mut first = LibraryService::new();
    let mut second = LibraryService::new();
    let old = first.choose(&store, one.path()).unwrap();
    let current = second.choose(&observer, two.path()).unwrap();
    assert_eq!(
        first
            .scan(
                &store,
                old.root_id.as_deref().unwrap(),
                old.generation,
                ScanAction::Next
            )
            .unwrap_err()
            .code,
        "LIBRARY_STALE_SNAPSHOT"
    );
    let actual = first.read(&store).unwrap();
    assert_eq!(actual.root_id, current.root_id);
    assert_eq!(actual.generation, current.generation);
}

#[test]
fn unsafe_archive_names_and_xml_doctype_are_not_followed() {
    let app = TempDir::new().unwrap();
    let media = TempDir::new().unwrap();
    archive(
        &media.path().join("unsafe.zip"),
        &[("../outside.png", &image_bytes())],
    );
    archive(&media.path().join("doctype.zip"), &[("001.png", &image_bytes()), ("ComicInfo.xml", b"<!DOCTYPE ComicInfo [<!ENTITY file SYSTEM 'file:///outside'>]><ComicInfo><Title>&file;</Title></ComicInfo>")]);
    let store = WorkbenchStore::open(app.path()).unwrap();
    let mut service = LibraryService::new();
    let selected = service.choose(&store, media.path()).unwrap();
    let result = finish(&mut service, &store, selected);
    assert_eq!(
        named(&result, "unsafe.zip").error_code.as_deref(),
        Some("LIBRARY_ARCHIVE_UNSAFE")
    );
    assert_eq!(
        named(&result, "doctype.zip").error_code.as_deref(),
        Some("LIBRARY_METADATA_INVALID")
    );
    assert_eq!(fs::read_dir(media.path()).unwrap().count(), 2);
}

#[cfg(unix)]
#[test]
fn symlink_roots_and_children_are_never_followed() {
    use std::os::unix::fs::symlink;
    let app = TempDir::new().unwrap();
    let media = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("outside.jpg"), image_bytes()).unwrap();
    symlink(outside.path(), media.path().join("linked-work")).unwrap();
    symlink(
        outside.path().join("outside.jpg"),
        media.path().join("linked.zip"),
    )
    .unwrap();
    let store = WorkbenchStore::open(app.path()).unwrap();
    let mut service = LibraryService::new();
    assert_eq!(
        service
            .choose(&store, &media.path().join("linked-work"))
            .unwrap_err()
            .code,
        "LIBRARY_UNSAFE_PATH"
    );
    let selected = service.choose(&store, media.path()).unwrap();
    let result = finish(&mut service, &store, selected);
    assert!(result.items.is_empty());
    assert_eq!(result.skipped, 2);
}

#[test]
fn malformed_document_is_preserved_and_does_not_select_or_scan() {
    let app = TempDir::new().unwrap();
    let media = TempDir::new().unwrap();
    let store = WorkbenchStore::open(app.path()).unwrap();
    let path = app.path().join(PRIVATE_DIRECTORY).join("library.json");
    let bytes = b"{broken private index";
    fs::write(&path, bytes).unwrap();
    let mut service = LibraryService::new();
    assert_eq!(
        service.choose(&store, media.path()).unwrap_err().code,
        "DOCUMENT_CORRUPT"
    );
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn cover_decoder_rejects_disguised_bytes_without_invalidating_index() {
    let app = TempDir::new().unwrap();
    let media = TempDir::new().unwrap();
    archive(
        &media.path().join("disguised.zip"),
        &[("001.jpg", b"not an image")],
    );
    let store = WorkbenchStore::open(app.path()).unwrap();
    let mut service = LibraryService::new();
    let selected = service.choose(&store, media.path()).unwrap();
    let result = finish(&mut service, &store, selected);
    assert_eq!(result.items[0].state, LibraryItemState::Indexed);
    assert_eq!(
        service
            .cover(
                &store,
                result.root_id.as_deref().unwrap(),
                result.generation,
                &result.items[0].id
            )
            .unwrap_err()
            .code,
        "LIBRARY_COVER_INVALID"
    );
    assert_eq!(service.read(&store).unwrap().revision, result.revision);
}

#[test]
fn temporary_chapter_is_not_counted_or_selected_as_cover_and_is_preserved() {
    let app = TempDir::new().unwrap();
    let media = TempDir::new().unwrap();
    jm_work(media.path(), "Work");
    let temporary = media.path().join("Work/.下载中-chapter-01");
    fs::create_dir(&temporary).unwrap();
    fs::write(temporary.join("000.jpg"), b"unfinished temporary image").unwrap();
    let store = WorkbenchStore::open(app.path()).unwrap();
    let mut service = LibraryService::new();
    let selected = service.choose(&store, media.path()).unwrap();
    let result = finish(&mut service, &store, selected);
    assert_eq!(result.items[0].page_count, Some(2));
    assert_eq!(
        result.items[0].error_code.as_deref(),
        Some("LIBRARY_DOWNLOAD_INCOMPLETE")
    );
    assert!(service
        .cover(
            &store,
            result.root_id.as_deref().unwrap(),
            result.generation,
            &result.items[0].id
        )
        .unwrap()
        .data_url
        .is_some());
    assert_eq!(
        fs::read(temporary.join("000.jpg")).unwrap(),
        b"unfinished temporary image"
    );
}

#[test]
fn standalone_downloader_cover_does_not_claim_a_content_page() {
    let app = TempDir::new().unwrap();
    let media = TempDir::new().unwrap();
    fs::create_dir(media.path().join("Cover only")).unwrap();
    fs::write(media.path().join("Cover only/cover.jpg"), image_bytes()).unwrap();
    let store = WorkbenchStore::open(app.path()).unwrap();
    let mut service = LibraryService::new();
    let selected = service.choose(&store, media.path()).unwrap();
    let result = finish(&mut service, &store, selected);
    assert_eq!(result.items[0].page_count, Some(0));
    assert_eq!(
        result.items[0].error_code.as_deref(),
        Some("LIBRARY_COVER_ONLY")
    );
    assert!(result.items[0].cover_available);
}

#[cfg(windows)]
#[test]
fn live_scan_holds_root_against_directory_redirection() {
    let app = TempDir::new().unwrap();
    let parent = TempDir::new().unwrap();
    let original = parent.path().join("selected");
    let moved = parent.path().join("moved");
    fs::create_dir(&original).unwrap();
    let store = WorkbenchStore::open(app.path()).unwrap();
    let mut service = LibraryService::new();
    service.choose(&store, &original).unwrap();
    assert!(fs::rename(&original, &moved).is_err());
    drop(service);
    fs::rename(&original, &moved).unwrap();
    fs::create_dir(&original).unwrap();
    let mut reopened = LibraryService::new();
    let cached = reopened.read(&store).unwrap();
    assert_eq!(
        reopened
            .scan(
                &store,
                cached.root_id.as_deref().unwrap(),
                cached.generation,
                ScanAction::Start
            )
            .unwrap_err()
            .code,
        "LIBRARY_ROOT_CHANGED"
    );
}
