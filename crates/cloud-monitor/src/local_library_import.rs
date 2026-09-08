//! V1.3 add-only local library import boundary.
//!
//! A successful V1.2 download still lives only in command-owned staging. This
//! module is a separate local-only gate that may copy an exactly verified staging
//! tree into a user-selected library root. It never overwrites an existing
//! destination, never deletes staging or old library content, never mutates
//! monitor-state/inventory, and never marks a task complete.

use crate::{
    assistant_task_gate::GateLedger,
    executor_handoff::{self, ExecutorCommand},
    filesystem_verifier,
    isolated_staging_execution::{
        IsolatedStagingExecutionResult, ISOLATED_STAGING_EXECUTION_SCHEMA_VERSION,
    },
    local_execution_orchestrator::{
        self, LocalExecutionReport, LOCAL_EXECUTION_ORCHESTRATOR_SCHEMA_VERSION,
    },
    local_executor::LocalExecutionPlan,
    monitor::State,
    source_completion::SourceCompletionProof,
    staging_manifest::{self, StagedArtifact, StagingManifest, ValidatedStagingManifest},
    verified_execution_receipt,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

pub const LOCAL_LIBRARY_IMPORT_SCHEMA_VERSION: u64 = 1;
pub const LOCAL_LIBRARY_SIDECAR: &str = "_mangamonitor.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalLibraryImportPlan {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub library_relative_dir: String,
    pub manifest_hash: String,
    pub file_count: u64,
    pub total_bytes: u64,
    pub current_approval_reverified: bool,
    pub staging_reverified: bool,
    pub explicit_confirmation_required: bool,
    pub library_import_authorized: bool,
    pub staging_preserved: bool,
    pub inventory_rescan_required: bool,
    pub inventory_mutation_authorized: bool,
    pub task_completion_authorized: bool,
    pub promotion_authorized: bool,
    pub replacement_authorized: bool,
    pub physical_delete_authorized: bool,
    pub production_enablement_authorized: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalLibraryImportReceipt {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub library_relative_dir: String,
    pub manifest_hash: String,
    pub file_count: u64,
    pub total_bytes: u64,
    pub sidecar_sha256: String,
    pub library_import_completed: bool,
    pub destination_verified: bool,
    pub staging_preserved: bool,
    pub inventory_rescan_required: bool,
    pub inventory_mutation_authorized: bool,
    pub task_completion_authorized: bool,
    pub promotion_authorized: bool,
    pub replacement_authorized: bool,
    pub physical_delete_authorized: bool,
    pub production_enablement_authorized: bool,
}

#[derive(Serialize)]
struct LocalLibrarySidecar<'a> {
    schema_version: u64,
    command_id: &'a str,
    task_id: &'a str,
    work_id: &'a str,
    task_revision: u64,
    target_hash: &'a str,
    source: &'a str,
    source_work_id: &'a str,
    action: &'a str,
    target: &'a crate::monitor::Target,
    completed_at: &'a str,
    library_relative_dir: &'a str,
    source_completion: &'a SourceCompletionProof,
    inventory_rescan_required: bool,
    inventory_mutation_authorized: bool,
    task_completion_authorized: bool,
    replacement_authorized: bool,
    physical_delete_authorized: bool,
}

struct ValidatedImport {
    public_plan: LocalLibraryImportPlan,
    local_plan: LocalExecutionPlan,
    validated_manifest: ValidatedStagingManifest,
    canonical_command_root: PathBuf,
    canonical_library_root: PathBuf,
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
        return Err("LOCAL_LIBRARY_LINK_OR_REPARSE_POINT_FORBIDDEN".into());
    }
    if !metadata.is_dir() {
        return Err("LOCAL_LIBRARY_EXPECTED_DIRECTORY".into());
    }
    fs::canonicalize(path).map_err(|_| "LOCAL_LIBRARY_CANONICALIZE_FAILED".into())
}

fn lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|_| "LOCAL_LIBRARY_FILE_OPEN_FAILED")?;
    let metadata = file
        .metadata()
        .map_err(|_| "LOCAL_LIBRARY_FILE_METADATA_FAILED")?;
    if !metadata.is_file() {
        return Err("LOCAL_LIBRARY_EXPECTED_REGULAR_FILE".into());
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "LOCAL_LIBRARY_FILE_READ_FAILED")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn join_portable(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(root.to_path_buf(), |path, component| path.join(component))
}

fn portable_relative(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "LOCAL_LIBRARY_PATH_ESCAPED_ROOT")?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(value) => parts.push(
                value
                    .to_str()
                    .ok_or("LOCAL_LIBRARY_NON_UTF8_PATH")?
                    .to_string(),
            ),
            _ => return Err("LOCAL_LIBRARY_INVALID_FILESYSTEM_PATH".into()),
        }
    }
    if parts.is_empty() {
        return Err("LOCAL_LIBRARY_INVALID_FILESYSTEM_PATH".into());
    }
    Ok(parts.join("/"))
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

