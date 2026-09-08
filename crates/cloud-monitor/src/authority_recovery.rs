//! Read-only classification of a staged checkpoint against current public authority.
//! Nothing here opens source clients, repairs state, or writes a file.
use crate::{
    monitor::{hash, State},
    persistence, scope_certificates,
};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::Path};

pub const RECOVERY_KIND: &str = "AUTHORITY_DRIFT_FULL";

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Classification {
    ResumableExact,
    AuthorityDriftRequiresFullRecovery,
    OptionsMismatch,
    CheckpointCorrupt,
    CurrentStateInvalid,
    NotPartial,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryEvidence {
    pub base_commit: String,
    pub durable_base_commit: String,
    pub state_hash: String,
    pub scan_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recovery {
    pub schema_version: u64,
    pub kind: String,
    pub generation_id: String,
    pub started_at: String,
    pub authority_hash: String,
    pub evidence: Vec<RecoveryEvidence>,
}

impl Recovery {
    fn expected_id(&self) -> String {
        hash(&(
            self.schema_version,
            &self.kind,
            &self.started_at,
            &self.authority_hash,
            &self.evidence,
        ))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u64,
    pub base_commit: String,
    pub state_hash: String,
    #[serde(default)]
    pub input_state_hash: Option<String>,
    pub scan_id: String,
    pub complete: bool,
    #[serde(default)]
    pub coverage_complete: Option<bool>,
    pub strategy_complete: bool,
    pub requested_mode: String,
    pub effective_requested_mode: String,
    pub batch_index: usize,
    pub batch_count: usize,
    #[serde(default)]
    pub batch_size: Option<usize>,
    pub selected_authors: Vec<String>,
    #[serde(default)]
    pub matcher_version: Option<String>,
    #[serde(default)]
    pub analysis_context: Option<String>,
    #[serde(default)]
    pub identity_authority_hash: Option<String>,
    #[serde(default)]
    pub all_authors: Option<Vec<String>>,
    #[serde(default)]
    pub recovery: Option<Recovery>,
}

pub struct Options<'a> {
    pub requested_mode: &'a str,
    pub threshold: usize,
    pub batch_size: usize,
    pub batch_index: usize,
    pub all_authors: &'a [String],
}

#[derive(Debug, Serialize)]
pub struct Preflight {
    pub classification: Classification,
    pub reason: String,
}

impl Preflight {
    fn new(classification: Classification, reason: impl Into<String>) -> Self {
        Self {
            classification,
            reason: reason.into(),
        }
    }
}

pub fn strategy_complete(state: &State) -> bool {
    state.scan.direct_failures.is_empty()
        && state.scan.selected_authors.iter().all(|author| {
            ["jm", "pica"].iter().all(|source| {
                state
                    .scan
                    .progress
                    .get(&State::cursor_key(source, author))
                    .is_some_and(|cursor| {
                        ["COMPLETE", "EARLY_STOP_HEURISTIC"].contains(&cursor.boundary.as_str())
                    })
            })
        })
}

pub fn is_partial(state: &State) -> bool {
    !state.scan.scan_id.is_empty()
        && !state.scan.selected_authors.is_empty()
        && !state.scan.complete
        && !strategy_complete(state)
}

pub fn author_selection(value: &Value) -> Result<Vec<String>, String> {
    if let Ok(names) = serde_json::from_value::<Vec<String>>(value.clone()) {
        return Ok(names);
    }
    value["authors"]
        .as_array()
        .ok_or("AUTHORS_JSON")?
        .iter()
        .filter(|author| author["enabled"] != false)
        .map(|author| {
            author["name"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "AUTHORS_JSON".into())
        })
        .collect()
}

// The first durable Phase3B export predates Scan.identity_authority_hash. Its
// manifest hashes the original struct field order, not an alphabetically sorted
// Value. Keep this one observed historical representation explicit; unknown or
// otherwise missing fields are not silently normalized into trusted evidence.
struct LegacyCheckpoint<'a>(&'a State);
struct LegacyScan<'a>(&'a crate::monitor::Scan);

