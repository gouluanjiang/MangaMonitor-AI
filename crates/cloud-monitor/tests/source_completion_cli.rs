use cloud_monitor::{
    executor_handoff::ExecutorCommand,
    local_executor,
    monitor::{hash, Target},
    source_completion::{ChapterCompletion, SourceCompletionTranscript, JM_UPSTREAM_COMMIT},
    staging_manifest::StagedArtifact,
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, process::Command, sync::atomic::{AtomicU64, Ordering}};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("mangamonitor-a6-5-cli-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

#[test]
fn source_completion_cli_is_offline_read_only_normalization() {
    let temp = Temp::new();
    let target: Target = serde_json::from_value(json!({
        "source_key":"jm:123456","author":"Writer","title":"A6.5 CLI",
        "version":{"chinese":true,"uncensored":null,"color":null,"translation":"","sample":null},
        "coverage":null
    })).unwrap();
    let target_hash = hash(&target);
    let task_id = "TASK_A6_5_CLI".to_string();
    let revision = 1;
    let digest = hash(&(task_id.as_str(), revision, target_hash.as_str()));
    let plan = local_executor::plan(&ExecutorCommand {
        schema_version:1, command_id:format!("EXEC_{}", &digest[..20]), task_id,
        work_id:"WORK_A6_5_CLI".into(), task_revision:revision, target_hash,
        source:"jm".into(), source_work_id:"123456".into(), action:"download".into(),
        intent:"DOWNLOAD_TO_STAGING_ONLY".into(), target,
    }).unwrap();
    let transcript = SourceCompletionTranscript {
        schema_version:1, command_id:plan.command_id.clone(), task_id:plan.task_id.clone(),
        work_id:plan.work_id.clone(), task_revision:plan.task_revision, target_hash:plan.target_hash.clone(),
        source:"jm".into(), source_work_id:plan.source_work_id.clone(), upstream_commit:JM_UPSTREAM_COMMIT.into(),
        scope:"FULL_SOURCE_WORK".into(), source_enumeration_complete:true, chapter_pagination:None,
        expected_chapter_count:1, all_scheduled_downloads_joined:true,
        chapters:vec![ChapterCompletion { chapter_id:"1001".into(), chapter_order:1, scheduled:true,
            joined:true, terminal_state:"COMPLETED".into(), expected_images:1, completed_images:1,
            failed_images:0, image_pagination:None, artifact_paths:vec!["chapter/001.webp".into()] }],
        artifacts:vec![StagedArtifact { relative_path:"chapter/001.webp".into(), size_bytes:1,
            sha256:"a".repeat(64) }],
    };
    let plan_path = temp.0.join("plan.json");
    let transcript_path = temp.0.join("transcript.json");
    fs::write(&plan_path, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
    fs::write(&transcript_path, serde_json::to_vec_pretty(&transcript).unwrap()).unwrap();
    let before_plan = fs::read(&plan_path).unwrap();
    let before_transcript = fs::read(&transcript_path).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_assistant-source-completion-normalize"))
        .arg("--plan").arg(&plan_path).arg("--transcript").arg(&transcript_path).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["source_contract_verified"], true);
    assert_eq!(value["execution_supported"], false);
    assert_eq!(value["manifest"]["expected_content_units"], 1);
    assert_eq!(value["physical_delete_authorized"], false);
    assert_eq!(fs::read(&plan_path).unwrap(), before_plan);
    assert_eq!(fs::read(&transcript_path).unwrap(), before_transcript);
}
