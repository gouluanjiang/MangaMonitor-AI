use super::*;
use serde_json::{json, Value};
use tauri::test::{get_ipc_response, mock_builder, MockRuntime};

fn fixture() -> (tempfile::TempDir, tauri::App<MockRuntime>) {
    let root = tempfile::tempdir().unwrap();
    let app = app_with_root(Ok(root.path().to_owned()));
    (root, app)
}

fn app_with_root(root: Result<PathBuf, StoreError>) -> tauri::App<MockRuntime> {
    app_builder(mock_builder())
        .manage(Arc::new(DesktopStore::new(root.clone())))
        .manage(Arc::new(DesktopAccounts::new(root)))
        // Compile the real manifest/capabilities, not an allow-all mock context.
        .build(tauri::generate_context!())
        .unwrap()
}

fn window(app: &tauri::App<MockRuntime>, label: &str) -> WebviewWindow<MockRuntime> {
    WebviewWindowBuilder::new(app, label, Default::default())
        .build()
        .unwrap()
}

fn invoke_from(
    window: &WebviewWindow<MockRuntime>,
    origin: &str,
    command: &str,
    body: Value,
) -> Result<Value, Value> {
    get_ipc_response(
        window,
        tauri::webview::InvokeRequest {
            cmd: command.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: origin.parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    )
    .map(|response| response.deserialize().unwrap())
}

fn invoke(window: &WebviewWindow<MockRuntime>, command: &str, body: Value) -> Result<Value, Value> {
    let origin = if cfg!(windows) {
        "http://tauri.localhost"
    } else {
        "tauri://localhost"
    };
    invoke_from(window, origin, command, body)
}

#[test]
fn unavailable_storage_can_be_retried_in_the_same_window_after_path_repair() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("app-data");
    std::fs::write(&root, b"a regular file blocks the app-data directory").unwrap();
    let app = app_with_root(Ok(root.clone()));
    let main = window(&app, "main");
    let error = invoke(&main, "read_preferences", json!({})).unwrap_err();
    assert_eq!(error.as_object().unwrap().len(), 1);
    assert!(error["code"].as_str().is_some());
    assert!(!error.to_string().contains("app-data"));
    assert!(invoke(
        &main,
        "write_preferences",
        json!({"expectedRevision": 0, "value": WorkbenchPreferences::default()}),
    )
    .is_err());
    assert!(root.is_file());

    std::fs::remove_file(&root).unwrap();
    let recovered = invoke(&main, "read_preferences", json!({})).unwrap();
    assert_eq!(recovered["revision"], json!(0));
    assert!(root.join(workbench_storage::PRIVATE_DIRECTORY).is_dir());
    let saved = invoke(
        &main,
        "write_preferences",
        json!({"expectedRevision": 0, "value": recovered["value"]}),
    )
    .unwrap();
    assert_eq!(saved["revision"], json!(1));
    assert_eq!(invoke(&main, "read_preferences", json!({})).unwrap(), saved);
}

#[test]
fn unresolved_app_data_path_keeps_the_window_available_and_reports_only_a_code() {
    let app = app_with_root(Err(StoreError {
        code: "APP_DATA_UNAVAILABLE",
    }));
    let main = window(&app, "main");
    for command in ["read_preferences", "read_booklists"] {
        assert_eq!(
            invoke(&main, command, json!({})).unwrap_err(),
            json!({"code": "APP_DATA_UNAVAILABLE"})
        );
    }
}

