use cloud_monitor::{
    executor_handoff::ExecutorCommand,
    local_executor,
    monitor::{hash, Target},
    source_bridge_request,
    source_completion::{
        JM_UPSTREAM_COMMIT, PICA_UPSTREAM_COMMIT, SOURCE_COMPLETION_SCHEMA_VERSION,
    },
};
use serde_json::json;

fn plan(source: &str, source_work_id: &str) -> local_executor::LocalExecutionPlan {
    let target: Target = serde_json::from_value(json!({
        "source_key":format!("{source}:{source_work_id}"),"author":"Writer","title":"A6.6",
        "version":{"chinese":true,"uncensored":null,"color":null,"translation":"","sample":null},"coverage":null
    })).unwrap();
    let target_hash = hash(&target);
    let task_id = format!("TASK_A6_6_{}", source.to_ascii_uppercase());
    let revision = 1;
    let digest = hash(&(task_id.as_str(), revision, target_hash.as_str()));
    local_executor::plan(&ExecutorCommand {
        schema_version:1, command_id:format!("EXEC_{}", &digest[..20]), task_id,
        work_id:"WORK_A6_6".into(), task_revision:revision, target_hash, source:source.into(),
        source_work_id:source_work_id.into(), action:"download".into(),
        intent:"DOWNLOAD_TO_STAGING_ONLY".into(), target,
    }).unwrap()
}

#[test]
fn jm_request_is_pinned_and_execution_disabled() {
    let plan = plan("jm", "123456");
    let request = source_bridge_request::build(&plan).unwrap();
    assert_eq!(request.source, "jm");
    assert_eq!(request.upstream_commit, JM_UPSTREAM_COMMIT);
    assert_eq!(request.auth_mode, "NONE");
    assert_eq!(
        request.completion_contract_version,
        SOURCE_COMPLETION_SCHEMA_VERSION
    );
    assert!(request.require_complete_chapter_enumeration);
    assert!(request.require_complete_image_enumeration);
    assert!(request.require_all_downloads_joined);
    assert!(request.require_terminal_completed_state);
    assert!(request.require_exact_artifact_hashes);
    assert!(!request.allow_task_create_return_as_completion);
    assert!(!request.network_execution_enabled);
    assert!(!request.staging_write_enabled);
    assert!(!request.inventory_mutation_authorized);
    assert!(!request.task_completion_authorized);
    assert!(!request.promotion_authorized);
    assert!(!request.replacement_authorized);
    assert!(!request.physical_delete_authorized);
    source_bridge_request::validate(&plan, &request).unwrap();
}

#[test]
fn pica_request_requires_token_but_contains_no_credential() {
    let plan = plan("pica", "0123456789abcdef01234567");
    let request = source_bridge_request::build(&plan).unwrap();
    assert_eq!(request.source, "pica");
    assert_eq!(request.upstream_commit, PICA_UPSTREAM_COMMIT);
    assert_eq!(request.auth_mode, "PICA_TOKEN_REQUIRED");
    assert_eq!(
        request.completion_contract_version,
        SOURCE_COMPLETION_SCHEMA_VERSION
    );
    assert!(!request.network_execution_enabled);
    assert!(!request.staging_write_enabled);
}

#[test]
fn tampering_completion_requirements_or_capabilities_fails_closed() {
    let plan = plan("jm", "123456");
    let mut request = source_bridge_request::build(&plan).unwrap();
    request.allow_task_create_return_as_completion = true;
    assert_eq!(
        source_bridge_request::validate(&plan, &request).unwrap_err(),
        "UNSAFE_SOURCE_BRIDGE_COMPLETION_REQUIREMENTS"
    );

    let mut request = source_bridge_request::build(&plan).unwrap();
    request.network_execution_enabled = true;
    assert_eq!(
        source_bridge_request::validate(&plan, &request).unwrap_err(),
        "UNSAFE_SOURCE_BRIDGE_CAPABILITIES"
    );
}

#[test]
fn tampered_source_upstream_or_contract_binding_fails_closed() {
    let plan = plan("pica", "0123456789abcdef01234567");
    let mut request = source_bridge_request::build(&plan).unwrap();
    request.upstream_commit = JM_UPSTREAM_COMMIT.into();
    assert_eq!(
        source_bridge_request::validate(&plan, &request).unwrap_err(),
        "SOURCE_BRIDGE_REQUEST_BINDING_MISMATCH"
    );

    let mut request = source_bridge_request::build(&plan).unwrap();
    request.completion_contract_version += 1;
    assert_eq!(
        source_bridge_request::validate(&plan, &request).unwrap_err(),
        "SOURCE_BRIDGE_REQUEST_BINDING_MISMATCH"
    );
}

#[test]
fn forged_or_invalid_plan_is_not_requestable() {
    let mut forged_plan = plan("jm", "123456");
    forged_plan.execution_supported = true;
    assert_eq!(
        source_bridge_request::build(&forged_plan).unwrap_err(),
        "INVALID_SOURCE_BRIDGE_PLAN"
    );

    let mut invalid_source_plan = plan("jm", "123456");
    invalid_source_plan.source_work_id = "../../escape".into();
    assert_eq!(
        source_bridge_request::build(&invalid_source_plan).unwrap_err(),
        "INVALID_SOURCE_BRIDGE_SOURCE_WORK_ID"
    );
}
