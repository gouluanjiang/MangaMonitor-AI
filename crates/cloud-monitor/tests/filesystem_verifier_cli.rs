use cloud_monitor::{
    executor_handoff::ExecutorCommand,
    local_executor,
    monitor::{hash, Target},
    staging_manifest::{StagedArtifact, StagingManifest},
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, process::Command, sync::atomic::{AtomicU64, Ordering}};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mangamonitor-a6-4-cli-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn filesystem_verifier_cli_is_read_only_and_emits_verified_binding() {
    let temp = Temp::new();
    let target: Target = serde_json::from_value(json!({
        "source_key":"jm:987654",
        "author":"Writer",
        "title":"A6.4 CLI fixture",
        "version":{"chinese":true,"uncensored":null,"color":null,"translation":"","sample":null},
        "coverage":null
    }))
    .unwrap();
    let target_hash = hash(&target);
    let task_id = "TASK_A6_4_CLI".to_string();
    let revision = 3;
    let digest = hash(&(task_id.as_str(), revision, target_hash.as_str()));
    let command = ExecutorCommand {
        schema_version: 1,
        command_id: format!("EXEC_{}", &digest[..20]),
        task_id,
        work_id: "WORK_A6_4_CLI".into(),
        task_revision: revision,
        target_hash,
        source: "jm".into(),
        source_work_id: "987654".into(),
        action: "download".into(),
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        target,
    };
    let plan = local_executor::plan(&command).unwrap();
    let command_root = temp.0.join("staging/commands").join(&plan.command_id);
    fs::create_dir_all(&command_root).unwrap();
    fs::write(command_root.join("001.jpg"), b"hello").unwrap();

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
            relative_path: "001.jpg".into(),
            size_bytes: 5,
            sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into(),
        }],
    };

    let plan_path = temp.0.join("plan.json");
    let manifest_path = temp.0.join("manifest.json");
    fs::write(&plan_path, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    let plan_before = fs::read(&plan_path).unwrap();
    let manifest_before = fs::read(&manifest_path).unwrap();
    let artifact_before = fs::read(command_root.join("001.jpg")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_assistant-staging-filesystem-verify"))
        .arg("--plan")
        .arg(&plan_path)
        .arg("--manifest")
        .arg(&manifest_path)
        .arg("--staging-root")
        .arg(temp.0.join("staging"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["filesystem_verified"], true);
    assert_eq!(result["command_id"], plan.command_id);
    assert_eq!(result["task_revision"], plan.task_revision);
    assert_eq!(result["target_hash"], plan.target_hash);
    assert_eq!(result["promotion_authorized"], false);
    assert_eq!(result["physical_delete_authorized"], false);

    assert_eq!(fs::read(&plan_path).unwrap(), plan_before);
    assert_eq!(fs::read(&manifest_path).unwrap(), manifest_before);
    assert_eq!(fs::read(command_root.join("001.jpg")).unwrap(), artifact_before);
}
