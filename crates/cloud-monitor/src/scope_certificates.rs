//! Strict, read-only trusted author-scope certificates.
//!
//! Certificates are authority-bearing input produced outside Actions. This
//! module validates candidates against the current public state and provides a
//! local certifier API without ever discovering or opening a local library in a
//! cloud run.
use crate::{matcher_m2::ScopeCertificate as MatcherCertificate, monitor::hash};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path};

pub const SCHEMA_VERSION: u64 = 1;
pub const ATTESTATION_SCHEMA_VERSION: u64 = 1;
pub const CERTIFICATE_FILE: &str = "scope-certificates.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScopeCertificateRecord {
    pub author: String,
    pub inventory_hash: String,
    pub complete_inventory_snapshot_hash: String,
    pub author_scope_projection_hash: String,
    pub author_scope_projection_count: u64,
    pub completeness_attestation_hash: String,
    pub rule_version: String,
    pub reference_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScopeCertificateDocument {
    pub schema_version: u64,
    pub certificate_set_hash: String,
    pub certificates: Vec<ScopeCertificateRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompletenessAttestation {
    pub schema_version: u64,
    pub snapshot_hash: String,
    pub inventory_hash: String,
    pub projection_hash: String,
    pub projection_count: u64,
    pub producer: String,
    pub producer_evidence_hash: String,
    pub attestation_hash: String,
}

#[derive(Clone, Debug)]
pub struct ValidatedScopeCertificates {
    pub document: ScopeCertificateDocument,
    pub matcher_certificates: Vec<MatcherCertificate>,
}

fn is_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn reference_hash(record: &ScopeCertificateRecord) -> String {
    hash(&(
        &record.author,
        &record.inventory_hash,
        &record.complete_inventory_snapshot_hash,
        &record.author_scope_projection_hash,
        record.author_scope_projection_count,
        &record.completeness_attestation_hash,
        &record.rule_version,
    ))
}

fn expected_attestation_hash(attestation: &CompletenessAttestation) -> String {
    hash(&(
        attestation.schema_version,
        &attestation.snapshot_hash,
        &attestation.inventory_hash,
        &attestation.projection_hash,
        attestation.projection_count,
        &attestation.producer,
        &attestation.producer_evidence_hash,
    ))
}

pub fn canonical_author_projection(author: &str, inventory: &Value) -> Result<Value, String> {
    if author.trim().is_empty() {
        return Err("LOCAL_CERTIFIER_AUTHOR_REQUIRED".into());
    }
    let mut works = inventory["works"]
        .as_array()
        .ok_or("LOCAL_CERTIFIER_WORKS_REQUIRED")?
        .iter()
        .filter(|work| {
            work["authors_confirmed"]
                .as_array()
                .is_some_and(|authors| authors.iter().any(|value| value.as_str() == Some(author)))
        })
        .cloned()
        .collect::<Vec<_>>();
    works.sort_by(|left, right| left["work_id"].as_str().cmp(&right["work_id"].as_str()));
    if works
        .iter()
        .any(|work| work["work_id"].as_str().is_none_or(|id| id.trim().is_empty()))
    {
        return Err("LOCAL_CERTIFIER_PROJECTION_WORK_ID_REQUIRED".into());
    }
    Ok(Value::Array(works))
}

pub fn certificate_set_hash(records: &[ScopeCertificateRecord]) -> String {
    hash(&records)
}

pub fn empty_document() -> ScopeCertificateDocument {
    ScopeCertificateDocument {
        schema_version: SCHEMA_VERSION,
        certificate_set_hash: certificate_set_hash(&[]),
        certificates: Vec::new(),
    }
}

pub fn load(path: &Path) -> Result<ScopeCertificateDocument, String> {
    serde_json::from_slice(&fs::read(path).map_err(|_| "SCOPE_CERTIFICATES_MISSING")?)
        .map_err(|_| "INVALID_SCOPE_CERTIFICATES".into())
}

pub fn validate_document(
    document: ScopeCertificateDocument,
    authors: &Value,
    inventory: &Value,
) -> Result<ValidatedScopeCertificates, String> {
    if document.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "UNSUPPORTED_SCOPE_CERTIFICATES_SCHEMA_{}",
            document.schema_version
        ));
    }
    if document.certificate_set_hash != certificate_set_hash(&document.certificates) {
        return Err("SCOPE_CERTIFICATE_SET_HASH_MISMATCH".into());
    }
    let enabled: BTreeSet<String> = authors["authors"]
        .as_array()
        .ok_or("INVALID_AUTHORS")?
        .iter()
        .filter(|author| author["enabled"] != false)
        .filter_map(|author| author["name"].as_str().map(str::to_owned))
        .collect();
    let current_inventory_hash = hash(inventory);
    let mut seen = BTreeSet::new();
    let mut matcher_certificates = Vec::with_capacity(document.certificates.len());
    for record in &document.certificates {
        if record.author.is_empty() || !seen.insert(record.author.clone()) {
            return Err("DUPLICATE_OR_EMPTY_SCOPE_CERTIFICATE_AUTHOR".into());
        }
        if !enabled.contains(&record.author) {
            return Err("SCOPE_CERTIFICATE_AUTHOR_NOT_ENABLED".into());
        }
        for value in [
            &record.inventory_hash,
            &record.complete_inventory_snapshot_hash,
            &record.author_scope_projection_hash,
            &record.completeness_attestation_hash,
            &record.reference_hash,
        ] {
            if !is_hash(value) {
                return Err("MALFORMED_SCOPE_CERTIFICATE_HASH".into());
            }
        }
        if record.inventory_hash != current_inventory_hash {
            return Err("SCOPE_CERTIFICATE_INVENTORY_HASH_STALE".into());
        }
        if record.rule_version != rules_core::title_m2::RULE_VERSION {
            return Err("SCOPE_CERTIFICATE_RULE_VERSION_STALE".into());
        }
        if record.reference_hash != reference_hash(record) {
            return Err("SCOPE_CERTIFICATE_REFERENCE_MISMATCH".into());
        }
        matcher_certificates.push(MatcherCertificate {
            author: record.author.clone(),
            inventory_hash: record.inventory_hash.clone(),
            reference: record.reference_hash.clone(),
        });
    }
    if document
        .certificates
        .windows(2)
        .any(|pair| pair[0].author >= pair[1].author)
    {
        return Err("SCOPE_CERTIFICATES_NOT_SORTED".into());
    }
    Ok(ValidatedScopeCertificates {
        document,
        matcher_certificates,
    })
}

