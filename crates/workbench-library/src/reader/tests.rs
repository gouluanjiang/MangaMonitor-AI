use super::*;
use crate::{LibraryPhase, LibraryService, ScanAction};
use image::DynamicImage;
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};
use workbench_storage::Source;
use zip::{write::SimpleFileOptions, ZipWriter};

fn png(width: u32) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(width, 12)
        .write_to(&mut out, ImageFormat::Png)
        .unwrap();
    out.into_inner()
}
fn zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut out = ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in entries {
        out.start_file(*name, SimpleFileOptions::default()).unwrap();
        out.write_all(bytes).unwrap();
    }
    out.finish().unwrap();
}
fn scan(store: &WorkbenchStore, root: &Path) -> crate::LibrarySnapshot {
    let mut service = LibraryService::new();
    let mut snapshot = service.choose(store, root).unwrap();
    while snapshot.phase == LibraryPhase::Reading {
        snapshot = service
            .scan(
                store,
                snapshot.root_id.as_deref().unwrap(),
                snapshot.generation,
                ScanAction::Next,
            )
            .unwrap();
    }
    snapshot
}

#[test]
fn indexed_zip_reads_original_requested_page_in_natural_chapter_order_without_registration() {
    let media = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let store = WorkbenchStore::open(private.path()).unwrap();
    let p1 = png(1200);
    let p2 = png(1300);
    let p10 = png(1400);
    let path = media.path().join("legacy.zip");
    zip(
        &path,
        &[
            ("chapter10/1.png", &p1),
            ("chapter2/10.png", &p10),
            ("chapter2/2.png", &p2),
            ("chapter2/1.png", &p1),
        ],
    );
    let ready = scan(&store, media.path());
    let before = fs::read(&path).unwrap();
    let library = store.read_library().unwrap();
    let reader = LocalReader::open(
        &store,
        ready.root_id.as_deref().unwrap(),
        ready.generation,
        &ready.items[0].id,
    )
    .unwrap();
    let chapters = reader.chapters();
    assert_eq!(
        chapters
            .iter()
            .map(|c| c.title.as_str())
            .collect::<Vec<_>>(),
        vec!["chapter2", "chapter10"]
    );
    assert_eq!(chapters[0].page_count, 3);
    let page = reader.page(&store, &chapters[0].id, 1).unwrap();
    assert_eq!(page.bytes, p2);
    assert_eq!(page.width, 1300);
    assert_eq!(page.mime, "image/png");
    assert_eq!(reader.page(&store, &chapters[0].id, 2).unwrap().bytes, p10);
    assert!(reader.page(&store, &chapters[0].id, 3).is_err());
    assert!(reader.page(&store, "../../untrusted", 0).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(store.read_library().unwrap(), library);
    assert_eq!(fs::read_dir(media.path()).unwrap().count(), 1);
    let mut changed = store.read_library().unwrap();
    changed.value.generation += 1;
    store
        .write_library(changed.revision, changed.value)
        .unwrap();
    assert_eq!(
        reader.page(&store, &chapters[0].id, 0).unwrap_err().code,
        "LIBRARY_STALE_SNAPSHOT"
    );
}

#[test]
fn source_preference_uses_confirmed_identifiers_not_filename_or_title_and_missing_is_distinct() {
    let media = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let store = WorkbenchStore::open(private.path()).unwrap();
    let path = media.path().join("[JM123] shared title.zip");
    zip(&path, &[("1.png", &png(100))]);
    scan(&store, media.path());
    let reference = LibraryReference {
        source: Source::Jm,
        work_id: "123".into(),
    };
    assert!(LocalReader::for_source(&store, &reference)
        .unwrap()
        .is_none());
    let mut document = store.read_library().unwrap();
    document.value.records[0].item.source_ref = Some(reference.clone());
    document.value.records[0].item.identity_evidence = Some(LibraryEvidence::Manual);
    document.value.records[0].manual_override = true;
    store
        .write_library(document.revision, document.value)
        .unwrap();
    let reader = LocalReader::for_source(&store, &reference)
        .unwrap()
        .unwrap();
    assert_eq!(reader.source_ref, Some(reference.clone()));
    drop(reader);
    fs::write(&path, b"a changed file").unwrap();
    assert!(LocalReader::for_source(&store, &reference).is_err());
    fs::remove_file(path).unwrap();
    assert!(LocalReader::for_source(&store, &reference)
        .unwrap()
        .is_none());
}

#[test]
fn metadata_index_excludes_only_generated_duplicate_cover_and_does_not_decode_unrequested_pages() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("synthetic.zip");
    zip(
        &path,
        &[
            ("cover.jpg", b"thumbnail"),
            ("_mangamonitor-layout.json", b"{}"),
            ("2/10.jpg", b"invalid image remains indexed"),
            ("2/2.jpg", b"second"),
            ("2/1.jpg", b"first"),
            ("10/1.jpg", b"last"),
        ],
    );
    let mut file = File::open(&path).unwrap();
    let chapters = pages::index(&mut file).unwrap();
    assert_eq!(chapters.len(), 2);
    assert_eq!(chapters[0].pages, vec!["2/1.jpg", "2/2.jpg", "2/10.jpg"]);
    drop(file);
    zip(
        &path,
        &[("cover.jpg", b"legacy cover"), ("1.jpg", b"first")],
    );
    let chapters = pages::index(&mut File::open(&path).unwrap()).unwrap();
    assert_eq!(chapters[0].pages.len(), 2);
    assert!(original_image(b"not an image".to_vec()).is_err());
}

#[test]
fn unsafe_zip_entries_are_rejected_and_large_headers_do_not_allocate_full_pixels() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("unsafe.zip");
    zip(&path, &[("../outside.png", b"not read")]);
    assert_eq!(
        pages::index(&mut File::open(path).unwrap())
            .err()
            .unwrap()
            .code,
        "LIBRARY_ARCHIVE_UNSAFE"
    );
    assert!(original_image(vec![0; archive::MAX_IMAGE_BYTES + 1]).is_err());
    assert!(original_image(png(20_001)).is_err());
}
