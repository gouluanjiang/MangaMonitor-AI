use cloud_monitor::{monitor::hash, scope_certificates};
use serde_json::json;

fn authors() -> serde_json::Value {
    json!({"schema_version":1,"authors":[{"author_id":"AUTHOR_0043","name":"santa","enabled":true}]})
}

fn inventory() -> serde_json::Value {
    json!({
        "schema_version":8,
        "works":[{"work_id":"LOCAL_1","authors_confirmed":["santa"]}]
    })
}

fn complete_snapshot() -> serde_json::Value {
    json!({
        "complete":true,
        "works":[{"work_id":"LOCAL_1","authors_confirmed":["santa"]}]
    })
}

fn attestation(snapshot: &serde_json::Value, projection: &serde_json::Value) -> scope_certificates::CompletenessAttestation {
    let producer_evidence_hash = hash(&"fixture-evidence");
    scope_certificates::CompletenessAttestation {
        schema_version: scope_certificates::ATTESTATION_SCHEMA_VERSION,
        snapshot_hash: hash(snapshot),
        inventory_hash: hash(&inventory()),
        projection_hash: hash(projection),
        projection_count: projection.as_array().unwrap().len() as u64,
        producer: "independent-fixture".into(),
        producer_evidence_hash: producer_evidence_hash.clone(),
        attestation_hash: hash(&(
            scope_certificates::ATTESTATION_SCHEMA_VERSION,
            hash(snapshot),
            hash(&inventory()),
            hash(projection),
            projection.as_array().unwrap().len() as u64,
            "independent-fixture",
            producer_evidence_hash,
        )),
    }
}

fn valid_record() -> scope_certificates::ScopeCertificateRecord {
    let snapshot = complete_snapshot();
    let projection = scope_certificates::canonical_author_projection("santa", &snapshot).unwrap();
    let attestation = attestation(&snapshot, &projection);
    scope_certificates::certify_snapshot(
        "santa",
        &authors(),
        &inventory(),
        &snapshot,
        &attestation,
        rules_core::title_m2::RULE_VERSION,
    )
    .unwrap()
}

#[test]
fn empty_document_is_explicit_and_preserves_no_authority_epoch() {
    let document = scope_certificates::empty_document();
    let validated = scope_certificates::validate_document(document, &authors(), &inventory()).unwrap();
    assert!(validated.matcher_certificates.is_empty());
    assert!(scope_certificates::state_authority_hash(&validated).is_empty());
}

#[test]
fn valid_certificate_is_strictly_bound_to_current_inventory_and_author() {
    let record = valid_record();
    let document = scope_certificates::ScopeCertificateDocument {
        schema_version: 1,
        certificate_set_hash: scope_certificates::certificate_set_hash(std::slice::from_ref(&record)),
        certificates: vec![record],
    };
    let validated = scope_certificates::validate_document(document, &authors(), &inventory()).unwrap();
    assert_eq!(validated.matcher_certificates.len(), 1);
    assert_eq!(scope_certificates::state_authority_hash(&validated).len(), 64);
    let mut drifted = inventory();
    drifted["works"] = json!([{"work_id":"DRIFT"}]);
    let err = scope_certificates::validate_document(validated.document, &authors(), &drifted).unwrap_err();
    assert_eq!(err, "SCOPE_CERTIFICATE_INVENTORY_HASH_STALE");
}

#[test]
fn malformed_future_and_unknown_fields_fail_closed() {
    let future = serde_json::from_value::<scope_certificates::ScopeCertificateDocument>(json!({
        "schema_version": 2,
        "certificate_set_hash": "x",
        "certificates": []
    }))
    .unwrap();
    assert_eq!(
        scope_certificates::validate_document(future, &authors(), &inventory()).unwrap_err(),
        "UNSUPPORTED_SCOPE_CERTIFICATES_SCHEMA_2"
    );
    assert!(serde_json::from_value::<scope_certificates::ScopeCertificateDocument>(json!({
        "schema_version": 1,
        "certificate_set_hash": "x",
        "certificates": [],
        "unexpected": true
    }))
    .is_err());
}

#[test]
fn local_certifier_requires_complete_snapshot_and_independent_attestation() {
    let mut snapshot = complete_snapshot();
    snapshot["complete"] = json!(false);
    let projection = scope_certificates::canonical_author_projection("santa", &snapshot).unwrap();
    let attestation = attestation(&snapshot, &projection);
    assert_eq!(
        scope_certificates::certify_snapshot(
            "santa", &authors(), &inventory(), &snapshot, &attestation, rules_core::title_m2::RULE_VERSION
        )
        .unwrap_err(),
        "LOCAL_CERTIFIER_REQUIRES_COMPLETE_SNAPSHOT"
    );
    snapshot["complete"] = json!(true);
    let mut no_independent = attestation;
    no_independent.producer = "local-certifier".into();
    assert_eq!(
        scope_certificates::certify_snapshot(
            "santa", &authors(), &inventory(), &snapshot, &no_independent, rules_core::title_m2::RULE_VERSION
        )
        .unwrap_err(),
        "LOCAL_CERTIFIER_REQUIRES_EXTERNAL_ATTESTATION_ENVELOPE"
    );
}

#[test]
fn projection_mismatch_and_rule_version_fail_closed() {
    let snapshot = json!({
        "complete":true,
        "works":[{"work_id":"LOCAL_2","authors_confirmed":["santa"]}]
    });
    let valid_projection = scope_certificates::canonical_author_projection("santa", &complete_snapshot()).unwrap();
    let attestation = attestation(&snapshot, &valid_projection);
    assert_eq!(
        scope_certificates::certify_snapshot(
            "santa", &authors(), &inventory(), &snapshot, &attestation, rules_core::title_m2::RULE_VERSION
        )
        .unwrap_err(),
        "LOCAL_CERTIFIER_AUTHOR_SCOPE_PROJECTION_MISMATCH"
    );

    let record = valid_record();
    let mut document = scope_certificates::ScopeCertificateDocument {
        schema_version: 1,
        certificate_set_hash: scope_certificates::certificate_set_hash(std::slice::from_ref(&record)),
        certificates: vec![record],
    };
    document.certificates[0].rule_version = "obsolete-rule".into();
    document.certificate_set_hash = scope_certificates::certificate_set_hash(&document.certificates);
    assert_eq!(
        scope_certificates::validate_document(document, &authors(), &inventory()).unwrap_err(),
        "SCOPE_CERTIFICATE_RULE_VERSION_STALE"
    );
}
