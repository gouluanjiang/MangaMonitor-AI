use cloud_monitor::{
    executor_handoff::ExecutorCommand,
    filesystem_verifier,
    local_executor,
    monitor::{hash, Target},
    staging_manifest::{StagedArtifact, StagingManifest},
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mangamonitor-a6-4-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn command() -> ExecutorCommand {
    let target: Target = serde_json::from_value(json!({
        "source_key":"jm:123456",
        "author":"Writer",
        "title":"A6.4 filesystem fixture",
        "version":{"chinese":true,"uncensored":null,"color":null,"translation":"","sample":null},
        "coverage":null
    }))
    .unwrap();
    let target_hash = hash(&target);
    let task_id = "TASK_A6_4".to_string();
    let task_revision = 7;
    let digest = hash(&(task_id.as_str(), task_revision, target_hash.as_str()));
    ExecutorCommand {
        schema_version: 1,
        command_id: format!("EXEC_{}", &digest[..20]),
        task_id,
        work_id: "WORK_A6_4".into(),
        task_revision,
        target_hash,
        source: "jm".into(),
        source_work_id: "123456".into(),
        action: "download".into(),
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        target,
    }
}

fn fixture() -> (
    TempRoot,
    local_executor::LocalExecutionPlan,
    StagingManifest,
    PathBuf,
) {
    let temp = TempRoot::new();
    let plan = local_executor::plan(&command()).unwrap();
    let command_root = temp.path().join("commands").join(&plan.command_id);
    fs::create_dir_all(command_root.join("chapter-01")).unwrap();
    fs::write(command_root.join("chapter-01/001.jpg"), b"hello").unwrap();
    fs::write(command_root.join("chapter-01/002.jpg"), b"world").unwrap();

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
                size_bytes: 5,
                sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
                    .into(),
            },
            StagedArtifact {
                relative_path: "chapter-01/002.jpg".into(),
                size_bytes: 5,
                sha256: "486ea46224d1bb4fb680f34f7c9ad96a8f24ec88be73ea8e5a6c65260e9cb8a7"
                    .into(),
            },
        ],
    };
    (temp, plan, manifest, command_root)
}

#[test]
fn exact_filesystem_tree_verifies_without_authorizing_mutation() {
    let (temp, plan, manifest, _) = fixture();
    let first = filesystem_verifier::verify(temp.path(), &plan, &manifest).unwrap();
    let second = filesystem_verifier::verify(temp.path(), &plan, &manifest).unwrap();
    assert_eq!(first, second);
    assert!(first.filesystem_verified);
    assert_eq!(first.file_count, 2);
    assert_eq!(first.total_bytes, 10);
    assert!(!first.inventory_mutation_authorized);
    assert!(!first.task_completion_authorized);
    assert!(!first.promotion_authorized);
    assert!(!first.replacement_authorized);
    assert!(!first.physical_delete_authorized);
}

#[test]
fn missing_and_extra_files_fail_closed() {
    let (temp, plan, manifest, command_root) = fixture();
    fs::remove_file(command_root.join("chapter-01/002.jpg")).unwrap();
    assert_eq!(
        filesystem_verifier::verify(temp.path(), &plan, &manifest).unwrap_err(),
        "STAGING_MANIFEST_FILE_MISSING"
    );

    let (temp, plan, manifest, command_root) = fixture();
    fs::write(command_root.join("chapter-01/003.jpg"), b"extra").unwrap();
    assert_eq!(
        filesystem_verifier::verify(temp.path(), &plan, &manifest).unwrap_err(),
        "STAGING_UNEXPECTED_FILE"
    );
}

#[test]
fn unexpected_directories_fail_closed() {
    let (temp, plan, manifest, command_root) = fixture();
    fs::create_dir(command_root.join("surprise")).unwrap();
    assert_eq!(
        filesystem_verifier::verify(temp.path(), &plan, &manifest).unwrap_err(),
        "STAGING_UNEXPECTED_DIRECTORY"
    );
}

#[test]
fn size_and_hash_drift_fail_closed() {
    let (temp, plan, manifest, command_root) = fixture();
    fs::write(command_root.join("chapter-01/001.jpg"), b"hello!").unwrap();
    assert_eq!(
        filesystem_verifier::verify(temp.path(), &plan, &manifest).unwrap_err(),
        "STAGING_FILE_SIZE_MISMATCH"
    );

    let (temp, plan, manifest, command_root) = fixture();
    fs::write(command_root.join("chapter-01/001.jpg"), b"HELLO").unwrap();
    assert_eq!(
        filesystem_verifier::verify(temp.path(), &plan, &manifest).unwrap_err(),
        "STAGING_FILE_HASH_MISMATCH"
    );
}

#[test]
fn staging_root_and_command_tree_must_exist_as_real_directories() {
    let (temp, plan, manifest, _) = fixture();
    let missing = temp.path().join("missing-root");
    assert_eq!(
        filesystem_verifier::verify(&missing, &plan, &manifest).unwrap_err(),
        "STAGING_ROOT_MISSING"
    );

    let empty = TempRoot::new();
    assert_eq!(
        filesystem_verifier::verify(empty.path(), &plan, &manifest).unwrap_err(),
        "STAGING_COMMANDS_DIRECTORY_MISSING"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_artifacts_fail_closed() {
    use std::os::unix::fs::symlink;

    let (temp, plan, manifest, command_root) = fixture();
    let outside = temp.path().join("outside.jpg");
    fs::write(&outside, b"hello").unwrap();
    let artifact = command_root.join("chapter-01/001.jpg");
    fs::remove_file(&artifact).unwrap();
    symlink(&outside, &artifact).unwrap();
    assert_eq!(
        filesystem_verifier::verify(temp.path(), &plan, &manifest).unwrap_err(),
        "STAGING_LINK_OR_REPARSE_POINT_FORBIDDEN"
    );
}

#[test]
fn forged_manifest_or_plan_is_rejected_before_filesystem_trust() {
    let (temp, plan, mut manifest, _) = fixture();
    manifest.task_revision += 1;
    assert_eq!(
        filesystem_verifier::verify(temp.path(), &plan, &manifest).unwrap_err(),
        "STAGING_MANIFEST_BINDING_MISMATCH"
    );

    let (temp, mut plan, manifest, _) = fixture();
    plan.staging_subdir = "commands/other".into();
    assert_eq!(
        filesystem_verifier::verify(temp.path(), &plan, &manifest).unwrap_err(),
        "INVALID_STAGING_PLAN_BINDING"
    );
}
