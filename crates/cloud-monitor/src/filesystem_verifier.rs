//! Read-only verification of actual A6 staging files against an accepted A6.3 manifest.
//!
//! A6.4 is deliberately verification-only: it never downloads, promotes, replaces,
//! mutates inventory, completes a task, or deletes anything.

use crate::{
    local_executor::LocalExecutionPlan,
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

pub const FILESYSTEM_VERIFIER_SCHEMA_VERSION: u64 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilesystemVerification {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub manifest_hash: String,
    pub file_count: u64,
    pub total_bytes: u64,
    pub filesystem_verified: bool,
    pub inventory_mutation_authorized: bool,
    pub task_completion_authorized: bool,
    pub promotion_authorized: bool,
    pub replacement_authorized: bool,
    pub physical_delete_authorized: bool,
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

fn checked_directory(path: &Path, missing: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| missing.to_string())?;
    if link_like(&metadata) {
        return Err("STAGING_LINK_OR_REPARSE_POINT_FORBIDDEN".into());
    }
    if !metadata.is_dir() {
        return Err("STAGING_EXPECTED_DIRECTORY".into());
    }
    Ok(())
}

fn portable_relative(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "STAGING_PATH_ESCAPED_ROOT")?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(value) => parts.push(
                value
                    .to_str()
                    .ok_or("STAGING_NON_UTF8_PATH")?
                    .to_string(),
            ),
            _ => return Err("STAGING_INVALID_FILESYSTEM_PATH".into()),
        }
    }
    if parts.is_empty() {
        return Err("STAGING_INVALID_FILESYSTEM_PATH".into());
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

fn walk_command_tree(
    root: &Path,
    directory: &Path,
    expected_dirs: &BTreeSet<String>,
    observed_files: &mut BTreeSet<String>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory).map_err(|_| "STAGING_DIRECTORY_READ_FAILED")?;
    for entry in entries {
        let entry = entry.map_err(|_| "STAGING_DIRECTORY_READ_FAILED")?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|_| "STAGING_METADATA_READ_FAILED")?;
        if link_like(&metadata) {
            return Err("STAGING_LINK_OR_REPARSE_POINT_FORBIDDEN".into());
        }
        let relative = portable_relative(root, &path)?;
        if metadata.is_dir() {
            if !expected_dirs.contains(&relative) {
                return Err("STAGING_UNEXPECTED_DIRECTORY".into());
            }
            walk_command_tree(root, &path, expected_dirs, observed_files)?;
        } else if metadata.is_file() {
            if !observed_files.insert(relative) {
                return Err("STAGING_DUPLICATE_FILESYSTEM_PATH".into());
            }
        } else {
            return Err("STAGING_SPECIAL_FILE_FORBIDDEN".into());
        }
    }
    Ok(())
}

fn join_portable(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(root.to_path_buf(), |path, component| path.join(component))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|_| "STAGING_FILE_OPEN_FAILED")?;
    let metadata = file
        .metadata()
        .map_err(|_| "STAGING_FILE_METADATA_FAILED")?;
    if !metadata.is_file() {
        return Err("STAGING_EXPECTED_REGULAR_FILE".into());
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "STAGING_FILE_READ_FAILED")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Verify one command-owned staging tree against an already validated A6.3 manifest.
///
/// The verifier is fail-closed. The command directory must contain exactly the
/// manifest files and only directories required as their ancestors. Links,
/// Windows reparse points, special files, missing files, extra files/directories,
/// size drift, and hash drift are rejected.
pub fn verify(
    staging_root: &Path,
    plan: &LocalExecutionPlan,
    manifest: &StagingManifest,
) -> Result<FilesystemVerification, String> {
    let validated = staging_manifest::validate(plan, manifest)?;

    checked_directory(staging_root, "STAGING_ROOT_MISSING")?;
    let commands_root = staging_root.join("commands");
    checked_directory(&commands_root, "STAGING_COMMANDS_DIRECTORY_MISSING")?;
    let command_root = commands_root.join(&plan.command_id);
    checked_directory(&command_root, "STAGING_COMMAND_DIRECTORY_MISSING")?;

    let canonical_staging =
        fs::canonicalize(staging_root).map_err(|_| "STAGING_ROOT_CANONICALIZE_FAILED")?;
    let canonical_command =
        fs::canonicalize(&command_root).map_err(|_| "STAGING_COMMAND_CANONICALIZE_FAILED")?;
    if !canonical_command.starts_with(&canonical_staging) {
        return Err("STAGING_COMMAND_ESCAPED_ROOT".into());
    }

    let expected_files: BTreeSet<String> = manifest
        .artifacts
        .iter()
        .map(|artifact| artifact.relative_path.clone())
        .collect();
    let expected_dirs = expected_directories(manifest);
    let mut observed_files = BTreeSet::new();
    walk_command_tree(
        &command_root,
        &command_root,
        &expected_dirs,
        &mut observed_files,
    )?;

    if observed_files != expected_files {
        return if observed_files.is_superset(&expected_files) {
            Err("STAGING_UNEXPECTED_FILE".into())
        } else {
            Err("STAGING_MANIFEST_FILE_MISSING".into())
        };
    }

    for artifact in &manifest.artifacts {
        let path = join_portable(&command_root, &artifact.relative_path);
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| "STAGING_MANIFEST_FILE_MISSING")?;
        if link_like(&metadata) {
            return Err("STAGING_LINK_OR_REPARSE_POINT_FORBIDDEN".into());
        }
        if !metadata.is_file() {
            return Err("STAGING_EXPECTED_REGULAR_FILE".into());
        }
        if metadata.len() != artifact.size_bytes {
            return Err("STAGING_FILE_SIZE_MISMATCH".into());
        }
        let canonical_file =
            fs::canonicalize(&path).map_err(|_| "STAGING_FILE_CANONICALIZE_FAILED")?;
        if !canonical_file.starts_with(&canonical_command) {
            return Err("STAGING_FILE_ESCAPED_COMMAND_ROOT".into());
        }
        if sha256_file(&path)? != artifact.sha256 {
            return Err("STAGING_FILE_HASH_MISMATCH".into());
        }
    }

    Ok(FilesystemVerification {
        schema_version: FILESYSTEM_VERIFIER_SCHEMA_VERSION,
        command_id: validated.command_id,
        task_id: validated.task_id,
        work_id: validated.work_id,
        task_revision: validated.task_revision,
        target_hash: validated.target_hash,
        manifest_hash: validated.manifest_hash,
        file_count: validated.file_count,
        total_bytes: validated.total_bytes,
        filesystem_verified: true,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    })
}
