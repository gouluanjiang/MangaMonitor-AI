use super::*;
use base64::Engine;
use std::{
    fs,
    io::{Cursor, Write},
};

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
