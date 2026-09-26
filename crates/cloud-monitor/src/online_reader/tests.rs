use super::*;
use std::io::Cursor;
use std::sync::atomic::AtomicUsize;

#[test]
fn reader_live_traffic_is_forbidden_in_actions_without_disabling_fixture_logic() {
    assert_eq!(
        runtime_is_allowed(Some("true")).unwrap_err().code,
        "READER_GITHUB_ACTIONS_FORBIDDEN"
    );
    assert!(runtime_is_allowed(None).is_ok());
    assert!(runtime_is_allowed(Some("false")).is_ok());
    assert!(reader().require_current().is_ok());
}

fn reader() -> OnlineReader {
    OnlineReader::new(
        ReaderSource::Jm,
        "123",
        "Synthetic reader book",
        None,
        || Ok(()),
    )
    .unwrap()
}

fn chapter(id: &str, order: u64) -> Chapter {
    Chapter {
        id: id.into(),
        title: format!("Chapter {order}"),
        order,
    }
}

fn pica_item(id: u64) -> PicaMediaItem {
    PicaMediaItem {
        media_id: format!("{id:024x}"),
        original_name: format!("{id}.jpg"),
        file_server: "https://storage-b.picacomic.com".into(),
        path: format!("media/fixture/{id}.jpg"),
        source_format: "jpg".into(),
    }
}

#[tokio::test]
async fn cancellation_aborts_waiters_and_revocation_discards_returning_results() {
    let online = Arc::new(reader());
    let started = Arc::new(Notify::new());
    let worker = Arc::clone(&online);
    let signal = Arc::clone(&started);
    let task = tokio::spawn(async move {
        worker
            .control
            .run(async {
                signal.notify_one();
                std::future::pending::<Result<(), String>>().await
            })
            .await
    });
    started.notified().await;
    online.cancel();
    assert_eq!(task.await.unwrap().unwrap_err(), "READER_CLOSED");
    assert_eq!(online.chapters(1).await.unwrap_err().code, "READER_CLOSED");

    let valid = Arc::new(AtomicBool::new(true));
    let lease = Arc::clone(&valid);
    let online = OnlineReader::new(ReaderSource::Jm, "123", "Book", None, move || {
        if lease.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err("SESSION_CHANGED".into())
        }
    })
    .unwrap();
    let result = online
        .control
        .run(async {
            valid.store(false, Ordering::Release);
            Ok(42)
        })
        .await;
    assert_eq!(result.unwrap_err(), "SESSION_CHANGED");
}

#[tokio::test]
async fn chapter_binding_rejects_unknown_id_before_source_requests_and_close_is_per_book() {
    let first = reader();
    let second = reader();
    assert_eq!(
        first.chapter("999").await.unwrap_err().code,
        "READER_CHAPTER_UNKNOWN"
    );
    assert_eq!(
        first.page("999", 0).await.err().unwrap().code,
        "READER_CHAPTER_UNKNOWN"
    );
    first.cancel();
    assert!(second.require_current().is_ok());
    assert_eq!(first.require_current().unwrap_err().code, "READER_CLOSED");
}

#[tokio::test]
async fn catalog_pages_reject_changes_and_duplicate_ids_without_losing_old_rows() {
    let online = reader();
    let mut state = online.metadata.lock().await;
    let scope = ReaderPageScope {
        total: 2,
        pages: 2,
        limit: 1,
    };
    let first = ChapterPage {
        items: vec![chapter("1", 1)],
        page: 1,
        has_more: true,
    };
    commit_directory(&mut state, first.clone(), Some(scope)).unwrap();
    let second = ChapterPage {
        items: vec![chapter("2", 2)],
        page: 2,
        has_more: false,
    };
    assert!(commit_directory(
        &mut state,
        second.clone(),
        Some(ReaderPageScope { total: 3, ..scope })
    )
    .is_err());
    assert!(commit_directory(
        &mut state,
        ChapterPage {
            items: vec![chapter("1", 2)],
            ..second.clone()
        },
        Some(scope)
    )
    .is_err());
    assert_eq!(state.chapters.len(), 1);
    assert_eq!(state.directory[&1], first);
    commit_directory(&mut state, second, Some(scope)).unwrap();
    assert_eq!(state.chapters.len(), 2);
}

