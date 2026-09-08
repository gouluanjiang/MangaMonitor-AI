//! A6.12 isolated command-staging execution kernel.
//!
//! This is the first A6 layer that may write media bytes, but it still owns no
//! source network client. Callers must provide an in-process fetch/process
//! function and an immediate A6.10 reauthorization callback. The kernel writes
//! only into a fresh `commands/<command_id>` tree, never overwrites existing
//! files, never deletes partial output, and returns success only after the A6.5
//! completion transcript plus A6.3/A6.4 verification chain succeeds.

use crate::{
    filesystem_verifier::{self, FilesystemVerification},
    image_download_authorization::ImageDownloadAuthorization,
    local_executor::LocalExecutionPlan,
    source_bridge_request,
    source_completion::{
        self, ChapterCompletion, SourceCompletionProof, SourceCompletionTranscript,
        SOURCE_COMPLETION_SCHEMA_VERSION,
    },
    source_media_descriptors::{self, MediaDescriptor, SourceMediaDescriptorSet},
    source_preflight::{SourcePreflightEvidence, SourcePreflightProof},
    staging_manifest::StagedArtifact,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, Metadata, OpenOptions},
    future::Future,
    io::Write,
    path::{Path, PathBuf},
};

pub const ISOLATED_STAGING_EXECUTION_SCHEMA_VERSION: u64 = 1;

/// Exact immutable trust inputs for one A6.12 command-staging execution.
///
/// Keeping the complete chain in one context makes it harder for a caller to
/// accidentally mix artifacts from different task/source generations.
pub struct IsolatedStagingExecutionContext<'a> {
    pub staging_root: &'a Path,
    pub plan: &'a LocalExecutionPlan,
    pub authorization: &'a ImageDownloadAuthorization,
    pub evidence: &'a SourcePreflightEvidence,
    pub preflight: &'a SourcePreflightProof,
    pub descriptors: &'a SourceMediaDescriptorSet,
}

/// Final, already-processed bytes for one exact A6.11 descriptor.
///
/// The fetch/process implementation must echo the exact descriptor binding and
/// declare the transform it actually applied. A6.12 never accepts opaque bytes
/// without this binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessedMedia {
    pub source_media_id: String,
    pub request_url: String,
    pub source_format: String,
    pub applied_transform: String,
    pub applied_transform_parameter: u64,
    pub bytes: Vec<u8>,
}

/// Serialize-only diagnostic result. This is evidence, not a reusable authority.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct IsolatedStagingExecutionResult {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub preflight_hash: String,
    pub staging_execution_completed: bool,
    pub source_completion: SourceCompletionProof,
    pub filesystem_verification: FilesystemVerification,
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