fn walk_import_tree(
    root: &Path,
    directory: &Path,
    expected_dirs: &BTreeSet<String>,
    observed_files: &mut BTreeSet<String>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|_| "LOCAL_LIBRARY_DIRECTORY_READ_FAILED")? {
        let entry = entry.map_err(|_| "LOCAL_LIBRARY_DIRECTORY_READ_FAILED")?;
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| "LOCAL_LIBRARY_METADATA_READ_FAILED")?;
        if link_like(&metadata) {
            return Err("LOCAL_LIBRARY_LINK_OR_REPARSE_POINT_FORBIDDEN".into());
        }
        let relative = portable_relative(root, &path)?;
        if metadata.is_dir() {
            if !expected_dirs.contains(&relative) {
                return Err("LOCAL_LIBRARY_UNEXPECTED_DIRECTORY".into());
            }
            walk_import_tree(root, &path, expected_dirs, observed_files)?;
        } else if metadata.is_file() {
            if !observed_files.insert(relative) {
                return Err("LOCAL_LIBRARY_DUPLICATE_FILESYSTEM_PATH".into());
            }
        } else {
            return Err("LOCAL_LIBRARY_SPECIAL_FILE_FORBIDDEN".into());
        }
    }
    Ok(())
}

fn report_has_no_downstream_authority(report: &LocalExecutionReport) -> bool {
    !report.inventory_mutation_authorized
        && !report.task_completion_authorized
        && !report.promotion_authorized
        && !report.replacement_authorized
        && !report.physical_delete_authorized
        && !report.production_enablement_authorized
}

fn validate_report_binding(
    command: &ExecutorCommand,
    plan: &LocalExecutionPlan,
    report: &LocalExecutionReport,
) -> Result<(), String> {
    if report.schema_version != LOCAL_EXECUTION_ORCHESTRATOR_SCHEMA_VERSION {
        return Err("LOCAL_LIBRARY_REPORT_SCHEMA_MISMATCH".into());
    }
    if report.command_id != command.command_id
        || report.task_id != command.task_id
        || report.work_id != command.work_id
        || report.task_revision != command.task_revision
        || report.target_hash != command.target_hash
        || report.source != command.source
        || report.source_work_id != command.source_work_id
        || report.staging_subdir != plan.staging_subdir
        || !report.staging_execution_completed
        || !lower_sha256(&report.preflight_hash)
    {
        return Err("LOCAL_LIBRARY_REPORT_BINDING_MISMATCH".into());
    }
    if !report_has_no_downstream_authority(report) {
        return Err("LOCAL_LIBRARY_REPORT_UNSAFE_AUTHORITY".into());
    }
    Ok(())
}

fn validate_current_receipt(
    state: &State,
    ledger: &GateLedger,
    command: &ExecutorCommand,
    report: &LocalExecutionReport,
) -> Result<LocalExecutionPlan, String> {
    let (local_plan, _) = local_execution_orchestrator::prepare_current(state, ledger, command)?;
    validate_report_binding(command, &local_plan, report)?;
    let current_view = executor_handoff::receipt_view(state, ledger, &report.receipt)?;
    if current_view["ready_for_inventory_verification"] != true
        || current_view["task_completion_authorized"] != false
        || current_view["replacement_authorized"] != false
        || current_view["physical_delete_authorized"] != false
    {
        return Err("LOCAL_LIBRARY_RECEIPT_NOT_CURRENT".into());
    }
    if current_view != report.receipt_view {
        return Err("LOCAL_LIBRARY_RECEIPT_VIEW_DRIFT".into());
    }
    Ok(local_plan)
}

fn validate_roots(staging_root: &Path, library_root: &Path) -> Result<(PathBuf, PathBuf), String> {
    let staging = checked_directory(staging_root, "LOCAL_LIBRARY_STAGING_ROOT_MISSING")?;
    let library = checked_directory(library_root, "LOCAL_LIBRARY_ROOT_MISSING")?;
    if staging == library || staging.starts_with(&library) || library.starts_with(&staging) {
        return Err("LOCAL_LIBRARY_ROOT_OVERLAPS_STAGING".into());
    }
    Ok((staging, library))
}

