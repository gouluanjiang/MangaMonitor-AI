use cloud_monitor::{
    executor_handoff::ExecutorCommand,
    local_executor,
    monitor::{hash, Target},
    staging_manifest::{self, StagedArtifact, StagingManifest},
};
use serde_json::json;

fn command() -> ExecutorCommand {
    let target: Target = serde_json::from_value(json!({
        "source_key":"jm:123456",
        "author":"Writer",
        "title":"A6.3 manifest fixture",
        "version":{"chinese":true,"uncensored":null,"color":null,"translation":"","sample":null},
        "coverage":null
    }))
    .unwrap();
    let target_hash = hash(&target);
    let task_id = "TASK_A6_3".to_string();
    let task_revision = 5;
    let digest = hash(&(task_id.as_str(), task_revision, target_hash.as_str()));
    ExecutorCommand {
        schema_version: 1,
        command_id: format!("EXEC_{}", &digest[..20]),
        task_id,
        work_id: "WORK_A6_3".into(),
        task_revision,
        target_hash,
        source: "jm".into(),
        source_work_id: "123456".into(),
        action: "download".into(),
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        target,
    }
}

fn fixture() -> (local_executor::LocalExecutionPlan, StagingManifest) {
    let plan = local_executor::plan(&command()).unwrap();
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
        expected_content_units: 2,
        completed_content_units: 2,
        failed_content_units: 0,
        artifacts: vec![
            StagedArtifact {
                relative_path: "chapter-01/001.jpg".into(),
                size_bytes: 100,
                sha256: "a".repeat(64),
            },
            StagedArtifact {
                relative_path: "chapter-01/002.jpg".into(),
                size_bytes: 200,
                sha256: "b".repeat(64),
            },
        ],
    };
    (plan, manifest)
}

#[test]
fn complete_manifest_yields_bound_completion_evidence_only() {
    let (plan, manifest) = fixture();
    let first = staging_manifest::validate(&plan, &manifest).unwrap();
    let second = staging_manifest::validate(&plan, &manifest).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.command_id, plan.command_id);
    assert_eq!(first.task_revision, plan.task_revision);
    assert_eq!(first.target_hash, plan.target_hash);
    assert_eq!(first.file_count, 2);
    assert_eq!(first.total_bytes, 300);
    assert_eq!(first.completion_evidence.artifact_manifest_hash, first.manifest_hash);
    assert!(first.completion_evidence.downloader_reported_full_completion);
    assert_eq!(first.completion_evidence.file_count, 2);
    assert!(!first.inventory_mutation_authorized);
    assert!(!first.task_completion_authorized);
    assert!(!first.promotion_authorized);
    assert!(!first.replacement_authorized);
    assert!(!first.physical_delete_authorized);
}

#[test]
fn incomplete_enumeration_or_unjoined_downloads_fail_closed() {
    let (plan, mut manifest) = fixture();
    manifest.source_enumeration_complete = false;
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "STAGING_SOURCE_ENUMERATION_INCOMPLETE"
    );

    let (plan, mut manifest) = fixture();
    manifest.all_scheduled_downloads_joined = false;
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "STAGING_DOWNLOADS_NOT_JOINED"
    );

    let (plan, mut manifest) = fixture();
    manifest.downloader_reported_full_completion = false;
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "STAGING_DOWNLOADER_NOT_COMPLETE"
    );
}

#[test]
fn incomplete_content_units_fail_closed() {
    let (plan, mut manifest) = fixture();
    manifest.completed_content_units = 1;
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "STAGING_CONTENT_UNITS_INCOMPLETE"
    );

    let (plan, mut manifest) = fixture();
    manifest.failed_content_units = 1;
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "STAGING_CONTENT_UNITS_INCOMPLETE"
    );
}

#[test]
fn manifest_binding_is_exact() {
    let (plan, mut manifest) = fixture();
    manifest.task_revision += 1;
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "STAGING_MANIFEST_BINDING_MISMATCH"
    );

    let (plan, mut manifest) = fixture();
    manifest.staging_subdir = "commands/other".into();
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "STAGING_MANIFEST_BINDING_MISMATCH"
    );
}

#[test]
fn unsafe_artifact_paths_and_windows_collisions_fail_closed() {
    let (plan, mut manifest) = fixture();
    manifest.artifacts[0].relative_path = "../escape.jpg".into();
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "INVALID_STAGING_ARTIFACT_PATH"
    );

    let (plan, mut manifest) = fixture();
    manifest.artifacts = vec![
        StagedArtifact {
            relative_path: "A.jpg".into(),
            size_bytes: 1,
            sha256: "a".repeat(64),
        },
        StagedArtifact {
            relative_path: "a.jpg".into(),
            size_bytes: 1,
            sha256: "b".repeat(64),
        },
    ];
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "STAGING_ARTIFACT_PATH_COLLISION"
    );
}

#[test]
fn artifact_list_must_be_canonical_and_metadata_valid() {
    let (plan, mut manifest) = fixture();
    manifest.artifacts.swap(0, 1);
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "STAGING_ARTIFACTS_NOT_STRICTLY_SORTED"
    );

    let (plan, mut manifest) = fixture();
    manifest.artifacts[0].sha256 = "ABC".into();
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "INVALID_STAGING_ARTIFACT_METADATA"
    );

    let (plan, mut manifest) = fixture();
    manifest.artifacts[0].size_bytes = 0;
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "INVALID_STAGING_ARTIFACT_METADATA"
    );
}
