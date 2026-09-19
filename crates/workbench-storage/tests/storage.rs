use base64::{engine::general_purpose::STANDARD, Engine};
use image::{DynamicImage, ImageFormat};
use serde_json::json;
use std::{
    fs::{self, File, OpenOptions},
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Arc, Barrier},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use workbench_storage::{
    background_from_path, Booklist, Booklists, ResourceProfile, Source, WorkIdentity,
    WorkbenchPreferences, WorkbenchStore, MAX_BACKGROUND_BYTES, MAX_SAFE_INTEGER,
    PRIVATE_DIRECTORY,
};

fn document_path(base: &Path, name: &str) -> PathBuf {
    base.join(PRIVATE_DIRECTORY).join(name)
}

fn list(id: &str, name: &str, members: Vec<WorkIdentity>) -> Booklist {
    Booklist {
        id: id.into(),
        name: name.into(),
        created_at: 1_700_000_000_000,
        updated_at: 1_700_000_000_000,
        archived: false,
        members,
    }
}

fn member(source: Source, id: &str) -> WorkIdentity {
    WorkIdentity {
        source,
        work_id: id.into(),
    }
}

fn encode_image(format: ImageFormat) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(2, 3)
        .write_to(&mut bytes, format)
        .unwrap();
    bytes.into_inner()
}

fn png_header_with_dimensions(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = encode_image(ImageFormat::Png);
    bytes[16..20].copy_from_slice(&width.to_be_bytes());
    bytes[20..24].copy_from_slice(&height.to_be_bytes());
    let mut crc = u32::MAX;
    for byte in &bytes[12..29] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    bytes[29..33].copy_from_slice(&(!crc).to_be_bytes());
    bytes
}

#[test]
fn missing_documents_default_once_and_independent_documents_survive_reopen() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    assert_eq!(store.read_preferences().unwrap().revision, 0);
    assert_eq!(store.read_booklists().unwrap().revision, 0);
    let mut preferences = WorkbenchPreferences::default();
    preferences.appearance.density = 9;
    let saved = store.write_preferences(0, preferences.clone()).unwrap();
    assert_eq!(saved.revision, 1);
    let booklists = Booklists {
        version: 1,
        lists: vec![list(
            "read_later",
            "待阅读",
            vec![member(Source::Jm, "123")],
        )],
    };
    store.write_booklists(0, booklists.clone()).unwrap();
    preferences.appearance.density = 5;
    assert_eq!(
        store
            .write_preferences(1, preferences.clone())
            .unwrap()
            .revision,
        2
    );
    drop(store);
    let reopened = WorkbenchStore::open(directory.path()).unwrap();
    assert_eq!(reopened.read_preferences().unwrap().value, preferences);
    assert_eq!(reopened.read_booklists().unwrap().value, booklists);
    assert_eq!(reopened.read_booklists().unwrap().revision, 1);
    assert!(!directory.path().join("preferences.json").exists());
}