impl Serialize for LegacyCheckpoint<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut output = serializer.serialize_struct("State", 8)?;
        output.serialize_field("authors", &self.0.authors)?;
        output.serialize_field("inventory", &self.0.inventory)?;
        output.serialize_field("catalog", &self.0.catalog)?;
        output.serialize_field("pending", &self.0.pending)?;
        output.serialize_field("review", &self.0.review)?;
        output.serialize_field("cleanup_review", &self.0.cleanup_review)?;
        output.serialize_field("decisions", &self.0.decisions)?;
        output.serialize_field("scan", &LegacyScan(&self.0.scan))?;
        output.end()
    }
}

impl Serialize for LegacyScan<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut output = serializer.serialize_struct("Scan", 14)?;
        output.serialize_field("scan_id", &self.0.scan_id)?;
        output.serialize_field("started_at", &self.0.started_at)?;
        output.serialize_field("selected_authors", &self.0.selected_authors)?;
        output.serialize_field("requested_mode", &self.0.requested_mode)?;
        output.serialize_field("threshold", &self.0.threshold)?;
        output.serialize_field("historical_ids", &self.0.historical_ids)?;
        output.serialize_field("progress", &self.0.progress)?;
        output.serialize_field("last_full", &self.0.last_full)?;
        output.serialize_field("event_keys", &self.0.event_keys)?;
        output.serialize_field("events", &self.0.events)?;
        output.serialize_field("complete", &self.0.complete)?;
        output.serialize_field("direct_failures", &self.0.direct_failures)?;
        output.serialize_field("inventory_repairs", &self.0.inventory_repairs)?;
        output.serialize_field("review_migration", &self.0.review_migration)?;
        output.end()
    }
}

fn checkpoint_hash(checkpoint: &State, raw: &Value) -> Result<String, String> {
    let current = serde_json::to_value(checkpoint).map_err(|_| "CHECKPOINT_SERIALIZE")?;
    if raw == &current {
        return Ok(hash(checkpoint));
    }
    if checkpoint.scan.identity_authority_hash.is_empty()
        && raw["scan"].get("identity_authority_hash").is_none()
        && raw
            == &serde_json::to_value(LegacyCheckpoint(checkpoint))
                .map_err(|_| "CHECKPOINT_SERIALIZE")?
    {
        return Ok(hash(&LegacyCheckpoint(checkpoint)));
    }
    Err("CHECKPOINT_SCHEMA".into())
}