fn validate_inputs(
    state: &State,
    ledger: &GateLedger,
    command: &ExecutorCommand,
    report: &LocalExecutionReport,
    staging_root: &Path,
    library_root: &Path,
) -> Result<ValidatedImport, String> {
    let local_plan = validate_current_receipt(state, ledger, command, report)?;

    let reconstructed = IsolatedStagingExecutionResult {
        schema_version: ISOLATED_STAGING_EXECUTION_SCHEMA_VERSION,
        command_id: report.command_id.clone(),
        task_id: report.task_id.clone(),
        work_id: report.work_id.clone(),
        task_revision: report.task_revision,
        target_hash: report.target_hash.clone(),
        source: report.source.clone(),
        source_work_id: report.source_work_id.clone(),
        preflight_hash: report.preflight_hash.clone(),
        staging_execution_completed: report.staging_execution_completed,
        source_completion: report.source_completion.clone(),
        filesystem_verification: report.filesystem_verification.clone(),
        inventory_mutation_authorized: report.inventory_mutation_authorized,
        task_completion_authorized: report.task_completion_authorized,
        promotion_authorized: report.promotion_authorized,
        replacement_authorized: report.replacement_authorized,
        physical_delete_authorized: report.physical_delete_authorized,
    };
    let rebuilt_receipt = verified_execution_receipt::build(
        command,
        &local_plan,
        &reconstructed,
        &report.receipt.completed_at,
    )?;
    if rebuilt_receipt != report.receipt {
        return Err("LOCAL_LIBRARY_RECEIPT_REBUILD_MISMATCH".into());
    }

    let validated_manifest =
        staging_manifest::validate(&local_plan, &report.source_completion.manifest)?;
    let fresh_filesystem = filesystem_verifier::verify(
        staging_root,
        &local_plan,
        &report.source_completion.manifest,
    )?;
    if fresh_filesystem != report.filesystem_verification
        || fresh_filesystem.manifest_hash != validated_manifest.manifest_hash
        || fresh_filesystem.file_count != validated_manifest.file_count
        || fresh_filesystem.total_bytes != validated_manifest.total_bytes
    {
        return Err("LOCAL_LIBRARY_STAGING_REVERIFY_MISMATCH".into());
    }

    let (canonical_staging_root, canonical_library_root) =
        validate_roots(staging_root, library_root)?;
    let command_root = canonical_staging_root
        .join("commands")
        .join(&local_plan.command_id);
    let canonical_command_root = checked_directory(
        &command_root,
        "LOCAL_LIBRARY_STAGING_COMMAND_ROOT_MISSING",
    )?;
    if !canonical_command_root.starts_with(&canonical_staging_root) {
        return Err("LOCAL_LIBRARY_STAGING_COMMAND_ESCAPED_ROOT".into());
    }

    let library_relative_dir = format!("mangamonitor-{}", command.command_id);
    if canonical_library_root.join(&library_relative_dir).exists() {
        return Err("LOCAL_LIBRARY_DESTINATION_ALREADY_EXISTS".into());
    }

    Ok(ValidatedImport {
        public_plan: LocalLibraryImportPlan {
            schema_version: LOCAL_LIBRARY_IMPORT_SCHEMA_VERSION,
            command_id: command.command_id.clone(),
            task_id: command.task_id.clone(),
            work_id: command.work_id.clone(),
            task_revision: command.task_revision,
            target_hash: command.target_hash.clone(),
            source: command.source.clone(),
            source_work_id: command.source_work_id.clone(),
            library_relative_dir,
            manifest_hash: validated_manifest.manifest_hash.clone(),
            file_count: validated_manifest.file_count,
            total_bytes: validated_manifest.total_bytes,
            current_approval_reverified: true,
            staging_reverified: true,
            explicit_confirmation_required: true,
            library_import_authorized: false,
            staging_preserved: true,
            inventory_rescan_required: true,
            inventory_mutation_authorized: false,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
            production_enablement_authorized: false,
        },
        local_plan,
        validated_manifest,
        canonical_command_root,
        canonical_library_root,
    })
}

/// Read-only dry-run for the add-only import gate.
pub fn plan(
    state: &State,
    ledger: &GateLedger,
    command: &ExecutorCommand,
    report: &LocalExecutionReport,
    staging_root: &Path,
    library_root: &Path,
) -> Result<LocalLibraryImportPlan, String> {
    Ok(validate_inputs(state, ledger, command, report, staging_root, library_root)?.public_plan)
}

fn reauthorize<Reload>(
    reload: &mut Reload,
    command: &ExecutorCommand,
    report: &LocalExecutionReport,
) -> Result<(), String>
where
    Reload: FnMut() -> Result<(State, GateLedger), String>,
{
    let (state, ledger) = reload()?;
    validate_current_receipt(&state, &ledger, command, report)?;
    Ok(())
}

