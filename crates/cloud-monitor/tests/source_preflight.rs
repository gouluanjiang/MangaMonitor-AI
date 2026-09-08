use cloud_monitor::{
    executor_handoff::ExecutorCommand,
    local_executor,
    monitor::{hash, Target},
    source_bridge_request,
    source_completion::{PaginationProof, JM_UPSTREAM_COMMIT, PICA_UPSTREAM_COMMIT},
    source_preflight::{self, PreflightChapter, SourcePreflightEvidence},
};
use serde_json::json;

fn plan_and_request(
    source: &str,
    source_work_id: &str,
) -> (
    local_executor::LocalExecutionPlan,
    source_bridge_request::SourceBridgeRequest,
) {
    let target: Target = serde_json::from_value(json!({
        "source_key":format!("{source}:{source_work_id}"),"author":"Writer","title":"A6.7",
        "version":{"chinese":true,"uncensored":null,"color":null,"translation":"","sample":null},"coverage":null
    }))
    .unwrap();
    let target_hash = hash(&target);
    let task_id = format!("TASK_A6_7_{}", source.to_ascii_uppercase());
    let revision = 1;
    let digest = hash(&(task_id.as_str(), revision, target_hash.as_str()));
    let plan = local_executor::plan(&ExecutorCommand {
        schema_version: 1,
        command_id: format!("EXEC_{}", &digest[..20]),
        task_id,
        work_id: "WORK_A6_7".into(),
        task_revision: revision,
        target_hash,
        source: source.into(),
        source_work_id: source_work_id.into(),
        action: "download".into(),
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        target,
    })
    .unwrap();
    let request = source_bridge_request::build(&plan).unwrap();
    (plan, request)
}

fn pages(total_pages: u64) -> PaginationProof {
    PaginationProof {
        total_pages,
        successful_pages: (1..=total_pages).collect(),
        failed_pages: vec![],
    }
}

fn jm_evidence(
    request: &source_bridge_request::SourceBridgeRequest,
) -> SourcePreflightEvidence {
    SourcePreflightEvidence {
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
    }
}

fn pica_evidence(
    request: &source_bridge_request::SourceBridgeRequest,
) -> SourcePreflightEvidence {
    SourcePreflightEvidence {
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
        chapter_pagination: Some(pages(2)),
        expected_chapter_count: 2,
        chapters: vec![
            PreflightChapter {
                chapter_id: "111111111111111111111111".into(),
                chapter_order: 1,
                expected_images: 5,
                image_pagination: Some(pages(2)),
            },
            PreflightChapter {
                chapter_id: "222222222222222222222222".into(),
                chapter_order: 2,
                expected_images: 6,
                image_pagination: Some(pages(3)),
            },
        ],
        image_bytes_downloaded: false,
        staging_written: false,
    }
}

#[test]
fn jm_preflight_proves_exact_expected_scope_without_download_authority() {
    let (plan, request) = plan_and_request("jm", "123456");
    assert_eq!(request.upstream_commit, JM_UPSTREAM_COMMIT);
    let evidence = jm_evidence(&request);
    let proof = source_preflight::validate(&plan, &request, &evidence).unwrap();
    assert_eq!(proof.expected_chapter_count, 2);
    assert_eq!(proof.expected_content_units, 7);
    assert_eq!(proof.chapters, evidence.chapters);
    assert_eq!(proof.chapter_pagination, evidence.chapter_pagination);
    assert_eq!(proof.preflight_hash, hash(&evidence));
    assert!(proof.source_scope_verified);
    assert!(!proof.image_download_authorized);
    assert!(!proof.staging_write_authorized);
    assert!(!proof.inventory_mutation_authorized);
    assert!(!proof.task_completion_authorized);
    assert!(!proof.promotion_authorized);
    assert!(!proof.replacement_authorized);
    assert!(!proof.physical_delete_authorized);
}

#[test]
fn pica_preflight_requires_complete_chapter_and_image_pagination() {
    let (plan, request) = plan_and_request("pica", "0123456789abcdef01234567");
    assert_eq!(request.upstream_commit, PICA_UPSTREAM_COMMIT);
    let evidence = pica_evidence(&request);
    let proof = source_preflight::validate(&plan, &request, &evidence).unwrap();
    assert_eq!(proof.expected_content_units, 11);
    assert_eq!(proof.chapters, evidence.chapters);
    assert_eq!(proof.chapter_pagination, evidence.chapter_pagination);

    let mut evidence = pica_evidence(&request);
    evidence.chapter_pagination.as_mut().unwrap().failed_pages = vec![2];
    assert_eq!(
        source_preflight::validate(&plan, &request, &evidence).unwrap_err(),
        "PICA_PREFLIGHT_CHAPTER_PAGINATION_INCOMPLETE"
    );

    let mut evidence = pica_evidence(&request);
    evidence.chapters[1]
        .image_pagination
        .as_mut()
        .unwrap()
        .successful_pages = vec![1, 3];
    assert_eq!(
        source_preflight::validate(&plan, &request, &evidence).unwrap_err(),
        "PICA_PREFLIGHT_IMAGE_PAGINATION_INCOMPLETE"
    );
}

#[test]
fn preflight_hash_changes_when_exact_scope_changes() {
    let (plan, request) = plan_and_request("jm", "123456");
    let evidence = jm_evidence(&request);
    let first = source_preflight::validate(&plan, &request, &evidence).unwrap();

    let mut changed = evidence.clone();
    changed.chapters[1].expected_images += 1;
    let second = source_preflight::validate(&plan, &request, &changed).unwrap();
    assert_ne!(first.preflight_hash, second.preflight_hash);
    assert_ne!(first.expected_content_units, second.expected_content_units);
}

#[test]
fn noncanonical_duplicate_or_empty_chapter_scope_fails_closed() {
    let (plan, request) = plan_and_request("jm", "123456");
    let mut evidence = jm_evidence(&request);
    evidence.chapters[1].chapter_id = evidence.chapters[0].chapter_id.clone();
    assert_eq!(
        source_preflight::validate(&plan, &request, &evidence).unwrap_err(),
        "SOURCE_PREFLIGHT_CHAPTERS_NOT_CANONICAL"
    );

    let mut evidence = jm_evidence(&request);
    evidence.chapters[0].expected_images = 0;
    assert_eq!(
        source_preflight::validate(&plan, &request, &evidence).unwrap_err(),
        "SOURCE_PREFLIGHT_IMAGES_EMPTY"
    );
}

#[test]
fn phase_boundary_and_binding_tampering_fail_closed() {
    let (plan, request) = plan_and_request("pica", "0123456789abcdef01234567");
    let mut evidence = pica_evidence(&request);
    evidence.image_bytes_downloaded = true;
    assert_eq!(
        source_preflight::validate(&plan, &request, &evidence).unwrap_err(),
        "SOURCE_PREFLIGHT_PHASE_BOUNDARY_VIOLATION"
    );

    let mut evidence = pica_evidence(&request);
    evidence.target_hash = "0".repeat(64);
    assert_eq!(
        source_preflight::validate(&plan, &request, &evidence).unwrap_err(),
        "SOURCE_PREFLIGHT_BINDING_MISMATCH"
    );
}