#[test]
fn stale_revision_cannot_overwrite_another_instance() {
    let directory = TempDir::new().unwrap();
    let first = WorkbenchStore::open(directory.path()).unwrap();
    let second = WorkbenchStore::open(directory.path()).unwrap();
    let mut preferences = first.read_preferences().unwrap().value;
    preferences.appearance.density = 5;
    first.write_preferences(0, preferences.clone()).unwrap();
    assert_eq!(
        second
            .write_preferences(0, WorkbenchPreferences::default())
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
    assert_eq!(second.read_preferences().unwrap().value, preferences);
}

#[test]
fn corrupt_unknown_fields_and_future_documents_are_preserved_and_block_writes() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let path = document_path(directory.path(), "booklists.json");
    let cases = [
        ("{broken".to_owned(), "DOCUMENT_CORRUPT"),
        (json!({"schemaVersion": 2, "revision": 1, "value": {"version": 1, "lists": []}}).to_string(), "UNSUPPORTED_SCHEMA"),
        (json!({"schemaVersion": 1, "revision": 1, "value": {"version": 2, "lists": []}}).to_string(), "UNSUPPORTED_SCHEMA"),
        (json!({"schemaVersion": 1, "revision": 1, "value": {"version": 1, "lists": [], "secret": "unrecognized"}}).to_string(), "DOCUMENT_CORRUPT"),
        ("{\"schemaVersion\":1,\"revision\":1,\"value\":{\"version\":1,\"version\":1,\"lists\":[]}}".to_owned(), "DOCUMENT_CORRUPT"),
        (json!({"schemaVersion": 1, "revision": 0, "value": {"version": 1, "lists": []}}).to_string(), "DOCUMENT_CORRUPT"),
    ];
    for (original, expected) in cases {
        fs::write(&path, &original).unwrap();
        assert_eq!(store.read_booklists().unwrap_err().code, expected);
        assert_eq!(
            store
                .write_booklists(0, Booklists::default())
                .unwrap_err()
                .code,
            expected
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }
    // A blocked document does not stop the other independent document.
    store
        .write_preferences(0, WorkbenchPreferences::default())
        .unwrap();
}

#[test]
fn oversized_document_is_not_loaded_or_replaced() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let path = document_path(directory.path(), "booklists.json");
    let file = File::create(&path).unwrap();
    file.set_len(5 * 1024 * 1024 + 1).unwrap();
    drop(file);
    assert_eq!(
        store.read_booklists().unwrap_err().code,
        "DOCUMENT_TOO_LARGE"
    );
    assert_eq!(
        store
            .write_booklists(0, Booklists::default())
            .unwrap_err()
            .code,
        "DOCUMENT_TOO_LARGE"
    );
    assert_eq!(fs::metadata(path).unwrap().len(), 5 * 1024 * 1024 + 1);
}

#[test]
fn a_partial_temporary_is_never_recovered_as_committed_state() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let temporary = document_path(directory.path(), ".booklists.json.999.0.tmp");
    let original = b"{\"schemaVersion\":1,\"revision\":99,";
    fs::write(&temporary, original).unwrap();
    assert_eq!(store.read_booklists().unwrap().revision, 0);
    store.write_booklists(0, Booklists::default()).unwrap();
    assert_eq!(fs::read(temporary).unwrap(), original);
    assert_eq!(store.read_booklists().unwrap().revision, 1);
}

#[test]
fn exhausted_or_unsafe_revisions_never_wrap() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let path = document_path(directory.path(), "booklists.json");
    let bytes =
        json!({"schemaVersion": 1, "revision": MAX_SAFE_INTEGER, "value": Booklists::default()})
            .to_string();
    fs::write(&path, &bytes).unwrap();
    assert_eq!(
        store
            .write_booklists(MAX_SAFE_INTEGER, Booklists::default())
            .unwrap_err()
            .code,
        "REVISION_EXHAUSTED"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), bytes);
    assert_eq!(
        store
            .write_booklists(MAX_SAFE_INTEGER + 1, Booklists::default())
            .unwrap_err()
            .code,
        "VALIDATION_FAILED"
    );
}

#[test]
fn independent_instances_allow_exactly_one_compare_and_swap_winner() {
    let directory = TempDir::new().unwrap();
    let barrier = Arc::new(Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let root = directory.path().to_owned();
            let barrier = barrier.clone();
            thread::spawn(move || {
                let store = WorkbenchStore::open(root).unwrap();
                barrier.wait();
                store.write_booklists(0, Booklists::default())
            })
        })
        .collect();
    let mut winners = 0;
    for worker in workers {
        match worker.join().unwrap() {
            Ok(document) => {
                winners += 1;
                assert_eq!(document.revision, 1);
            }
            Err(error) => assert!(["BUSY", "REVISION_CONFLICT"].contains(&error.code)),
        }
    }
    assert_eq!(winners, 1);
    let store = WorkbenchStore::open(directory.path()).unwrap();
    assert_eq!(store.read_booklists().unwrap().revision, 1);
}

#[test]
fn live_os_lock_is_busy_but_an_unlocked_existing_lock_file_is_not_stale() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let path = document_path(directory.path(), ".workbench.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .unwrap();
    lock.lock().unwrap();
    assert_eq!(store.read_preferences().unwrap_err().code, "BUSY");
    assert_eq!(
        store
            .write_booklists(0, Booklists::default())
            .unwrap_err()
            .code,
        "BUSY"
    );
    drop(lock);
    assert!(path.exists());
    assert_eq!(
        store
            .write_booklists(0, Booklists::default())
            .unwrap()
            .revision,
        1
    );
}

