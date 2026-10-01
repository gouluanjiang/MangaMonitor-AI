//! Synthetic metadata only: no account, source request, media, or user files.
use workbench_storage::{work_date_is_valid, JmDownloadMetadata, LibraryReference, Source};

fn metadata(source: Source) -> JmDownloadMetadata {
    JmDownloadMetadata {
        work_id: match source {
            Source::Jm => "123456".into(),
            Source::Pica => "0123456789abcdef01234567".into(),
        },
        title: "Synthetic example".into(),
        authors: vec!["Synthetic author".into()],
        tags: vec!["Synthetic tag".into()],
        description: Some("Synthetic description\nwith a second line\tand a tab".into()),
        version_updated_at: None,
    }
}

// Freeze the pre-change rules independently, so diagnostic work cannot silently
// relax admission, trim/rewrite a field, or change a saved task's binding bytes.
fn prior_admission(m: &JmDownloadMetadata, source: Source) -> bool {
    let text = |v: &str, n| {
        !v.trim().is_empty() && v.chars().count() <= n && !v.chars().any(char::is_control)
    };
    LibraryReference {
        source,
        work_id: m.work_id.clone(),
    }
    .is_valid()
        && (source != Source::Jm || m.work_id.parse::<i64>().is_ok_and(|id| id > 0))
        && text(&m.title, 500)
        && m.authors.len() <= 100
        && m.authors.iter().all(|v| text(v, 200))
        && m.tags.len() <= 200
        && m.tags.iter().all(|v| text(v, 200))
        && m.version_updated_at
            .as_deref()
            .is_none_or(work_date_is_valid)
        && m.description.as_ref().is_none_or(|v| {
            v.len() <= 32768
                && !v
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        })
}

