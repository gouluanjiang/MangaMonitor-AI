//! V1.4 read-only local inventory rescan.
//!
//! This boundary re-verifies a completed V1.3 imported directory against its
//! import receipt, completion sidecar, canonical staging manifest, and actual
//! filesystem tree. It produces an observation report only. It never mutates
//! monitor-state/inventory, completes a task, replaces content, deletes files,
//! or enables production.

use crate::{
    local_executor::{LocalExecutionPlan, LOCAL_EXECUTOR_SCHEMA_VERSION},
    local_library_import_gate::{
        LocalLibraryImportReceipt, LOCAL_LIBRARY_IMPORT_SCHEMA_VERSION, LOCAL_LIBRARY_SIDECAR,
    },
    monitor::{hash, Target},
    source_completion::{
        SourceCompletionProof, JM_UPSTREAM_COMMIT, PICA_UPSTREAM_COMMIT,
        SOURCE_COMPLETION_SCHEMA_VERSION,
    },
    staging_manifest::{self, StagingManifest},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, Metadata},
    io::Read,
    path::{Component, Path, PathBuf},
};

pub const LOCAL_INVENTORY_RESCAN_SCHEMA_VERSION: u64 = 1;
const MAX_SIDECAR_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalInventoryRescanReport {
    pub schema_version: u64,
    pub rescan_id: String,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub library_relative_dir: String,
    pub sidecar_sha256: String,
    pub manifest_hash: String,
    pub file_count: u64,
    pub total_bytes: u64,
    pub sidecar_verified: bool,
    pub manifest_verified: bool,
    pub filesystem_verified: bool,
    pub inventory_observation_complete: bool,
    pub inventory_observation_hash: String,
    pub inventory_mutation_authorized: bool,
    pub task_completion_authorized: bool,
    pub promotion_authorized: bool,
    pub replacement_authorized: bool,
    pub physical_delete_authorized: bool,
    pub production_enablement_authorized: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct ImportedSidecar {
    schema_version: u64,
    command_id: String,
    task_id: String,
    work_id: String,
    task_revision: u64,
    target_hash: String,
    source: String,
    source_work_id: String,
    action: String,
    target: Target,
    completed_at: String,
    library_relative_dir: String,
    source_completion: SourceCompletionProof,
    inventory_rescan_required: bool,
    inventory_mutation_authorized: bool,
    task_completion_authorized: bool,
    replacement_authorized: bool,
    physical_delete_authorized: bool,
}

fn lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn safe_command_id(value: &str) -> bool {
    value.starts_with("EXEC_")
        && value.len() > "EXEC_".len()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn link_like(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn checked_directory(path: &Path, missing: &str) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| missing.to_string())?;
    if link_like(&metadata) {
        return Err("LOCAL_RESCAN_LINK_OR_REPARSE_POINT_FORBIDDEN".into());
    }
    if !metadata.is_dir() {
        return Err("LOCAL_RESCAN_EXPECTED_DIRECTORY".into());
    }
    fs::canonicalize(path).map_err(|_| "LOCAL_RESCAN_CANONICALIZE_FAILED".into())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|_| "LOCAL_RESCAN_FILE_OPEN_FAILED")?;
    let metadata = file
        .metadata()
        .map_err(|_| "LOCAL_RESCAN_FILE_METADATA_FAILED")?;
    if !metadata.is_file() {
        return Err("LOCAL_RESCAN_EXPECTED_REGULAR_FILE".into());
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "LOCAL_RESCAN_FILE_READ_FAILED")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn portable_relative(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "LOCAL_RESCAN_PATH_ESCAPED_ROOT")?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(value) => parts.push(
                value
                    .to_str()
                    .ok_or("LOCAL_RESCAN_NON_UTF8_PATH")?
                    .to_string(),
            ),
            _ => return Err("LOCAL_RESCAN_INVALID_FILESYSTEM_PATH".into()),
        }
    }
    if parts.is_empty() {
        return Err("LOCAL_RESCAN_INVALID_FILESYSTEM_PATH".into());
    }
    Ok(parts.join("/"))
}

fn join_portable(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(root.to_path_buf(), |path, component| path.join(component))
}

fn expected_directories(manifest: &StagingManifest) -> BTreeSet<String> {
    let mut directories = BTreeSet::new();
    for artifact in &manifest.artifacts {
        let mut parts: Vec<&str> = artifact.relative_path.split('/').collect();
        parts.pop();
        while !parts.is_empty() {
            directories.insert(parts.join("/"));
            parts.pop();
        }
    }
    directories
}

