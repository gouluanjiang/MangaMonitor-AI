use cloud_monitor::{monitor::hash, scope_certificates};
use serde_json::json;

fn authors() -> serde_json::Value {
    json!({"schema_version":1,"authors":[{"author_id":"AUTHOR_0043","name":"santa","enabled":true}]})
}

fn inventory() -> serde_json::Value {
    json!({"schema_version":8,"works":[]})
}

fn valid_record() -> scope_certificates::ScopeCertificateRecord {
    let snapshot = json!({"complete":true,"works":[{"work_id":"LOCAL_1"}]});
    let projection = json!([{"work_id":"LOCAL_1"}]);
    let attestation = scope_certificates::CompletenessAttestation {
        snapshot_hash: hash(&snapshot),
        inventory_hash: hash(&inventory()),
        projection_hash: hash(&projection),
        projection_count: 1,
        producer: "independent-fixture".into(),
        attestation_hash: hash(&(
            hash(&snapshot),
            hash(&inventory()),
            hash(&projection),
            1_u64,
            "independent-fixture",
        )),
    };
    scope_certificates::certify_snapshot(
        "santa",
        &authors(),
        &inventory(),
        &snapshot,
        &projection,
        &attestation,
        "title-m2-test",
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
    let mut snapshot = json!({"complete":false,"works":[]});
    let projection = json!([]);
    let attestation = scope_certificates::CompletenessAttestation {
        snapshot_hash: hash(&snapshot),
        inventory_hash: hash(&inventory()),
        projection_hash: hash(&projection),
        projection_count: 0,
        producer: "independent-fixture".into(),
        attestation_hash: hash(&(
            hash(&snapshot),
            hash(&inventory()),
            hash(&projection),
            0_u64,
            "independent-fixture",
        )),
    };
    assert_eq!(
        scope_certificates::certify_snapshot(
            "santa", &authors(), &inventory(), &snapshot, &projection, &attestation, "rule"
        )
        .unwrap_err(),
        "LOCAL_CERTIFIER_REQUIRES_COMPLETE_SNAPSHOT"
    );
    snapshot["complete"] = json!(true);
    let mut no_independent = attestation;
    no_independent.producer = "local-certifier".into();
    assert_eq!(
        scope_certificates::certify_snapshot(
            "santa", &authors(), &inventory(), &snapshot, &projection, &no_independent, "rule"
        )
        .unwrap_err(),
        "LOCAL_CERTIFIER_REQUIRES_INDEPENDENT_ATTESTATION"
    );
}