fn ensure_parent_directories(root: &Path, relative_path: &str) -> Result<(), String> {
    let path = join_portable(root, relative_path);
    let parent = path.parent().ok_or("LOCAL_LIBRARY_ARTIFACT_PARENT_MISSING")?;
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| "LOCAL_LIBRARY_PATH_ESCAPED_DESTINATION")?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err("LOCAL_LIBRARY_INVALID_FILESYSTEM_PATH".into());
        };
        current.push(value);
        match fs::create_dir(&current) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err("LOCAL_LIBRARY_DIRECTORY_CREATE_FAILED".into()),
        }
        let metadata = fs::symlink_metadata(&current)
            .map_err(|_| "LOCAL_LIBRARY_DIRECTORY_METADATA_FAILED")?;
        if link_like(&metadata) || !metadata.is_dir() {
            return Err("LOCAL_LIBRARY_LINK_OR_REPARSE_POINT_FORBIDDEN".into());
        }
    }
    Ok(())
}

fn copy_one_artifact(
    command_root: &Path,
    destination_root: &Path,
    artifact: &StagedArtifact,
) -> Result<(), String> {
    let source = join_portable(command_root, &artifact.relative_path);
    let source_metadata =
        fs::symlink_metadata(&source).map_err(|_| "LOCAL_LIBRARY_STAGING_FILE_MISSING")?;
    if link_like(&source_metadata) || !source_metadata.is_file() {
        return Err("LOCAL_LIBRARY_STAGING_FILE_NOT_REGULAR".into());
    }
    let canonical_source =
        fs::canonicalize(&source).map_err(|_| "LOCAL_LIBRARY_STAGING_FILE_CANONICALIZE_FAILED")?;
    if !canonical_source.starts_with(command_root) {
        return Err("LOCAL_LIBRARY_STAGING_FILE_ESCAPED_COMMAND".into());
    }

    ensure_parent_directories(destination_root, &artifact.relative_path)?;
    let destination = join_portable(destination_root, &artifact.relative_path);
    let mut input = File::open(&source).map_err(|_| "LOCAL_LIBRARY_STAGING_FILE_OPEN_FAILED")?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                "LOCAL_LIBRARY_FILE_ALREADY_EXISTS".to_string()
            } else {
                "LOCAL_LIBRARY_FILE_CREATE_FAILED".to_string()
            }
        })?;

    let mut hasher = Sha256::new();
    let mut copied = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|_| "LOCAL_LIBRARY_STAGING_FILE_READ_FAILED")?;
        if read == 0 {
            break;
        }
        let chunk = &buffer[..read];
        hasher.update(chunk);
        output
            .write_all(chunk)
            .map_err(|_| "LOCAL_LIBRARY_FILE_WRITE_FAILED")?;
        copied = copied
            .checked_add(u64::try_from(read).map_err(|_| "LOCAL_LIBRARY_SIZE_OVERFLOW")?)
            .ok_or("LOCAL_LIBRARY_SIZE_OVERFLOW")?;
    }
    output
        .sync_all()
        .map_err(|_| "LOCAL_LIBRARY_FILE_SYNC_FAILED")?;

    let source_hash = format!("{:x}", hasher.finalize());
    if copied != artifact.size_bytes || source_hash != artifact.sha256 {
        return Err("LOCAL_LIBRARY_STAGING_CHANGED_DURING_COPY".into());
    }
    let destination_metadata = fs::symlink_metadata(&destination)
        .map_err(|_| "LOCAL_LIBRARY_FILE_METADATA_FAILED")?;
    if link_like(&destination_metadata)
        || !destination_metadata.is_file()
        || destination_metadata.len() != artifact.size_bytes
        || sha256_file(&destination)? != artifact.sha256
    {
        return Err("LOCAL_LIBRARY_DESTINATION_FILE_VERIFY_FAILED".into());
    }
    Ok(())
}

fn verify_media_tree(root: &Path, manifest: &StagingManifest) -> Result<(), String> {
    let canonical_root = checked_directory(root, "LOCAL_LIBRARY_IMPORTED_ROOT_MISSING")?;
    let expected_dirs = expected_directories(manifest);
    let expected_files: BTreeSet<String> = manifest
        .artifacts
        .iter()
        .map(|artifact| artifact.relative_path.clone())
        .collect();
    let mut observed_files = BTreeSet::new();
    walk_import_tree(
        &canonical_root,
        &canonical_root,
        &expected_dirs,
        &mut observed_files,
    )?;
    if observed_files != expected_files {
        return Err("LOCAL_LIBRARY_IMPORTED_TREE_MISMATCH".into());
    }
    for artifact in &manifest.artifacts {
        let path = join_portable(&canonical_root, &artifact.relative_path);
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| "LOCAL_LIBRARY_IMPORTED_FILE_MISSING")?;
        if link_like(&metadata)
            || !metadata.is_file()
            || metadata.len() != artifact.size_bytes
            || sha256_file(&path)? != artifact.sha256
        {
            return Err("LOCAL_LIBRARY_IMPORTED_FILE_VERIFY_FAILED".into());
        }
    }
    Ok(())
}