/// Empty certificates preserve the existing no-authority context. A non-empty
/// set becomes the identity authority epoch used by State::context().
pub fn state_authority_hash(validated: &ValidatedScopeCertificates) -> String {
    if validated.matcher_certificates.is_empty() {
        String::new()
    } else {
        validated.document.certificate_set_hash.clone()
    }
}

/// Read-only local certifier. Both author projections are derived here from
/// the complete local snapshot and current public inventory. The external
/// attestation envelope is checked and hash-bound, but this function does not
/// by itself prove the identity or independence of its producer.
pub fn certify_snapshot(
    author: &str,
    public_authors: &Value,
    public_inventory: &Value,
    complete_snapshot: &Value,
    attestation: &CompletenessAttestation,
    rule_version: &str,
) -> Result<ScopeCertificateRecord, String> {
    if author.is_empty() || complete_snapshot["complete"] != true {
        return Err("LOCAL_CERTIFIER_REQUIRES_COMPLETE_SNAPSHOT".into());
    }
    let enabled = public_authors["authors"]
        .as_array()
        .ok_or("LOCAL_CERTIFIER_INVALID_AUTHORS")?
        .iter()
        .any(|item| item["name"].as_str() == Some(author) && item["enabled"] != false);
    if !enabled {
        return Err("LOCAL_CERTIFIER_AUTHOR_NOT_ENABLED".into());
    }
    if attestation.schema_version != ATTESTATION_SCHEMA_VERSION
        || attestation.producer.is_empty()
        || attestation.producer == "local-certifier"
        || !is_hash(&attestation.producer_evidence_hash)
    {
        return Err("LOCAL_CERTIFIER_REQUIRES_EXTERNAL_ATTESTATION_ENVELOPE".into());
    }
    let snapshot_hash = hash(complete_snapshot);
    let inventory_hash = hash(public_inventory);
    let local_projection = canonical_author_projection(author, complete_snapshot)?;
    let public_projection = canonical_author_projection(author, public_inventory)?;
    if local_projection != public_projection {
        return Err("LOCAL_CERTIFIER_AUTHOR_SCOPE_PROJECTION_MISMATCH".into());
    }
    let projection_hash = hash(&local_projection);
    let projection_count = local_projection.as_array().unwrap().len() as u64;
    if attestation.snapshot_hash != snapshot_hash
        || attestation.inventory_hash != inventory_hash
        || attestation.projection_hash != projection_hash
        || attestation.projection_count != projection_count
        || attestation.attestation_hash != expected_attestation_hash(attestation)
    {
        return Err("LOCAL_CERTIFIER_ATTESTATION_MISMATCH".into());
    }
    if rule_version != rules_core::title_m2::RULE_VERSION {
        return Err("LOCAL_CERTIFIER_RULE_VERSION_MISMATCH".into());
    }
    let mut record = ScopeCertificateRecord {
        author: author.into(),
        inventory_hash,
        complete_inventory_snapshot_hash: snapshot_hash,
        author_scope_projection_hash: projection_hash,
        author_scope_projection_count: projection_count,
        completeness_attestation_hash: attestation.attestation_hash.clone(),
        rule_version: rule_version.into(),
        reference_hash: String::new(),
    };
    record.reference_hash = reference_hash(&record);
    Ok(record)
}