#[test]
fn child_process_lock_holder() {
    let Some(root) = std::env::var_os("WORKBENCH_LOCK_TEST_ROOT") else {
        return;
    };
    let path = PathBuf::from(root);
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.join(".workbench.lock"))
        .unwrap();
    lock.lock().unwrap();
    fs::write(path.join("child-ready"), b"ready").unwrap();
    thread::sleep(Duration::from_secs(30));
}

#[test]
fn process_exit_releases_the_os_lock_without_deleting_the_lock_file() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let root = directory.path().join(PRIVATE_DIRECTORY);
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_process_lock_holder", "--nocapture"])
        .env("WORKBENCH_LOCK_TEST_ROOT", &root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !root.join("child-ready").exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    let ready = root.join("child-ready").exists();
    let result = if ready {
        Some(store.read_booklists())
    } else {
        None
    };
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(ready, "child process did not acquire its lock");
    assert_eq!(result.unwrap().unwrap_err().code, "BUSY");
    assert_eq!(
        store
            .write_booklists(0, Booklists::default())
            .unwrap()
            .revision,
        1
    );
}

#[test]
fn background_picker_accepts_actual_png_jpeg_webp_and_returns_basename_only() {
    let directory = TempDir::new().unwrap();
    for (format, mime) in [
        (ImageFormat::Png, "image/png"),
        (ImageFormat::Jpeg, "image/jpeg"),
        (ImageFormat::WebP, "image/webp"),
    ] {
        let path = directory.path().join("my-background.bin");
        fs::write(&path, encode_image(format)).unwrap();
        let selected = background_from_path(&path).unwrap();
        assert_eq!(selected.background_name, "my-background.bin");
        assert!(selected
            .background_image
            .starts_with(&format!("data:{mime};base64,")));
        assert!(!serde_json::to_string(&selected)
            .unwrap()
            .contains(directory.path().to_str().unwrap()));
    }
}

#[test]
fn background_limits_and_mime_signature_decode_checks_block_invalid_data() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let mut preferences = WorkbenchPreferences::default();
    preferences.appearance.background_name = Some("saved.png".into());
    let png = encode_image(ImageFormat::Png);
    preferences.appearance.background_image =
        Some(format!("data:image/jpeg;base64,{}", STANDARD.encode(&png)));
    assert_eq!(
        store
            .write_preferences(0, preferences.clone())
            .unwrap_err()
            .code,
        "BACKGROUND_INVALID"
    );
    preferences.appearance.background_image = Some(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(b"\x89PNG\r\n\x1a\ntruncated")
    ));
    assert_eq!(
        store
            .write_preferences(0, preferences.clone())
            .unwrap_err()
            .code,
        "BACKGROUND_INVALID"
    );
    preferences.appearance.background_image = Some("data:image/svg+xml;base64,PHN2Zz4=".into());
    assert_eq!(
        store
            .write_preferences(0, preferences.clone())
            .unwrap_err()
            .code,
        "BACKGROUND_INVALID"
    );
    preferences.appearance.background_image =
        Some(format!("data:image/png;base64,{}", STANDARD.encode(&png)));
    preferences.appearance.background_name = Some("saved.png".into());
    store.write_preferences(0, preferences).unwrap();
    let oversized = directory.path().join("oversized.png");
    let file = File::create(&oversized).unwrap();
    file.set_len(MAX_BACKGROUND_BYTES as u64 + 1).unwrap();
    drop(file);
    assert_eq!(
        background_from_path(&oversized).unwrap_err().code,
        "BACKGROUND_TOO_LARGE"
    );
    let mut corrupted = serde_json::to_value(store.read_preferences().unwrap().value).unwrap();
    corrupted["appearance"]["backgroundImage"] = json!("data:image/png;base64,iVBORw0KGgo=");
    let path = document_path(directory.path(), "preferences.json");
    let bytes = json!({"schemaVersion":1,"revision":1,"value":corrupted}).to_string();
    fs::write(&path, &bytes).unwrap();
    assert_eq!(
        store.read_preferences().unwrap_err().code,
        "DOCUMENT_CORRUPT"
    );
    assert_eq!(
        store
            .write_preferences(1, WorkbenchPreferences::default())
            .unwrap_err()
            .code,
        "DOCUMENT_CORRUPT"
    );
    assert_eq!(fs::read_to_string(path).unwrap(), bytes);
}