/// A manifest is a binding, not a hint. A damaged checkpoint must never be
/// mistaken for legitimate authority drift just because its context differs.
pub fn bound_checkpoint(dir: &Path) -> Result<(State, Manifest), String> {
    let checkpoint = persistence::load_checkpoint(dir)?;
    let raw_checkpoint: Value = serde_json::from_slice(
        &fs::read(dir.join("checkpoint.json")).map_err(|_| "CHECKPOINT_READ")?,
    )
    .map_err(|_| "CHECKPOINT_JSON")?;
    let checkpoint_hash = checkpoint_hash(&checkpoint, &raw_checkpoint)?;
    if checkpoint
        .authors
        .get("schema_version")
        .is_some_and(|v| v.as_u64() != Some(1))
        || checkpoint
            .inventory
            .get("schema_version")
            .is_some_and(|v| v.as_u64().is_none_or(|n| !(1..=8).contains(&n)))
    {
        return Err("CHECKPOINT_SCHEMA".into());
    }
    let manifest: Manifest = serde_json::from_slice(
        &fs::read(dir.join("state-manifest.json")).map_err(|_| "MANIFEST_READ")?,
    )
    .map_err(|_| "MANIFEST_SCHEMA")?;
    if manifest.schema_version != 1
        || manifest.state_hash != checkpoint_hash
        || manifest.scan_id != checkpoint.scan.scan_id
        || manifest.complete != checkpoint.scan.complete
        || manifest
            .coverage_complete
            .is_some_and(|complete| complete != checkpoint.scan.complete)
        || manifest
            .identity_authority_hash
            .as_ref()
            .is_some_and(|authority| authority != &checkpoint.scan.identity_authority_hash)
        || manifest.strategy_complete != strategy_complete(&checkpoint)
        || manifest.selected_authors != checkpoint.scan.selected_authors
        || manifest.effective_requested_mode != checkpoint.scan.requested_mode
        || manifest.batch_count == 0
        || manifest.batch_index >= manifest.batch_count
        || checkpoint.scan.threshold == 0
        || !["full", "incremental"].contains(&checkpoint.scan.requested_mode.as_str())
        || !["full", "incremental", "monthly"].contains(&manifest.requested_mode.as_str())
        || (checkpoint.scan.scan_id.starts_with("recovery-") && manifest.recovery.is_none())
    {
        return Err("CHECKPOINT_MANIFEST_BINDING".into());
    }
    if let Some(batch_size) = manifest.batch_size {
        if batch_size == 0 || batch_size > 200 {
            return Err("CHECKPOINT_BATCH_BINDING".into());
        }
        if let Some(authors) = &manifest.all_authors {
            let selected: Vec<_> = authors
                .iter()
                .skip(manifest.batch_index.saturating_mul(batch_size))
                .take(batch_size)
                .cloned()
                .collect();
            if authors.is_empty()
                || authors.len().div_ceil(batch_size) != manifest.batch_count
                || authors
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != authors.len()
                || authors
                    .iter()
                    .any(|author| !checkpoint.author_names().contains(author))
                || selected != checkpoint.scan.selected_authors
            {
                return Err("CHECKPOINT_BATCH_BINDING".into());
            }
        }
    }
    if let Some(recovery) = &manifest.recovery {
        if recovery.schema_version != 1
            || recovery.kind != RECOVERY_KIND
            || recovery.generation_id != recovery.expected_id()
            || recovery.evidence.is_empty()
            || checkpoint.scan.requested_mode != "full"
            || manifest.batch_size.is_none()
            || manifest.all_authors.is_none()
            || recovery
                .evidence
                .iter()
                .any(|e| e.state_hash.len() != 64 || e.scan_id.is_empty())
            || checkpoint.scan.scan_id
                != format!(
                    "recovery-{}-batch-{}",
                    recovery.generation_id, manifest.batch_index
                )
        {
            return Err("RECOVERY_MANIFEST_BINDING".into());
        }
    }
    Ok((checkpoint, manifest))
}