/// The environment guard is intentionally before opening the path. Formal
/// Actions runs therefore cannot use this local-inventory entry point.
pub fn certify_snapshot_file(
    author: &str,
    public_authors: &Value,
    public_inventory: &Value,
    snapshot_path: &Path,
    attestation: &CompletenessAttestation,
    rule_version: &str,
) -> Result<ScopeCertificateRecord, String> {
    if std::env::var("GITHUB_ACTIONS").is_ok_and(|value| value == "true") {
        return Err("LOCAL_CERTIFIER_FORBIDDEN_IN_GITHUB_ACTIONS".into());
    }
    let snapshot: Value = serde_json::from_slice(
        &fs::read(snapshot_path).map_err(|_| "LOCAL_CERTIFIER_SNAPSHOT_READ")?,
    )
    .map_err(|_| "LOCAL_CERTIFIER_SNAPSHOT_JSON")?;
    certify_snapshot(author, public_authors, public_inventory, &snapshot, attestation, rule_version)
}

pub fn certificate_change(document: &ScopeCertificateDocument) -> Value {
    serde_json::json!({
        "schema_version": 1,
        "operation": "replace",
        "target_file": CERTIFICATE_FILE,
        "certificate_set_hash": &document.certificate_set_hash,
        "certificate_count": document.certificates.len(),
        "inventory_hash": document.certificates.first().map(|c| c.inventory_hash.clone()),
        "authority_source": "external_attestation_envelope_candidate"
    })
}

pub fn stage_candidate(
    state_dir: &Path,
    output: &Path,
    document: ScopeCertificateDocument,
) -> Result<(), String> {
    if output.exists() {
        return Err("SCOPE_CERTIFICATE_OUTPUT_EXISTS".into());
    }
    if document.certificates.is_empty() && document != empty_document() {
        return Err("SCOPE_CERTIFICATE_CANDIDATE_EMPTY_NOT_CANONICAL".into());
    }
    let state = crate::persistence::load(state_dir)?;
    validate_document(document.clone(), &state.authors, &state.inventory)?;
    fs::create_dir_all(output).map_err(|_| "SCOPE_CERTIFICATE_OUTPUT_CREATE")?;
    crate::persistence::write_json(&output.join(CERTIFICATE_FILE), &document)?;
    crate::persistence::write_json(
        &output.join("scope-certificate-change.json"),
        &certificate_change(&document),
    )?;
    Ok(())
}

pub fn verify_staged_candidate(state_dir: &Path, staging: &Path) -> Result<Value, String> {
    let state = crate::persistence::load(state_dir)?;
    let document: ScopeCertificateDocument = serde_json::from_slice(
        &fs::read(staging.join(CERTIFICATE_FILE)).map_err(|_| "SCOPE_CERTIFICATE_STAGED_READ")?,
    )
    .map_err(|_| "SCOPE_CERTIFICATE_STAGED_JSON")?;
    let audit: Value = serde_json::from_slice(
        &fs::read(staging.join("scope-certificate-change.json"))
            .map_err(|_| "SCOPE_CERTIFICATE_AUDIT_READ")?,
    )
    .map_err(|_| "SCOPE_CERTIFICATE_AUDIT_JSON")?;
    validate_document(document.clone(), &state.authors, &state.inventory)?;
    if (document.certificates.is_empty() && document != empty_document())
        || certificate_change(&document) != audit
    {
        return Err("SCOPE_CERTIFICATE_STAGED_REPLAY_MISMATCH".into());
    }
    Ok(serde_json::to_value(document).map_err(|_| "SCOPE_CERTIFICATE_SERIALIZE")?)
}
