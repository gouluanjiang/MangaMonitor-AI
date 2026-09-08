use rules_core::title_identity::{compare, parse_title, Comparison, ContentType};
use serde_json::Value;

fn fixture(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/matcher-m1")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn adversarial(id: &str) {
    let cases = fixture("adversarial-cases.json");
    let c = cases
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap();
    let b_type = match c["type_b"].as_str() {
        Some("unknown") => None,
        Some(s) => Some(serde_json::from_value::<ContentType>(Value::String(s.into())).unwrap()),
        None => Some(ContentType::Manga),
    };
    let a = parse_title(c["a"].as_str(), Some(ContentType::Manga));
    let b = parse_title(c["b"].as_str(), b_type);
    assert_ne!(compare(&a, &b), Comparison::ExactRepresentation, "{id}");
    assert_eq!(compare(&a, &b), compare(&b, &a), "symmetric {id}");
}
macro_rules! negative { ($($id:ident),+ $(,)?) => { $(#[test] fn $id() { adversarial(stringify!($id)); })+ }; }
negative!(
    number_2_3,
    number_3_4,
    front_back,
    upper_lower,
    episode_collection,
    extra_main,
    bonus_main,
    after_story_main,
    manga_cg,
    manga_artbook,
    manga_novel,
    manga_settings,
    low_information_same_author,
    part,
    volume,
    chapter,
    episode,
    range,
    bracket_extra,
    subtitle,
    unsafe_glyph,
    unknown_type,
    inline_metadata,
    generic_brackets
);

#[test]
fn real_samples_keep_hand_reviewed_identity_evidence() {
    let cases = fixture("real-parser-cases.json");
    assert_eq!(cases.as_array().unwrap().len(), 15);
    for c in cases.as_array().unwrap() {
        let p = parse_title(c["raw"].as_str(), Some(ContentType::Manga));
        assert_eq!(p.raw.as_deref(), c["raw"].as_str());
        assert!(
            p.core_text.contains(c["preserved"].as_str().unwrap()),
            "{}: {}",
            c["source_key"],
            p.core_text
        );
        if let Some(kind) = c["kind"].as_str().filter(|s| !s.is_empty()) {
            assert!(
                p.structure.iter().any(|t| t.kind == kind),
                "{} {kind}",
                c["source_key"]
            );
        }
        if let Some(issue) = c["issue"].as_str().filter(|s| !s.is_empty()) {
            assert!(
                p.issues.iter().any(|s| s == issue),
                "{} {issue}",
                c["source_key"]
            );
        }
        for t in &p.structure {
            assert_eq!(p.core_text[t.start..t.end], t.lexeme);
        }
    }
}

#[test]
fn corpus_is_complete_and_fixtures_are_real_unmodified_source_titles() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source: Value = serde_json::from_slice(
        &std::fs::read(root.join("evidence/phase3b-run-33998989019/review-export.json")).unwrap(),
    )
    .unwrap();
    let audit = fixture("evidence-analysis.json");
    let rows = audit["items"].as_array().unwrap();
    assert_eq!(rows.len(), 181);
    let ids: std::collections::BTreeSet<_> = rows
        .iter()
        .map(|r| r["source_key"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 181);
    for r in rows {
        let s = source["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["source_key"] == r["source_key"])
            .unwrap();
        assert_eq!(s["reason"], "UNRESOLVED_TITLE_IDENTITY");
        assert_eq!(s["raw_title"], r["raw_title"]);
        let p = parse_title(r["raw_title"].as_str(), None);
        assert_eq!(p.raw.as_deref(), r["raw_title"].as_str());
        assert_eq!(compare(&p, &p), Comparison::NeedsReview);
        for t in &p.structure {
            assert_eq!(p.core_text[t.start..t.end], t.lexeme);
        }
    }
    for c in fixture("real-parser-cases.json").as_array().unwrap() {
        let s = rows
            .iter()
            .find(|s| s["source_key"] == c["source_key"])
            .unwrap();
        assert_eq!(c["raw"], s["raw_title"]);
    }
    for (group, count) in [
        ("NULL_LOCAL_PRIMARY", 17),
        ("MALFORMED_BRACKETS", 5),
        ("IDENTITY_SENSITIVE_OR_LOW_INFORMATION", 9),
        ("LOCAL_PRIMARY_LITERAL_WITH_RESIDUAL", 21),
        ("LOCAL_REPRESENTATION_DIFFERENCE", 10),
        ("NO_LITERAL_LOCAL_PRIMARY_WITNESS", 119),
    ] {
        assert_eq!(
            rows.iter().filter(|r| r["primary_group"] == group).count(),
            count
        );
    }
}

#[test]
fn normalization_is_nfkc_casefold_only_and_metadata_is_audited() {
    let a = parse_title(
        Some("[Chinese] ＡＢＣＤ １２ [DL版]"),
        Some(ContentType::Manga),
    );
    let b = parse_title(Some("abcd 12"), Some(ContentType::Manga));
    assert_eq!(a.core_text, "abcd 12");
    assert_eq!(a.metadata, ["[chinese]", "[dl版]"]);
    assert_eq!(compare(&a, &b), Comparison::ExactRepresentation);
    assert_eq!(
        parse_title(Some("か\u{3099}くせい"), None).core_text,
        "がくせい"
    );
    assert_eq!(parse_title(Some("Straße"), None).core_text, "strasse");
    for raw in [
        "作品 3–4",
        "作品 Part 3",
        "作品 前篇",
        "作品 上",
        "作品 Extra",
        "作品 総集編",
        "はるこす++",
        "作品 II",
    ] {
        let p = parse_title(Some(raw), None);
        assert_eq!(p.core_text, rules_core::conservative_title(raw));
    }
}

#[test]
fn missing_primary_and_unknown_types_never_become_false_or_exact() {
    let p = parse_title(None, None);
    assert_eq!(p.raw, None);
    assert_eq!(p.content_type, None);
    assert!(p.issues.iter().any(|s| s == "MISSING_TITLE"));
    assert_eq!(compare(&p, &p), Comparison::NeedsReview);
}

#[test]
fn number_tokens_have_no_inferred_unit_or_coverage() {
    let p = parse_title(
        Some("作品 95式 ２年G組１學期 01 9999999999999999999999999 1–5"),
        None,
    );
    let numbers: Vec<_> = p
        .structure
        .iter()
        .filter(|t| t.kind == "number_unclassified")
        .map(|t| t.lexeme.as_str())
        .collect();
    assert_eq!(
        numbers,
        ["95", "2", "1", "01", "9999999999999999999999999", "1", "5"]
    );
    assert!(p
        .structure
        .iter()
        .any(|t| t.kind == "range_unclassified" && t.lexeme == "1–5"));
}

#[test]
fn unapproved_wrappers_and_glyphs_are_preserved() {
    for raw in [
        "(C97) 作品标题",
        "[社团 (作者)] 作品标题",
        "[汉化组] 作品标题",
        "作品标题 (COMIC 2026)",
        "作品标题 (原神)",
        "作品标题 溫泉編",
        "作品标题 里",
    ] {
        let p = parse_title(Some(raw), Some(ContentType::Manga));
        assert_eq!(p.core_text, rules_core::conservative_title(raw));
        assert!(p.metadata.is_empty());
    }
}
