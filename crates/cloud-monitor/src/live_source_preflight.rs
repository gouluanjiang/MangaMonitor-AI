//! A6.9 live, metadata-only source preflight.
//!
//! This is the first A6 layer allowed to perform source network reads. It
//! deliberately exposes no image-byte download or staging-write path. Current
//! authorization is checked immediately before enumeration and again after the
//! source metadata reads complete. A changed task/gate generation invalidates
//! the entire enumeration result.

use crate::{
    assistant_task_gate::GateLedger,
    executor_handoff::ExecutorCommand,
    local_executor::LocalExecutionPlan,
    monitor::State,
    source_bridge_request::SourceBridgeRequest,
    source_completion::PaginationProof,
    source_preflight::{self, PreflightChapter, SourcePreflightEvidence, SourcePreflightProof},
    source_preflight_authorization::{self, SourcePreflightAuthorization},
};
use serde::Serialize;

pub const LIVE_SOURCE_PREFLIGHT_SCHEMA_VERSION: u64 = 1;
pub const PICA_PREFLIGHT_MAX_PAGES: u64 = 1_000;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct LiveSourcePreflightResult {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub authorization_stable: bool,
    pub pre_authorization_state_hash: String,
    pub post_authorization_state_hash: String,
    pub pre_authorization_gate_hash: String,
    pub post_authorization_gate_hash: String,
    pub source_metadata_read_completed: bool,
    pub evidence: SourcePreflightEvidence,
    pub proof: SourcePreflightProof,
    pub image_download_authorized: bool,
    pub staging_write_authorized: bool,
    pub inventory_mutation_authorized: bool,
    pub task_completion_authorized: bool,
    pub promotion_authorized: bool,
    pub replacement_authorized: bool,
    pub physical_delete_authorized: bool,
}

fn authorization_unchanged(
    before: &SourcePreflightAuthorization,
    after: &SourcePreflightAuthorization,
) -> Result<(), String> {
    if before.command_id != after.command_id
        || before.task_id != after.task_id
        || before.work_id != after.work_id
        || before.task_revision != after.task_revision
        || before.target_hash != after.target_hash
        || before.source != after.source
        || before.source_work_id != after.source_work_id
        || before.current_state_binding_hash != after.current_state_binding_hash
        || before.gate_ledger_hash != after.gate_ledger_hash
        || !before.source_metadata_read_authorized
        || !after.source_metadata_read_authorized
        || before.reusable_permit
        || after.reusable_permit
    {
        return Err("SOURCE_PREFLIGHT_AUTHORIZATION_CHANGED_DURING_ENUMERATION".into());
    }
    Ok(())
}

fn base_evidence(request: &SourceBridgeRequest) -> SourcePreflightEvidence {
    SourcePreflightEvidence {
        schema_version: source_preflight::SOURCE_PREFLIGHT_SCHEMA_VERSION,
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
        chapter_pagination: None,
        expected_chapter_count: 0,
        chapters: Vec::new(),
        image_bytes_downloaded: false,
        staging_written: false,
    }
}

async fn enumerate_jm(
    request: &SourceBridgeRequest,
    download: bool,
) -> Result<SourcePreflightEvidence, String> {
    let mut client = if download {
        jm_adapter::JmClient::new_for_download(jm_adapter::DEFAULT_DOMAIN)?
    } else {
        jm_adapter::JmClient::new(jm_adapter::DEFAULT_DOMAIN)?
    };
    let chapters = client.preflight_chapters(&request.source_work_id).await?;
    let expected_chapter_count =
        u64::try_from(chapters.len()).map_err(|_| "JM_PREFLIGHT_CHAPTER_COUNT_OVERFLOW")?;
    if expected_chapter_count == 0 {
        return Err("JM_PREFLIGHT_CHAPTERS_EMPTY".into());
    }

    let mut result = Vec::with_capacity(chapters.len());
    for chapter in chapters {
        let expected_images = client
            .preflight_chapter_image_count(&chapter.chapter_id)
            .await?;
        result.push(PreflightChapter {
            chapter_id: chapter.chapter_id,
            chapter_order: chapter.chapter_order,
            expected_images,
            image_pagination: None,
        });
    }

    let mut evidence = base_evidence(request);
    evidence.expected_chapter_count = expected_chapter_count;
    evidence.chapters = result;
    Ok(evidence)
}