pub fn preflight(input: &Path, checkpoint_dir: &Path, options: &Options<'_>) -> Preflight {
    let (checkpoint, manifest) = match bound_checkpoint(checkpoint_dir) {
        Ok(value) => value,
        Err(reason) => return Preflight::new(Classification::CheckpointCorrupt, reason),
    };
    let mut current = match persistence::load(input) {
        Ok(state) => state,
        Err(reason) => return Preflight::new(Classification::CurrentStateInvalid, reason),
    };
    let certificates = scope_certificates::load(&input.join(scope_certificates::CERTIFICATE_FILE))
        .and_then(|doc| {
            scope_certificates::validate_document(doc, &current.authors, &current.inventory)
        });
    let certificates = match certificates {
        Ok(value) => value,
        Err(reason) => return Preflight::new(Classification::CurrentStateInvalid, reason),
    };
    let current_partial = is_partial(&current);
    let same_scan = hash(&current.scan) == hash(&checkpoint.scan);
    if !same_scan && manifest.input_state_hash.as_ref() != Some(&hash(&current)) {
        return Preflight::new(
            Classification::CurrentStateInvalid,
            if manifest.input_state_hash.is_none() {
                "LEGACY_RESUME_INPUT_BUSINESS_UNBOUND"
            } else {
                "RESUME_INPUT_BUSINESS_STATE_MISMATCH"
            },
        );
    }
    current.scan.identity_authority_hash = scope_certificates::state_authority_hash(&certificates);
    if !is_partial(&checkpoint) {
        return Preflight::new(Classification::NotPartial, "CHECKPOINT_NOT_PARTIAL");
    }
    // context() uses the currently compiled rule version on both values, so
    // equality alone cannot detect a matcher upgrade. Bind new checkpoints to
    // the persisted rule epoch; legacy nonempty catalogs retain entry evidence.
    if manifest.matcher_version.is_none() && checkpoint.catalog.is_empty() {
        return Preflight::new(
            Classification::CheckpointCorrupt,
            "LEGACY_CHECKPOINT_RULE_VERSION_UNBOUND",
        );
    }
    let checkpoint_context = checkpoint.context();
    if manifest.matcher_version.as_deref() == Some(rules_core::title_m2::RULE_VERSION)
        && manifest
            .analysis_context
            .as_ref()
            .is_some_and(|context| context != &checkpoint_context)
    {
        return Preflight::new(
            Classification::CheckpointCorrupt,
            "CHECKPOINT_ANALYSIS_CONTEXT_BINDING",
        );
    }
    let rules_changed = manifest
        .matcher_version
        .as_deref()
        .is_some_and(|v| v != rules_core::title_m2::RULE_VERSION)
        || checkpoint.catalog.values().any(|entry| {
            entry.analysis_context != checkpoint_context
                || entry.matcher_version != rules_core::title_m2::RULE_VERSION
        });
    let old_authors = match &manifest.all_authors {
        Some(authors) => authors.clone(),
        None => match author_selection(&checkpoint.authors) {
            Ok(authors) => authors,
            Err(reason) => return Preflight::new(Classification::CheckpointCorrupt, reason),
        },
    };
    let old_selected: Vec<_> = old_authors
        .iter()
        .skip(options.batch_index.saturating_mul(options.batch_size))
        .take(options.batch_size)
        .cloned()
        .collect();
    let registry_changed = current.authors != checkpoint.authors;
    let selection_valid = options.all_authors == old_authors
        || (registry_changed
            && author_selection(&current.authors).is_ok_and(|a| a == options.all_authors));
    if options.batch_size == 0
        || options.batch_size > 200
        || options.threshold != checkpoint.scan.threshold
        || options.requested_mode != manifest.requested_mode
        || options.batch_index != manifest.batch_index
        || manifest
            .batch_size
            .is_some_and(|size| size != options.batch_size)
        || old_selected != checkpoint.scan.selected_authors
        || !selection_valid
        || (manifest.recovery.is_none()
            && checkpoint.scan.requested_mode
                != if options.requested_mode == "monthly" {
                    "incremental"
                } else {
                    options.requested_mode
                })
    {
        return Preflight::new(Classification::OptionsMismatch, "RESUME_OPTIONS_MISMATCH");
    }
    if current.context() != checkpoint_context || rules_changed {
        // Recovery must start from the durable interrupted generation; a caller
        // cannot combine arbitrary old progress with a different/complete input.
        if !current_partial || !same_scan {
            return Preflight::new(
                Classification::CheckpointCorrupt,
                "RECOVERY_INPUT_SCAN_MISMATCH",
            );
        }
        Preflight::new(
            Classification::AuthorityDriftRequiresFullRecovery,
            "RESUME_AUTHORITY_MISMATCH",
        )
    } else {
        Preflight::new(Classification::ResumableExact, "RESUME_AUTHORITY_EXACT")
    }
}

pub fn recovery_from(
    checkpoint_dir: &Path,
    durable_base: &str,
    authority_hash: String,
    started_at: String,
) -> Result<Recovery, String> {
    let (checkpoint, manifest) = bound_checkpoint(checkpoint_dir)?;
    let mut evidence = manifest.recovery.map(|r| r.evidence).unwrap_or_default();
    evidence.push(RecoveryEvidence {
        base_commit: manifest.base_commit,
        durable_base_commit: durable_base.into(),
        state_hash: manifest.state_hash,
        scan_id: checkpoint.scan.scan_id,
    });
    let mut recovery = Recovery {
        schema_version: 1,
        kind: RECOVERY_KIND.into(),
        generation_id: String::new(),
        started_at,
        authority_hash,
        evidence,
    };
    recovery.generation_id = recovery.expected_id();
    Ok(recovery)
}
