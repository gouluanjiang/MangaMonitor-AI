use cloud_monitor::{
    executor_handoff::ExecutorCommand,
    local_executor,
    monitor::{hash, Target},
    source_completion::{
        self, ChapterCompletion, PaginationProof, SourceCompletionTranscript, JM_UPSTREAM_COMMIT,
        PICA_UPSTREAM_COMMIT,
    },
    staging_manifest::StagedArtifact,
};
use serde_json::json;

fn plan(source: &str, source_work_id: &str) -> local_executor::LocalExecutionPlan {
    let target: Target = serde_json::from_value(json!({
        "source_key":format!("{source}:{source_work_id}"),
        "author":"Writer",
        "title":"A6.5 fixture",
        "version":{"chinese":true,"uncensored":null,"color":null,"translation":"","sample":null},
        "coverage":null
    }))
    .unwrap();
    let target_hash = hash(&target);
    let task_id = format!("TASK_A6_5_{}", source.to_ascii_uppercase());
    let revision = 2;
    let digest = hash(&(task_id.as_str(), revision, target_hash.as_str()));
    local_executor::plan(&ExecutorCommand {
        schema_version: 1,
        command_id: format!("EXEC_{}", &digest[..20]),
        task_id,
        work_id: format!("WORK_A6_5_{}", source.to_ascii_uppercase()),
        task_revision: revision,
        target_hash,
        source: source.into(),
        source_work_id: source_work_id.into(),
        action: "download".into(),
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        target,
    })
    .unwrap()
}

fn artifact(path: &str, byte: char) -> StagedArtifact {
    StagedArtifact {
        relative_path: path.into(),
        size_bytes: 1,
        sha256: byte.to_string().repeat(64),
    }
}

fn complete_chapter(
    id: &str,
    order: u64,
    paths: &[&str],
    pagination: Option<PaginationProof>,
) -> ChapterCompletion {
    ChapterCompletion {
        chapter_id: id.into(),
        chapter_order: order,
        scheduled: true,
        joined: true,
        terminal_state: "COMPLETED".into(),
        expected_images: paths.len() as u64,
        completed_images: paths.len() as u64,
        failed_images: 0,
        image_pagination: pagination,
        artifact_paths: paths.iter().map(|value| (*value).into()).collect(),
    }
}

fn jm_fixture() -> (local_executor::LocalExecutionPlan, SourceCompletionTranscript) {
    let plan = plan("jm", "123456");
    let artifacts = vec![
        artifact("chapter-01/001.webp", 'a'),
        artifact("chapter-01/002.webp", 'b'),
        artifact("chapter-02/001.webp", 'c'),
    ];
    let transcript = SourceCompletionTranscript {
        schema_version: 1,
        command_id: plan.command_id.clone(),
        task_id: plan.task_id.clone(),
        work_id: plan.work_id.clone(),
        task_revision: plan.task_revision,
        target_hash: plan.target_hash.clone(),
        source: "jm".into(),
        source_work_id: plan.source_work_id.clone(),
        upstream_commit: JM_UPSTREAM_COMMIT.into(),
        scope: "FULL_SOURCE_WORK".into(),
        source_enumeration_complete: true,
        chapter_pagination: None,
        expected_chapter_count: 2,
        all_scheduled_downloads_joined: true,
        chapters: vec![
            complete_chapter("1001", 1, &["chapter-01/001.webp", "chapter-01/002.webp"], None),
            complete_chapter("1002", 2, &["chapter-02/001.webp"], None),
        ],
        artifacts,
    };
    (plan, transcript)
}

fn pica_fixture() -> (local_executor::LocalExecutionPlan, SourceCompletionTranscript) {
    let plan = plan("pica", "0123456789abcdef01234567");
    let chapter_id = "abcdef0123456789abcdef01";
    let artifacts = vec![artifact("chapter-01/001.jpg", 'd'), artifact("chapter-01/002.jpg", 'e')];
    let transcript = SourceCompletionTranscript {
        schema_version: 1,
        command_id: plan.command_id.clone(),
        task_id: plan.task_id.clone(),
        work_id: plan.work_id.clone(),
        task_revision: plan.task_revision,
        target_hash: plan.target_hash.clone(),
        source: "pica".into(),
        source_work_id: plan.source_work_id.clone(),
        upstream_commit: PICA_UPSTREAM_COMMIT.into(),
        scope: "FULL_SOURCE_WORK".into(),
        source_enumeration_complete: true,
        chapter_pagination: Some(PaginationProof {
            total_pages: 2,
            successful_pages: vec![1, 2],
            failed_pages: vec![],
        }),
        expected_chapter_count: 1,
        all_scheduled_downloads_joined: true,
        chapters: vec![complete_chapter(
            chapter_id,
            1,
            &["chapter-01/001.jpg", "chapter-01/002.jpg"],
            Some(PaginationProof {
                total_pages: 2,
                successful_pages: vec![1, 2],
                failed_pages: vec![],
            }),
        )],
        artifacts,
    };
    (plan, transcript)
}

