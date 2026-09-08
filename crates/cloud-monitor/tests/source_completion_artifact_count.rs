use cloud_monitor::{
    executor_handoff::ExecutorCommand,
    local_executor,
    monitor::{hash, Target},
    source_completion::{self, ChapterCompletion, SourceCompletionTranscript, JM_UPSTREAM_COMMIT},
    staging_manifest::StagedArtifact,
};
use serde_json::json;

#[test]
fn claimed_image_count_must_equal_staged_content_artifact_count() {
    let target: Target = serde_json::from_value(json!({
        "source_key":"jm:123456","author":"Writer","title":"A6.5 artifact count",
        "version":{"chinese":true,"uncensored":null,"color":null,"translation":"","sample":null},
        "coverage":null
    })).unwrap();
    let target_hash = hash(&target);
    let task_id = "TASK_A6_5_COUNT".to_string();
    let revision = 1;
    let digest = hash(&(task_id.as_str(), revision, target_hash.as_str()));
    let plan = local_executor::plan(&ExecutorCommand {
        schema_version:1, command_id:format!("EXEC_{}", &digest[..20]), task_id,
        work_id:"WORK_A6_5_COUNT".into(), task_revision:revision, target_hash,
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
            joined:true, terminal_state:"COMPLETED".into(), expected_images:2, completed_images:2,
            failed_images:0, image_pagination:None, artifact_paths:vec!["chapter/001.webp".into()] }],
        artifacts:vec![StagedArtifact { relative_path:"chapter/001.webp".into(), size_bytes:1,
            sha256:"a".repeat(64) }],
    };
    assert_eq!(
        source_completion::normalize(&plan, &transcript).unwrap_err(),
        "SOURCE_COMPLETION_IMAGE_ARTIFACT_COUNT_MISMATCH"
    );
}
