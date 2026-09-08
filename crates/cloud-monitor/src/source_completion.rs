//! Offline source-specific completion proof normalization for A6.5.
//!
//! This module does not call JM/Pica or download anything. It defines the proof
//! a future source bridge must produce before A6.3/A6.4 can trust staged output.

use crate::{
    local_executor::LocalExecutionPlan,
    staging_manifest::{self, StagedArtifact, StagingManifest},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const SOURCE_COMPLETION_SCHEMA_VERSION: u64 = 1;
pub const JM_UPSTREAM_COMMIT: &str = "f0cdd724af6892002f2fb7be883b88832cebe7e9";
pub const PICA_UPSTREAM_COMMIT: &str = "77c8b62ede42b3afc074506d092313816af8092d";
const FULL_SCOPE: &str = "FULL_SOURCE_WORK";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaginationProof {
    pub total_pages: u64,
    pub successful_pages: Vec<u64>,
    #[serde(default)]
    pub failed_pages: Vec<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChapterCompletion {
    pub chapter_id: String,
    pub chapter_order: u64,
    pub scheduled: bool,
    pub joined: bool,
    pub terminal_state: String,
    pub expected_images: u64,
    pub completed_images: u64,
    pub failed_images: u64,
    /// Pica requires a complete image-pagination proof. JM must leave this null.
    pub image_pagination: Option<PaginationProof>,
    /// Content artifacts produced for this chapter. A6.5 deliberately models
    /// page/image artifacts only; auxiliary metadata/cover output is not yet
    /// part of the trusted bridge contract.
    pub artifact_paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceCompletionTranscript {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub upstream_commit: String,
    pub scope: String,
    pub source_enumeration_complete: bool,
    /// Required for Pica because its upstream chapter aggregation can otherwise
    /// silently omit a later failed chapter page. JM must leave this null.
    pub chapter_pagination: Option<PaginationProof>,
    pub expected_chapter_count: u64,
    pub all_scheduled_downloads_joined: bool,
    pub chapters: Vec<ChapterCompletion>,
    pub artifacts: Vec<StagedArtifact>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceCompletionProof {
    pub schema_version: u64,
    pub source: String,
    pub upstream_commit: String,
    pub source_contract_verified: bool,
    pub execution_supported: bool,
    pub manifest: StagingManifest,
    pub inventory_mutation_authorized: bool,
    pub task_completion_authorized: bool,
    pub promotion_authorized: bool,
    pub replacement_authorized: bool,
    pub physical_delete_authorized: bool,
}

fn pagination_complete(proof: &PaginationProof) -> bool {
    if proof.total_pages == 0 || !proof.failed_pages.is_empty() {
        return false;
    }
    let expected: Vec<u64> = (1..=proof.total_pages).collect();
    proof.successful_pages == expected
}

fn pica_id(value: &str) -> bool {
    value.len() == 24 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn jm_id(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
}

fn validate_binding(plan: &LocalExecutionPlan, t: &SourceCompletionTranscript) -> Result<(), String> {
    if t.schema_version != SOURCE_COMPLETION_SCHEMA_VERSION {
        return Err("INVALID_SOURCE_COMPLETION_SCHEMA".into());
    }
    if t.command_id != plan.command_id
        || t.task_id != plan.task_id
        || t.work_id != plan.work_id
        || t.task_revision != plan.task_revision
        || t.target_hash != plan.target_hash
        || t.source_work_id != plan.source_work_id
    {
        return Err("SOURCE_COMPLETION_BINDING_MISMATCH".into());
    }
    if t.scope != FULL_SCOPE {
        return Err("SOURCE_COMPLETION_SCOPE_NOT_FULL_WORK".into());
    }
    let expected_backend = match t.source.as_str() {
        "jm" => "JM",
        "pica" => "PICA",
        _ => return Err("UNSUPPORTED_SOURCE_COMPLETION_SOURCE".into()),
    };
    if plan.backend != expected_backend {
        return Err("SOURCE_COMPLETION_BACKEND_MISMATCH".into());
    }
    Ok(())
}

fn validate_source_specific(t: &SourceCompletionTranscript) -> Result<(), String> {
    match t.source.as_str() {
        "jm" => {
            if t.upstream_commit != JM_UPSTREAM_COMMIT {
                return Err("JM_UPSTREAM_COMMIT_MISMATCH".into());
            }
            if !jm_id(&t.source_work_id) || t.chapter_pagination.is_some() {
                return Err("INVALID_JM_COMPLETION_EVIDENCE".into());
            }
            for chapter in &t.chapters {
                if !jm_id(&chapter.chapter_id) || chapter.image_pagination.is_some() {
                    return Err("INVALID_JM_COMPLETION_EVIDENCE".into());
                }
            }
        }
        "pica" => {
            if t.upstream_commit != PICA_UPSTREAM_COMMIT {
                return Err("PICA_UPSTREAM_COMMIT_MISMATCH".into());
            }
            if !pica_id(&t.source_work_id) {
                return Err("INVALID_PICA_COMPLETION_EVIDENCE".into());
            }
            let chapter_pages = t
                .chapter_pagination
                .as_ref()
                .ok_or("PICA_CHAPTER_PAGINATION_MISSING")?;
            if !pagination_complete(chapter_pages) {
                return Err("PICA_CHAPTER_PAGINATION_INCOMPLETE".into());
            }
            for chapter in &t.chapters {
                if !pica_id(&chapter.chapter_id) {
                    return Err("INVALID_PICA_COMPLETION_EVIDENCE".into());
                }
                let image_pages = chapter
                    .image_pagination
                    .as_ref()
                    .ok_or("PICA_IMAGE_PAGINATION_MISSING")?;
                if !pagination_complete(image_pages) {
                    return Err("PICA_IMAGE_PAGINATION_INCOMPLETE".into());
                }
            }
        }
        _ => return Err("UNSUPPORTED_SOURCE_COMPLETION_SOURCE".into()),
    }
    Ok(())
}

fn validate_chapters(t: &SourceCompletionTranscript) -> Result<u64, String> {
    if !t.source_enumeration_complete {
        return Err("SOURCE_COMPLETION_ENUMERATION_INCOMPLETE".into());
    }
    if !t.all_scheduled_downloads_joined {
        return Err("SOURCE_COMPLETION_TASKS_NOT_JOINED".into());
    }
    if t.expected_chapter_count == 0
        || t.expected_chapter_count
            != u64::try_from(t.chapters.len())
                .map_err(|_| "SOURCE_COMPLETION_CHAPTER_COUNT_OVERFLOW")?
    {
        return Err("SOURCE_COMPLETION_CHAPTER_COUNT_MISMATCH".into());
    }

    let mut chapter_ids = BTreeSet::new();
    let mut chapter_orders = BTreeSet::new();
    let mut previous_order = 0u64;
    let mut content_paths = BTreeSet::new();
    let mut total_expected_images = 0u64;

    for chapter in &t.chapters {
        if chapter.chapter_order == 0
            || chapter.chapter_order <= previous_order
            || !chapter_ids.insert(chapter.chapter_id.clone())
            || !chapter_orders.insert(chapter.chapter_order)
        {
            return Err("SOURCE_COMPLETION_CHAPTERS_NOT_CANONICAL".into());
        }
        previous_order = chapter.chapter_order;
        if !chapter.scheduled {
            return Err("SOURCE_COMPLETION_CHAPTER_NOT_SCHEDULED".into());
        }
        if !chapter.joined {
            return Err("SOURCE_COMPLETION_CHAPTER_NOT_JOINED".into());
        }
        if chapter.terminal_state != "COMPLETED" {
            return Err("SOURCE_COMPLETION_CHAPTER_NOT_COMPLETED".into());
        }
        if chapter.expected_images == 0
            || chapter.completed_images != chapter.expected_images
            || chapter.failed_images != 0
        {
            return Err("SOURCE_COMPLETION_IMAGES_INCOMPLETE".into());
        }
        if chapter.artifact_paths.is_empty() {
            return Err("SOURCE_COMPLETION_CHAPTER_ARTIFACTS_EMPTY".into());
        }
        if u64::try_from(chapter.artifact_paths.len())
            .map_err(|_| "SOURCE_COMPLETION_ARTIFACT_COUNT_OVERFLOW")?
            != chapter.expected_images
        {
            return Err("SOURCE_COMPLETION_IMAGE_ARTIFACT_COUNT_MISMATCH".into());
        }
        let mut previous_path: Option<&str> = None;
        for path in &chapter.artifact_paths {
            if previous_path.is_some_and(|previous| previous >= path.as_str()) {
                return Err("SOURCE_COMPLETION_ARTIFACT_PATHS_NOT_CANONICAL".into());
            }
            previous_path = Some(path);
            if !content_paths.insert(path.clone()) {
                return Err("SOURCE_COMPLETION_ARTIFACT_ASSIGNED_TWICE".into());
            }
        }
        total_expected_images = total_expected_images
            .checked_add(chapter.expected_images)
            .ok_or("SOURCE_COMPLETION_IMAGE_COUNT_OVERFLOW")?;
    }

    let manifest_paths: BTreeSet<String> = t
        .artifacts
        .iter()
        .map(|artifact| artifact.relative_path.clone())
        .collect();
    if manifest_paths.len() != t.artifacts.len() || content_paths != manifest_paths {
        return Err("SOURCE_COMPLETION_ARTIFACT_SET_MISMATCH".into());
    }
    Ok(total_expected_images)
}

/// Normalize a complete offline source transcript into the accepted A6.3 manifest.
/// A spawn/create return is intentionally insufficient: every chapter must be
/// scheduled, joined, terminal `COMPLETED`, and have complete image accounting.
pub fn normalize(
    plan: &LocalExecutionPlan,
    transcript: &SourceCompletionTranscript,
) -> Result<SourceCompletionProof, String> {
    validate_binding(plan, transcript)?;
    validate_source_specific(transcript)?;
    let expected_images = validate_chapters(transcript)?;

    let manifest = StagingManifest {
        schema_version: staging_manifest::STAGING_MANIFEST_SCHEMA_VERSION,
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
        expected_content_units: expected_images,
        completed_content_units: expected_images,
        failed_content_units: 0,
        artifacts: transcript.artifacts.clone(),
    };
    staging_manifest::validate(plan, &manifest)?;

    Ok(SourceCompletionProof {
        schema_version: SOURCE_COMPLETION_SCHEMA_VERSION,
        source: transcript.source.clone(),
        upstream_commit: transcript.upstream_commit.clone(),
        source_contract_verified: true,
        execution_supported: false,
        manifest,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    })
}