fn walk_tree(
    root: &Path,
    directory: &Path,
    expected_dirs: &BTreeSet<String>,
    observed_files: &mut BTreeSet<String>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|_| "LOCAL_RESCAN_DIRECTORY_READ_FAILED")? {
        let entry = entry.map_err(|_| "LOCAL_RESCAN_DIRECTORY_READ_FAILED")?;
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| "LOCAL_RESCAN_METADATA_READ_FAILED")?;
        if link_like(&metadata) {
            return Err("LOCAL_RESCAN_LINK_OR_REPARSE_POINT_FORBIDDEN".into());
        }
        let relative = portable_relative(root, &path)?;
        if metadata.is_dir() {
            if !expected_dirs.contains(&relative) {
                return Err("LOCAL_RESCAN_UNEXPECTED_DIRECTORY".into());
            }
            walk_tree(root, &path, expected_dirs, observed_files)?;
        } else if metadata.is_file() {
            if !observed_files.insert(relative) {
                return Err("LOCAL_RESCAN_DUPLICATE_FILESYSTEM_PATH".into());
            }
        } else {
            return Err("LOCAL_RESCAN_SPECIAL_FILE_FORBIDDEN".into());
        }
    }
    Ok(())
}

fn receipt_is_safe(receipt: &LocalLibraryImportReceipt) -> bool {
    receipt.library_import_completed
        && receipt.destination_verified
        && receipt.staging_preserved
        && receipt.inventory_rescan_required
        && !receipt.inventory_mutation_authorized
        && !receipt.task_completion_authorized
        && !receipt.promotion_authorized
        && !receipt.replacement_authorized
        && !receipt.physical_delete_authorized
        && !receipt.production_enablement_authorized
}

fn validate_receipt(receipt: &LocalLibraryImportReceipt) -> Result<(), String> {
    if receipt.schema_version != LOCAL_LIBRARY_IMPORT_SCHEMA_VERSION {
        return Err("LOCAL_RESCAN_RECEIPT_SCHEMA_MISMATCH".into());
    }
    if !safe_command_id(&receipt.command_id)
        || receipt.task_id.trim().is_empty()
        || receipt.work_id.trim().is_empty()
        || receipt.task_revision == 0
        || !lower_sha256(&receipt.target_hash)
        || !matches!(receipt.source.as_str(), "jm" | "pica")
        || receipt.source_work_id.trim().is_empty()
        || !lower_sha256(&receipt.sidecar_sha256)
        || !lower_sha256(&receipt.manifest_hash)
        || receipt.file_count == 0
        || receipt.total_bytes == 0
        || receipt.library_relative_dir != format!("mangamonitor-{}", receipt.command_id)
        || receipt.library_relative_dir.contains('/')
        || receipt.library_relative_dir.contains('\\')
    {
        return Err("LOCAL_RESCAN_RECEIPT_BINDING_INVALID".into());
    }
    if !receipt_is_safe(receipt) {
        return Err("LOCAL_RESCAN_RECEIPT_UNSAFE_AUTHORITY".into());
    }
    Ok(())
}

