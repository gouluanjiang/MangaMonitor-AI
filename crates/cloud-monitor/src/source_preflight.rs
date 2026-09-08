//! Offline expected-scope proof contract for A6.7 source preflight.
//!
//! This module binds complete source enumeration to an exact A6.6 request.
//! It downloads no image bytes and writes no staging files. A later source
//! runner must produce this evidence from real pinned-source reads before any
//! image download can be considered.

use crate::{
    local_executor::LocalExecutionPlan,
    monitor::hash,
    source_bridge_request::{self, SourceBridgeRequest},
    source_completion::{
        PaginationProof, SourceCompletionTranscript, JM_UPSTREAM_COMMIT, PICA_UPSTREAM_COMMIT,
        SOURCE_COMPLETION_SCHEMA_VERSION,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const SOURCE_PREFLIGHT_SCHEMA_VERSION: u64 = 1;
const FULL_SCOPE: &str = "FULL_SOURCE_WORK";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreflightChapter {
    pub chapter_id: String,
    pub chapter_order: u64,
    pub expected_images: u64,
    /// Pica requires complete image pagination. JM must leave this null.
    pub image_pagination: Option<PaginationProof>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourcePreflightEvidence {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub upstream_commit: String,
    pub completion_contract_version: u64,
    pub scope: String,
    pub source_enumeration_complete: bool,
    /// Pica requires complete whole-work chapter pagination. JM must leave null.
    pub chapter_pagination: Option<PaginationProof>,
    pub expected_chapter_count: u64,
    pub chapters: Vec<PreflightChapter>,
    /// A6.7 is enumeration-only. Any claim that image bytes were downloaded or
    /// staging was written crosses the phase boundary and is rejected.
    pub image_bytes_downloaded: bool,
    pub staging_written: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourcePreflightProof {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub upstream_commit: String,
    pub completion_contract_version: u64,
    pub scope: String,
    /// Hash of the complete canonical expected-scope evidence. Future execution
    /// can bind its observed transcript to this exact pre-download generation.
    pub preflight_hash: String,
    pub chapter_pagination: Option<PaginationProof>,
    pub expected_chapter_count: u64,
    pub chapters: Vec<PreflightChapter>,
    pub expected_content_units: u64,
    pub source_scope_verified: bool,
    pub image_download_authorized: bool,
    pub staging_write_authorized: bool,
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
    proof.successful_pages == (1..=proof.total_pages).collect::<Vec<_>>()
}

fn jm_id(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn pica_id(value: &str) -> bool {
    value.len() == 24 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_binding(
    plan: &LocalExecutionPlan,
    request: &SourceBridgeRequest,
    evidence: &SourcePreflightEvidence,
) -> Result<(), String> {
    source_bridge_request::validate(plan, request)?;
    if evidence.schema_version != SOURCE_PREFLIGHT_SCHEMA_VERSION {
        return Err("INVALID_SOURCE_PREFLIGHT_SCHEMA".into());
    }
    if evidence.command_id != request.command_id
        || evidence.task_id != request.task_id
        || evidence.work_id != request.work_id
        || evidence.task_revision != request.task_revision
        || evidence.target_hash != request.target_hash
        || evidence.source != request.source
        || evidence.source_work_id != request.source_work_id
        || evidence.upstream_commit != request.upstream_commit
        || evidence.completion_contract_version != request.completion_contract_version
        || evidence.scope != request.scope
        || evidence.scope != FULL_SCOPE
    {
        return Err("SOURCE_PREFLIGHT_BINDING_MISMATCH".into());
    }
    if evidence.completion_contract_version != SOURCE_COMPLETION_SCHEMA_VERSION {
        return Err("SOURCE_PREFLIGHT_COMPLETION_CONTRACT_MISMATCH".into());
    }
    if !evidence.source_enumeration_complete {
        return Err("SOURCE_PREFLIGHT_ENUMERATION_INCOMPLETE".into());
    }
    if evidence.image_bytes_downloaded || evidence.staging_written {
        return Err("SOURCE_PREFLIGHT_PHASE_BOUNDARY_VIOLATION".into());
    }
    Ok(())
}

fn validate_source_specific(evidence: &SourcePreflightEvidence) -> Result<(), String> {
    match evidence.source.as_str() {
        "jm" => {
            if evidence.upstream_commit != JM_UPSTREAM_COMMIT
                || !jm_id(&evidence.source_work_id)
                || evidence.chapter_pagination.is_some()
            {
                return Err("INVALID_JM_PREFLIGHT_EVIDENCE".into());
            }
            for chapter in &evidence.chapters {
                if !jm_id(&chapter.chapter_id) || chapter.image_pagination.is_some() {
                    return Err("INVALID_JM_PREFLIGHT_EVIDENCE".into());
                }
            }
        }
        "pica" => {
            if evidence.upstream_commit != PICA_UPSTREAM_COMMIT || !pica_id(&evidence.source_work_id)
            {
                return Err("INVALID_PICA_PREFLIGHT_EVIDENCE".into());
            }
            let chapter_pages = evidence
                .chapter_pagination
                .as_ref()
                .ok_or("PICA_PREFLIGHT_CHAPTER_PAGINATION_MISSING")?;
            if !pagination_complete(chapter_pages) {
                return Err("PICA_PREFLIGHT_CHAPTER_PAGINATION_INCOMPLETE".into());
            }
            for chapter in &evidence.chapters {
                if !pica_id(&chapter.chapter_id) {
                    return Err("INVALID_PICA_PREFLIGHT_EVIDENCE".into());
                }
                let image_pages = chapter
                    .image_pagination
                    .as_ref()
                    .ok_or("PICA_PREFLIGHT_IMAGE_PAGINATION_MISSING")?;
                if !pagination_complete(image_pages) {
                    return Err("PICA_PREFLIGHT_IMAGE_PAGINATION_INCOMPLETE".into());
                }
            }
        }
        _ => return Err("UNSUPPORTED_SOURCE_PREFLIGHT_SOURCE".into()),
    }
    Ok(())
}

fn validate_chapters(evidence: &SourcePreflightEvidence) -> Result<u64, String> {
    if evidence.expected_chapter_count == 0
        || evidence.expected_chapter_count
            != u64::try_from(evidence.chapters.len())
                .map_err(|_| "SOURCE_PREFLIGHT_CHAPTER_COUNT_OVERFLOW")?
    {
        return Err("SOURCE_PREFLIGHT_CHAPTER_COUNT_MISMATCH".into());
    }

    let mut ids = BTreeSet::new();
    let mut orders = BTreeSet::new();
    let mut previous_order = 0u64;
    let mut expected_content_units = 0u64;

    for chapter in &evidence.chapters {
        if chapter.chapter_order == 0
            || chapter.chapter_order <= previous_order
            || !ids.insert(chapter.chapter_id.clone())
            || !orders.insert(chapter.chapter_order)
        {
            return Err("SOURCE_PREFLIGHT_CHAPTERS_NOT_CANONICAL".into());
        }
        previous_order = chapter.chapter_order;
        if chapter.expected_images == 0 {
            return Err("SOURCE_PREFLIGHT_IMAGES_EMPTY".into());
        }
        expected_content_units = expected_content_units
            .checked_add(chapter.expected_images)
            .ok_or("SOURCE_PREFLIGHT_IMAGE_COUNT_OVERFLOW")?;
    }

    if expected_content_units == 0 {
        return Err("SOURCE_PREFLIGHT_IMAGES_EMPTY".into());
    }
    Ok(expected_content_units)
}

/// Validate a complete expected source scope before image-byte download.
pub fn validate(
    plan: &LocalExecutionPlan,
    request: &SourceBridgeRequest,
    evidence: &SourcePreflightEvidence,
) -> Result<SourcePreflightProof, String> {
    validate_binding(plan, request, evidence)?;
    validate_source_specific(evidence)?;
    let expected_content_units = validate_chapters(evidence)?;

    Ok(SourcePreflightProof {
        schema_version: SOURCE_PREFLIGHT_SCHEMA_VERSION,
        command_id: evidence.command_id.clone(),
        task_id: evidence.task_id.clone(),
        work_id: evidence.work_id.clone(),
        task_revision: evidence.task_revision,
        target_hash: evidence.target_hash.clone(),
        source: evidence.source.clone(),
        source_work_id: evidence.source_work_id.clone(),
        upstream_commit: evidence.upstream_commit.clone(),
        completion_contract_version: evidence.completion_contract_version,
        scope: evidence.scope.clone(),
        preflight_hash: hash(evidence),
        chapter_pagination: evidence.chapter_pagination.clone(),
        expected_chapter_count: evidence.expected_chapter_count,
        chapters: evidence.chapters.clone(),
        expected_content_units,
        source_scope_verified: true,
        image_download_authorized: false,
        staging_write_authorized: false,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    })
}

/// Recompute the accepted preflight proof and require a later A6.5 completion
/// transcript to describe the exact same source scope. Equal aggregate counts
/// are insufficient if any chapter ID/order/image count/pagination differs.
pub fn validate_completion_scope(
    plan: &LocalExecutionPlan,
    request: &SourceBridgeRequest,
    evidence: &SourcePreflightEvidence,
    transcript: &SourceCompletionTranscript,
) -> Result<SourcePreflightProof, String> {
    let proof = validate(plan, request, evidence)?;
    if transcript.schema_version != proof.completion_contract_version
        || transcript.command_id != proof.command_id
        || transcript.task_id != proof.task_id
        || transcript.work_id != proof.work_id
        || transcript.task_revision != proof.task_revision
        || transcript.target_hash != proof.target_hash
        || transcript.source != proof.source
        || transcript.source_work_id != proof.source_work_id
        || transcript.upstream_commit != proof.upstream_commit
        || transcript.scope != proof.scope
        || transcript.expected_chapter_count != proof.expected_chapter_count
        || transcript.chapter_pagination != proof.chapter_pagination
        || transcript.chapters.len() != proof.chapters.len()
    {
        return Err("SOURCE_COMPLETION_PREFLIGHT_BINDING_MISMATCH".into());
    }

    for (expected, observed) in proof.chapters.iter().zip(&transcript.chapters) {
        if observed.chapter_id != expected.chapter_id
            || observed.chapter_order != expected.chapter_order
            || observed.expected_images != expected.expected_images
            || observed.image_pagination != expected.image_pagination
        {
            return Err("SOURCE_COMPLETION_PREFLIGHT_SCOPE_MISMATCH".into());
        }
    }
    Ok(proof)
}