fn write_sidecar(
    destination_root: &Path,
    command: &ExecutorCommand,
    report: &LocalExecutionReport,
    library_relative_dir: &str,
) -> Result<String, String> {
    let sidecar = LocalLibrarySidecar {
        schema_version: LOCAL_LIBRARY_IMPORT_SCHEMA_VERSION,
        command_id: &command.command_id,
        task_id: &command.task_id,
        work_id: &command.work_id,
        task_revision: command.task_revision,
        target_hash: &command.target_hash,
        source: &command.source,
        source_work_id: &command.source_work_id,
        action: &command.action,
        target: &command.target,
        completed_at: &report.receipt.completed_at,
        library_relative_dir,
        source_completion: &report.source_completion,
        inventory_rescan_required: true,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    };
    let mut bytes = serde_json::to_vec_pretty(&sidecar)
        .map_err(|_| "LOCAL_LIBRARY_SIDECAR_SERIALIZE_FAILED")?;
    bytes.push(b'\n');
    let hash = sha256_bytes(&bytes);
    let path = destination_root.join(LOCAL_LIBRARY_SIDECAR);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|_| "LOCAL_LIBRARY_SIDECAR_CREATE_FAILED")?;
    file.write_all(&bytes)
        .map_err(|_| "LOCAL_LIBRARY_SIDECAR_WRITE_FAILED")?;
    file.sync_all()
        .map_err(|_| "LOCAL_LIBRARY_SIDECAR_SYNC_FAILED")?;
    if sha256_file(&path)? != hash {
        return Err("LOCAL_LIBRARY_SIDECAR_VERIFY_FAILED".into());
    }
    Ok(hash)
}

fn verify_complete_tree(
    root: &Path,
    manifest: &StagingManifest,
    sidecar_sha256: &str,
) -> Result<(), String> {
    let canonical_root = checked_directory(root, "LOCAL_LIBRARY_IMPORTED_ROOT_MISSING")?;
    let expected_dirs = expected_directories(manifest);
    let mut expected_files: BTreeSet<String> = manifest
        .artifacts
        .iter()
        .map(|artifact| artifact.relative_path.clone())
        .collect();
    expected_files.insert(LOCAL_LIBRARY_SIDECAR.into());
    let mut observed_files = BTreeSet::new();
    walk_import_tree(
        &canonical_root,
        &canonical_root,
        &expected_dirs,
        &mut observed_files,
    )?;
    if observed_files != expected_files {
        return Err("LOCAL_LIBRARY_IMPORTED_TREE_MISMATCH".into());
    }
    for artifact in &manifest.artifacts {
        let path = join_portable(&canonical_root, &artifact.relative_path);
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| "LOCAL_LIBRARY_IMPORTED_FILE_MISSING")?;
        if link_like(&metadata)
            || !metadata.is_file()
            || metadata.len() != artifact.size_bytes
            || sha256_file(&path)? != artifact.sha256
        {
            return Err("LOCAL_LIBRARY_IMPORTED_FILE_VERIFY_FAILED".into());
        }
    }
    let sidecar = canonical_root.join(LOCAL_LIBRARY_SIDECAR);
    if !lower_sha256(sidecar_sha256) || sha256_file(&sidecar)? != sidecar_sha256 {
        return Err("LOCAL_LIBRARY_IMPORTED_SIDECAR_VERIFY_FAILED".into());
    }
    Ok(())
}