#[tokio::test]
async fn out_of_range_image_does_not_fetch_and_metadata_cache_is_bounded() {
    let online = reader();
    {
        let mut state = online.metadata.lock().await;
        let row = chapter("1", 1);
        state.chapters.insert(row.id.clone(), row.clone());
        state.images.insert(
            row.id.clone(),
            PageCatalog {
                info: ChapterInfo {
                    id: row.id,
                    title: row.title,
                    order: row.order,
                    page_count: 1,
                },
                jm: vec![JmMediaItem {
                    filename: "001.webp".into(),
                    source_format: "webp".into(),
                    block_num: 0,
                }],
                pica_scope: None,
                pica_pages: BTreeMap::new(),
                pica_positions: BTreeMap::new(),
                pica_ids: BTreeMap::new(),
            },
        );
    }
    assert_eq!(
        online.page("1", 1).await.err().unwrap().code,
        "READER_PAGE_OUT_OF_RANGE"
    );
    let scope = ReaderPageScope {
        total: 20,
        pages: 20,
        limit: 1,
    };
    let mut catalog = PageCatalog {
        info: ChapterInfo {
            id: "1".into(),
            title: "Chapter".into(),
            order: 1,
            page_count: 20,
        },
        jm: vec![],
        pica_scope: Some(scope),
        pica_pages: BTreeMap::new(),
        pica_positions: BTreeMap::new(),
        pica_ids: BTreeMap::new(),
    };
    for page in 1..=20 {
        commit_image_page(
            &mut catalog,
            page,
            pica_adapter::reader::ReaderMediaPage {
                scope,
                items: vec![pica_item(page)],
            },
        )
        .unwrap();
        assert!(catalog.pica_pages.len() <= MAX_CACHED_IMAGE_PAGES);
    }
    // URL metadata for these old pages has been evicted, but their identities
    // still prevent moved/replaced pages from masquerading as valid retries.
    assert!(!catalog.pica_pages.contains_key(&1));
    for (page, id) in [(2, 1), (1, 21)] {
        assert!(commit_image_page(
            &mut catalog,
            page,
            pica_adapter::reader::ReaderMediaPage {
                scope,
                items: vec![pica_item(id)],
            },
        )
        .is_err());
    }
    commit_image_page(
        &mut catalog,
        1,
        pica_adapter::reader::ReaderMediaPage {
            scope,
            items: vec![pica_item(1)],
        },
    )
    .unwrap();
    assert!(catalog.pica_pages.len() <= MAX_CACHED_IMAGE_PAGES);
    assert_eq!(catalog.pica_ids.len(), 20);
    assert_eq!(catalog.pica_positions.len(), 20);
    assert!(commit_image_page(
        &mut catalog,
        1,
        pica_adapter::reader::ReaderMediaPage {
            scope,
            items: vec![pica_item(20)]
        }
    )
    .is_err());
    assert!(commit_image_page(
        &mut catalog,
        1,
        pica_adapter::reader::ReaderMediaPage {
            scope: ReaderPageScope {
                total: 21,
                pages: 21,
                ..scope
            },
            items: vec![pica_item(1)]
        }
    )
    .is_err());
}

#[test]
fn pica_preserves_detected_bytes_and_jm_shares_the_download_pixel_transform() {
    let source = ::image::RgbImage::from_fn(3, 4, |_, y| ::image::Rgb([(y * 55) as u8, 10, 20]));
    let mut encoded = Cursor::new(Vec::new());
    ::image::DynamicImage::ImageRgb8(source)
        .write_to(&mut encoded, ::image::ImageFormat::WebP)
        .unwrap();
    let bytes = encoded.into_inner();
    let pica = super::image::decode(ImageDescriptor::Pica(pica_item(1)), bytes.clone()).unwrap();
    assert_eq!(pica.mime, "image/webp");
    assert_eq!((pica.width, pica.height), (3, 4));
    assert_eq!(pica.bytes, bytes);
    let jm = super::image::decode(
        ImageDescriptor::Jm {
            chapter: "123".into(),
            item: JmMediaItem {
                filename: "001.webp".into(),
                source_format: "webp".into(),
                block_num: 2,
            },
        },
        bytes.clone(),
    )
    .unwrap();
    assert_eq!(jm.mime, "image/jpeg");
    assert_eq!((jm.width, jm.height), (3, 4));
    assert_eq!(
        jm.bytes,
        crate::jm_media_transform::apply("webp", "JM_SCRAMBLE_BLOCKS_JPEG", 2, bytes).unwrap()
    );
    assert!(super::image::decode(
        ImageDescriptor::Pica(pica_item(1)),
        b"<html>Unavailable</html>".to_vec()
    )
    .is_err());
}

#[tokio::test]
async fn failed_image_operation_can_be_retried_without_poisoning_reader() {
    let online = reader();
    let attempts = AtomicUsize::new(0);
    assert!(online
        .control
        .run(async {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>("READER_IMAGE_INVALID".into())
        })
        .await
        .is_err());
    online
        .control
        .run(async {
            attempts.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert!(online.require_current().is_ok());
}