async fn enumerate_pica<Guard>(
    request: &SourceBridgeRequest,
    token: &str,
    download: bool,
    mut before_request: Guard,
) -> Result<SourcePreflightEvidence, String>
where
    Guard: FnMut() -> Result<(), String>,
{
    if token.trim().is_empty() {
        return Err("PICA_PREFLIGHT_TOKEN_REQUIRED".into());
    }
    let mut client = if download {
        pica_adapter::PicaClient::new_for_download(token.to_owned())?
    } else {
        pica_adapter::PicaClient::new(token.to_owned())?
    };
    let chapters = client
        .preflight_chapters_with_guard(
            &request.source_work_id,
            PICA_PREFLIGHT_MAX_PAGES,
            &mut before_request,
        )
        .await?;
    let expected_chapter_count = u64::try_from(chapters.chapters.len())
        .map_err(|_| "PICA_PREFLIGHT_CHAPTER_COUNT_OVERFLOW")?;
    if expected_chapter_count == 0 {
        return Err("PICA_PREFLIGHT_CHAPTERS_EMPTY".into());
    }

    let chapter_pagination = PaginationProof {
        total_pages: chapters.total_pages,
        successful_pages: chapters.successful_pages,
        failed_pages: Vec::new(),
    };
    let mut result = Vec::with_capacity(chapters.chapters.len());
    for chapter in chapters.chapters {
        let images = client
            .preflight_chapter_images_with_guard(
                &request.source_work_id,
                chapter.chapter_order,
                PICA_PREFLIGHT_MAX_PAGES,
                &mut before_request,
            )
            .await?;
        result.push(PreflightChapter {
            chapter_id: chapter.chapter_id,
            chapter_order: chapter.chapter_order,
            expected_images: images.expected_images,
            image_pagination: Some(PaginationProof {
                total_pages: images.total_pages,
                successful_pages: images.successful_pages,
                failed_pages: Vec::new(),
            }),
        });
    }

    let mut evidence = base_evidence(request);
    evidence.chapter_pagination = Some(chapter_pagination);
    evidence.expected_chapter_count = expected_chapter_count;
    evidence.chapters = result;
    Ok(evidence)
}

async fn enumerate_source(
    request: &SourceBridgeRequest,
    pica_token: Option<&str>,
    download: bool,
) -> Result<SourcePreflightEvidence, String> {
    match request.source.as_str() {
        "jm" => enumerate_jm(request, download).await,
        "pica" => {
            enumerate_pica(
                request,
                pica_token.ok_or("PICA_PREFLIGHT_TOKEN_REQUIRED")?,
                download,
                || Ok(()),
            )
            .await
        }
        _ => Err("UNSUPPORTED_LIVE_SOURCE_PREFLIGHT_SOURCE".into()),
    }
}

async fn run_with_enumerator<Enumerate, EnumerateFuture, Reload>(
    state: &State,
    ledger: &GateLedger,
    command: &ExecutorCommand,
    plan: &LocalExecutionPlan,
    request: &SourceBridgeRequest,
    enumerate: Enumerate,
    reload: Reload,
) -> Result<LiveSourcePreflightResult, String>
where
    Enumerate: FnOnce() -> EnumerateFuture,
    EnumerateFuture: std::future::Future<Output = Result<SourcePreflightEvidence, String>>,
    Reload: FnOnce() -> Result<(State, GateLedger), String>,
{
    let before = source_preflight_authorization::authorize(state, ledger, command, plan, request)?;
    let evidence = enumerate().await?;

    let (post_state, post_ledger) = reload()?;
    let after = source_preflight_authorization::authorize(
        &post_state,
        &post_ledger,
        command,
        plan,
        request,
    )?;
    authorization_unchanged(&before, &after)?;
    preflight_result(request, plan, evidence, before, after)
}

fn preflight_result(
    request: &SourceBridgeRequest,
    plan: &LocalExecutionPlan,
    evidence: SourcePreflightEvidence,
    before: SourcePreflightAuthorization,
    after: SourcePreflightAuthorization,
) -> Result<LiveSourcePreflightResult, String> {
    let proof = source_preflight::validate(plan, request, &evidence)?;
    Ok(LiveSourcePreflightResult {
        schema_version: LIVE_SOURCE_PREFLIGHT_SCHEMA_VERSION,
        command_id: request.command_id.clone(),
        task_id: request.task_id.clone(),
        work_id: request.work_id.clone(),
        task_revision: request.task_revision,
        target_hash: request.target_hash.clone(),
        source: request.source.clone(),
        source_work_id: request.source_work_id.clone(),
        authorization_stable: true,
        pre_authorization_state_hash: before.current_state_binding_hash,
        post_authorization_state_hash: after.current_state_binding_hash,
        pre_authorization_gate_hash: before.gate_ledger_hash,
        post_authorization_gate_hash: after.gate_ledger_hash,
        source_metadata_read_completed: true,
        evidence,
        proof,
        image_download_authorized: false,
        staging_write_authorized: false,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    })
}

