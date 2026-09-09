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