#[test]
fn resource_presets_are_enforced_and_custom_values_remain_bounded() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let mut preferences = WorkbenchPreferences::default();
    preferences.resources.profile = ResourceProfile::Economy;
    assert_eq!(
        store
            .write_preferences(0, preferences.clone())
            .unwrap_err()
            .code,
        "VALIDATION_FAILED"
    );
    preferences.resources.simultaneous_works = 1;
    preferences.resources.image_requests = 2;
    store.write_preferences(0, preferences.clone()).unwrap();
    preferences.resources.profile = ResourceProfile::Custom;
    preferences.resources.simultaneous_works = 4;
    preferences.resources.image_requests = 8;
    store.write_preferences(1, preferences.clone()).unwrap();
    preferences.resources.image_requests = 9;
    assert_eq!(
        store.write_preferences(2, preferences).unwrap_err().code,
        "VALIDATION_FAILED"
    );
}

#[test]
fn background_dimension_bombs_are_rejected_before_pixel_decompression() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("dimensions.png");
    for (width, height) in [(8193, 1), (1, 8193), (6000, 4001)] {
        fs::write(&path, png_header_with_dimensions(width, height)).unwrap();
        assert_eq!(
            background_from_path(&path).unwrap_err().code,
            "BACKGROUND_DIMENSIONS"
        );
    }
}

#[test]
fn background_name_and_data_are_paired_and_use_trimmed_utf16_bounds() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let mut preferences = WorkbenchPreferences::default();
    preferences.appearance.background_name = Some("alone.png".into());
    assert_eq!(
        store
            .write_preferences(0, preferences.clone())
            .unwrap_err()
            .code,
        "VALIDATION_FAILED"
    );
    preferences.appearance.background_name = None;
    preferences.appearance.background_image = Some(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(encode_image(ImageFormat::Png))
    ));
    assert_eq!(
        store
            .write_preferences(0, preferences.clone())
            .unwrap_err()
            .code,
        "VALIDATION_FAILED"
    );
    for invalid in [
        " padded.png".to_owned(),
        "padded.png ".to_owned(),
        "x".repeat(181),
        "😀".repeat(91),
    ] {
        preferences.appearance.background_name = Some(invalid);
        assert_eq!(
            store
                .write_preferences(0, preferences.clone())
                .unwrap_err()
                .code,
            "VALIDATION_FAILED"
        );
    }
    preferences.appearance.background_name = Some("😀".repeat(90));
    store.write_preferences(0, preferences).unwrap();
    let long_name = format!(" {}.png", "x".repeat(190));
    let path = directory.path().join(long_name);
    fs::write(&path, encode_image(ImageFormat::Png)).unwrap();
    let selected = background_from_path(&path).unwrap();
    assert_eq!(selected.background_name.encode_utf16().count(), 180);
    assert_eq!(selected.background_name.trim(), selected.background_name);
    let mut roundtrip = WorkbenchPreferences::default();
    roundtrip.appearance.background_image = Some(selected.background_image);
    roundtrip.appearance.background_name = Some(selected.background_name);
    store.write_preferences(1, roundtrip).unwrap();
}

#[cfg(unix)]
#[test]
fn background_picker_strips_control_characters_from_the_display_name() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("  my\nbackground.png");
    fs::write(&path, encode_image(ImageFormat::Png)).unwrap();
    assert_eq!(
        background_from_path(&path).unwrap().background_name,
        "mybackground.png"
    );
}

#[test]
fn cross_source_and_unknown_members_survive_and_duplicate_identity_is_rejected() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let mut booklists = Booklists {
        version: 1,
        lists: vec![list(
            "one",
            "Reading",
            vec![
                member(Source::Jm, "same"),
                member(Source::Pica, "same"),
                member(Source::Pica, "unknown-native-id"),
            ],
        )],
    };
    store.write_booklists(0, booklists.clone()).unwrap();
    assert_eq!(store.read_booklists().unwrap().value, booklists);
    booklists.lists[0].members.push(member(Source::Jm, "same"));
    assert_eq!(
        store.write_booklists(1, booklists).unwrap_err().code,
        "VALIDATION_FAILED"
    );
}

