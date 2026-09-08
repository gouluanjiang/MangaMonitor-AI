use cloud_monitor::{
    executor_handoff::ExecutorCommand,
    local_executor,
    monitor::{hash, Target},
    source_completion::{
        self, ChapterCompletion, PaginationProof, SourceCompletionTranscript,
        SOURCE_COMPLETION_SCHEMA_VERSION,
    },
};
use serde_json::Value;
use state_model::Version;

fn plan(source: &str) -> cloud_monitor::local_executor::LocalExecutionPlan {
    let source_work_id = if source == "jm" {
        "123456".to_string()
    } else {
        "0123456789abcdef01234567".to_string()
    };
    let target = Target {
        source_key: format!("{source}:{source_work_id}"),
        author: "Writer".into(),
        title: "Thaw Protocol".into(),
        version: Version::default(),
        coverage: Value::Null,
    };
    let target_hash = hash(&target);
    let task_id = "TASK_THAW_PROTOCOL";
    let task_revision = 1u64;
    let digest = hash(&(task_id, task_revision, target_hash.as_str()));
    let command = ExecutorCommand {
        schema_version: 1,
        command_id: format!("EXEC_{}", &digest[..20]),
        task_id: task_id.into(),
        work_id: "WORK_THAW_PROTOCOL".into(),
        task_revision,
        target_hash,
        source: source.into(),
        source_work_id,
        action: "download".into(),
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        target,
    };
    local_executor::plan(&command).unwrap()
}

fn chapter(image_pagination: Option<PaginationProof>) -> ChapterCompletion {
    ChapterCompletion {
        chapter_id: "111111111111111111111111".into(),
        chapter_order: 1,
        scheduled: true,
        joined: true,
        terminal_state: "COMPLETED".into(),
        expected_images: 2,
        completed_images: 2,
        failed_images: 0,
        image_pagination,
        artifact_paths: vec![
            "chapters/000001-111111111111111111111111/000001.jpg".into(),
            "chapters/000001-111111111111111111111111/000002.jpg".into(),
        ],
    }
}

#[test]
fn pica_incomplete_image_pagination_cannot_be_normalized_as_complete() {
    let plan = plan("pica");
    let transcript = SourceCompletionTranscript {
        schema_version: SOURCE_COMPLETION_SCHEMA_VERSION,
        command_id: plan.command_id.clone(),
        task_id: plan.task_id.clone(),
        work_id: plan.work_id.clone(),
        task_revision: plan.task_revision,
        target_hash: plan.target_hash.clone(),
        source: "pica".into(),
        source_work_id: plan.source_work_id.clone(),
        upstream_commit: source_completion::PICA_UPSTREAM_COMMIT.into(),
        scope: "FULL_SOURCE_WORK".into(),
        source_enumeration_complete: true,
        chapter_pagination: Some(PaginationProof {
            total_pages: 1,
            successful_pages: vec![1],
            failed_pages: vec![],
        }),
        expected_chapter_count: 1,
        all_scheduled_downloads_joined: true,
        chapters: vec![chapter(Some(PaginationProof {
            total_pages: 2,
            successful_pages: vec![1],
            failed_pages: vec![2],
        }))],
        artifacts: vec![],
    };

    assert_eq!(
        source_completion::normalize(&plan, &transcript).unwrap_err(),
        "PICA_IMAGE_PAGINATION_INCOMPLETE"
    );
}

#[test]
fn jm_partial_image_completion_cannot_be_normalized_as_complete() {
    let plan = plan("jm");
    let mut jm_chapter = ChapterCompletion {
        chapter_id: "123456".into(),
        chapter_order: 1,
        scheduled: true,
        joined: true,
        terminal_state: "COMPLETED".into(),
        expected_images: 2,
        completed_images: 1,
        failed_images: 1,
        image_pagination: None,
        artifact_paths: vec!["chapters/000001-123456/000001.webp".into()],
    };
    let transcript = SourceCompletionTranscript {
        schema_version: SOURCE_COMPLETION_SCHEMA_VERSION,
        command_id: plan.command_id.clone(),
        task_id: plan.task_id.clone(),
        work_id: plan.work_id.clone(),
        task_revision: plan.task_revision,
        target_hash: plan.target_hash.clone(),
        source: "jm".into(),
        source_work_id: plan.source_work_id.clone(),
        upstream_commit: source_completion::JM_UPSTREAM_COMMIT.into(),
        scope: "FULL_SOURCE_WORK".into(),
        source_enumeration_complete: true,
        chapter_pagination: None,
        expected_chapter_count: 1,
        all_scheduled_downloads_joined: true,
        chapters: vec![jm_chapter.clone()],
        artifacts: vec![],
    };

    assert_eq!(
        source_completion::normalize(&plan, &transcript).unwrap_err(),
        "SOURCE_COMPLETION_IMAGES_INCOMPLETE"
    );

    jm_chapter.completed_images = 2;
    jm_chapter.failed_images = 0;
    jm_chapter.joined = false;
    let mut not_joined = transcript;
    not_joined.chapters = vec![jm_chapter];
    assert_eq!(
        source_completion::normalize(&plan, &not_joined).unwrap_err(),
        "SOURCE_COMPLETION_CHAPTER_NOT_JOINED"
    );
}