/// Perform one live, metadata-only source enumeration between two immediate
/// current-generation authorization checks.
///
/// Pica credentials are accepted only as an in-memory reference and are never
/// placed in request/evidence/proof/result structures. Callers should source the
/// token out-of-band (the A6.9 CLI uses an environment variable).
pub async fn run_live<Reload>(
    state: &State,
    ledger: &GateLedger,
    command: &ExecutorCommand,
    plan: &LocalExecutionPlan,
    request: &SourceBridgeRequest,
    pica_token: Option<&str>,
    reload: Reload,
) -> Result<LiveSourcePreflightResult, String>
where
    Reload: FnOnce() -> Result<(State, GateLedger), String>,
{
    run_with_enumerator(
        state,
        ledger,
        command,
        plan,
        request,
        || enumerate_source(request, pica_token, false),
        reload,
    )
    .await
}

/// Explicit desktop downloads use immediate metadata pacing. Pica pagination
/// also reloads the caller's account/task scope before every metadata request.
pub(crate) async fn run_for_download<Reload>(
    state: &State,
    ledger: &GateLedger,
    command: &ExecutorCommand,
    plan: &LocalExecutionPlan,
    request: &SourceBridgeRequest,
    pica_token: Option<&str>,
    mut reload: Reload,
) -> Result<LiveSourcePreflightResult, String>
where
    Reload: FnMut() -> Result<(State, GateLedger), String>,
{
    if request.source != "pica" {
        return run_with_enumerator(
            state,
            ledger,
            command,
            plan,
            request,
            || enumerate_source(request, pica_token, true),
            reload,
        )
        .await;
    }
    let before = source_preflight_authorization::authorize(state, ledger, command, plan, request)?;
    let mut check_current = || {
        let (state, ledger) = reload()?;
        let current =
            source_preflight_authorization::authorize(&state, &ledger, command, plan, request)?;
        authorization_unchanged(&before, &current)
    };
    let evidence = enumerate_pica(
        request,
        pica_token.ok_or("PICA_PREFLIGHT_TOKEN_REQUIRED")?,
        true,
        &mut check_current,
    )
    .await?;
    let (state, ledger) = reload()?;
    let after = source_preflight_authorization::authorize(&state, &ledger, command, plan, request)?;
    authorization_unchanged(&before, &after)?;
    preflight_result(request, plan, evidence, before, after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assistant_task_gate::{target_hash, GateRecord},
        monitor::{hash, Decisions, Scan, Target, Task},
        source_bridge_request,
    };
    use serde_json::{json, Value};
    use state_model::Version;
    use std::collections::BTreeMap;

    fn setup() -> (
        State,
        GateLedger,
        ExecutorCommand,
        LocalExecutionPlan,
        SourceBridgeRequest,
        SourcePreflightEvidence,
    ) {
        setup_source("jm", "123456")
    }

    fn setup_source(
        source: &str,
        id: &str,
    ) -> (
        State,
        GateLedger,
        ExecutorCommand,
        LocalExecutionPlan,
        SourceBridgeRequest,
        SourcePreflightEvidence,
    ) {
        let task = Task {
            task_id: "TASK_A6_9".into(),
            work_id: "WORK_A6_9".into(),
            first_seen: "fixed".into(),
            task_revision: 1,
            target: Target {
                source_key: format!("{source}:{id}"),
                author: "Writer".into(),
                title: "A6.9".into(),
                version: Version::default(),
                coverage: Value::Null,
            },
            action: "download".into(),
            status: "pending".into(),
            old_local_item_ids: Vec::new(),
            binding_authority_hash: String::new(),
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
        let ledger = GateLedger {
            schema_version: 1,
            records: vec![GateRecord {
                task_id: task.task_id.clone(),
                task_revision: task.task_revision,
                target_hash: target_hash(&task),
                assistant_recommended: true,
                user_approved: true,
            }],
        };
        let target_hash = target_hash(&task);
        let digest = hash(&(
            task.task_id.as_str(),
            task.task_revision,
            target_hash.as_str(),
        ));
        let command = ExecutorCommand {
            schema_version: 1,
            command_id: format!("EXEC_{}", &digest[..20]),
            task_id: task.task_id.clone(),
            work_id: task.work_id.clone(),
            task_revision: task.task_revision,
            target_hash,
            source: source.into(),
            source_work_id: id.into(),
            action: "download".into(),
            intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
            target: task.target.clone(),
        };
        let plan = crate::local_executor::plan(&command).unwrap();
        let request = source_bridge_request::build(&plan).unwrap();
        let mut evidence = base_evidence(&request);
        evidence.expected_chapter_count = 1;
        evidence.chapters = vec![PreflightChapter {
            chapter_id: id.into(),
            chapter_order: 1,
            expected_images: 3,
            image_pagination: (source == "pica").then(|| PaginationProof {
                total_pages: 1,
                successful_pages: vec![1],
                failed_pages: vec![],
            }),
        }];
        if source == "pica" {
            evidence.chapter_pagination = Some(PaginationProof {
                total_pages: 1,
                successful_pages: vec![1],
                failed_pages: vec![],
            });
        }
        (state, ledger, command, plan, request, evidence)
    }

    #[tokio::test]
    async fn pica_download_reloads_account_scope_before_its_first_metadata_request() {
        let (state, ledger, command, plan, request, _) =
            setup_source("pica", "111111111111111111111111");
        let mut checks = 0;
        let result = run_for_download(
            &state,
            &ledger,
            &command,
            &plan,
            &request,
            Some("synthetic-token"),
            || {
                checks += 1;
                Err("SESSION_EXPIRED".into())
            },
        )
        .await;
        assert_eq!(result.unwrap_err(), "SESSION_EXPIRED");
        assert_eq!(checks, 1);
    }

    #[tokio::test]
    async fn stable_authorization_accepts_metadata_only_evidence() {
        let (state, ledger, command, plan, request, evidence) = setup();
        let post_state = state.clone();
        let post_ledger = ledger.clone();
        let result = run_with_enumerator(
            &state,
            &ledger,
            &command,
            &plan,
            &request,
            || async { Ok(evidence) },
            || Ok((post_state, post_ledger)),
        )
        .await
        .unwrap();
        assert!(result.authorization_stable);
        assert!(result.source_metadata_read_completed);
        assert_eq!(result.proof.expected_content_units, 3);
        assert!(!result.image_download_authorized);
        assert!(!result.staging_write_authorized);
        assert!(!result.inventory_mutation_authorized);
        assert!(!result.task_completion_authorized);
        assert!(!result.promotion_authorized);
        assert!(!result.replacement_authorized);
        assert!(!result.physical_delete_authorized);
    }

    #[tokio::test]
    async fn approval_change_during_enumeration_invalidates_result() {
        let (state, ledger, command, plan, request, evidence) = setup();
        let post_state = state.clone();
        let mut post_ledger = ledger.clone();
        post_ledger.records[0].user_approved = false;
        assert_eq!(
            run_with_enumerator(
                &state,
                &ledger,
                &command,
                &plan,
                &request,
                || async { Ok(evidence) },
                || Ok((post_state, post_ledger)),
            )
            .await
            .unwrap_err(),
            "SOURCE_PREFLIGHT_CURRENT_APPROVAL_REQUIRED"
        );
    }

    #[tokio::test]
    async fn unrelated_gate_ledger_change_during_enumeration_fails_closed() {
        let (state, ledger, command, plan, request, evidence) = setup();
        let post_state = state.clone();
        let mut post_ledger = ledger.clone();
        post_ledger.records[0].assistant_recommended = false;
        assert_eq!(
            run_with_enumerator(
                &state,
                &ledger,
                &command,
                &plan,
                &request,
                || async { Ok(evidence) },
                || Ok((post_state, post_ledger)),
            )
            .await
            .unwrap_err(),
            "SOURCE_PREFLIGHT_AUTHORIZATION_CHANGED_DURING_ENUMERATION"
        );
    }

    #[tokio::test]
    async fn changed_task_generation_during_enumeration_invalidates_result() {
        let (state, ledger, command, plan, request, evidence) = setup();
        let mut post_state = state.clone();
        post_state
            .pending
            .get_mut("WORK_A6_9")
            .unwrap()
            .task_revision += 1;
        let post_ledger = ledger.clone();
        assert_eq!(
            run_with_enumerator(
                &state,
                &ledger,
                &command,
                &plan,
                &request,
                || async { Ok(evidence) },
                || Ok((post_state, post_ledger)),
            )
            .await
            .unwrap_err(),
            "SOURCE_PREFLIGHT_COMMAND_NOT_CURRENT"
        );
    }
}