fn read_sidecar(destination_root: &Path, expected_hash: &str) -> Result<ImportedSidecar, String> {
    let path = destination_root.join(LOCAL_LIBRARY_SIDECAR);
    let metadata = fs::symlink_metadata(&path).map_err(|_| "LOCAL_RESCAN_SIDECAR_MISSING")?;
    if link_like(&metadata) || !metadata.is_file() {
        return Err("LOCAL_RESCAN_SIDECAR_NOT_REGULAR".into());
    }
    if metadata.len() == 0 || metadata.len() > MAX_SIDECAR_BYTES {
        return Err("LOCAL_RESCAN_SIDECAR_SIZE_INVALID".into());
    }
    let bytes = fs::read(&path).map_err(|_| "LOCAL_RESCAN_SIDECAR_READ_FAILED")?;
    let actual_hash = format!("{:x}", Sha256::digest(&bytes));
    if actual_hash != expected_hash {
        return Err("LOCAL_RESCAN_SIDECAR_HASH_MISMATCH".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "LOCAL_RESCAN_SIDECAR_JSON_INVALID".into())
}

fn validate_sidecar_binding(
    receipt: &LocalLibraryImportReceipt,
    sidecar: &ImportedSidecar,
) -> Result<LocalExecutionPlan, String> {
    if sidecar.schema_version != LOCAL_LIBRARY_IMPORT_SCHEMA_VERSION
        || sidecar.command_id != receipt.command_id
        || sidecar.task_id != receipt.task_id
        || sidecar.work_id != receipt.work_id
        || sidecar.task_revision != receipt.task_revision
        || sidecar.target_hash != receipt.target_hash
        || sidecar.source != receipt.source
        || sidecar.source_work_id != receipt.source_work_id
        || sidecar.library_relative_dir != receipt.library_relative_dir
        || sidecar.action != "download"
        || sidecar.completed_at.trim().is_empty()
        || !sidecar.inventory_rescan_required
        || sidecar.inventory_mutation_authorized
        || sidecar.task_completion_authorized
        || sidecar.replacement_authorized
        || sidecar.physical_delete_authorized
    {
        return Err("LOCAL_RESCAN_SIDECAR_BINDING_MISMATCH".into());
    }
    if hash(&sidecar.target) != receipt.target_hash
        || sidecar.target.source_key
            != format!("{}:{}", receipt.source, receipt.source_work_id)
    {
        return Err("LOCAL_RESCAN_TARGET_BINDING_MISMATCH".into());
    }

    let completion = &sidecar.source_completion;
    let expected_upstream = match receipt.source.as_str() {
        "jm" => JM_UPSTREAM_COMMIT,
        "pica" => PICA_UPSTREAM_COMMIT,
        _ => return Err("LOCAL_RESCAN_UNSUPPORTED_SOURCE".into()),
    };
    if completion.schema_version != SOURCE_COMPLETION_SCHEMA_VERSION
        || completion.source != receipt.source
        || completion.upstream_commit != expected_upstream
        || !completion.source_contract_verified
        || completion.execution_supported
        || completion.inventory_mutation_authorized
        || completion.task_completion_authorized
        || completion.promotion_authorized
        || completion.replacement_authorized
        || completion.physical_delete_authorized
    {
        return Err("LOCAL_RESCAN_SOURCE_COMPLETION_INVALID".into());
    }

    let backend = match receipt.source.as_str() {
        "jm" => "JM",
        "pica" => "PICA",
        _ => return Err("LOCAL_RESCAN_UNSUPPORTED_SOURCE".into()),
    };
    Ok(LocalExecutionPlan {
        schema_version: LOCAL_EXECUTOR_SCHEMA_VERSION,
        command_id: receipt.command_id.clone(),
        task_id: receipt.task_id.clone(),
        work_id: receipt.work_id.clone(),
        task_revision: receipt.task_revision,
        target_hash: receipt.target_hash.clone(),
        backend: backend.into(),
        source_work_id: receipt.source_work_id.clone(),
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        staging_subdir: format!("commands/{}", receipt.command_id),
        execution_supported: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    })
}

fn verify_filesystem(
    destination_root: &Path,
    manifest: &StagingManifest,
    receipt: &LocalLibraryImportReceipt,
) -> Result<(), String> {
    let expected_dirs = expected_directories(manifest);
    let mut expected_files: BTreeSet<String> = manifest
        .artifacts
        .iter()
        .map(|artifact| artifact.relative_path.clone())
        .collect();
    if !expected_files.insert(LOCAL_LIBRARY_SIDECAR.into()) {
        return Err("LOCAL_RESCAN_SIDECAR_COLLIDES_WITH_MANIFEST".into());
    }
    let mut observed_files = BTreeSet::new();
    walk_tree(
        destination_root,
        destination_root,
        &expected_dirs,
        &mut observed_files,
    )?;
    if observed_files != expected_files {
        return Err("LOCAL_RESCAN_IMPORTED_TREE_MISMATCH".into());
    }

    let mut total_bytes = 0u64;
    for artifact in &manifest.artifacts {
        let path = join_portable(destination_root, &artifact.relative_path);
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| "LOCAL_RESCAN_IMPORTED_FILE_MISSING")?;
        if link_like(&metadata)
            || !metadata.is_file()
            || metadata.len() != artifact.size_bytes
            || sha256_file(&path)? != artifact.sha256
        {
            return Err("LOCAL_RESCAN_IMPORTED_FILE_VERIFY_FAILED".into());
        }
        total_bytes = total_bytes
            .checked_add(artifact.size_bytes)
            .ok_or("LOCAL_RESCAN_TOTAL_SIZE_OVERFLOW")?;
    }
    if total_bytes != receipt.total_bytes {
        return Err("LOCAL_RESCAN_TOTAL_BYTES_MISMATCH".into());
    }

    // The sidecar was parsed earlier to establish the manifest/binding. Recheck
    // it at the end of the filesystem pass so a concurrent sidecar replacement
    // cannot yield a stale successful observation report.
    let sidecar_path = destination_root.join(LOCAL_LIBRARY_SIDECAR);
    let sidecar_metadata =
        fs::symlink_metadata(&sidecar_path).map_err(|_| "LOCAL_RESCAN_SIDECAR_REVERIFY_FAILED")?;
    if link_like(&sidecar_metadata)
        || !sidecar_metadata.is_file()
        || sha256_file(&sidecar_path)? != receipt.sidecar_sha256
    {
        return Err("LOCAL_RESCAN_SIDECAR_REVERIFY_FAILED".into());
    }
    Ok(())
}

