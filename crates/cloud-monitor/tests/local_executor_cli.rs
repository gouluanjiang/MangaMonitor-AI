use cloud_monitor::{
    executor_handoff::ExecutorCommand,
    monitor::{hash, Target},
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, process::Command};

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

fn command() -> ExecutorCommand {
    let target: Target = serde_json::from_value(json!({
        "source_key":"jm:123456",
        "author":"Writer",
        "title":"A6.2 CLI fixture",
        "version":{"chinese":true,"uncensored":null,"color":null,"translation":"","sample":null},
        "coverage":null
    }))
    .unwrap();
    let target_hash = hash(&target);
    let task_id = "TASK_A6_2_CLI".to_string();
    let task_revision = 2;
    let digest = hash(&(task_id.as_str(), task_revision, target_hash.as_str()));
    ExecutorCommand {
        schema_version: 1,
        command_id: format!("EXEC_{}", &digest[..20]),
        task_id,
        work_id: "WORK_A6_2_CLI".into(),
        task_revision,
        target_hash,
        source: "jm".into(),
        source_work_id: "123456".into(),
        action: "download".into(),
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        target,
    }
}

#[test]
fn local_executor_plan_cli_is_offline_and_non_executable() {
    let path = unique("a6-local-command.json");
    fs::write(&path, serde_json::to_vec_pretty(&command()).unwrap()).unwrap();
    let before = fs::read(&path).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_assistant-local-executor-plan"))
        .args(["--command"])
        .arg(&path)
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["backend"], "JM");
    assert_eq!(value["intent"], "DOWNLOAD_TO_STAGING_ONLY");
    assert_eq!(value["execution_supported"], false);
    assert_eq!(value["promotion_authorized"], false);
    assert_eq!(value["replacement_authorized"], false);
    assert_eq!(value["physical_delete_authorized"], false);
    assert_eq!(before, fs::read(&path).unwrap());

    fs::remove_file(path).unwrap();
}

#[test]
fn local_executor_plan_cli_rejects_forged_command() {
    let path = unique("a6-local-forged.json");
    let mut forged = command();
    forged.target.title.push_str(" forged");
    fs::write(&path, serde_json::to_vec_pretty(&forged).unwrap()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_assistant-local-executor-plan"))
        .args(["--command"])
        .arg(&path)
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "LOCAL_EXECUTOR_TARGET_HASH_MISMATCH"
    );

    fs::remove_file(path).unwrap();
}
