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
    pub snapshot_hash: String,
    pub inventory_hash: String,
    pub projection_hash: String,
    pub projection_count: u64,
    pub producer: String,
    pub attestation_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
        &attestation.snapshot_hash,
        &attestation.inventory_hash,
        &attestation.projection_hash,
        attestation.projection_count,
        &attestation.producer,
    ))
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
        if record.rule_version.is_empty() || record.reference_hash != reference_hash(record) {
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

/// Read-only local certifier. The snapshot and projection are supplied by the
/// caller, while completeness is independently attested and hash-bound.
pub fn certify_snapshot(
    author: &str,
    public_authors: &Value,
    public_inventory: &Value,
    complete_snapshot: &Value,
    author_scope_projection: &Value,
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
    if attestation.producer.is_empty() || attestation.producer == "local-certifier" {
        return Err("LOCAL_CERTIFIER_REQUIRES_INDEPENDENT_ATTESTATION".into());
    }
    let snapshot_hash = hash(complete_snapshot);
    let inventory_hash = hash(public_inventory);
    let projection_hash = hash(author_scope_projection);
    let projection_count = author_scope_projection
        .as_array()
        .ok_or("LOCAL_CERTIFIER_INVALID_PROJECTION")?
        .len() as u64;
    if attestation.snapshot_hash != snapshot_hash
        || attestation.inventory_hash != inventory_hash
        || attestation.projection_hash != projection_hash
        || attestation.projection_count != projection_count
        || attestation.attestation_hash != expected_attestation_hash(attestation)
    {
        return Err("LOCAL_CERTIFIER_ATTESTATION_MISMATCH".into());
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
    projection: &Value,
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
    certify_snapshot(
        author,
        public_authors,
        public_inventory,
        &snapshot,
        projection,
        attestation,
        rule_version,
    )
}

pub fn certificate_change(document: &ScopeCertificateDocument) -> Value {
    serde_json::json!({
        "schema_version": 1,
        "operation": "replace",
        "target_file": CERTIFICATE_FILE,
        "certificate_set_hash": &document.certificate_set_hash,
        "certificate_count": document.certificates.len(),
        "inventory_hash": document.certificates.first().map(|c| c.inventory_hash.clone()),
        "authority_source": "independent_local_attestation_candidate"
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
    if document.certificates.is_empty() {
        return Err("SCOPE_CERTIFICATE_CANDIDATE_EMPTY".into());
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
    if document.certificates.is_empty() || certificate_change(&document) != audit {
        return Err("SCOPE_CERTIFICATE_STAGED_REPLAY_MISMATCH".into());
    }
    Ok(serde_json::to_value(document).map_err(|_| "SCOPE_CERTIFICATE_SERIALIZE")?)
}
