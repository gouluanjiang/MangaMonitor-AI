use workbench_storage::{JmDownloadMetadata, Source};

#[test]
fn missing_version_date_preserves_legacy_metadata_bytes_and_accepts_known_canonical_dates() {
    let original =
        r#"{"workId":"123","title":"Synthetic","authors":[],"tags":[],"description":null}"#;
    let mut metadata: JmDownloadMetadata = serde_json::from_str(original).unwrap();
    assert_eq!(metadata.version_updated_at, None);
    assert_eq!(serde_json::to_string(&metadata).unwrap(), original);
    assert!(metadata.is_valid());
    for date in ["2026-09-15", "2026-09-15T12:34:56.000Z"] {
        metadata.version_updated_at = Some(date.into());
        assert!(metadata.is_valid());
        let loaded: JmDownloadMetadata =
            serde_json::from_slice(&serde_json::to_vec(&metadata).unwrap()).unwrap();
        assert_eq!(loaded, metadata);
    }
    metadata.work_id = "0123456789abcdef01234567".into();
    assert!(metadata.is_valid_for(Source::Pica));
}

#[test]
fn malformed_and_placeholder_version_dates_cannot_enter_task_metadata() {
    let mut metadata: JmDownloadMetadata = serde_json::from_str(
        r#"{"workId":"123","title":"Synthetic","authors":[],"tags":[],"description":null}"#,
    )
    .unwrap();
    for date in [
        "",
        "1970-01-01",
        "1970-01-01T00:00:00Z",
        "2026-02-30",
        "2026-09-15 12:00:00",
        "1750000000",
    ] {
        metadata.version_updated_at = Some(date.into());
        assert!(!metadata.is_valid(), "{date}");
    }
}
