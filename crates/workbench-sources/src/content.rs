//! Explicit category/tag evidence only; never inspect a title or author name.
use unicode_normalization::UnicodeNormalization;

pub fn is_bl_tag(tag: &str) -> bool {
    let normalized = tag.nfkc().collect::<String>().to_lowercase()
        .replace(['\u{2018}', '\u{2019}'], "'")
        .split_whitespace().collect::<Vec<_>>().join(" ");
    matches!(normalized.as_str(),
        "bl" | "耽美" | "yaoi" | "boys love" | "boys' love" | "boy's love"
        | "boys-love" | "boys_love" | "boyslove" | "ボーイズラブ" | "男男"
        | "bl漫畫" | "bl漫画" | "bl漫" | "bl向" | "耽美漫畫" | "耽美漫画" | "耽美向")
}

pub fn retained_content_tags(tags: &[String]) -> Vec<String> {
    let mut retained = crate::retained_language_tags(tags);
    if let Some(tag) = tags.iter().find(|tag| is_bl_tag(tag)) {
        retained.push(tag.trim().to_owned());
    }
    retained
}

pub fn inherit_content_tags(incoming: &[String], prior: &[String]) -> Vec<String> {
    let mut tags = crate::inherit_language_tags(incoming, prior);
    if !tags.iter().any(|tag| is_bl_tag(tag)) && tags.len() < 128 {
        if let Some(tag) = prior.iter().find(|tag| is_bl_tag(tag)) {
            tags.push(tag.trim().to_owned());
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_labels_only_and_width_normalization() {
        for tag in ["BL", "ＢＬ", " 耽美 ", "YaOi", "Boys’ Love", "ＢＬ漫畫", "ボーイズラブ"] {
            assert!(is_bl_tag(tag), "{tag}");
        }
        for tag in ["", "非BL", "非ＢＬ", "BLではない", "GL", "black", "blonde", "bl artist", "百合"] {
            assert!(!is_bl_tag(tag), "{tag}");
        }
    }

    #[test]
    fn compact_and_lightweight_responses_preserve_bl_evidence() {
        let previous = vec!["中文".into(), "生肉".into(), "Tag".into(), "ＢＬ".into(), "Yaoi".into()];
        assert_eq!(retained_content_tags(&previous), ["中文", "生肉", "ＢＬ"]);
        assert_eq!(inherit_content_tags(&[], &previous), ["中文", "生肉", "ＢＬ"]);
        assert_eq!(inherit_content_tags(&["日文".into()], &previous), ["日文", "ＢＬ"]);
    }
}
