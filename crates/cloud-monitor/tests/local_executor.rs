use cloud_monitor::{
    executor_handoff::ExecutorCommand,
    local_executor::plan,
    monitor::{hash, Target},
};
use serde_json::json;

fn command(source: &str, source_work_id: &str) -> ExecutorCommand {
    let target: Target = serde_json::from_value(json!({
        "source_key": format!("{source}:{source_work_id}"),
        "author": "Writer",
        "title": "A6.2 local executor fixture",
        "version": {
            "chinese": true,
            "uncensored": null,
            "color": null,
            "translation": "",
            "sample": null
        },
        "coverage": null
    }))
    .unwrap();
    let target_hash = hash(&target);
    let task_id = "TASK_A6_2".to_string();
    let task_revision = 3;
    let digest = hash(&(task_id.as_str(), task_revision, target_hash.as_str()));
    ExecutorCommand {
        schema_version: 1,
        command_id: format!("EXEC_{}", &digest[..20]),
        task_id,
        work_id: "WORK_A6_2".into(),
        task_revision,
        target_hash,
        source: source.into(),
        source_work_id: source_work_id.into(),
        action: "download".into(),
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        target,
    }
}

#[test]
fn local_plan_is_bound_and_non_destructive() {
    let command = command("jm", "123456");
    let plan = plan(&command).unwrap();
    assert_eq!(plan.backend, "JM");
    assert_eq!(plan.command_id, command.command_id);
    assert_eq!(plan.task_id, command.task_id);
    assert_eq!(plan.work_id, command.work_id);
    assert_eq!(plan.task_revision, command.task_revision);
    assert_eq!(plan.target_hash, command.target_hash);
    assert_eq!(plan.intent, "DOWNLOAD_TO_STAGING_ONLY");
    assert_eq!(plan.staging_subdir, format!("commands/{}", plan.command_id));
    assert!(!plan.execution_supported);
    assert!(!plan.promotion_authorized);
    assert!(!plan.replacement_authorized);
    assert!(!plan.physical_delete_authorized);
}

#[test]
fn pica_is_routed_but_still_not_executable() {
    let plan = plan(&command("pica", "abcdef")).unwrap();
    assert_eq!(plan.backend, "PICA");
    assert!(!plan.execution_supported);
}

#[test]
fn forged_or_unsafe_commands_fail_closed() {
    let mut forged = command("jm", "123456");
    forged.target.title.push_str(" changed");
    assert_eq!(
        plan(&forged).unwrap_err(),
        "LOCAL_EXECUTOR_TARGET_HASH_MISMATCH"
    );

    let mut unsafe_intent = command("jm", "123456");
    unsafe_intent.intent = "REPLACE_AND_DELETE".into();
    assert_eq!(
        plan(&unsafe_intent).unwrap_err(),
        "UNSAFE_LOCAL_EXECUTOR_INTENT"
    );

    let mut mismatched_source = command("jm", "123456");
    mismatched_source.source_work_id = "999999".into();
    assert_eq!(
        plan(&mismatched_source).unwrap_err(),
        "LOCAL_EXECUTOR_SOURCE_BINDING_MISMATCH"
    );
}

#[test]
fn command_generation_and_namespace_are_revalidated_locally() {
    let mut wrong_revision = command("jm", "123456");
    wrong_revision.task_revision += 1;
    assert_eq!(
        plan(&wrong_revision).unwrap_err(),
        "INVALID_LOCAL_EXECUTOR_BINDING"
    );

    let mut wrong_id = command("jm", "123456");
    wrong_id.command_id = "EXEC_not_the_bound_generation".into();
    assert_eq!(plan(&wrong_id).unwrap_err(), "INVALID_LOCAL_EXECUTOR_BINDING");

    let mut path_like_id = command("jm", "123456");
    path_like_id.command_id = "../escape".into();
    assert_eq!(
        plan(&path_like_id).unwrap_err(),
        "INVALID_LOCAL_EXECUTOR_BINDING"
    );
}

#[test]
fn unsupported_schema_action_or_source_fail_closed() {
    let mut schema = command("jm", "123456");
    schema.schema_version = 2;
    assert_eq!(
        plan(&schema).unwrap_err(),
        "INVALID_LOCAL_EXECUTOR_COMMAND_SCHEMA"
    );

    let mut action = command("jm", "123456");
    action.action = "delete".into();
    assert_eq!(
        plan(&action).unwrap_err(),
        "UNSUPPORTED_LOCAL_EXECUTOR_ACTION"
    );

    let unsupported = command("other", "123456");
    assert_eq!(
        plan(&unsupported).unwrap_err(),
        "UNSUPPORTED_LOCAL_EXECUTOR_SOURCE"
    );
}
