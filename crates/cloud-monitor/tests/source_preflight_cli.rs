use cloud_monitor::{
    executor_handoff::ExecutorCommand,
    local_executor,
    monitor::{hash, Target},
    source_bridge_request,
    source_completion::PaginationProof,
    source_preflight::{PreflightChapter, SourcePreflightEvidence},
};
use serde_json::{json, Value};
use std::{fs, process::Command};

#[test]
fn source_preflight_cli_is_read_only_and_emits_disabled_scope_proof() {
    let source_work_id = "0123456789abcdef01234567";
    let target: Target = serde_json::from_value(json!({
        "source_key":format!("pica:{source_work_id}"),"author":"Writer","title":"A6.7 CLI",
        "version":{"chinese":true,"uncensored":null,"color":null,"translation":"","sample":null},"coverage":null
    }))
    .unwrap();
    let target_hash = hash(&target);
    let task_id = "TASK_A6_7_CLI".to_string();
    let revision = 1;
    let digest = hash(&(task_id.as_str(), revision, target_hash.as_str()));
    let plan = local_executor::plan(&ExecutorCommand {
        schema_version: 1,
        command_id: format!("EXEC_{}", &digest[..20]),
        task_id,
        work_id: "WORK_A6_7_CLI".into(),
        task_revision: revision,
        target_hash,
        source: "pica".into(),
        source_work_id: source_work_id.into(),
        action: "download".into(),
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        target,
    })
    .unwrap();
    let request = source_bridge_request::build(&plan).unwrap();
    let pagination = PaginationProof {
        total_pages: 1,
        successful_pages: vec![1],
        failed_pages: vec![],
    };
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
        chapter_pagination: Some(pagination.clone()),
        expected_chapter_count: 1,
        chapters: vec![PreflightChapter {
            chapter_id: "111111111111111111111111".into(),
            chapter_order: 1,
            expected_images: 3,
            image_pagination: Some(pagination),
        }],
        image_bytes_downloaded: false,
        staging_written: false,
    };

    let base = std::env::temp_dir().join(format!("mangamonitor-a6-7-cli-{}", std::process::id()));
    let plan_path = base.with_extension("plan.json");
    let request_path = base.with_extension("request.json");
    let evidence_path = base.with_extension("evidence.json");
    let plan_bytes = serde_json::to_vec_pretty(&plan).unwrap();
    let request_bytes = serde_json::to_vec_pretty(&request).unwrap();
    let evidence_bytes = serde_json::to_vec_pretty(&evidence).unwrap();
    fs::write(&plan_path, &plan_bytes).unwrap();
    fs::write(&request_path, &request_bytes).unwrap();
    fs::write(&evidence_path, &evidence_bytes).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_assistant-source-preflight-check"))
        .arg("--plan")
        .arg(&plan_path)
        .arg("--request")
        .arg(&request_path)
        .arg("--evidence")
        .arg(&evidence_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["source_scope_verified"], true);
    assert_eq!(value["expected_content_units"], 3);
    assert_eq!(value["image_download_authorized"], false);
    assert_eq!(value["staging_write_authorized"], false);
    assert_eq!(value["physical_delete_authorized"], false);
    assert_eq!(fs::read(&plan_path).unwrap(), plan_bytes);
    assert_eq!(fs::read(&request_path).unwrap(), request_bytes);
    assert_eq!(fs::read(&evidence_path).unwrap(), evidence_bytes);

    let _ = fs::remove_file(plan_path);
    let _ = fs::remove_file(request_path);
    let _ = fs::remove_file(evidence_path);
}