#[test]
fn rejected_fields_get_redacted_stable_codes_without_changing_admission_or_bytes() {
    type Change = (fn(&mut JmDownloadMetadata), &'static str);
    let cases: &[Change] = &[
        (
            |m| m.work_id = "private-invalid-id".into(),
            "DOWNLOAD_METADATA_ID_INVALID",
        ),
        (
            |m| m.title = " \t\n".into(),
            "DOWNLOAD_METADATA_TITLE_EMPTY",
        ),
        (
            |m| m.title = "界".repeat(501),
            "DOWNLOAD_METADATA_TITLE_TOO_LONG",
        ),
        (
            |m| m.title = "private-title\nline".into(),
            "DOWNLOAD_METADATA_TITLE_CONTROL",
        ),
        (
            |m| m.authors = vec!["A".into(); 101],
            "DOWNLOAD_METADATA_AUTHORS_TOO_MANY",
        ),
        (
            |m| m.authors = vec![" ".into()],
            "DOWNLOAD_METADATA_AUTHOR_EMPTY",
        ),
        (
            |m| m.authors = vec!["🧪".repeat(201)],
            "DOWNLOAD_METADATA_AUTHOR_TOO_LONG",
        ),
        (
            |m| m.authors = vec!["private-author\u{85}".into()],
            "DOWNLOAD_METADATA_AUTHOR_CONTROL",
        ),
        (
            |m| m.tags = vec!["tag".into(); 201],
            "DOWNLOAD_METADATA_TAGS_TOO_MANY",
        ),
        (|m| m.tags = vec!["".into()], "DOWNLOAD_METADATA_TAG_EMPTY"),
        (
            |m| m.tags = vec!["t".repeat(201)],
            "DOWNLOAD_METADATA_TAG_TOO_LONG",
        ),
        (
            |m| m.tags = vec!["private-tag\0".into()],
            "DOWNLOAD_METADATA_TAG_CONTROL",
        ),
        (
            |m| m.version_updated_at = Some("2026-02-30".into()),
            "DOWNLOAD_METADATA_DATE_INVALID",
        ),
        (
            |m| m.description = Some("🧪".repeat(8193)),
            "DOWNLOAD_METADATA_DESCRIPTION_TOO_LONG",
        ),
        (
            |m| m.description = Some("private-description\u{b}".into()),
            "DOWNLOAD_METADATA_DESCRIPTION_CONTROL",
        ),
    ];
    for source in [Source::Jm, Source::Pica] {
        for (change, code) in cases {
            let mut m = metadata(source);
            change(&mut m);
            let before = serde_json::to_vec(&m).unwrap();
            assert!(!prior_admission(&m, source));
            assert!(!m.is_valid_for(source));
            let error = m.validate_for(source).unwrap_err();
            assert_eq!(error.code, *code);
            assert_eq!(
                serde_json::to_value(error).unwrap(),
                serde_json::json!({"code":code})
            );
            assert_eq!(serde_json::to_vec(&m).unwrap(), before);
        }
    }
}

#[test]
fn unicode_boundaries_optional_fields_and_source_identity_keep_the_original_rules() {
    for source in [Source::Jm, Source::Pica] {
        let mut m = metadata(source);
        m.title = "🧪".repeat(500);
        m.authors = vec!["界".repeat(200); 100];
        m.tags = vec!["🧪".repeat(200); 200];
        m.description = Some("🧪".repeat(8192));
        m.version_updated_at = Some("2026-10-01T00:00:00.000Z".into());
        assert!(prior_admission(&m, source));
        assert!(m.validate_for(source).is_ok());
        m.authors.clear();
        m.tags.clear();
        m.description = None;
        m.version_updated_at = None;
        assert!(m.validate_for(source).is_ok());
        assert!(serde_json::to_value(&m)
            .unwrap()
            .get("versionUpdatedAt")
            .is_none());
        assert!(!m.is_valid_for(if source == Source::Jm {
            Source::Pica
        } else {
            Source::Jm
        }));
        for c in (0..=159).filter_map(char::from_u32) {
            m.title = format!("Synthetic{c}title");
            assert_eq!(m.is_valid_for(source), prior_admission(&m, source));
        }
    }
    let mut jm = metadata(Source::Jm);
    jm.work_id = i64::MAX.to_string();
    assert!(jm.validate_for(Source::Jm).is_ok());
    jm.work_id = (i64::MAX as u64 + 1).to_string();
    assert_eq!(
        jm.validate_for(Source::Jm).unwrap_err().code,
        "DOWNLOAD_METADATA_ID_INVALID"
    );
}

#[test]
fn new_downloads_normalize_only_whitespace_controls_in_tags() {
    for source in [Source::Jm, Source::Pica] {
        for c in ['\t', '\n', '\u{b}', '\u{c}', '\r', '\u{85}'] {
            let mut original = metadata(source);
            original.tags = vec![
                format!(" {c}Synthetic{c} tag{c} "),
                " Keep  spacing ".into(),
            ];
            let original_bytes = serde_json::to_vec(&original).unwrap();
            assert_eq!(
                original.validate_for(source).unwrap_err().code,
                "DOWNLOAD_METADATA_TAG_CONTROL"
            );
            let prepared = original.clone().for_new_download(source).unwrap();
            let mut expected = original.clone();
            expected.tags[0] = "Synthetic tag".into();
            assert_eq!(prepared, expected);
            assert!(prepared.validate_for(source).is_ok());
            assert_eq!(prepared.clone().for_new_download(source).unwrap(), prepared);
            assert_eq!(serde_json::to_vec(&original).unwrap(), original_bytes);
        }
        let mut valid = metadata(source);
        valid.tags = vec![" Unchanged  spacing ".into(), "Unicode　space".into()];
        assert_eq!(valid.clone().for_new_download(source).unwrap(), valid);
    }
}

#[test]
fn tag_normalization_keeps_non_whitespace_controls_and_raw_bounds_rejected() {
    for source in [Source::Jm, Source::Pica] {
        for c in (0..=159)
            .filter_map(char::from_u32)
            .filter(|c| c.is_control() && !c.is_whitespace())
        {
            let mut m = metadata(source);
            m.tags = vec![format!("Synthetic\n{c}tag")];
            assert_eq!(
                m.for_new_download(source).unwrap_err().code,
                "DOWNLOAD_METADATA_TAG_CONTROL"
            );
        }
        let mut m = metadata(source);
        m.tags = vec!["tag\n".into(), format!("tag{}", "\t".repeat(198))];
        assert_eq!(
            m.for_new_download(source).unwrap_err().code,
            "DOWNLOAD_METADATA_TAG_TOO_LONG"
        );
        let mut m = metadata(source);
        m.tags = vec!["tag\n".into(), " \t\r\n".into()];
        assert_eq!(
            m.for_new_download(source).unwrap_err().code,
            "DOWNLOAD_METADATA_TAG_EMPTY"
        );
        let mut m = metadata(source);
        m.tags = vec!["tag\n".into(); 201];
        assert_eq!(
            m.for_new_download(source).unwrap_err().code,
            "DOWNLOAD_METADATA_TAGS_TOO_MANY"
        );
        let mut m = metadata(source);
        m.tags = vec!["tag\r\n".into()];
        m.title = "Synthetic\ntitle".into();
        assert_eq!(
            m.for_new_download(source).unwrap_err().code,
            "DOWNLOAD_METADATA_TITLE_CONTROL"
        );
        let mut m = metadata(source);
        m.tags = vec!["tag\n".into()];
        m.work_id = "not-an-id".into();
        assert_eq!(
            m.for_new_download(source).unwrap_err().code,
            "DOWNLOAD_METADATA_ID_INVALID"
        );
        let mut m = metadata(source);
        m.tags = vec!["tag\n".into()];
        m.description = Some("Synthetic\0description".into());
        assert_eq!(
            m.for_new_download(source).unwrap_err().code,
            "DOWNLOAD_METADATA_DESCRIPTION_CONTROL"
        );
    }
}