fn join_portable(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(root.to_path_buf(), |path, component| path.join(component))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn exact_processed_binding(
    descriptor: &MediaDescriptor,
    processed: &ProcessedMedia,
) -> Result<(), String> {
    if processed.source_media_id != descriptor.source_media_id
        || processed.request_url != descriptor.request_url
        || processed.source_format != descriptor.source_format
        || processed.applied_transform != descriptor.transform
        || processed.applied_transform_parameter != descriptor.transform_parameter
    {
        return Err("PROCESSED_MEDIA_DESCRIPTOR_BINDING_MISMATCH".into());
    }
    if processed.bytes.is_empty() {
        return Err("PROCESSED_MEDIA_INVALID_IMAGE_BYTES".into());
    }
    crate::media_validation::validate(&descriptor.source_format, &processed.bytes)
        .map_err(|_| "PROCESSED_MEDIA_INVALID_IMAGE_BYTES")?;
    Ok(())
}

fn exact_authorization_generation(
    initial: &ImageDownloadAuthorization,
    current: ImageDownloadAuthorization,
) -> Result<(), String> {
    if &current != initial {
        return Err("MEDIA_TRANSFER_AUTHORIZATION_GENERATION_CHANGED".into());
    }
    Ok(())
}

fn validate_plan_chain(
    plan: &LocalExecutionPlan,
    authorization: &ImageDownloadAuthorization,
    evidence: &SourcePreflightEvidence,
) -> Result<(), String> {
    let request = source_bridge_request::build(plan)?;
    if request.command_id != authorization.command_id
        || request.task_id != authorization.task_id
        || request.work_id != authorization.work_id
        || request.task_revision != authorization.task_revision
        || request.target_hash != authorization.target_hash
        || request.source != authorization.source
        || request.source_work_id != authorization.source_work_id
        || request.staging_subdir != authorization.staging_subdir
        || request.command_id != evidence.command_id
        || request.task_id != evidence.task_id
        || request.work_id != evidence.work_id
        || request.task_revision != evidence.task_revision
        || request.target_hash != evidence.target_hash
        || request.source != evidence.source
        || request.source_work_id != evidence.source_work_id
        || request.upstream_commit != evidence.upstream_commit
        || request.scope != evidence.scope
        || request.completion_contract_version != evidence.completion_contract_version
    {
        return Err("ISOLATED_STAGING_PLAN_CHAIN_MISMATCH".into());
    }
    Ok(())
}

fn prepare_command_root(staging_root: &Path, plan: &LocalExecutionPlan) -> Result<PathBuf, String> {
    checked_directory(staging_root, "STAGING_ROOT_MISSING")?;
    let commands_root = staging_root.join("commands");
    checked_directory(&commands_root, "STAGING_COMMANDS_DIRECTORY_MISSING")?;

    let command_root = commands_root.join(&plan.command_id);
    match fs::create_dir(&command_root) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err("STAGING_COMMAND_DIRECTORY_ALREADY_EXISTS".into())
        }
        Err(_) => return Err("STAGING_COMMAND_DIRECTORY_CREATE_FAILED".into()),
    }
    let chapters_root = command_root.join("chapters");
    fs::create_dir(&chapters_root).map_err(|_| "STAGING_CHAPTERS_DIRECTORY_CREATE_FAILED")?;
    Ok(command_root)
}

fn write_new_file(
    command_root: &Path,
    relative_path: &str,
    bytes: &[u8],
) -> Result<StagedArtifact, String> {
    let path = join_portable(command_root, relative_path);
    let parent = path.parent().ok_or("STAGING_ARTIFACT_PARENT_MISSING")?;
    checked_directory(parent, "STAGING_ARTIFACT_PARENT_MISSING")?;

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                "STAGING_ARTIFACT_ALREADY_EXISTS".to_string()
            } else {
                "STAGING_ARTIFACT_CREATE_FAILED".to_string()
            }
        })?;
    file.write_all(bytes)
        .map_err(|_| "STAGING_ARTIFACT_WRITE_FAILED")?;
    file.sync_all()
        .map_err(|_| "STAGING_ARTIFACT_SYNC_FAILED")?;

    let metadata = fs::symlink_metadata(&path).map_err(|_| "STAGING_ARTIFACT_METADATA_FAILED")?;
    if link_like(&metadata) || !metadata.is_file() {
        return Err("STAGING_ARTIFACT_NOT_REGULAR_FILE".into());
    }
    let size_bytes = u64::try_from(bytes.len()).map_err(|_| "STAGING_ARTIFACT_SIZE_OVERFLOW")?;
    if metadata.len() != size_bytes {
        return Err("STAGING_ARTIFACT_SIZE_MISMATCH".into());
    }

    let canonical_command =
        fs::canonicalize(command_root).map_err(|_| "STAGING_COMMAND_CANONICALIZE_FAILED")?;
    let canonical_file =
        fs::canonicalize(&path).map_err(|_| "STAGING_ARTIFACT_CANONICALIZE_FAILED")?;
    if !canonical_file.starts_with(&canonical_command) {
        return Err("STAGING_ARTIFACT_ESCAPED_COMMAND_ROOT".into());
    }

    Ok(StagedArtifact {
        relative_path: relative_path.into(),
        size_bytes,
        sha256: sha256_bytes(bytes),
    })
}

