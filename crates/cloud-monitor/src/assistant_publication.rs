//! V1 assistant-state publication verification.
//!
//! A3/A4/A5 editors deliberately write proposed state into isolated staging
//! directories. This module is the fail-closed bridge used immediately before
//! publishing one of those proposals into `monitor-state`.
//!
//! It never performs Git operations and never writes live state. Instead it
//! replays the staged audit against the *current* live state and requires the
//! freshly regenerated payload, audit, and preview to equal the staged files
//! exactly. Any scan/review/inventory/task/gate drift therefore invalidates a
//! stale proposal.

use crate::{
    assistant_author, assistant_decision, assistant_task_gate, monitor, persistence,
    scope_certificates,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::BTreeSet, fs, path::Path};

pub const ASSISTANT_PUBLICATION_SCHEMA_VERSION: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Author,
    Decision,
    TaskGate,
    ScopeCertificate,
}

impl Kind {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "author" => Ok(Self::Author),
            "decision" => Ok(Self::Decision),
            "task-gate" => Ok(Self::TaskGate),
            "scope-certificate" => Ok(Self::ScopeCertificate),
            _ => Err("ASSISTANT_PUBLICATION_INVALID_KIND".into()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Author => "author",
            Self::Decision => "decision",
            Self::TaskGate => "task-gate",
            Self::ScopeCertificate => "scope-certificate",
        }
    }

    fn target_file(self) -> &'static str {
        match self {
            Self::Author => "authors.json",
            Self::Decision => "decisions.json",
            Self::TaskGate => "assistant-task-gates.json",
            Self::ScopeCertificate => scope_certificates::CERTIFICATE_FILE,
        }
    }

    fn expected_files(self) -> &'static [&'static str] {
        match self {
            Self::Author => &["author-change.json", "authors.json"],
            Self::Decision => &["decision-change.json", "decisions.json", "reanalyze-preview.json"],
            Self::TaskGate => &[
                "assistant-task-gates.json",
                "executor-preview.json",
                "task-gate-change.json",
            ],
            Self::ScopeCertificate => &["scope-certificates.json", "scope-certificate-change.json"],
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PublicationCheck {
    pub schema_version: u64,
    pub kind: String,
    pub target_file: String,
    pub payload_hash: String,
    pub staging_verified: bool,
    pub monitor_state_mutation_authorized: bool,
    pub manga_file_access_authorized: bool,
    pub download_execution_authorized: bool,
    pub production_enablement_authorized: bool,
}

fn read_json(path: &Path, code: &str) -> Result<Value, String> {
    let bytes = fs::read(path).map_err(|_| code.to_owned())?;
    serde_json::from_slice(&bytes).map_err(|_| format!("{code}_JSON"))
}

fn exact_file_set(staging: &Path, kind: Kind) -> Result<(), String> {
    let mut actual = BTreeSet::new();
    let entries = fs::read_dir(staging).map_err(|_| "ASSISTANT_PUBLICATION_STAGING_READ")?;
    for entry in entries {
        let entry = entry.map_err(|_| "ASSISTANT_PUBLICATION_STAGING_READ")?;
        let file_type = entry
            .file_type()
            .map_err(|_| "ASSISTANT_PUBLICATION_STAGING_FILE_TYPE")?;
        if !file_type.is_file() || file_type.is_symlink() {
            return Err("ASSISTANT_PUBLICATION_UNEXPECTED_STAGING_ENTRY".into());
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "ASSISTANT_PUBLICATION_NON_UTF8_FILENAME")?;
        actual.insert(name);
    }
    let expected: BTreeSet<_> = kind
        .expected_files()
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    if actual != expected {
        return Err("ASSISTANT_PUBLICATION_STAGING_FILE_SET_MISMATCH".into());
    }
    Ok(())
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("ASSISTANT_PUBLICATION_AUDIT_{field}_INVALID"))
}

fn required_u64(value: &Value, field: &str) -> Result<u64, String> {
    value[field]
        .as_u64()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("ASSISTANT_PUBLICATION_AUDIT_{field}_INVALID"))
}

fn verify_author(state_dir: &Path, staging: &Path) -> Result<Value, String> {
    let current = read_json(
        &state_dir.join("authors.json"),
        "ASSISTANT_PUBLICATION_AUTHORS_READ",
    )?;
    let staged_payload = read_json(
        &staging.join("authors.json"),
        "ASSISTANT_PUBLICATION_STAGED_AUTHORS_READ",
    )?;
    let staged_audit = read_json(
        &staging.join("author-change.json"),
        "ASSISTANT_PUBLICATION_AUTHOR_AUDIT_READ",
    )?;
    let operation = required_str(&staged_audit, "operation")?;
    let requested_name = required_str(&staged_audit, "requested_name")?;
    let (payload, audit) = assistant_author::plan(&current, operation, requested_name)?;
    if payload != staged_payload || audit != staged_audit {
        return Err("ASSISTANT_PUBLICATION_AUTHOR_REPLAY_MISMATCH".into());
    }
    Ok(staged_payload)
}