#[test]
fn delayed_worker_keeps_storage_alive_and_reports_success_only_after_the_write() {
    use std::sync::mpsc;
    use std::time::Duration;

    let root = tempfile::tempdir().unwrap();
    let state = Arc::new(DesktopStore::new(Ok(root.path().to_owned())));
    let weak_state = Arc::downgrade(&state);
    let document_path = root
        .path()
        .join(workbench_storage::PRIVATE_DIRECTORY)
        .join("preferences.json");
    assert!(!document_path.parent().unwrap().exists());
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let worker = tauri::async_runtime::spawn(async move {
        let result = with_store(state, move |store| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            store.write_preferences(0, WorkbenchPreferences::default())
        })
        .await;
        result_tx.send(result).unwrap();
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let state_is_alive = weak_state.upgrade().is_some();
    let response_pending = matches!(result_rx.try_recv(), Err(mpsc::TryRecvError::Empty));
    let document_pending = !document_path.exists();
    // Release before assertions so even a failed assertion cannot strand the worker.
    release_tx.send(()).unwrap();
    let saved = result_rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    tauri::async_runtime::block_on(worker).unwrap();
    assert!(state_is_alive);
    assert!(response_pending);
    assert!(document_pending);
    assert_eq!(saved.revision, 1);
    assert!(document_path.is_file());
}

#[test]
fn main_preferences_roundtrip_survives_store_reopen() {
    let (root, app) = fixture();
    let main = window(&app, "main");
    let initial = invoke(&main, "read_preferences", json!({})).unwrap();
    let mut value = initial["value"].clone();
    value["appearance"]["density"] = json!(5);
    let updated = invoke(
        &main,
        "write_preferences",
        json!({"expectedRevision": initial["revision"], "value": value}),
    )
    .unwrap();
    assert_eq!(updated["revision"], json!(1));
    assert_eq!(updated["value"]["appearance"]["density"], json!(5));
    assert_eq!(
        invoke(&main, "read_preferences", json!({})).unwrap(),
        updated
    );
    let reopened = WorkbenchStore::open(root.path()).unwrap();
    assert_eq!(
        serde_json::to_value(reopened.read_preferences().unwrap()).unwrap(),
        updated
    );
}

#[test]
fn stale_preference_write_preserves_the_new_document() {
    let (_root, app) = fixture();
    let main = window(&app, "main");
    let initial = invoke(&main, "read_preferences", json!({})).unwrap();
    let arguments = json!({"expectedRevision": initial["revision"], "value": initial["value"]});
    let saved = invoke(&main, "write_preferences", arguments.clone()).unwrap();
    assert!(invoke(&main, "write_preferences", arguments).is_err());
    assert_eq!(invoke(&main, "read_preferences", json!({})).unwrap(), saved);
}

#[test]
fn cross_source_booklist_roundtrip_uses_a_separate_document() {
    let (root, app) = fixture();
    let main = window(&app, "main");
    let preferences = invoke(&main, "read_preferences", json!({})).unwrap();
    let initial = invoke(&main, "read_booklists", json!({})).unwrap();
    let value = json!({
        "version": 1,
        "lists": [{
            "id": "list_1", "name": "Read later",
            "createdAt": 1700000000000_u64, "updatedAt": 1700000000000_u64,
            "archived": false,
            "members": [{"source": "JM", "workId": "123"}, {"source": "Pica", "workId": "123"}]
        }]
    });
    let updated = invoke(
        &main,
        "write_booklists",
        json!({"expectedRevision": initial["revision"], "value": value}),
    )
    .unwrap();
    assert_eq!(updated["revision"], json!(1));
    assert_eq!(
        updated["value"]["lists"][0]["members"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(invoke(&main, "read_booklists", json!({})).unwrap(), updated);
    assert_eq!(
        invoke(&main, "read_preferences", json!({})).unwrap(),
        preferences
    );
    let reopened = WorkbenchStore::open(root.path()).unwrap();
    assert_eq!(
        serde_json::to_value(reopened.read_booklists().unwrap()).unwrap(),
        updated
    );
}

#[test]
fn secondary_window_is_denied_every_native_command() {
    let (_root, app) = fixture();
    let main = window(&app, "main");
    let secondary = window(&app, "secondary");
    let preferences = invoke(&main, "read_preferences", json!({})).unwrap();
    let booklists = invoke(&main, "read_booklists", json!({})).unwrap();
    for (command, body) in [
        ("read_preferences", json!({})),
        (
            "write_preferences",
            json!({"expectedRevision": 0, "value": preferences["value"]}),
        ),
        ("read_booklists", json!({})),
        (
            "write_booklists",
            json!({"expectedRevision": 0, "value": booklists["value"]}),
        ),
        ("choose_background", json!({})),
    ] {
        let error = invoke(&secondary, command, body).unwrap_err();
        // A capability rejection happens before the command's defense-in-depth guard.
        assert!(
            error.is_string(),
            "{command}: expected Tauri ACL error, got {error}"
        );
    }
    assert_eq!(
        invoke(&main, "read_preferences", json!({})).unwrap(),
        preferences
    );
    assert_eq!(
        invoke(&main, "read_booklists", json!({})).unwrap(),
        booklists
    );
}

#[test]
fn remote_origin_cannot_read_documents_or_open_the_picker() {
    let (_root, app) = fixture();
    let main = window(&app, "main");
    for command in ["read_preferences", "read_booklists", "choose_background"] {
        assert!(invoke_from(&main, "https://example.invalid", command, json!({})).is_err());
    }
}

#[test]
fn main_cannot_invoke_generic_file_or_dialog_apis() {
    let (_root, app) = fixture();
    let main = window(&app, "main");
    for command in [
        "plugin:dialog|open",
        "plugin:fs|read_file",
        "plugin:shell|execute",
    ] {
        assert!(invoke(&main, command, json!({})).is_err());
    }
}

#[test]
fn navigation_is_limited_to_packaged_assets_and_the_development_server() {
    for url in [
        "tauri://localhost/index.html",
        "http://tauri.localhost/index.html",
        "https://tauri.localhost/",
    ] {
        assert!(trusted_navigation(&url.parse().unwrap(), false));
    }
    let development = "http://127.0.0.1:4173/".parse().unwrap();
    assert!(trusted_navigation(&development, true));
    assert!(!trusted_navigation(&development, false));
    for url in [
        "https://example.com",
        "file:///C:/private.txt",
        "http://127.0.0.1:4174",
        "http://tauri.localhost.example.com/",
        "http://user@tauri.localhost/",
    ] {
        assert!(!trusted_navigation(&url.parse().unwrap(), true));
    }
}

fn account_commands() -> Vec<(&'static str, Value)> {
    vec![
        ("source_accounts", json!({})),
        (
            "source_catalog",
            json!({"source":"JM","sessionId":"stale","folderId":null,"reverse":false,"action":"read"}),
        ),
        // Invalid login data is deliberate: these IPC tests never authenticate.
        (
            "source_login",
            json!({"source":"JM","username":"","password":"","remember":false}),
        ),
        ("source_logout", json!({"source":"JM","sessionId":null})),
        (
            "source_query",
            json!({"source":"JM","sessionId":"stale","kind":"favorites","query":"","folderId":null,"page":1}),
        ),
        (
            "source_favorite",
            json!({"source":"JM","sessionId":"stale","workId":"123","desired":true}),
        ),
        (
            "source_cover",
            json!({"source":"JM","sessionId":"stale","workId":"123"}),
        ),
        (
            "source_following",
            json!({"source":"JM","sessionId":"stale"}),
        ),
        (
            "source_follow",
            json!({"source":"JM","sessionId":"stale","kind":"author","value":"Author","desired":true,"expectedRevision":0}),
        ),
    ]
}

#[test]
fn account_commands_are_denied_for_secondary_windows_and_remote_origins() {
    let (_root, app) = fixture();
    let main = window(&app, "main");
    let secondary = window(&app, "secondary");
    for (command, body) in account_commands() {
        let secondary_error = invoke(&secondary, command, body.clone()).unwrap_err();
        assert!(
            secondary_error.is_string(),
            "{command}: expected capability denial"
        );
        let remote_error =
            invoke_from(&main, "https://example.invalid", command, body).unwrap_err();
        assert!(
            remote_error.is_string(),
            "{command}: expected remote-origin denial"
        );
    }
}

#[test]
fn library_commands_require_main_packaged_origin_and_never_offer_generic_paths() {
    let (_root, app) = fixture();
    let main = window(&app, "main");
    let secondary = window(&app, "secondary");
    let id = "a".repeat(64);
    let commands = [
        ("library_read", json!({})),
        ("library_choose", json!({})),
        (
            "library_scan",
            json!({"rootId":id,"generation":1,"action":"next"}),
        ),
        (
            "library_cover",
            json!({"rootId":id,"generation":1,"entryId":id}),
        ),
        (
            "library_link",
            json!({"rootId":id,"generation":1,"entryId":id,"reference":null}),
        ),
        ("phone_library_read", json!({})),
        ("phone_library_import", json!({"revision":0})),
        (
            "phone_library_mark",
            json!({"revision":0,"name":"Example","reference":null}),
        ),
        ("phone_library_unmark", json!({"revision":0,"entryId":id})),
    ];
    for (command, body) in commands {
        assert!(
            invoke(&secondary, command, body.clone())
                .unwrap_err()
                .is_string(),
            "{command}"
        );
        assert!(
            invoke_from(&main, "https://example.invalid", command, body)
                .unwrap_err()
                .is_string(),
            "{command}"
        );
    }
}

#[test]
fn download_commands_require_the_main_packaged_window() {
    let (_root, app) = fixture();
    let main = window(&app, "main");
    let secondary = window(&app, "secondary");
    for (command, body) in [
        ("jm_download_read", json!({})),
        ("jm_download_read", json!({"recheckFiles": false})),
        (
            "jm_download_prepare",
            json!({"scope":{"source":"JM","sessionId":"stale"},"input":"123","rootId":"a".repeat(64),"generation":1}),
        ),
        (
            "jm_download_confirm",
            json!({"planId":"a".repeat(64),"expectedRevision":1}),
        ),
        (
            "jm_download_control",
            json!({"scope":{"source":"JM","sessionId":"stale"},"taskId":"a".repeat(64),"expectedRevision":1,"action":"pause"}),
        ),
    ] {
        assert!(
            invoke(&secondary, command, body.clone())
                .unwrap_err()
                .is_string(),
            "{command}"
        );
        assert!(
            invoke_from(&main, "https://example.invalid", command, body)
                .unwrap_err()
                .is_string(),
            "{command}"
        );
    }
}

#[test]
fn reading_empty_download_queue_never_creates_media_or_changes_phone_inventory() {
    let (root, app) = fixture();
    let main = window(&app, "main");
    let phone = invoke(&main, "phone_library_read", json!({})).unwrap();
    let library = invoke(&main, "library_read", json!({})).unwrap();
    let queue = invoke(&main, "jm_download_read", json!({})).unwrap();
    assert_eq!(queue, json!({"revision":0,"tasks":[]}));
    for body in [
        json!({}),
        json!({"recheckFiles": true}),
        json!({"recheckFiles": false}),
    ] {
        assert_eq!(invoke(&main, "jm_download_read", body).unwrap(), queue);
    }
    assert_eq!(
        invoke(&main, "phone_library_read", json!({})).unwrap(),
        phone
    );
    assert_eq!(invoke(&main, "library_read", json!({})).unwrap(), library);
    assert!(!root
        .path()
        .join(workbench_storage::PRIVATE_DIRECTORY)
        .join("download-staging-v1")
        .exists());
}

#[test]
fn unknown_download_plan_never_creates_a_task_and_ci_refuses_live_execution() {
    let (_root, app) = fixture();
    let main = window(&app, "main");
    let problem = invoke(
        &main,
        "jm_download_confirm",
        json!({"planId":"a".repeat(64),"expectedRevision":1}),
    )
    .unwrap_err();
    let expected = if std::env::var("GITHUB_ACTIONS").is_ok_and(|v| v.eq_ignore_ascii_case("true"))
    {
        "DOWNLOAD_LIVE_EXECUTION_DISABLED_IN_CI"
    } else {
        "DOWNLOAD_PLAN_EXPIRED"
    };
    assert_eq!(problem, json!({"code":expected}));
    assert_eq!(
        invoke(&main, "jm_download_read", json!({})).unwrap()["tasks"],
        json!([])
    );
}

#[test]
fn phone_marks_survive_native_restart_without_removing_pc_copy() {
    let (root, app) = fixture();
    let main = window(&app, "main");
    let pc = root.path().join("pc-copy.zip");
    std::fs::write(&pc, b"retained PC bytes").unwrap();
    let initial = invoke(&main, "phone_library_read", json!({})).unwrap();
    assert_eq!(initial["importedNames"], json!([]));
    let marked = invoke(
        &main,
        "phone_library_mark",
        json!({
            "revision":initial["revision"],"name":"Example / subtitle.zip",
            "reference":{"source":"JM","workId":"123"}
        }),
    )
    .unwrap();
    assert_eq!(
        marked["manualEntries"][0]["name"],
        json!("Example / subtitle.zip")
    );
    assert_eq!(
        invoke(&main, "phone_library_read", json!({})).unwrap(),
        marked
    );
    drop(main);
    drop(app);
    let app = app_with_root(Ok(root.path().to_owned()));
    let main = window(&app, "main");
    assert_eq!(
        invoke(&main, "phone_library_read", json!({})).unwrap(),
        marked
    );
    assert_eq!(
        invoke(
            &main,
            "phone_library_mark",
            json!({"revision":0,"name":"Another","reference":null})
        )
        .unwrap_err(),
        json!({"code":"REVISION_CONFLICT"})
    );
    let unmarked = invoke(
        &main,
        "phone_library_unmark",
        json!({"revision":marked["revision"],"entryId":marked["manualEntries"][0]["id"]}),
    )
    .unwrap();
    assert_eq!(unmarked["manualEntries"], json!([]));
    assert_eq!(std::fs::read(&pc).unwrap(), b"retained PC bytes");
}

#[test]
fn native_library_restore_does_not_start_a_scan_or_accept_renderer_file_paths() {
    let (root, app) = fixture();
    let main = window(&app, "main");
    let before = invoke(&main, "library_read", json!({})).unwrap();
    assert_eq!(before["rootId"], Value::Null);
    assert_eq!(before["phase"], json!("idle"));
    let error = invoke(
        &main,
        "library_cover",
        json!({
            "rootId":"C:/private","generation":1,"entryId":"../private.txt"
        }),
    )
    .unwrap_err();
    assert!(error["code"].as_str().is_some());
    assert!(!error.to_string().contains("private.txt"));
    assert_eq!(invoke(&main, "library_read", json!({})).unwrap(), before);
    assert!(!root
        .path()
        .join(workbench_storage::PRIVATE_DIRECTORY)
        .join("library.json")
        .exists());
}

#[test]
fn account_state_uses_only_empty_test_vault_and_no_account_operations_fail_closed() {
    let (_root, app) = fixture();
    let main = window(&app, "main");
    let expected = json!([
        {"source":"JM","sessionId":null,"accountId":null,"displayName":null,"state":"disconnected","remembered":false,"errorCode":null},
        {"source":"Pica","sessionId":null,"accountId":null,"displayName":null,"state":"disconnected","remembered":false,"errorCode":null}
    ]);
    assert_eq!(
        invoke(&main, "source_accounts", json!({})).unwrap(),
        expected
    );
    assert_eq!(
        invoke(&main, "source_accounts", json!({"refresh":true})).unwrap(),
        expected
    );
    for (command, body) in account_commands() {
        match command {
            "source_accounts" | "source_logout" => continue,
            "source_login" => assert_eq!(
                invoke(&main, command, body).unwrap_err(),
                json!({"code":"LOGIN_INPUT_INVALID"})
            ),
            _ => assert_eq!(
                invoke(&main, command, body).unwrap_err(),
                json!({"code":"SESSION_CHANGED"})
            ),
        }
    }
    // Null is only accepted for the already-observed disconnected generation.
    let logged_out = invoke(
        &main,
        "source_logout",
        json!({"source":"JM","sessionId":null}),
    )
    .unwrap();
    assert_eq!(logged_out, expected[0]);
    assert_eq!(
        invoke(&main, "source_accounts", json!({})).unwrap(),
        expected
    );
}

#[test]
fn account_inputs_reject_unknown_sources_kinds_and_unsafe_boundaries() {
    let (_root, app) = fixture();
    let main = window(&app, "main");
    for (command, mut body) in account_commands() {
        if command == "source_accounts" {
            continue;
        }
        body["source"] = json!("unknown-source");
        assert!(invoke(&main, command, body).unwrap_err().is_string());
    }
    let invalid_logins = [
        json!({"source":"JM","username":"","password":"private-fixture-value","remember":false}),
        json!({"source":"Pica","username":"account","password":"secret\r\nheader","remember":true}),
        json!({"source":"JM","username":"account","password":"x".repeat(4097),"remember":false}),
    ];
    for body in invalid_logins {
        let error = invoke(&main, "source_login", body).unwrap_err();
        assert_eq!(error, json!({"code":"LOGIN_INPUT_INVALID"}));
    }
    for body in [
        json!({"source":"JM","sessionId":"stale","kind":"search","query":"query","folderId":null,"page":0}),
        json!({"source":"JM","sessionId":"stale","kind":"search","query":"query\n","folderId":null,"page":1}),
    ] {
        assert_eq!(
            invoke(&main, "source_query", body).unwrap_err(),
            json!({"code":"QUERY_INVALID"})
        );
    }
    assert!(invoke(&main, "source_query", json!({"source":"JM","sessionId":"stale","kind":"download","query":"","folderId":null,"page":1})).unwrap_err().is_string());
    assert!(invoke(&main, "source_follow", json!({"source":"JM","sessionId":"stale","kind":"path","value":"x","desired":true,"expectedRevision":0})).unwrap_err().is_string());
    assert_eq!(invoke(&main, "source_follow", json!({"source":"JM","sessionId":"stale","kind":"author","value":"bad\nname","desired":true,"expectedRevision":0})).unwrap_err(), json!({"code":"FOLLOWING_INPUT_INVALID"}));
}

#[test]
fn account_initialization_failure_keeps_window_and_old_commands_available() {
    let app = app_with_root(Err(StoreError {
        code: "APP_DATA_UNAVAILABLE",
    }));
    let main = window(&app, "main");
    for refresh in [false, true] {
        assert_eq!(
            invoke(&main, "source_accounts", json!({"refresh":refresh})).unwrap_err(),
            json!({"code":"APP_DATA_UNAVAILABLE"})
        );
    }
    assert_eq!(
        invoke(&main, "read_preferences", json!({})).unwrap_err(),
        json!({"code":"APP_DATA_UNAVAILABLE"})
    );
    assert!(app.get_webview_window("main").is_some());
}

fn seed_legacy_startup_cover(root: &std::path::Path) -> (PathBuf, PathBuf, Vec<u8>) {
    let private = root.join(workbench_storage::PRIVATE_DIRECTORY);
    let key = "1".repeat(64);
    let registry=serde_json::to_vec(&json!({"version":1,"accounts":[{"key":key,"catalogPeak":0,"coverPeak":64*1024*1024,"usedAt":1}]})).unwrap();
    let registry_path = private.join("cache-registry-v1.json");
    let cover_path = private.join(format!("cache-{key}-cover-000.bin"));
    std::fs::write(&registry_path, &registry).unwrap();
    std::fs::write(&cover_path, b"legacy-cover").unwrap();
    (registry_path, cover_path, registry)
}

#[test]
fn concurrent_first_document_reads_share_the_store_only_after_one_legacy_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let seeded = WorkbenchStore::open(root.path()).unwrap();
    let mut preferences = WorkbenchPreferences::default();
    preferences.appearance.density = 9;
    let expected_preferences = seeded.write_preferences(0, preferences).unwrap();
    let expected_booklists = seeded.write_booklists(0, Booklists::default()).unwrap();
    let private = root.path().join(workbench_storage::PRIVATE_DIRECTORY);
    let prefs_before = std::fs::read(private.join("preferences.json")).unwrap();
    let lists_before = std::fs::read(private.join("booklists.json")).unwrap();
    let (registry_path, cover_path, legacy_registry) = seed_legacy_startup_cover(root.path());
    drop(seeded);
    let state = Arc::new(DesktopStore::new(Ok(root.path().to_owned())));
    let ready = Arc::new(std::sync::Barrier::new(3));
    let preferences_worker = {
        let state = Arc::clone(&state);
        let ready = Arc::clone(&ready);
        let cover = cover_path.clone();
        std::thread::spawn(move || {
            ready.wait();
            let store = state.open().unwrap();
            assert!(!cover.exists());
            let value = store.read_preferences().unwrap();
            (store, value)
        })
    };
    let booklists_worker = {
        let state = Arc::clone(&state);
        let ready = Arc::clone(&ready);
        let cover = cover_path.clone();
        std::thread::spawn(move || {
            ready.wait();
            let store = state.open().unwrap();
            assert!(!cover.exists());
            let value = store.read_booklists().unwrap();
            (store, value)
        })
    };
    ready.wait();
    let (first, preferences) = preferences_worker.join().unwrap();
    let (second, booklists) = booklists_worker.join().unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(preferences, expected_preferences);
    assert_eq!(booklists, expected_booklists);
    assert_eq!(
        std::fs::read(private.join("preferences.json")).unwrap(),
        prefs_before
    );
    assert_eq!(
        std::fs::read(private.join("booklists.json")).unwrap(),
        lists_before
    );
    let registry: Value = serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
    assert_eq!(registry["accounts"][0]["coverPeak"], 0);
    // Publishing the store completes automatic initialization even if a later
    // process writes legacy data again. There is no repeated per-command cleanup.
    std::fs::write(registry_path, legacy_registry).unwrap();
    std::fs::write(&cover_path, b"later-legacy-cover").unwrap();
    assert!(Arc::ptr_eq(&first, &state.open().unwrap()));
    assert!(cover_path.exists());
    assert_eq!(
        state.open().unwrap().read_preferences().unwrap(),
        expected_preferences
    );
    assert_eq!(
        state.open().unwrap().read_booklists().unwrap(),
        expected_booklists
    );
}

#[test]
fn failed_startup_cleanup_still_publishes_one_readable_store_and_is_not_repeated() {
    let root = tempfile::tempdir().unwrap();
    let seeded = WorkbenchStore::open(root.path()).unwrap();
    let expected_preferences = seeded
        .write_preferences(0, WorkbenchPreferences::default())
        .unwrap();
    let expected_booklists = seeded.write_booklists(0, Booklists::default()).unwrap();
    let (registry_path, cover_path, legacy_registry) = seed_legacy_startup_cover(root.path());
    std::fs::write(&registry_path, b"corrupt-registry").unwrap();
    drop(seeded);
    let state = Arc::new(DesktopStore::new(Ok(root.path().to_owned())));
    let first = state.open().unwrap();
    assert_eq!(first.read_preferences().unwrap(), expected_preferences);
    assert_eq!(first.read_booklists().unwrap(), expected_booklists);
    assert_eq!(std::fs::read(&registry_path).unwrap(), b"corrupt-registry");
    assert!(cover_path.exists());
    std::fs::write(registry_path, legacy_registry).unwrap();
    let second = state.open().unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert!(cover_path.exists());
    assert_eq!(second.read_preferences().unwrap(), expected_preferences);
    assert_eq!(second.read_booklists().unwrap(), expected_booklists);
}
