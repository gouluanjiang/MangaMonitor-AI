use cloud_monitor::{
    executor_handoff::ExecutorCommand,
    local_executor,
    monitor::{hash, Target},
    source_bridge_request,
    source_completion::{ChapterCompletion, SourceCompletionTranscript},
    source_preflight::{self, PreflightChapter, SourcePreflightEvidence},
};
use serde_json::json;

fn setup() -> (
    local_executor::LocalExecutionPlan,
    source_bridge_request::SourceBridgeRequest,
    SourcePreflightEvidence,
) {
    let target: Target = serde_json::from_value(json!({
        "source_key":"jm:123456","author":"Writer","title":"A6.7 Completion Scope",
        "version":{"chinese":true,"uncensored":null,"color":null,"translation":"","sample":null},"coverage":null
    }))
    .unwrap();
    let target_hash = hash(&target);
    let task_id = "TASK_A6_7_SCOPE".to_string();
    let revision = 1;
    let digest = hash(&(task_id.as_str(), revision, target_hash.as_str()));
    let plan = local_executor::plan(&ExecutorCommand {
        schema_version: 1,
        command_id: format!("EXEC_{}", &digest[..20]),
        task_id,
        work_id: "WORK_A6_7_SCOPE".into(),
        task_revision: revision,
        target_hash,
        source: "jm".into(),
        source_work_id: "123456".into(),
        action: "download".into(),
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        target,
    })
    .unwrap();
    let request = source_bridge_request::build(&plan).unwrap();
    let evidence = SourcePreflightEvidence {
        schema_version: 1,
        command_id: request.command_id.clone(),
        task_id: request.task_id.clone(),
        work_id: request.work_id.clone(),
        task_revision: request.task_revision,
        target_hash: request.target_hash.clone(),
        source: request.source.clone(),
        source_work_id: request.source_work_id.clone(),
        upstream_commit: request.upstream_commit.clone(),
        completion_contract_version: request.completion_contract_version,
        scope: request.scope.clone(),
        source_enumeration_complete: true,
        chapter_pagination: None,
        expected_chapter_count: 2,
        chapters: vec![
            PreflightChapter {
                chapter_id: "1001".into(),
                chapter_order: 1,
                expected_images: 3,
                image_pagination: None,
            },
            PreflightChapter {
                chapter_id: "1002".into(),
                chapter_order: 2,
                expected_images: 4,
                image_pagination: None,
            },
        ],
        image_bytes_downloaded: false,
        staging_written: false,
    };
    (plan, request, evidence)
}

fn transcript(evidence: &SourcePreflightEvidence) -> SourceCompletionTranscript {
    SourceCompletionTranscript {
        schema_version: evidence.completion_contract_version,
        command_id: evidence.command_id.clone(),
        task_id: evidence.task_id.clone(),
        work_id: evidence.work_id.clone(),
        task_revision: evidence.task_revision,
        target_hash: evidence.target_hash.clone(),
        source: evidence.source.clone(),
        source_work_id: evidence.source_work_id.clone(),
        upstream_commit: evidence.upstream_commit.clone(),
        scope: evidence.scope.clone(),
        source_enumeration_complete: true,
        chapter_pagination: evidence.chapter_pagination.clone(),
        expected_chapter_count: evidence.expected_chapter_count,
        all_scheduled_downloads_joined: true,
        chapters: evidence
            .chapters
            .iter()
            .map(|chapter| ChapterCompletion {
                chapter_id: chapter.chapter_id.clone(),
                chapter_order: chapter.chapter_order,
                scheduled: true,
                joined: true,
                terminal_state: "COMPLETED".into(),
                expected_images: chapter.expected_images,
                completed_images: chapter.expected_images,
                failed_images: 0,
                image_pagination: chapter.image_pagination.clone(),
                artifact_paths: vec![],
            })
            .collect(),
        artifacts: vec![],
    }
}

#[test]
fn exact_completion_scope_matches_preflight() {
    let (plan, request, evidence) = setup();
    let observed = transcript(&evidence);
    let proof = source_preflight::validate_completion_scope(&plan, &request, &evidence, &observed)
        .unwrap();
    assert_eq!(proof.expected_content_units, 7);
    assert_eq!(proof.chapters, evidence.chapters);
}

#[test]
fn equal_total_images_with_wrong_per_chapter_scope_fails_closed() {
    let (plan, request, evidence) = setup();
    let mut observed = transcript(&evidence);
    observed.chapters[0].expected_images = 4;
    observed.chapters[0].completed_images = 4;
    observed.chapters[1].expected_images = 3;
    observed.chapters[1].completed_images = 3;
    assert_eq!(
        source_preflight::validate_completion_scope(&plan, &request, &evidence, &observed)
            .unwrap_err(),
        "SOURCE_COMPLETION_PREFLIGHT_SCOPE_MISMATCH"
    );
}

#[test]
fn wrong_chapter_identity_fails_even_when_counts_match() {
    let (plan, request, evidence) = setup();
    let mut observed = transcript(&evidence);
    observed.chapters[1].chapter_id = "1003".into();
    assert_eq!(
        source_preflight::validate_completion_scope(&plan, &request, &evidence, &observed)
            .unwrap_err(),
        "SOURCE_COMPLETION_PREFLIGHT_SCOPE_MISMATCH"
    );
}
