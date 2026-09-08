use cloud_monitor::assistant_author;
use serde_json::{json, Value};
use std::{fs, path::{Path, PathBuf}, process::Command};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn unique(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "mangamonitor-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn registry() -> Value {
    json!({
        "schema_version": 1,
        "authors": [
            {"author_id":"AUTHOR_0001","name":"Existing","enabled":true},
            {"author_id":"AUTHOR_0002","name":"Disabled","enabled":false}
        ]
    })
}

#[test]
fn add_disable_and_reenable_are_stable_and_idempotent() {
    let seed = registry();
    let (added, audit) = assistant_author::plan(&seed, "add", " New Author ").unwrap();
    assert_eq!(seed["authors"].as_array().unwrap().len(), 2);
    assert_eq!(added["authors"].as_array().unwrap().len(), 3);
    assert_eq!(audit["outcome"], "ADDED_NEW");
    let new_id = audit["author_id"].as_str().unwrap().to_owned();
    assert!(new_id.starts_with("AUTHOR_USER_"));
    assert_eq!(added["authors"][2]["name"], "New Author");

    let (again, audit) = assistant_author::plan(&added, "add", "new author").unwrap();
    assert_eq!(again, added);
    assert_eq!(audit["outcome"], "NOOP_ALREADY_ENABLED");
    assert_eq!(audit["author_id"], new_id);

    let (disabled, audit) = assistant_author::plan(&added, "disable", "NEW AUTHOR").unwrap();
    assert_eq!(audit["outcome"], "DISABLED_EXISTING");
    assert_eq!(audit["author_id"], new_id);
    assert_eq!(disabled["authors"][2]["enabled"], false);
    assert_eq!(disabled["authors"][2]["author_id"], new_id);

    let (reenabled, audit) = assistant_author::plan(&disabled, "add", "New Author").unwrap();
    assert_eq!(audit["outcome"], "REENABLED_EXISTING");
    assert_eq!(reenabled["authors"][2]["enabled"], true);
    assert_eq!(reenabled["authors"][2]["author_id"], new_id);
}

#[test]
fn unknown_empty_and_ambiguous_author_operations_fail_closed() {
    let seed = registry();
    assert_eq!(
        assistant_author::plan(&seed, "enable", "Unknown").unwrap_err(),
        "ASSISTANT_AUTHOR_NOT_FOUND"
    );
    assert_eq!(
        assistant_author::plan(&seed, "disable", "Unknown").unwrap_err(),
        "ASSISTANT_AUTHOR_NOT_FOUND"
    );
    assert_eq!(
        assistant_author::plan(&seed, "add", "   ").unwrap_err(),
        "INVALID_ASSISTANT_AUTHOR_NAME"
    );

    let case_collision = json!({"authors":[
        {"author_id":"A","name":"Writer","enabled":true},
        {"author_id":"B","name":"writer","enabled":true}
    ]});
    assert_eq!(
        assistant_author::plan(&case_collision, "add", "Other").unwrap_err(),
        "AMBIGUOUS_ASSISTANT_AUTHOR_CANONICAL_KEY"
    );

    let closed_alias_collision = json!({"authors":[
        {"author_id":"A","name":"10駅","enabled":true},
        {"author_id":"B","name":"10驛","enabled":true}
    ]});
    assert_eq!(
        assistant_author::plan(&closed_alias_collision, "add", "Other").unwrap_err(),
        "AMBIGUOUS_ASSISTANT_AUTHOR_CANONICAL_KEY"
    );
}

#[test]
fn enable_disable_preserve_imported_identity_and_noops_are_audited() {
    let seed = registry();
    let (enabled, audit) = assistant_author::plan(&seed, "enable", "Disabled").unwrap();
    assert_eq!(audit["outcome"], "ENABLED_EXISTING");
    assert_eq!(enabled["authors"][1]["author_id"], "AUTHOR_0002");
    assert_eq!(enabled["authors"][1]["name"], "Disabled");

    let (enabled_again, audit) = assistant_author::plan(&enabled, "enable", "disabled").unwrap();
    assert_eq!(audit["outcome"], "NOOP_ALREADY_ENABLED");
    assert_eq!(enabled_again, enabled);

    let (still_disabled, audit) = assistant_author::plan(&seed, "disable", "disabled").unwrap();
    assert_eq!(audit["outcome"], "NOOP_ALREADY_DISABLED");
    assert_eq!(still_disabled, seed);
}

#[test]
fn assistant_author_cli_stages_without_mutating_input_and_refuses_overwrite() {
    let input = root().join("monitor-state/authors.json");
    let before = fs::read(&input).unwrap();
    let output = unique("assistant-author-edit");
    let run = Command::new(env!("CARGO_BIN_EXE_assistant-author-edit"))
        .current_dir(root())
        .args(["--input"])
        .arg(&input)
        .args(["--output"])
        .arg(&output)
        .args(["--operation", "add", "--name", "Assistant Test Author"])
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .output()
        .unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(fs::read(&input).unwrap(), before);
    let proposed: Value = serde_json::from_slice(&fs::read(output.join("authors.json")).unwrap()).unwrap();
    let audit: Value = serde_json::from_slice(&fs::read(output.join("author-change.json")).unwrap()).unwrap();
    assert_eq!(audit["outcome"], "ADDED_NEW");
    assert!(proposed["authors"].as_array().unwrap().iter().any(|author| author["name"] == "Assistant Test Author"));

    let second = Command::new(env!("CARGO_BIN_EXE_assistant-author-edit"))
        .current_dir(root())
        .args(["--input"])
        .arg(&input)
        .args(["--output"])
        .arg(&output)
        .args(["--operation", "add", "--name", "Another Author"])
        .output()
        .unwrap();
    assert!(!second.status.success());
    assert!(String::from_utf8_lossy(&second.stderr).contains("ASSISTANT_AUTHOR_OUTPUT_EXISTS"));
    assert_eq!(fs::read(&input).unwrap(), before);
    fs::remove_dir_all(output).unwrap();
}