/// Execute one already-validated A6.11 work set into a fresh command-owned
/// staging tree using caller-provided, in-process media fetching/processing.
///
/// The reauthorization callback is invoked before any filesystem mutation,
/// before every source fetch, before every file write, and after the complete
/// filesystem verification. Any generation change fails closed. Partial files
/// are deliberately never deleted by this layer.
pub async fn execute_with_fetcher<Fetch, FetchFuture, Reauthorize>(
    context: IsolatedStagingExecutionContext<'_>,
    mut fetch: Fetch,
    mut reauthorize: Reauthorize,
) -> Result<IsolatedStagingExecutionResult, String>
where
    Fetch: FnMut(MediaDescriptor) -> FetchFuture,
    FetchFuture: Future<Output = Result<ProcessedMedia, String>>,
    Reauthorize: FnMut() -> Result<ImageDownloadAuthorization, String>,
{
    let IsolatedStagingExecutionContext {
        staging_root,
        plan,
        authorization,
        evidence,
        preflight,
        descriptors,
    } = context;

    validate_plan_chain(plan, authorization, evidence)?;
    source_media_descriptors::validate(authorization, evidence, preflight, descriptors)?;
    exact_authorization_generation(authorization, reauthorize()?)?;

    let command_root = prepare_command_root(staging_root, plan)?;
    let chapters_root = command_root.join("chapters");
    let mut artifacts = Vec::with_capacity(
        usize::try_from(descriptors.expected_content_units)
            .map_err(|_| "STAGING_ARTIFACT_COUNT_OVERFLOW")?,
    );
    let mut chapter_completions = Vec::with_capacity(descriptors.chapters.len());

    for (chapter, expected) in descriptors.chapters.iter().zip(&preflight.chapters) {
        let chapter_dir_name = format!("{:06}-{}", chapter.chapter_order, chapter.chapter_id);
        let chapter_dir = chapters_root.join(&chapter_dir_name);
        fs::create_dir(&chapter_dir).map_err(|_| "STAGING_CHAPTER_DIRECTORY_CREATE_FAILED")?;

        let mut artifact_paths = Vec::with_capacity(chapter.media.len());
        for descriptor in &chapter.media {
            exact_authorization_generation(authorization, reauthorize()?)?;
            let processed = fetch(descriptor.clone()).await?;
            exact_processed_binding(descriptor, &processed)?;
            exact_authorization_generation(authorization, reauthorize()?)?;

            let artifact = write_new_file(
                &command_root,
                &descriptor.relative_path,
                &processed.bytes,
            )?;
            artifact_paths.push(artifact.relative_path.clone());
            artifacts.push(artifact);
        }

        chapter_completions.push(ChapterCompletion {
            chapter_id: chapter.chapter_id.clone(),
            chapter_order: chapter.chapter_order,
            scheduled: true,
            joined: true,
            terminal_state: "COMPLETED".into(),
            expected_images: expected.expected_images,
            completed_images: expected.expected_images,
            failed_images: 0,
            image_pagination: expected.image_pagination.clone(),
            artifact_paths,
        });
    }

    artifacts.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let transcript = SourceCompletionTranscript {
        schema_version: SOURCE_COMPLETION_SCHEMA_VERSION,
        command_id: authorization.command_id.clone(),
        task_id: authorization.task_id.clone(),
        work_id: authorization.work_id.clone(),
        task_revision: authorization.task_revision,
        target_hash: authorization.target_hash.clone(),
        source: authorization.source.clone(),
        source_work_id: authorization.source_work_id.clone(),
        upstream_commit: evidence.upstream_commit.clone(),
        scope: evidence.scope.clone(),
        source_enumeration_complete: true,
        chapter_pagination: preflight.chapter_pagination.clone(),
        expected_chapter_count: authorization.expected_chapter_count,
        all_scheduled_downloads_joined: true,
        chapters: chapter_completions,
        artifacts,
    };

    exact_authorization_generation(authorization, reauthorize()?)?;
    let source_completion = source_completion::normalize(plan, &transcript)?;
    let filesystem_verification =
        filesystem_verifier::verify(staging_root, plan, &source_completion.manifest)?;
    exact_authorization_generation(authorization, reauthorize()?)?;

    Ok(IsolatedStagingExecutionResult {
        schema_version: ISOLATED_STAGING_EXECUTION_SCHEMA_VERSION,
        command_id: authorization.command_id.clone(),
        task_id: authorization.task_id.clone(),
        work_id: authorization.work_id.clone(),
        task_revision: authorization.task_revision,
        target_hash: authorization.target_hash.clone(),
        source: authorization.source.clone(),
        source_work_id: authorization.source_work_id.clone(),
        preflight_hash: authorization.preflight_hash.clone(),
        staging_execution_completed: true,
        source_completion,
        filesystem_verification,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    })
}