fn verify_decision(state_dir: &Path, staging: &Path) -> Result<Value, String> {
    let state = persistence::load(state_dir)?;
    let staged_payload = read_json(
        &staging.join("decisions.json"),
        "ASSISTANT_PUBLICATION_STAGED_DECISIONS_READ",
    )?;
    let staged_audit = read_json(
        &staging.join("decision-change.json"),
        "ASSISTANT_PUBLICATION_DECISION_AUDIT_READ",
    )?;
    let staged_preview = read_json(
        &staging.join("reanalyze-preview.json"),
        "ASSISTANT_PUBLICATION_DECISION_PREVIEW_READ",
    )?;
    let review_id = required_str(&staged_audit, "review_id")?;
    let decision = required_str(&staged_audit, "decision")?;
    let work_id = match &staged_audit["work_id"] {
        Value::Null => None,
        Value::String(value) if !value.trim().is_empty() => Some(value.as_str()),
        _ => return Err("ASSISTANT_PUBLICATION_AUDIT_work_id_INVALID".into()),
    };
    let (payload, audit, preview) =
        assistant_decision::plan(&state, review_id, decision, work_id)?;
    if payload != staged_payload || audit != staged_audit || preview != staged_preview {
        return Err("ASSISTANT_PUBLICATION_DECISION_REPLAY_MISMATCH".into());
    }
    Ok(staged_payload)
}

fn current_gate_ledger(state_dir: &Path) -> Result<assistant_task_gate::GateLedger, String> {
    let path = state_dir.join("assistant-task-gates.json");
    if !path.exists() {
        return Ok(assistant_task_gate::GateLedger::default());
    }
    assistant_task_gate::parse_ledger(read_json(
        &path,
        "ASSISTANT_PUBLICATION_TASK_GATE_CURRENT_READ",
    )?)
}

fn verify_task_gate(state_dir: &Path, staging: &Path) -> Result<Value, String> {
    let state = persistence::load(state_dir)?;
    let current = current_gate_ledger(state_dir)?;
    let staged_payload = read_json(
        &staging.join("assistant-task-gates.json"),
        "ASSISTANT_PUBLICATION_STAGED_TASK_GATES_READ",
    )?;
    let staged_ledger = assistant_task_gate::parse_ledger(staged_payload.clone())?;
    let staged_audit = read_json(
        &staging.join("task-gate-change.json"),
        "ASSISTANT_PUBLICATION_TASK_GATE_AUDIT_READ",
    )?;
    let staged_preview = read_json(
        &staging.join("executor-preview.json"),
        "ASSISTANT_PUBLICATION_TASK_GATE_PREVIEW_READ",
    )?;
    let operation = required_str(&staged_audit, "operation")?;
    let task_id = required_str(&staged_audit, "task_id")?;
    let revision = required_u64(&staged_audit, "task_revision")?;
    let target_hash = required_str(&staged_audit, "target_hash")?;
    let (payload, audit, preview) = assistant_task_gate::plan(
        &state,
        &current,
        operation,
        task_id,
        revision,
        target_hash,
    )?;
    if payload != staged_ledger || audit != staged_audit || preview != staged_preview {
        return Err("ASSISTANT_PUBLICATION_TASK_GATE_REPLAY_MISMATCH".into());
    }
    Ok(staged_payload)
}

fn verify_scope_certificate(state_dir: &Path, staging: &Path) -> Result<Value, String> {
    scope_certificates::verify_staged_candidate(state_dir, staging)
}

/// Revalidate one staged A3/A4/A5 mutation against current monitor state.
///
/// Success authorizes publishing exactly one JSON state file. It never grants
/// source/media access, download execution, local manga-library access, or
/// production enablement.
pub fn check(state_dir: &Path, staging: &Path, kind: &str) -> Result<PublicationCheck, String> {
    if !state_dir.is_dir() {
        return Err("ASSISTANT_PUBLICATION_STATE_DIR_MISSING".into());
    }
    if !staging.is_dir() {
        return Err("ASSISTANT_PUBLICATION_STAGING_DIR_MISSING".into());
    }
    let kind = Kind::parse(kind)?;
    exact_file_set(staging, kind)?;
    let payload = match kind {
        Kind::Author => verify_author(state_dir, staging)?,
        Kind::Decision => verify_decision(state_dir, staging)?,
        Kind::TaskGate => verify_task_gate(state_dir, staging)?,
        Kind::ScopeCertificate => verify_scope_certificate(state_dir, staging)?,
    };
    Ok(PublicationCheck {
        schema_version: ASSISTANT_PUBLICATION_SCHEMA_VERSION,
        kind: kind.name().into(),
        target_file: kind.target_file().into(),
        payload_hash: monitor::hash(&payload),
        staging_verified: true,
        monitor_state_mutation_authorized: true,
        manga_file_access_authorized: false,
        download_execution_authorized: false,
        production_enablement_authorized: false,
    })
}

pub fn check_json(state_dir: &Path, staging: &Path, kind: &str) -> Result<Value, String> {
    serde_json::to_value(check(state_dir, staging, kind)?)
        .map_err(|_| "ASSISTANT_PUBLICATION_RESULT_SERIALIZE".into())
        .map(|mut value| {
            value["scope"] = json!("MONITOR_STATE_SINGLE_FILE_ONLY");
            value
        })
}
