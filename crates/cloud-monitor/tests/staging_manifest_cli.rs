use cloud_monitor::{
    executor_handoff::ExecutorCommand,
    local_executor,
    monitor::{hash, Target},
    staging_manifest::{StagedArtifact, StagingManifest},
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

fn pair() -> (local_executor::LocalExecutionPlan, StagingManifest) {
    let target: Target = serde_json::from_value(json!({
        "source_key":"pica:abcdef",
        "author":"Writer",
        "title":"A6.3 CLI fixture",
        "version":{"chinese":true,"uncensored":null,"color":null,"translation":"","sample":null},
        "coverage":null
    }))
    .unwrap();
    let target_hash = hash(&target);
    let task_id = "TASK_A6_3_CLI".to_string();
    let task_revision = 1;
    let digest = hash(&(task_id.as_str(), task_revision, target_hash.as_str()));
    let command = ExecutorCommand {
        schema_version: 1,
        command_id: format!("EXEC_{}", &digest[..20]),
        task_id,
        work_id: "WORK_A6_3_CLI".into(),
        task_revision,
        target_hash,
        source: "pica".into(),
        source_work_id: "abcdef".into(),
        action: "download".into(),
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        target,
    };
    let plan = local_executor::plan(&command).unwrap();
    let manifest = StagingManifest {
        schema_version: 1,
        command_id: plan.command_id.clone(),
        task_id: plan.task_id.clone(),
        work_id: plan.work_id.clone(),
        task_revision: plan.task_revision,
        target_hash: plan.target_hash.clone(),
        backend: plan.backend.clone(),
        source_work_id: plan.source_work_id.clone(),
        staging_subdir: plan.staging_subdir.clone(),
        source_enumeration_complete: true,
        all_scheduled_downloads_joined: true,
        downloader_reported_full_completion: true,
        expected_content_units: 1,
        completed_content_units: 1,
        failed_content_units: 0,
        artifacts: vec![StagedArtifact {
            relative_path: "chapter-01/001.jpg".into(),
            size_bytes: 42,
            sha256: "c".repeat(64),
        }],
    };
    (plan, manifest)
}

#[test]
fn manifest_cli_is_offline_read_only_and_non_destructive() {
    let (plan, manifest) = pair();
    let plan_path = unique("a6-3-plan.json");
    let manifest_path = unique("a6-3-manifest.json");
    fs::write(&plan_path, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let plan_before = fs::read(&plan_path).unwrap();
    let manifest_before = fs::read(&manifest_path).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_assistant-staging-manifest-check"))
        .args(["--plan"])
        .arg(&plan_path)
        .args(["--manifest"])
        .arg(&manifest_path)
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
    assert_eq!(value["file_count"], 1);
    assert_eq!(value["total_bytes"], 42);
    assert_eq!(value["inventory_mutation_authorized"], false);
    assert_eq!(value["task_completion_authorized"], false);
    assert_eq!(value["promotion_authorized"], false);
    assert_eq!(value["replacement_authorized"], false);
    assert_eq!(value["physical_delete_authorized"], false);
    assert_eq!(plan_before, fs::read(&plan_path).unwrap());
    assert_eq!(manifest_before, fs::read(&manifest_path).unwrap());

    fs::remove_file(plan_path).unwrap();
    fs::remove_file(manifest_path).unwrap();
}
