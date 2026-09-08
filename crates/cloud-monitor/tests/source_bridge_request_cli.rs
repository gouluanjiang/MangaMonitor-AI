use cloud_monitor::{executor_handoff::ExecutorCommand, local_executor, monitor::{hash, Target}};
use serde_json::{json, Value};
use std::{fs, process::Command};

#[test]
fn source_bridge_request_cli_emits_disabled_pica_request_without_mutating_input() {
    let target: Target = serde_json::from_value(json!({
        "source_key":"pica:0123456789abcdef01234567","author":"Writer","title":"A6.6 CLI",
        "version":{"chinese":true,"uncensored":null,"color":null,"translation":"","sample":null},"coverage":null
    })).unwrap();
    let target_hash = hash(&target);
    let task_id = "TASK_A6_6_CLI".to_string();
    let revision = 1;
    let digest = hash(&(task_id.as_str(), revision, target_hash.as_str()));
    let plan = local_executor::plan(&ExecutorCommand {
        schema_version:1, command_id:format!("EXEC_{}", &digest[..20]), task_id,
        work_id:"WORK_A6_6_CLI".into(), task_revision:revision, target_hash,
        source:"pica".into(), source_work_id:"0123456789abcdef01234567".into(), action:"download".into(),
        intent:"DOWNLOAD_TO_STAGING_ONLY".into(), target,
    }).unwrap();
    let path = std::env::temp_dir().join(format!("mangamonitor-a6-6-cli-{}.json", std::process::id()));
    let _ = fs::remove_file(&path);
    fs::write(&path, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
    let before = fs::read(&path).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_assistant-source-bridge-request"))
        .arg("--plan").arg(&path).output().unwrap();
    let _ = fs::remove_file(&path);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["source"], "pica");
    assert_eq!(value["auth_mode"], "PICA_TOKEN_REQUIRED");
    assert_eq!(value["network_execution_enabled"], false);
    assert_eq!(value["staging_write_enabled"], false);
    assert_eq!(value["physical_delete_authorized"], false);
    assert_eq!(before, serde_json::to_vec_pretty(&plan).unwrap());
}
