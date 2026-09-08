use cloud_monitor::{
    executor_handoff::ExecutorCommand,
    local_executor,
    monitor::{hash, Target},
    staging_manifest::{self, StagedArtifact, StagingManifest},
};
use serde_json::json;

fn fixture() -> (local_executor::LocalExecutionPlan, StagingManifest) {
    let target: Target = serde_json::from_value(json!({
        "source_key":"jm:123456",
        "author":"Writer",
        "title":"A6.3 hardening fixture",
        "version":{"chinese":true,"uncensored":null,"color":null,"translation":"","sample":null},
        "coverage":null
    }))
    .unwrap();
    let target_hash = hash(&target);
    let task_id = "TASK_A6_3_HARDEN".to_string();
    let task_revision = 2;
    let digest = hash(&(task_id.as_str(), task_revision, target_hash.as_str()));
    let command = ExecutorCommand {
        schema_version: 1,
        command_id: format!("EXEC_{}", &digest[..20]),
        task_id,
        work_id: "WORK_A6_3_HARDEN".into(),
        task_revision,
        target_hash,
        source: "jm".into(),
        source_work_id: "123456".into(),
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
            relative_path: "chapter/001.jpg".into(),
            size_bytes: 1,
            sha256: "a".repeat(64),
        }],
    };
    (plan, manifest)
}

#[test]
fn forged_local_plan_cannot_be_used_as_a_trust_bypass() {
    let (mut plan, mut manifest) = fixture();
    plan.intent = "REPLACE_AND_DELETE".into();
    manifest.staging_subdir = plan.staging_subdir.clone();
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "INVALID_STAGING_PLAN_BINDING"
    );

    let (mut plan, mut manifest) = fixture();
    plan.command_id = "EXEC_forged".into();
    plan.staging_subdir = format!("commands/{}", plan.command_id);
    manifest.command_id = plan.command_id.clone();
    manifest.staging_subdir = plan.staging_subdir.clone();
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "INVALID_STAGING_PLAN_BINDING"
    );

    let (mut plan, mut manifest) = fixture();
    plan.execution_supported = true;
    manifest.command_id = plan.command_id.clone();
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "UNSAFE_STAGING_PLAN_CAPABILITIES"
    );
}

#[test]
fn windows_reserved_names_are_rejected() {
    for path in ["CON", "con.jpg", "dir/PRN.png", "COM1.txt", "LPT9.bin", "aux.dat"] {
        let (plan, mut manifest) = fixture();
        manifest.artifacts[0].relative_path = path.into();
        assert_eq!(
            staging_manifest::validate(&plan, &manifest).unwrap_err(),
            "INVALID_STAGING_ARTIFACT_PATH",
            "path should fail closed: {path}"
        );
    }
}

#[test]
fn target_hash_and_staging_namespace_are_revalidated_on_plan() {
    let (mut plan, mut manifest) = fixture();
    plan.target_hash = "0".repeat(64);
    manifest.target_hash = plan.target_hash.clone();
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "INVALID_STAGING_PLAN_BINDING"
    );

    let (mut plan, mut manifest) = fixture();
    plan.staging_subdir = "commands/another".into();
    manifest.staging_subdir = plan.staging_subdir.clone();
    assert_eq!(
        staging_manifest::validate(&plan, &manifest).unwrap_err(),
        "INVALID_STAGING_PLAN_BINDING"
    );
}
