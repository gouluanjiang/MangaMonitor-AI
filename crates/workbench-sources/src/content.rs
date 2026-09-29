//! Explicit category/tag evidence only; never inspect a title or author name.
use unicode_normalization::UnicodeNormalization;

fn normalized_label(tag: &str) -> String {
    tag.nfkc()
        .collect::<String>()
        .to_lowercase()
        .replace(['\u{2018}', '\u{2019}'], "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn is_bl_tag(tag: &str) -> bool {
    matches!(
        normalized_label(tag).as_str(),
        "bl" | "耽美"
            | "yaoi"
            | "boys love"
            | "boys' love"
            | "boy's love"
            | "boys-love"
            | "boys_love"
            | "boyslove"
            | "ボーイズラブ"
            | "男男"
            | "bl漫畫"
            | "bl漫画"
            | "bl漫"
            | "bl向"
            | "耽美漫畫"
            | "耽美漫画"
            | "耽美向"
            | "耽美花園"
            | "耽美花园"
    )
}

pub fn is_ai_tag(tag: &str) -> bool {
    // Only separators within a complete known label are optional, never substrings.
    matches!(
        normalized_label(tag).replace([' ', '_', '-'], "").as_str(),
        "ai" | "aigc"
            | "ai漫画"
            | "ai漫畫"
            | "ai作画"
            | "ai作畫"
            | "ai绘画"
            | "ai繪畫"
            | "ai绘图"
            | "ai繪圖"
            | "ai绘制"
            | "ai繪製"
            | "ai生成"
            | "ai生成漫画"
            | "ai生成漫畫"
            | "ai生成作品"
            | "aiart"
            | "aiartwork"
            | "aicomic"
            | "aicomics"
            | "aigenerated"
            | "aigeneratedart"
            | "aigeneratedcomic"
            | "aigeneratedcomics"
            | "aiイラスト"
            | "aiコミック"
            | "aiマンガ"
            | "ai絵"
    )
}

/// A JM source-scope label, not a global language/content exclusion. Callers
/// must also check Source::Jm before excluding a work on this evidence.
pub fn is_jm_english_category(tag: &str) -> bool {
    normalized_label(tag) == "english manga"
}

pub fn is_blocked_tag(tag: &str) -> bool {
    is_bl_tag(tag) || is_ai_tag(tag)
}

pub fn retained_content_tags(tags: &[String]) -> Vec<String> {
    let mut retained = crate::retained_language_tags(tags);
    for matches in [
        is_bl_tag as fn(&str) -> bool,
        is_ai_tag,
        is_jm_english_category,
    ] {
        if let Some(tag) = tags.iter().find(|tag| matches(tag)) {
            retained.push(tag.trim().to_owned());
        }
    }
    retained
}

pub fn inherit_content_tags(incoming: &[String], prior: &[String]) -> Vec<String> {
    let mut tags = crate::inherit_language_tags(incoming, prior);
    for matches in [
        is_bl_tag as fn(&str) -> bool,
        is_ai_tag,
        is_jm_english_category,
    ] {
        if tags.iter().any(|tag| matches(tag)) {
            continue;
        }
        if let Some(tag) = prior.iter().find(|tag| matches(tag)) {
            if tags.len() >= 128 {
                if let Some(index) = tags.iter().rposition(|tag| {
                    crate::language_tag_kind(tag).is_none()
                        && !is_blocked_tag(tag)
                        && !is_jm_english_category(tag)
                }) {
                    tags.remove(index);
                } else {
                    tags = retained_content_tags(&tags);
                }
            }
            tags.push(tag.trim().to_owned());
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_label_matrix_is_shared_with_frontend() {
        let matrix: serde_json::Value =
            serde_json::from_str(include_str!("content-labels.test.json")).unwrap();
        for (group, matches) in [("bl", is_bl_tag as fn(&str) -> bool), ("ai", is_ai_tag)] {
            for tag in matrix[group].as_array().unwrap() {
                let tag = tag.as_str().unwrap();
                assert!(matches(tag), "{tag}");
                assert!(is_blocked_tag(tag), "{tag}");
            }
        }
        for tag in matrix["visible"].as_array().unwrap() {
            let tag = tag.as_str().unwrap();
            assert!(!is_blocked_tag(tag), "{tag}");
        }
    }

    #[test]
    fn compact_and_lightweight_responses_preserve_bl_evidence() {
        let previous = vec![
            "中文".into(),
            "生肉".into(),
            "Tag".into(),
            "ＢＬ".into(),
            "Yaoi".into(),
        ];
        assert_eq!(retained_content_tags(&previous), ["中文", "生肉", "ＢＬ"]);
        assert_eq!(
            inherit_content_tags(&[], &previous),
            ["中文", "生肉", "ＢＬ"]
        );
        assert_eq!(
            inherit_content_tags(&["日文".into()], &previous),
            ["日文", "ＢＬ"]
        );
    }

    #[test]
    fn compact_and_saturated_responses_preserve_each_blocked_kind() {
        let previous = vec![
            "中文".into(),
            "生肉".into(),
            "耽美花園".into(),
            "AI作画".into(),
            "AI".into(),
        ];
        assert_eq!(
            retained_content_tags(&previous),
            ["中文", "生肉", "耽美花園", "AI作画"]
        );
        let incoming: Vec<_> = (0..128).map(|n| format!("ordinary{n}")).collect();
        let inherited = inherit_content_tags(&incoming, &previous);
        assert_eq!(inherited.len(), 128);
        assert!(inherited.iter().any(|tag| is_bl_tag(tag)));
        assert!(inherited.iter().any(|tag| is_ai_tag(tag)));
        let evidence_only = vec!["中文".into(); 128];
        let inherited = inherit_content_tags(&evidence_only, &previous);
        assert_eq!(inherited, ["中文", "耽美花園", "AI作画"]);
    }
}

#[cfg(test)]
mod english_scope_tests {
    use super::*;

    #[test]
    fn jm_scope_marker_is_exact_and_not_a_global_content_block() {
        for label in [
            "English Manga",
            "  ENGLISH   MANGA  ",
            "Ｅｎｇｌｉｓｈ　Ｍａｎｇａ",
        ] {
            assert!(is_jm_english_category(label));
            assert!(!is_blocked_tag(label));
        }
        for label in [
            "English",
            "英語",
            "Not English Manga",
            "English Manga extras",
        ] {
            assert!(!is_jm_english_category(label));
        }
        let old = vec!["中文".into(), "English Manga".into()];
        assert_eq!(retained_content_tags(&old), ["中文", "English Manga"]);
        assert_eq!(inherit_content_tags(&[], &old), ["中文", "English Manga"]);
        let full: Vec<_> = (0..128).map(|i| format!("tag{i}")).collect();
        let inherited = inherit_content_tags(&full, &old);
        assert_eq!(inherited.len(), 128);
        assert!(inherited.iter().any(|tag| is_jm_english_category(tag)));
    }
}