/// Execute the separate add-only local import gate.
///
/// The exact command ID must be repeated as explicit confirmation. The final
/// destination is atomically *reserved* with `create_dir`, which cannot replace
/// an existing file or directory. Every artifact is then created with
/// `create_new`. The sidecar is written last and is the completed-import marker.
/// If a copy or current-approval recheck fails, partial output is retained but has
/// no sidecar and therefore cannot be mistaken for a completed V1.3 import.
pub fn execute_add_only<Reload>(
    state: &State,
    ledger: &GateLedger,
    command: &ExecutorCommand,
    report: &LocalExecutionReport,
    staging_root: &Path,
    library_root: &Path,
    confirmation_command_id: &str,
    mut reload: Reload,
) -> Result<LocalLibraryImportReceipt, String>
where
    Reload: FnMut() -> Result<(State, GateLedger), String>,
{
    if confirmation_command_id != command.command_id {
        return Err("LOCAL_LIBRARY_EXPLICIT_CONFIRMATION_REQUIRED".into());
    }
    let validated = validate_inputs(state, ledger, command, report, staging_root, library_root)?;
    let public = &validated.public_plan;
    let destination_root = validated
        .canonical_library_root
        .join(&public.library_relative_dir);

    match fs::create_dir(&destination_root) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err("LOCAL_LIBRARY_DESTINATION_ALREADY_EXISTS".into());
        }
        Err(_) => return Err("LOCAL_LIBRARY_DESTINATION_CREATE_FAILED".into()),
    }
    let destination_metadata = fs::symlink_metadata(&destination_root)
        .map_err(|_| "LOCAL_LIBRARY_DESTINATION_METADATA_FAILED")?;
    if link_like(&destination_metadata) || !destination_metadata.is_dir() {
        return Err("LOCAL_LIBRARY_DESTINATION_UNSAFE".into());
    }

    for artifact in &report.source_completion.manifest.artifacts {
        reauthorize(&mut reload, command, report)?;
        copy_one_artifact(
            &validated.canonical_command_root,
            &destination_root,
            artifact,
        )?;
    }
    verify_media_tree(&destination_root, &report.source_completion.manifest)?;

    // Linearization point for completion: current approval must still be exact
    // immediately before the completion marker is created.
    reauthorize(&mut reload, command, report)?;
    let sidecar_sha256 = write_sidecar(
        &destination_root,
        command,
        report,
        &public.library_relative_dir,
    )?;
    verify_complete_tree(
        &destination_root,
        &report.source_completion.manifest,
        &sidecar_sha256,
    )?;

    Ok(LocalLibraryImportReceipt {
        schema_version: LOCAL_LIBRARY_IMPORT_SCHEMA_VERSION,
        command_id: command.command_id.clone(),
        task_id: command.task_id.clone(),
        work_id: command.work_id.clone(),
        task_revision: command.task_revision,
        target_hash: command.target_hash.clone(),
        source: command.source.clone(),
        source_work_id: command.source_work_id.clone(),
        library_relative_dir: public.library_relative_dir.clone(),
        manifest_hash: validated.validated_manifest.manifest_hash,
        file_count: validated.validated_manifest.file_count,
        total_bytes: validated.validated_manifest.total_bytes,
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assistant_task_gate::{target_hash, GateRecord},
        filesystem_verifier::FILESYSTEM_VERIFIER_SCHEMA_VERSION,
        monitor::{hash, Decisions, Scan, Target, Task},
        source_completion::{JM_UPSTREAM_COMMIT, SOURCE_COMPLETION_SCHEMA_VERSION},
        staging_manifest::{StagingManifest, STAGING_MANIFEST_SCHEMA_VERSION},
    };
    use serde_json::{json, Value};
    use state_model::Version;
    use std::{
        collections::BTreeMap,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(1);

    fn fixture_identity() -> (State, GateLedger, ExecutorCommand) {
        let task = Task {
            task_id: "TASK_V1_3".into(),
            work_id: "WORK_V1_3".into(),
            first_seen: "fixed".into(),
            task_revision: 1,
            target: Target {
                source_key: "jm:123456".into(),
                author: "Writer".into(),
                title: "V1.3".into(),
                version: Version::default(),
                coverage: Value::Null,
            },
            action: "download".into(),
            status: "pending".into(),
            old_local_item_ids: Vec::new(),
        };
        let state = State {
            authors: json!({"authors":[]}),
            inventory: json!({"works":[]}),
            catalog: BTreeMap::new(),
            pending: BTreeMap::from([(task.work_id.clone(), task.clone())]),
            review: BTreeMap::new(),
            cleanup_review: json!([]),
            decisions: Decisions::default(),
            scan: Scan::default(),
        };
        let target_hash = target_hash(&task);
        let ledger = GateLedger {
            schema_version: 1,
            records: vec![GateRecord {
                task_id: task.task_id.clone(),
                task_revision: task.task_revision,
                target_hash: target_hash.clone(),
                assistant_recommended: true,
                user_approved: true,
            }],
        };
        let digest = hash(&(
            task.task_id.as_str(),
            task.task_revision,
            target_hash.as_str(),
        ));
        let command = ExecutorCommand {
            schema_version: 1,
            command_id: format!("EXEC_{}", &digest[..20]),
            task_id: task.task_id,
            work_id: task.work_id,
            task_revision: task.task_revision,
            target_hash,
            source: "jm".into(),
            source_work_id: "123456".into(),
            action: task.action,
            intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
            target: task.target,
        };
        (state, ledger, command)
    }

    fn test_roots(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "mangamonitor-v1-3-{name}-{}-{id}",
            std::process::id()
        ));
        let staging = root.join("staging");
        let library = root.join("library");
        fs::create_dir_all(staging.join("commands")).unwrap();
        fs::create_dir_all(&library).unwrap();
        (root, staging, library)
    }

    fn build_report(
        state: &State,
        ledger: &GateLedger,
        command: &ExecutorCommand,
        staging_root: &Path,
    ) -> LocalExecutionReport {
        let local_plan = crate::local_executor::plan(command).unwrap();
        let command_root = staging_root.join("commands").join(&command.command_id);
        let chapter = command_root.join("chapters/000001-123456");
        fs::create_dir_all(&chapter).unwrap();
        let samples: [(&str, &[u8]); 2] = [
            ("chapters/000001-123456/000001.gif", b"GIF89aV1_3_A"),
            ("chapters/000001-123456/000002.gif", b"GIF89aV1_3_B"),
        ];
        let mut artifacts = Vec::new();
        for (relative, bytes) in samples {
            fs::write(join_portable(&command_root, relative), bytes).unwrap();
            artifacts.push(StagedArtifact {
                relative_path: relative.into(),
                size_bytes: u64::try_from(bytes.len()).unwrap(),
                sha256: sha256_bytes(bytes),
            });
        }
        let manifest = StagingManifest {
            schema_version: STAGING_MANIFEST_SCHEMA_VERSION,
            command_id: local_plan.command_id.clone(),
            task_id: local_plan.task_id.clone(),
            work_id: local_plan.work_id.clone(),
            task_revision: local_plan.task_revision,
            target_hash: local_plan.target_hash.clone(),
            backend: local_plan.backend.clone(),
            source_work_id: local_plan.source_work_id.clone(),
            staging_subdir: local_plan.staging_subdir.clone(),
            source_enumeration_complete: true,
            all_scheduled_downloads_joined: true,
            downloader_reported_full_completion: true,
            expected_content_units: 2,
            completed_content_units: 2,
            failed_content_units: 0,
            artifacts,
        };
        let source_completion = SourceCompletionProof {
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
        let filesystem_verification = filesystem_verifier::verify(
            staging_root,
            &local_plan,
            &source_completion.manifest,
        )
        .unwrap();
        assert_eq!(
            filesystem_verification.schema_version,
            FILESYSTEM_VERIFIER_SCHEMA_VERSION
        );
        let execution = IsolatedStagingExecutionResult {
            schema_version: ISOLATED_STAGING_EXECUTION_SCHEMA_VERSION,
            command_id: command.command_id.clone(),
            task_id: command.task_id.clone(),
            work_id: command.work_id.clone(),
            task_revision: command.task_revision,
            target_hash: command.target_hash.clone(),
            source: command.source.clone(),
            source_work_id: command.source_work_id.clone(),
            preflight_hash: "a".repeat(64),
            staging_execution_completed: true,
            source_completion: source_completion.clone(),
            filesystem_verification: filesystem_verification.clone(),
            inventory_mutation_authorized: false,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
        };
        let receipt = verified_execution_receipt::build(
            command,
            &local_plan,
            &execution,
            "2026-09-07T09:30:00Z",
        )
        .unwrap();
        let receipt_view = executor_handoff::receipt_view(state, ledger, &receipt).unwrap();
        LocalExecutionReport {
            schema_version: LOCAL_EXECUTION_ORCHESTRATOR_SCHEMA_VERSION,
            command_id: command.command_id.clone(),
            task_id: command.task_id.clone(),
            work_id: command.work_id.clone(),
            task_revision: command.task_revision,
            target_hash: command.target_hash.clone(),
            source: command.source.clone(),
            source_work_id: command.source_work_id.clone(),
            staging_subdir: local_plan.staging_subdir,
            preflight_hash: execution.preflight_hash,
            staging_execution_completed: true,
            receipt,
            source_completion,
            filesystem_verification,
            receipt_view,
            inventory_mutation_authorized: false,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
            production_enablement_authorized: false,
        }
    }

    #[test]
    fn exact_current_report_imports_add_only_and_preserves_staging() {
        let (state, ledger, command) = fixture_identity();
        let (root, staging, library) = test_roots("success");
        let report = build_report(&state, &ledger, &command, &staging);
        let dry_run = plan(&state, &ledger, &command, &report, &staging, &library).unwrap();
        assert!(!dry_run.library_import_authorized);
        assert!(dry_run.explicit_confirmation_required);
        let receipt = execute_add_only(
            &state,
            &ledger,
            &command,
            &report,
            &staging,
            &library,
            &command.command_id,
            || Ok((state.clone(), ledger.clone())),
        )
        .unwrap();
        assert!(receipt.library_import_completed);
        assert!(receipt.destination_verified);
        assert!(receipt.staging_preserved);
        assert!(receipt.inventory_rescan_required);
        assert!(!receipt.inventory_mutation_authorized);
        assert!(!receipt.task_completion_authorized);
        assert!(!receipt.replacement_authorized);
        assert!(!receipt.physical_delete_authorized);
        assert!(
            staging
                .join("commands")
                .join(&command.command_id)
                .join("chapters/000001-123456/000001.gif")
                .is_file()
        );
        assert!(
            library
                .join(&receipt.library_relative_dir)
                .join(LOCAL_LIBRARY_SIDECAR)
                .is_file()
        );
        assert_eq!(
            execute_add_only(
                &state,
                &ledger,
                &command,
                &report,
                &staging,
                &library,
                &command.command_id,
                || Ok((state.clone(), ledger.clone())),
            )
            .unwrap_err(),
            "LOCAL_LIBRARY_DESTINATION_ALREADY_EXISTS"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn confirmation_and_current_approval_are_required_before_mutation() {
        let (state, ledger, command) = fixture_identity();
        let (root, staging, library) = test_roots("approval");
        let report = build_report(&state, &ledger, &command, &staging);
        assert_eq!(
            execute_add_only(
                &state,
                &ledger,
                &command,
                &report,
                &staging,
                &library,
                "wrong",
                || Ok((state.clone(), ledger.clone())),
            )
            .unwrap_err(),
            "LOCAL_LIBRARY_EXPLICIT_CONFIRMATION_REQUIRED"
        );
        assert_eq!(fs::read_dir(&library).unwrap().count(), 0);

        let mut revoked = ledger.clone();
        revoked.records[0].user_approved = false;
        assert!(execute_add_only(
            &state,
            &revoked,
            &command,
            &report,
            &staging,
            &library,
            &command.command_id,
            || Ok((state.clone(), revoked.clone())),
        )
        .is_err());
        assert_eq!(fs::read_dir(&library).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preexisting_destination_is_never_overwritten() {
        let (state, ledger, command) = fixture_identity();
        let (root, staging, library) = test_roots("collision");
        let report = build_report(&state, &ledger, &command, &staging);
        let destination = library.join(format!("mangamonitor-{}", command.command_id));
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("sentinel.txt"), b"KEEP").unwrap();
        assert_eq!(
            execute_add_only(
                &state,
                &ledger,
                &command,
                &report,
                &staging,
                &library,
                &command.command_id,
                || Ok((state.clone(), ledger.clone())),
            )
            .unwrap_err(),
            "LOCAL_LIBRARY_DESTINATION_ALREADY_EXISTS"
        );
        assert_eq!(fs::read(destination.join("sentinel.txt")).unwrap(), b"KEEP");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn revoked_approval_mid_import_keeps_partial_without_completion_marker() {
        let (state, ledger, command) = fixture_identity();
        let (root, staging, library) = test_roots("mid-revoke");
        let report = build_report(&state, &ledger, &command, &staging);
        let mut revoked = ledger.clone();
        revoked.records[0].user_approved = false;
        let mut calls = 0u8;
        let error = execute_add_only(
            &state,
            &ledger,
            &command,
            &report,
            &staging,
            &library,
            &command.command_id,
            || {
                calls += 1;
                if calls == 1 {
                    Ok((state.clone(), ledger.clone()))
                } else {
                    Ok((state.clone(), revoked.clone()))
                }
            },
        )
        .unwrap_err();
        assert_eq!(error, "SOURCE_PREFLIGHT_CURRENT_APPROVAL_REQUIRED");
        let destination = library.join(format!("mangamonitor-{}", command.command_id));
        assert!(
            destination
                .join("chapters/000001-123456/000001.gif")
                .is_file()
        );
        assert!(
            !destination
                .join("chapters/000001-123456/000002.gif")
                .exists()
        );
        assert!(!destination.join(LOCAL_LIBRARY_SIDECAR).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn staging_drift_or_forged_authority_fails_closed() {
        let (state, ledger, command) = fixture_identity();
        let (root, staging, library) = test_roots("drift");
        let mut report = build_report(&state, &ledger, &command, &staging);
        report.promotion_authorized = true;
        assert_eq!(
            plan(&state, &ledger, &command, &report, &staging, &library).unwrap_err(),
            "LOCAL_LIBRARY_REPORT_UNSAFE_AUTHORITY"
        );
        report.promotion_authorized = false;
        fs::write(
            staging
                .join("commands")
                .join(&command.command_id)
                .join("chapters/000001-123456/000001.gif"),
            b"GIF89aCHANGED",
        )
        .unwrap();
        assert!(plan(&state, &ledger, &command, &report, &staging, &library).is_err());
        assert_eq!(fs::read_dir(&library).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }
}