#[test]
fn active_names_are_trimmed_case_sensitive_unique_and_archive_preserves_members() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let mut booklists = Booklists {
        version: 1,
        lists: vec![list("one", "Read", vec![]), list("two", "read", vec![])],
    };
    store.write_booklists(0, booklists.clone()).unwrap();
    booklists.lists[1].name = "Read".into();
    assert_eq!(
        store
            .write_booklists(1, booklists.clone())
            .unwrap_err()
            .code,
        "VALIDATION_FAILED"
    );
    booklists.lists[1].archived = true;
    booklists.lists[1]
        .members
        .push(member(Source::Jm, "archived-member"));
    store.write_booklists(1, booklists.clone()).unwrap();
    assert_eq!(store.read_booklists().unwrap().value, booklists);
    booklists.lists[0].name = " Read ".into();
    assert_eq!(
        store
            .write_booklists(2, booklists.clone())
            .unwrap_err()
            .code,
        "VALIDATION_FAILED"
    );
    booklists.lists[0].name = "line\nbreak".into();
    assert_eq!(
        store.write_booklists(2, booklists).unwrap_err().code,
        "VALIDATION_FAILED"
    );
}

#[test]
fn booklist_ids_timestamps_and_member_budgets_are_validated() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let invalid_lists = [
        list("../escape", "One", vec![]),
        list("a", &"好".repeat(81), vec![]),
        list("a", "One", vec![member(Source::Jm, "a/b")]),
        list(&"x".repeat(81), "One", vec![]),
        Booklist {
            updated_at: MAX_SAFE_INTEGER + 1,
            ..list("a", "One", vec![])
        },
        Booklist {
            updated_at: 0,
            ..list("a", "One", vec![])
        },
        list(
            "a",
            "One",
            (0..2_001)
                .map(|i| member(Source::Jm, &i.to_string()))
                .collect(),
        ),
    ];
    for invalid in invalid_lists {
        assert_eq!(
            store
                .write_booklists(
                    0,
                    Booklists {
                        version: 1,
                        lists: vec![invalid]
                    }
                )
                .unwrap_err()
                .code,
            "VALIDATION_FAILED"
        );
    }
    let excess = Booklists {
        version: 1,
        lists: (0..101)
            .map(|i| list(&i.to_string(), &i.to_string(), vec![]))
            .collect(),
    };
    assert_eq!(
        store.write_booklists(0, excess).unwrap_err().code,
        "VALIDATION_FAILED"
    );
    let excess_members = Booklists {
        version: 1,
        lists: (0..11)
            .map(|i| {
                list(
                    &i.to_string(),
                    &i.to_string(),
                    (0..2_000)
                        .map(|j| member(Source::Jm, &j.to_string()))
                        .collect(),
                )
            })
            .collect(),
    };
    assert_eq!(
        store.write_booklists(0, excess_members).unwrap_err().code,
        "VALIDATION_FAILED"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_root_document_lock_and_background_are_refused() {
    use std::os::unix::fs::symlink;
    let directory = TempDir::new().unwrap();
    let external = TempDir::new().unwrap();
    let alias = directory.path().join("alias");
    symlink(external.path(), &alias).unwrap();
    assert!(matches!(WorkbenchStore::open(&alias), Err(error) if error.code == "UNSAFE_PATH"));
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let target = external.path().join("original.json");
    fs::write(&target, b"keep me").unwrap();
    let document = document_path(directory.path(), "booklists.json");
    symlink(&target, &document).unwrap();
    assert_eq!(
        store
            .write_booklists(0, Booklists::default())
            .unwrap_err()
            .code,
        "UNSAFE_PATH"
    );
    assert_eq!(fs::read(&target).unwrap(), b"keep me");
    let image = external.path().join("image.png");
    fs::write(&image, encode_image(ImageFormat::Png)).unwrap();
    let image_alias = directory.path().join("image.png");
    symlink(&image, &image_alias).unwrap();
    assert_eq!(
        background_from_path(&image_alias).unwrap_err().code,
        "UNSAFE_PATH"
    );
    let lock = document_path(directory.path(), ".workbench.lock");
    fs::remove_file(&lock).unwrap();
    symlink(&target, &lock).unwrap();
    assert_eq!(store.read_preferences().unwrap_err().code, "UNSAFE_PATH");
}

#[cfg(windows)]
#[test]
fn windows_directory_junction_is_refused_without_following_its_target() {
    let directory = TempDir::new().unwrap();
    let external = TempDir::new().unwrap();
    let alias = directory.path().join("junction");
    let status = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&alias)
        .arg(external.path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "junction fixture creation failed");
    let result = WorkbenchStore::open(&alias);
    // Remove only the fixture junction; never recurse through the external target.
    fs::remove_dir(&alias).unwrap();
    assert!(matches!(result, Err(error) if error.code == "UNSAFE_PATH"));
    assert!(!external.path().join(PRIVATE_DIRECTORY).exists());
}

