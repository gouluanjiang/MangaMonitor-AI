use rules_core::title_m2::*;
use serde_json::{json, Value};
fn p(s: &str) -> Identity {
    parse(Some(s), Some("Writer"), None, Some("manga"))
}
fn field(s: &str, name: &str, expected: Value) {
    let id = p(s);
    assert!(id.issues.is_empty(), "{}: {:?}", s, id.issues);
    assert_eq!(serde_json::to_value(&id.fields).unwrap()[name], expected);
    for e in &id.evidence {
        if let Some([a, b]) = e.span {
            assert_eq!(&id.normalized[a..b], e.text);
        }
    }
}
macro_rules! scalar {
    ($test:ident,$s:literal,$f:literal,$v:literal) => {
        #[test]
        fn $test() {
            field($s, $f, json!($v));
        }
    };
}
macro_rules! number {($test:ident,$s:literal,$f:literal,$v:literal)=>{#[test]fn $test(){field($s,$f,json!({"first":$v,"last":null}));}};}
number!(series_boundary, "作品标题 3", "series_number", "3");
number!(
    series_closed_attached,
    "くーねるすまた3",
    "series_number",
    "3"
);
number!(volume_boundary, "作品标题 第3卷", "volume", "3");
number!(volume_english, "作品标题 Volume 3", "volume", "3");
number!(episode_cjk, "作品标题第3話", "episode", "3");
number!(episode_english, "作品标题 episode 3", "episode", "3");
number!(chapter_english, "作品标题 chapter 3", "chapter", "3");
number!(part_bracket, "作品标题 [Part 3]", "part", "3");
scalar!(front_field, "作品标题 前篇", "front_back", "front");
scalar!(back_closed_alias, "作品标题 後篇", "front_back", "back");
scalar!(upper_boundary, "作品标题 上", "upper_lower", "upper");
scalar!(lower_bracket, "作品标题 [下篇]", "upper_lower", "lower");
scalar!(extra_boundary, "作品标题 Extra", "extra", "extra");
scalar!(bonus_boundary, "作品标题 bonus", "extra", "bonus");
scalar!(
    after_story_boundary,
    "作品标题 after-story",
    "extra",
    "after-story"
);
scalar!(
    collection_boundary,
    "作品标题 総集編",
    "collection",
    "collection"
);
scalar!(
    subtitle_preserved,
    "作品标题 ―人物副题―",
    "subtitle",
    "人物副题"
);
#[test]
fn range_has_explicit_unit_and_provenance() {
    field(
        "作品标题 第1–5話",
        "episode",
        json!({"first":"1","last":"5"}),
    );
}
#[test]
fn ordinary_body_characters_do_not_create_structure() {
    for text in [
        "上海下雨的第一天",
        "世界第一个数字95式",
        "２年G組１學期!!",
        "123号街道的故事",
        "作品标题 2026夏",
        "作品标题 2026",
        "上上下下的日常",
        "Haruhi Lingerie",
        "Novelty Adventure",
    ] {
        let a = p(text);
        assert_eq!(a.fields.series_number, None, "{text}");
        assert_eq!(a.fields.volume, None);
        assert_eq!(a.fields.episode, None);
        assert_eq!(a.fields.upper_lower, None);
        assert_eq!(a.fields.content_type.as_deref(), Some("manga"));
        assert_eq!(a.core, rules_core::conservative_title(text));
    }
}
#[test]
fn all_m1_adversarial_cases_remain_nonmatches_in_m2() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../fixtures/matcher-m1/adversarial-cases.json"
    ))
    .unwrap();
    for c in cases.as_array().unwrap() {
        let a = p(c["a"].as_str().unwrap());
        let b = parse(
            c["b"].as_str(),
            Some("Writer"),
            None,
            match c["type_b"].as_str() {
                Some("unknown") => None,
                Some(s) => Some(s),
                None => Some("manga"),
            },
        );
        assert_ne!(compare(&a, &b), Relation::Exact, "{}", c["id"]);
    }
}
#[test]
fn closed_wrappers_have_traceable_spans() {
    let a=parse(Some("[空氣系☆漢化] (C105) [HoneyRoad(Bee導師)] 配達バニーガールとサービスえっち2 [中國翻譯] [DL版]"),Some("Bee導師"),None,Some("manga"));
    let b = parse(
        Some("配達バニーガールとサービスえっち 2"),
        Some("Bee導師"),
        None,
        Some("manga"),
    );
    assert_eq!(compare(&a, &b), Relation::Exact);
    assert_eq!(
        a.evidence.iter().filter(|e| e.field == "metadata").count(),
        5
    );
    for e in a.evidence {
        assert!(!e.reference.is_empty());
        if let Some([x, y]) = e.span {
            assert_eq!(a.normalized[x..y], e.text);
        }
    }
}
#[test]
fn wrappers_are_never_substring_or_interior_permissions() {
    for raw in [
        "作品标题 (C97)",
        "作品标题 [DL版] 残余",
        "[不是空氣系☆漢化] 作品标题",
        "[假的漢化组] 作品标题",
        "[wrong (Writer)] 作品标题",
        "[HoneyRoad (someone else)] 作品标题",
        "(C97 extra) 作品标题",
        "(COMIC 99) 作品标题",
        "作品标题 [Part 3 bonus]",
        "作品标题 [重要正文]",
        "[中国翻译 第3话] 作品标题",
    ] {
        let a = p(raw);
        assert!(!a.issues.is_empty(), "{raw}");
        assert_ne!(compare(&a, &p("作品标题")), Relation::Exact);
    }
}
#[test]
fn creator_pair_requires_confirmed_exact_author() {
    let a = parse(
        Some("[Hello Girls! (10駅)] 作品标题"),
        Some("10驛"),
        None,
        Some("manga"),
    );
    assert!(a.issues.contains("UNINTERPRETED_ANNOTATION"));
    let b = parse(
        Some("[HoneyRoad (Bee導師)] 作品标题"),
        None,
        None,
        Some("manga"),
    );
    assert!(!b.issues.is_empty());
}
#[test]
fn no_global_glyph_or_fandom_aliases() {
    for (a, b) in [
        ("作品标题 编", "作品标题 編"),
        ("作品标题 里", "作品标题 裏"),
        ("作品标题 10駅", "作品标题 10驛"),
    ] {
        assert_ne!(compare(&p(a), &p(b)), Relation::Exact);
    }
    let a = parse(
        Some("作品标题 (Fate/stay night)"),
        Some("Writer"),
        None,
        Some("manga"),
    );
    let b = parse(
        Some("作品标题"),
        Some("Writer"),
        Some("Fate stay night"),
        Some("manga"),
    );
    assert_ne!(compare(&a, &b), Relation::Exact);
}
#[test]
fn known_fandom_is_a_field_not_discarded_text() {
    let a = p("作品标题 (原神)");
    assert_eq!(a.fields.fandom.as_deref(), Some("原神"));
    assert_ne!(compare(&a, &p("作品标题")), Relation::Exact);
    let b = parse(
        Some("作品标题"),
        Some("Writer"),
        Some("原神"),
        Some("manga"),
    );
    assert_eq!(compare(&a, &b), Relation::Exact);
}
#[test]
fn malformed_and_bilingual_are_never_exact() {
    for s in [
        "作品标题 [",
        "作品标题 (原神))",
        "[作品标题)",
        "作品标题 | translated name",
        "作品标题 丨译名",
    ] {
        let a = p(s);
        assert_ne!(compare(&a, &a), Relation::Exact);
    }
}
#[test]
fn duplicate_fields_and_type_conflicts_fail_closed() {
    for s in [
        "作品标题 Part 2 Part 3",
        "作品标题 [CG] [manga]",
        "作品标题 上 下",
    ] {
        assert!(!p(s).issues.is_empty(), "{s}");
    }
}
#[test]
fn unknown_type_and_numeric_only_titles_are_review() {
    let a = parse(Some("作品标题"), None, None, None);
    assert_eq!(a.fields.content_type, None);
    assert_eq!(compare(&a, &a), Relation::Insufficient);
    assert!(!p("123456").issues.is_empty());
}
#[test]
fn omitted_and_explicit_structure_never_collapse() {
    for s in [
        "作品标题 2",
        "作品标题 第3話",
        "作品标题 Part 2",
        "作品标题 前篇",
        "作品标题 上",
        "作品标题 Extra",
        "作品标题 总集篇",
    ] {
        assert_ne!(compare(&p(s), &p("作品标题")), Relation::Exact);
    }
}
#[test]
fn typed_range_is_not_episode_or_collection() {
    let a = p("作品标题 第1-5話");
    assert_ne!(compare(&a, &p("作品标题 第3話")), Relation::Exact);
    assert_ne!(compare(&a, &p("作品标题 第1-5話 总集篇")), Relation::Exact);
}
#[test]
fn nfkc_casefold_keep_provenance_and_leading_zero_distinction() {
    let a = p("ＡＢＣＤ Part ３");
    let b = p("abcd part 3");
    assert_eq!(compare(&a, &b), Relation::Exact);
    assert_ne!(
        compare(&p("作品标题 03"), &p("作品标题 3")),
        Relation::Exact
    );
}