#[test]
fn jm_complete_joined_transcript_normalizes_but_authorizes_nothing() {
    let (plan, transcript) = jm_fixture();
    let proof = source_completion::normalize(&plan, &transcript).unwrap();
    assert!(proof.source_contract_verified);
    assert!(!proof.execution_supported);
    assert_eq!(proof.manifest.expected_content_units, 3);
    assert!(proof.manifest.downloader_reported_full_completion);
    assert!(!proof.promotion_authorized);
    assert!(!proof.replacement_authorized);
    assert!(!proof.physical_delete_authorized);
}

#[test]
fn jm_spawn_or_create_only_is_not_completion() {
    let (plan, mut transcript) = jm_fixture();
    transcript.chapters[0].joined = false;
    transcript.chapters[0].terminal_state = "DOWNLOADING".into();
    assert_eq!(
        source_completion::normalize(&plan, &transcript).unwrap_err(),
        "SOURCE_COMPLETION_CHAPTER_NOT_JOINED"
    );
}

#[test]
fn one_incomplete_jm_image_or_chapter_fails_closed() {
    let (plan, mut transcript) = jm_fixture();
    transcript.chapters[1].completed_images = 0;
    assert_eq!(
        source_completion::normalize(&plan, &transcript).unwrap_err(),
        "SOURCE_COMPLETION_IMAGES_INCOMPLETE"
    );

    let (plan, mut transcript) = jm_fixture();
    transcript.chapters[1].terminal_state = "FAILED".into();
    assert_eq!(
        source_completion::normalize(&plan, &transcript).unwrap_err(),
        "SOURCE_COMPLETION_CHAPTER_NOT_COMPLETED"
    );
}

#[test]
fn pica_complete_chapter_and_image_pagination_normalizes() {
    let (plan, transcript) = pica_fixture();
    let proof = source_completion::normalize(&plan, &transcript).unwrap();
    assert_eq!(proof.source, "pica");
    assert_eq!(proof.upstream_commit, PICA_UPSTREAM_COMMIT);
    assert_eq!(proof.manifest.expected_content_units, 2);
}

#[test]
fn pica_later_chapter_page_failure_cannot_be_silently_dropped() {
    let (plan, mut transcript) = pica_fixture();
    transcript.chapter_pagination = Some(PaginationProof {
        total_pages: 2,
        successful_pages: vec![1],
        failed_pages: vec![2],
    });
    assert_eq!(
        source_completion::normalize(&plan, &transcript).unwrap_err(),
        "PICA_CHAPTER_PAGINATION_INCOMPLETE"
    );
}

#[test]
fn pica_later_image_page_failure_is_not_completion() {
    let (plan, mut transcript) = pica_fixture();
    transcript.chapters[0].image_pagination = Some(PaginationProof {
        total_pages: 3,
        successful_pages: vec![1, 2],
        failed_pages: vec![3],
    });
    assert_eq!(
        source_completion::normalize(&plan, &transcript).unwrap_err(),
        "PICA_IMAGE_PAGINATION_INCOMPLETE"
    );
}

#[test]
fn missing_duplicate_or_unassigned_units_fail_closed() {
    let (plan, mut transcript) = jm_fixture();
    transcript.expected_chapter_count = 3;
    assert_eq!(
        source_completion::normalize(&plan, &transcript).unwrap_err(),
        "SOURCE_COMPLETION_CHAPTER_COUNT_MISMATCH"
    );

    let (plan, mut transcript) = jm_fixture();
    transcript.chapters[1].chapter_id = transcript.chapters[0].chapter_id.clone();
    assert_eq!(
        source_completion::normalize(&plan, &transcript).unwrap_err(),
        "SOURCE_COMPLETION_CHAPTERS_NOT_CANONICAL"
    );

    let (plan, mut transcript) = jm_fixture();
    transcript.chapters[1].artifact_paths.clear();
    assert_eq!(
        source_completion::normalize(&plan, &transcript).unwrap_err(),
        "SOURCE_COMPLETION_CHAPTER_ARTIFACTS_EMPTY"
    );
}

#[test]
fn wrong_upstream_commit_and_forged_plan_binding_fail_closed() {
    let (plan, mut transcript) = jm_fixture();
    transcript.upstream_commit = "wrong".into();
    assert_eq!(
        source_completion::normalize(&plan, &transcript).unwrap_err(),
        "JM_UPSTREAM_COMMIT_MISMATCH"
    );

    let (mut plan, transcript) = pica_fixture();
    plan.backend = "JM".into();
    assert_eq!(
        source_completion::normalize(&plan, &transcript).unwrap_err(),
        "SOURCE_COMPLETION_BACKEND_MISMATCH"
    );
}