/// Re-scan one completed V1.3 import and return a deterministic observation.
///
/// `library_root` is never included in the report. Only the opaque relative
/// import directory from the V1.3 receipt is surfaced, so user filesystem paths
/// are not copied into audit/state artifacts.
pub fn rescan(
    library_root: &Path,
    receipt: &LocalLibraryImportReceipt,
) -> Result<LocalInventoryRescanReport, String> {
    validate_receipt(receipt)?;
    let canonical_library_root = checked_directory(library_root, "LOCAL_RESCAN_LIBRARY_ROOT_MISSING")?;
    let destination = canonical_library_root.join(&receipt.library_relative_dir);
    let canonical_destination =
        checked_directory(&destination, "LOCAL_RESCAN_IMPORTED_ROOT_MISSING")?;
    if canonical_destination.parent() != Some(canonical_library_root.as_path()) {
        return Err("LOCAL_RESCAN_IMPORTED_ROOT_NOT_DIRECT_CHILD".into());
    }

    let sidecar = read_sidecar(&canonical_destination, &receipt.sidecar_sha256)?;
    let plan = validate_sidecar_binding(receipt, &sidecar)?;
    let validated = staging_manifest::validate(&plan, &sidecar.source_completion.manifest)?;
    if validated.manifest_hash != receipt.manifest_hash
        || validated.file_count != receipt.file_count
        || validated.total_bytes != receipt.total_bytes
        || validated.inventory_mutation_authorized
        || validated.task_completion_authorized
        || validated.promotion_authorized
        || validated.replacement_authorized
        || validated.physical_delete_authorized
    {
        return Err("LOCAL_RESCAN_MANIFEST_RECEIPT_MISMATCH".into());
    }
    verify_filesystem(
        &canonical_destination,
        &sidecar.source_completion.manifest,
        receipt,
    )?;

    let observation_hash = hash(&(
        receipt.work_id.as_str(),
        receipt.target_hash.as_str(),
        receipt.source.as_str(),
        receipt.source_work_id.as_str(),
        receipt.library_relative_dir.as_str(),
        receipt.sidecar_sha256.as_str(),
        receipt.manifest_hash.as_str(),
        receipt.file_count,
        receipt.total_bytes,
    ));
    let rescan_id = format!("RESCAN_{}", &observation_hash[..20]);
    Ok(LocalInventoryRescanReport {
        schema_version: LOCAL_INVENTORY_RESCAN_SCHEMA_VERSION,
        rescan_id,
        command_id: receipt.command_id.clone(),
        task_id: receipt.task_id.clone(),
        work_id: receipt.work_id.clone(),
        task_revision: receipt.task_revision,
        target_hash: receipt.target_hash.clone(),
        source: receipt.source.clone(),
        source_work_id: receipt.source_work_id.clone(),
        library_relative_dir: receipt.library_relative_dir.clone(),
        sidecar_sha256: receipt.sidecar_sha256.clone(),
        manifest_hash: receipt.manifest_hash.clone(),
        file_count: receipt.file_count,
        total_bytes: receipt.total_bytes,
        sidecar_verified: true,
        manifest_verified: true,
        filesystem_verified: true,
        inventory_observation_complete: true,
        inventory_observation_hash: observation_hash,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
        production_enablement_authorized: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::staging_manifest::{StagedArtifact, StagingManifest};
    use serde_json::{json, Value};
    use state_model::Version;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Fixture {
        root: PathBuf,
        receipt: LocalLibraryImportReceipt,
        artifact_path: PathBuf,
        sidecar_path: PathBuf,
    }

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("mangamonitor-v1-4-{label}-{nonce}"))
    }

    fn make_fixture() -> Fixture {
        let root = temp_root("fixture");
        fs::create_dir_all(&root).expect("library root");
        let task_id = "TASK_v1_4_fixture";
        let task_revision = 1u64;
        let target = Target {
            source_key: "jm:123456".into(),
            author: "fixture-author".into(),
            title: "fixture-title".into(),
            version: Version::default(),
            coverage: Value::Null,
        };
        let target_hash = hash(&target);
        let command_id = format!(
            "EXEC_{}",
            &hash(&(task_id, task_revision, target_hash.as_str()))[..20]
        );
        let library_relative_dir = format!("mangamonitor-{command_id}");
        let destination = root.join(&library_relative_dir);
        let chapter = destination.join("chapter-1");
        fs::create_dir_all(&chapter).expect("chapter dir");
        let artifact_path = chapter.join("001.gif");
        let artifact_bytes = b"GIF89a-v1-4-fixture";
        fs::write(&artifact_path, artifact_bytes).expect("artifact");
        let artifact = StagedArtifact {
            relative_path: "chapter-1/001.gif".into(),
            size_bytes: artifact_bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(artifact_bytes)),
        };
        let manifest = StagingManifest {
            schema_version: staging_manifest::STAGING_MANIFEST_SCHEMA_VERSION,
            command_id: command_id.clone(),
            task_id: task_id.into(),
            work_id: "WORK_v1_4_fixture".into(),
            task_revision,
            target_hash: target_hash.clone(),
            backend: "JM".into(),
            source_work_id: "123456".into(),
            staging_subdir: format!("commands/{command_id}"),
            source_enumeration_complete: true,
            all_scheduled_downloads_joined: true,
            downloader_reported_full_completion: true,
            expected_content_units: 1,
            completed_content_units: 1,
            failed_content_units: 0,
            artifacts: vec![artifact],
        };
        let plan = LocalExecutionPlan {
            schema_version: LOCAL_EXECUTOR_SCHEMA_VERSION,
            command_id: command_id.clone(),
            task_id: task_id.into(),
            work_id: "WORK_v1_4_fixture".into(),
            task_revision,
            target_hash: target_hash.clone(),
            backend: "JM".into(),
            source_work_id: "123456".into(),
            intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
            staging_subdir: format!("commands/{command_id}"),
            execution_supported: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
        };
        let validated = staging_manifest::validate(&plan, &manifest).expect("valid manifest");
        let completion = SourceCompletionProof {
            schema_version: SOURCE_COMPLETION_SCHEMA_VERSION,
            source: "jm".into(),
            upstream_commit: JM_UPSTREAM_COMMIT.into(),
            source_contract_verified: true,
            execution_supported: false,
            manifest,
            inventory_mutation_authorized: false,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
        };
        let sidecar = json!({
            "schema_version": LOCAL_LIBRARY_IMPORT_SCHEMA_VERSION,
            "command_id": command_id,
            "task_id": task_id,
            "work_id": "WORK_v1_4_fixture",
            "task_revision": task_revision,
            "target_hash": target_hash,
            "source": "jm",
            "source_work_id": "123456",
            "action": "download",
            "target": target,
            "completed_at": "2026-09-07T00:00:00Z",
            "library_relative_dir": library_relative_dir,
            "source_completion": completion,
            "inventory_rescan_required": true,
            "inventory_mutation_authorized": false,
            "task_completion_authorized": false,
            "replacement_authorized": false,
            "physical_delete_authorized": false
        });
        let mut sidecar_bytes = serde_json::to_vec_pretty(&sidecar).expect("sidecar json");
        sidecar_bytes.push(b'\n');
        let sidecar_path = destination.join(LOCAL_LIBRARY_SIDECAR);
        fs::write(&sidecar_path, &sidecar_bytes).expect("sidecar");
        let sidecar_sha256 = format!("{:x}", Sha256::digest(&sidecar_bytes));
        let receipt = LocalLibraryImportReceipt {
            schema_version: LOCAL_LIBRARY_IMPORT_SCHEMA_VERSION,
            command_id: sidecar["command_id"].as_str().unwrap().into(),
            task_id: task_id.into(),
            work_id: "WORK_v1_4_fixture".into(),
            task_revision,
            target_hash: sidecar["target_hash"].as_str().unwrap().into(),
            source: "jm".into(),
            source_work_id: "123456".into(),
            library_relative_dir: sidecar["library_relative_dir"].as_str().unwrap().into(),
            manifest_hash: validated.manifest_hash,
            file_count: validated.file_count,
            total_bytes: validated.total_bytes,
            sidecar_sha256,
            library_import_completed: true,
            destination_verified: true,
            staging_preserved: true,
            inventory_rescan_required: true,
            inventory_mutation_authorized: false,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
            production_enablement_authorized: false,
        };
        Fixture {
            root,
            receipt,
            artifact_path,
            sidecar_path,
        }
    }

    #[test]
    fn exact_import_rescans_deterministically_without_authority() {
        let fixture = make_fixture();
        let before = fs::read(&fixture.artifact_path).expect("before");
        let first = rescan(&fixture.root, &fixture.receipt).expect("first rescan");
        let second = rescan(&fixture.root, &fixture.receipt).expect("second rescan");
        assert_eq!(first, second);
        assert!(first.sidecar_verified);
        assert!(first.manifest_verified);
        assert!(first.filesystem_verified);
        assert!(first.inventory_observation_complete);
        assert!(!first.inventory_mutation_authorized);
        assert!(!first.task_completion_authorized);
        assert!(!first.replacement_authorized);
        assert!(!first.physical_delete_authorized);
        assert!(!first.production_enablement_authorized);
        assert_eq!(before, fs::read(&fixture.artifact_path).expect("after"));
        fs::remove_dir_all(&fixture.root).ok();
    }

    #[test]
    fn changed_or_extra_media_fails_closed() {
        let fixture = make_fixture();
        fs::write(&fixture.artifact_path, b"GIF89a-tampered").expect("tamper");
        assert_eq!(
            rescan(&fixture.root, &fixture.receipt).unwrap_err(),
            "LOCAL_RESCAN_IMPORTED_FILE_VERIFY_FAILED"
        );
        fs::remove_dir_all(&fixture.root).ok();

        let fixture = make_fixture();
        let destination = fixture.root.join(&fixture.receipt.library_relative_dir);
        fs::write(destination.join("extra.bin"), b"extra").expect("extra");
        assert_eq!(
            rescan(&fixture.root, &fixture.receipt).unwrap_err(),
            "LOCAL_RESCAN_IMPORTED_TREE_MISMATCH"
        );
        fs::remove_dir_all(&fixture.root).ok();
    }

    #[test]
    fn receipt_authority_or_sidecar_drift_is_rejected() {
        let fixture = make_fixture();
        let mut unsafe_receipt = fixture.receipt.clone();
        unsafe_receipt.task_completion_authorized = true;
        assert_eq!(
            rescan(&fixture.root, &unsafe_receipt).unwrap_err(),
            "LOCAL_RESCAN_RECEIPT_UNSAFE_AUTHORITY"
        );
        fs::remove_dir_all(&fixture.root).ok();

        let fixture = make_fixture();
        let mut value: Value = serde_json::from_slice(
            &fs::read(&fixture.sidecar_path).expect("read sidecar"),
        )
        .expect("parse sidecar");
        value["work_id"] = json!("WORK_forged");
        let mut bytes = serde_json::to_vec_pretty(&value).expect("serialize forged sidecar");
        bytes.push(b'\n');
        fs::write(&fixture.sidecar_path, &bytes).expect("write forged sidecar");
        let mut forged_receipt = fixture.receipt.clone();
        forged_receipt.sidecar_sha256 = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(
            rescan(&fixture.root, &forged_receipt).unwrap_err(),
            "LOCAL_RESCAN_SIDECAR_BINDING_MISMATCH"
        );
        fs::remove_dir_all(&fixture.root).ok();
    }

    #[test]
    fn unsafe_relative_directory_or_missing_sidecar_fails_closed() {
        let fixture = make_fixture();
        let mut traversal = fixture.receipt.clone();
        traversal.library_relative_dir = "../escape".into();
        assert_eq!(
            rescan(&fixture.root, &traversal).unwrap_err(),
            "LOCAL_RESCAN_RECEIPT_BINDING_INVALID"
        );
        fs::remove_dir_all(&fixture.root).ok();

        let fixture = make_fixture();
        fs::remove_file(&fixture.sidecar_path).expect("remove sidecar");
        assert_eq!(
            rescan(&fixture.root, &fixture.receipt).unwrap_err(),
            "LOCAL_RESCAN_SIDECAR_MISSING"
        );
        fs::remove_dir_all(&fixture.root).ok();
    }
}