#[cfg(windows)]
#[test]
fn windows_store_holds_its_private_directory_against_redirection() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    store.write_booklists(0, Booklists::default()).unwrap();
    let original = directory.path().join(PRIVATE_DIRECTORY);
    let moved = directory.path().join("moved-preview");
    assert!(fs::rename(&original, &moved).is_err());
    assert_eq!(store.read_booklists().unwrap().revision, 1);
    drop(store);
    fs::rename(&original, &moved).unwrap();
    fs::rename(&moved, &original).unwrap();
    assert_eq!(
        WorkbenchStore::open(directory.path())
            .unwrap()
            .read_booklists()
            .unwrap()
            .revision,
        1
    );
}

#[test]
fn error_serialization_contains_only_a_stable_code() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let path = document_path(directory.path(), "preferences.json");
    fs::write(path, b"private damaged contents").unwrap();
    let error = store.read_preferences().unwrap_err();
    assert_eq!(
        serde_json::to_string(&error).unwrap(),
        "{\"code\":\"DOCUMENT_CORRUPT\"}"
    );
}

#[test]
fn picker_names_match_javascript_trim_and_preserve_inner_bom() {
    let directory = TempDir::new().unwrap();
    let store = WorkbenchStore::open(directory.path()).unwrap();
    let cases = [
        ("\u{feff} wall\u{85}paper.png \u{feff}", "wallpaper.png"),
        ("\u{feff} a\u{feff}b.png \u{feff}", "a\u{feff}b.png"),
    ];
    for (revision, (filename, expected)) in cases.into_iter().enumerate() {
        let image_path = directory.path().join(filename);
        fs::write(&image_path, encode_image(ImageFormat::Png)).unwrap();
        let selected = background_from_path(&image_path).unwrap();
        assert_eq!(selected.background_name, expected);
        let mut preferences = WorkbenchPreferences::default();
        preferences.appearance.background_image = Some(selected.background_image);
        preferences.appearance.background_name = Some(selected.background_name);
        store
            .write_preferences(revision as u64, preferences.clone())
            .unwrap();
        assert_eq!(store.read_preferences().unwrap().value, preferences);
    }
}

#[test]
fn picker_name_truncation_does_not_split_an_astral_character() {
    let directory = TempDir::new().unwrap();
    for prefix in [178, 179] {
        let filename = format!("{}😀.png", "x".repeat(prefix));
        let image_path = directory.path().join(filename);
        fs::write(&image_path, encode_image(ImageFormat::Png)).unwrap();
        let selected = background_from_path(&image_path).unwrap();
        let expected = format!(
            "{}{}",
            "x".repeat(prefix),
            if prefix == 178 { "😀" } else { "" }
        );
        assert_eq!(selected.background_name, expected);
        assert!(selected.background_name.encode_utf16().count() <= 180);
    }
}

#[cfg(unix)]
#[test]
fn native_picker_removes_backslash_from_unix_display_names() {
    let directory = TempDir::new().unwrap();
    let image_path = directory.path().join("a\\b.png");
    fs::write(&image_path, encode_image(ImageFormat::Png)).unwrap();
    assert_eq!(
        background_from_path(&image_path).unwrap().background_name,
        "ab.png"
    );
}
