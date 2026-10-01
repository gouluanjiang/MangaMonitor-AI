use super::*;
use base64::Engine;
use std::{
    fs,
    io::{Cursor, Write},
};

#[test]
fn native_registered_reader_windows_bind_requests_isolate_pages_and_keep_mutations_main_only() {
    let (private, app) = fixture();
    let main = window(&app, "main");
    let media = tempfile::tempdir().unwrap();
    let mut encoded = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(8, 8)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    for name in ["synthetic-a.zip", "synthetic-b.zip"] {
        let mut zip = zip::ZipWriter::new(fs::File::create(media.path().join(name)).unwrap());
        zip.start_file("1.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(encoded.get_ref()).unwrap();
        zip.finish().unwrap();
    }
    let store = WorkbenchStore::open(private.path()).unwrap();
    let mut service = workbench_library::LibraryService::new();
    let mut snapshot = service.choose(&store, media.path()).unwrap();
    while snapshot.phase == workbench_library::LibraryPhase::Reading {
        snapshot = service
            .scan(
                &store,
                snapshot.root_id.as_deref().unwrap(),
                snapshot.generation,
                workbench_library::ScanAction::Next,
            )
            .unwrap();
    }
    let request = |index: usize| json!({"kind":"library","rootId":snapshot.root_id,"generation":snapshot.generation,"entryId":snapshot.items[index].id});
    let readers = app.state::<Arc<crate::reader_windows::ReaderWindows>>();
    let reserved_a = readers
        .reserve(serde_json::from_value(request(0)).unwrap())
        .unwrap();
    let reserved_b = readers
        .reserve(serde_json::from_value(request(1)).unwrap())
        .unwrap();
    let a = window(&app, &reserved_a.label);
    let b = window(&app, &reserved_b.label);
    let forged = window(&app, "reader-window-forged");
    assert_eq!(
        invoke(&a, "reader_window_context", json!({})).unwrap()["request"],
        request(0)
    );
    assert_eq!(
        invoke(&forged, "reader_window_context", json!({})).unwrap_err(),
        json!({"code":"FORBIDDEN"})
    );
    assert!(invoke(
        &a,
        "reader_open",
        json!({"requestId":"wrong-book","request":request(1)})
    )
    .is_err());
    assert!(invoke_from(
        &a,
        "https://example.invalid",
        "reader_window_context",
        json!({})
    )
    .is_err());
    let book_a = invoke(
        &a,
        "reader_open",
        json!({"requestId":"shared-token","request":request(0)}),
    )
    .unwrap();
    let book_b = invoke(
        &b,
        "reader_open",
        json!({"requestId":"shared-token","request":request(1)}),
    )
    .unwrap();
    let book_main = invoke(
        &main,
        "reader_open",
        json!({"requestId":"shared-token","request":request(0)}),
    )
    .unwrap();
    assert_ne!(book_a["readerId"], book_b["readerId"]);
    assert_ne!(book_a["readerId"], book_main["readerId"]);
    let page_a = json!({"readerId":book_a["readerId"],"chapterId":book_a["chapters"][0]["id"],"pageIndex":0});
    assert_eq!(
        invoke(&a, "reader_page", page_a.clone()).unwrap()["width"],
        8
    );
    assert_eq!(
        invoke(&b, "reader_page", page_a.clone()).unwrap_err(),
        json!({"code":"READER_CLOSED"})
    );
    let before = invoke(&main, "jm_download_read", json!({})).unwrap();
    for (command, body) in [
        ("source_accounts", json!({})),
        ("special_start", json!({})),
        (
            "special_set",
            json!({"scopes":[],"author":"Synthetic","enabled":true}),
        ),
        ("special_mark_read", json!({"scopes":[],"identity":null})),
        ("library_choose", json!({})),
        ("jm_download_read", json!({})),
        ("reader_window_open", json!({"request":request(0)})),
        ("reader_main_close", json!({})),
        ("reader_main_ready", json!({})),
    ] {
        assert!(invoke(&a, command, body).is_err(), "{command}");
    }
    // A local book, foreign session ID, or caller-supplied work ID cannot become
    // a download handoff. No download or library action is performed by these IPCs.
    assert!(invoke(
        &a,
        "reader_window_download",
        json!({"readerId":book_a["readerId"]})
    )
    .is_err());
    assert!(invoke(
        &a,
        "reader_window_download",
        json!({"readerId":book_b["readerId"],"source":"JM","workId":"12345"})
    )
    .is_err());
    assert_eq!(
        invoke(&main, "jm_download_read", json!({})).unwrap(),
        before
    );
    invoke(
        &b,
        "reader_cancel_open",
        json!({"requestId":"shared-token"}),
    )
    .unwrap();
    assert!(invoke(&a, "reader_page", page_a.clone()).is_ok());
    invoke(&a, "reader_save_position", json!({"readerId":book_a["readerId"],"position":{"chapterId":book_a["chapters"][0]["id"],"pageIndex":0,"offset":0.3}})).unwrap();
    invoke(&a, "reader_close", json!({"readerId":book_a["readerId"]})).unwrap();
    assert!(invoke(&a, "reader_page", page_a).is_err());
    let resumed = invoke(
        &a,
        "reader_open",
        json!({"requestId":"resumed","request":request(0)}),
    )
    .unwrap();
    assert_eq!(resumed["position"]["offset"], 0.3);
    let main_page = json!({"readerId":book_main["readerId"],"chapterId":book_main["chapters"][0]["id"],"pageIndex":0});
    assert!(invoke(&main, "reader_page", main_page).is_ok());
}

#[test]
fn reader_commands_are_main_origin_only_and_reject_paths_unknown_sessions_and_invalid_positions() {
    let (_root, app) = fixture();
    let main = window(&app, "main");
    let secondary = window(&app, "secondary");
    for (command, body) in [
        (
            "reader_open",
            json!({"requestId":"native-origin","request":{"kind":"library","rootId":"a".repeat(64),"generation":1,"entryId":"b".repeat(64)}}),
        ),
        ("reader_cancel_open", json!({"requestId":"native-origin"})),
        (
            "reader_chapter",
            json!({"readerId":"missing","chapterId":"chapter"}),
        ),
        (
            "reader_page",
            json!({"readerId":"missing","chapterId":"chapter","pageIndex":0}),
        ),
        (
            "reader_save_position",
            json!({"readerId":"missing","position":{"chapterId":"chapter","pageIndex":0,"offset":0.5}}),
        ),
        ("reader_close", json!({"readerId":"missing"})),
        ("reader_fullscreen", json!({"fullscreen":true})),
    ] {
        assert!(invoke(&secondary, command, body.clone()).is_err());
        assert!(invoke_from(&main, "https://example.invalid", command, body).is_err());
    }
    assert!(invoke(&main,"reader_open",json!({"requestId":"native-bad-path","request":{"kind":"library","path":"C:\\untrusted.zip"}})).is_err());
    assert!(invoke(&main,"reader_open",json!({"requestId":"native-bad-source","request":{"kind":"source","source":"Pica","sessionId":"synthetic","workId":"https://example.invalid"}})).is_err());
    assert_eq!(
        invoke(
            &main,
            "reader_page",
            json!({"readerId":"missing","chapterId":"chapter","pageIndex":0})
        )
        .unwrap_err(),
        json!({"code":"READER_CLOSED"})
    );
}

#[test]
fn native_zip_reader_preserves_original_pages_resumes_position_and_revokes_old_sessions_without_media_writes(
) {
    let (private, app) = fixture();
    let main = window(&app, "main");
    let media = tempfile::tempdir().unwrap();
    let path = media.path().join("synthetic-reader.zip");
    let mut pixels = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(1200, 4)
        .write_to(&mut pixels, image::ImageFormat::Png)
        .unwrap();
    let image = pixels.into_inner();
    let mut writer = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    for name in ["chapter/1.png", "chapter/2.png"] {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&image).unwrap();
    }
    writer.finish().unwrap();
    let store = WorkbenchStore::open(private.path()).unwrap();
    let mut service = workbench_library::LibraryService::new();
    let mut snapshot = service.choose(&store, media.path()).unwrap();
    while snapshot.phase == workbench_library::LibraryPhase::Reading {
        snapshot = service
            .scan(
                &store,
                snapshot.root_id.as_deref().unwrap(),
                snapshot.generation,
                workbench_library::ScanAction::Next,
            )
            .unwrap();
    }
    let before = fs::read(&path).unwrap();
    let library = store.read_library().unwrap();
    let mut request = json!({"requestId":"native-first","request":{"kind":"library","rootId":snapshot.root_id,"generation":snapshot.generation,"entryId":snapshot.items[0].id}});
    let first = invoke(&main, "reader_open", request.clone()).unwrap();
    assert_eq!(first["origin"], "library");
    assert_eq!(first["chapters"][0]["pageCount"], 2);
    assert!(first["position"].is_null());
    let id = first["readerId"].clone();
    let chapter = first["chapters"][0]["id"].clone();
    let page = invoke(
        &main,
        "reader_page",
        json!({"readerId":id,"chapterId":chapter,"pageIndex":1}),
    )
    .unwrap();
    assert_eq!(page["width"], 1200);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(
            page["dataUrl"]
                .as_str()
                .unwrap()
                .strip_prefix("data:image/png;base64,")
                .unwrap(),
        )
        .unwrap();
    assert_eq!(bytes, image);
    let position = json!({"chapterId":chapter,"pageIndex":1,"offset":0.4});
    invoke(
        &main,
        "reader_save_position",
        json!({"readerId":id,"position":position}),
    )
    .unwrap();
    assert!(invoke(
        &main,
        "reader_save_position",
        json!({"readerId":id,"position":{"chapterId":chapter,"pageIndex":2,"offset":0.0}})
    )
    .is_err());
    invoke(&main, "reader_close", json!({"readerId":id})).unwrap();
    assert!(invoke(
        &main,
        "reader_page",
        json!({"readerId":id,"chapterId":chapter,"pageIndex":0})
    )
    .is_err());
    request["requestId"] = json!("native-second");
    let second = invoke(&main, "reader_open", request.clone()).unwrap();
    assert_eq!(second["position"], position);
    assert_ne!(second["readerId"], id);
    // Closing an obsolete overlay cannot close the newly opened book.
    invoke(&main, "reader_close", json!({"readerId":id})).unwrap();
    invoke(
        &main,
        "reader_cancel_open",
        json!({"requestId":"native-first"}),
    )
    .unwrap();
    invoke(
        &main,
        "reader_chapter",
        json!({"readerId":second["readerId"],"chapterId":chapter}),
    )
    .unwrap();
    request["requestId"] = json!("native-third");
    let third = invoke(&main, "reader_open", request).unwrap();
    assert!(invoke(
        &main,
        "reader_page",
        json!({"readerId":second["readerId"],"chapterId":chapter,"pageIndex":0})
    )
    .is_err());
    // Cancellation also covers the publication/IPC-delivery race: the UI may
    // still be waiting for open when the native session has already published.
    invoke(
        &main,
        "reader_cancel_open",
        json!({"requestId":"native-third"}),
    )
    .unwrap();
    assert!(invoke(
        &main,
        "reader_chapter",
        json!({"readerId":third["readerId"],"chapterId":chapter})
    )
    .is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(store.read_library().unwrap(), library);
    assert_eq!(fs::read_dir(media.path()).unwrap().count(), 1);
    assert!(!private
        .path()
        .join(workbench_storage::PRIVATE_DIRECTORY)
        .join("downloads.json")
        .exists());
}
