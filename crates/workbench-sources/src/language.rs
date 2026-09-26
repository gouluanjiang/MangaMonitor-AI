/// Explicit source language labels only. `Untranslated` also includes the
/// source's literal 生肉 label, which does not establish Japanese as its language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanguageTagKind {
    Chinese,
    Untranslated,
}

pub fn language_tag_kind(tag: &str) -> Option<LanguageTagKind> {
    let tag = tag.trim();
    if matches!(
        tag,
        "中文"
            | "汉化"
            | "漢化"
            | "简体中文"
            | "簡體中文"
            | "繁体中文"
            | "繁體中文"
            | "中国語"
            | "中國語"
    ) || tag.eq_ignore_ascii_case("chinese")
    {
        Some(LanguageTagKind::Chinese)
    } else if matches!(tag, "日文" | "日语" | "日語" | "日本語" | "生肉")
        || tag.eq_ignore_ascii_case("japanese")
    {
        Some(LanguageTagKind::Untranslated)
    } else {
        None
    }
}

/// Keep the first explicit label for each kind, preserving conflicting evidence.
pub fn retained_language_tags(tags: &[String]) -> Vec<String> {
    let mut retained = Vec::with_capacity(2);
    let mut kinds = Vec::with_capacity(2);
    for tag in tags {
        if let Some(kind) = language_tag_kind(tag).filter(|kind| !kinds.contains(kind)) {
            kinds.push(kind);
            retained.push(tag.trim().to_owned());
        }
    }
    retained
}

/// A fresh explicit label, including a conflict, replaces historical language
/// evidence. Otherwise only the prior language labels supplement current tags.
pub fn inherit_language_tags(incoming: &[String], prior: &[String]) -> Vec<String> {
    let mut tags = incoming.to_vec();
    if !incoming.iter().any(|tag| language_tag_kind(tag).is_some()) {
        let retained = retained_language_tags(prior);
        // Do not keep only one side when an older projection leaves no room.
        if tags.len().saturating_add(retained.len()) <= 66 {
            tags.extend(retained);
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn only_exact_language_labels_are_evidence() {
        for tag in [
            "中文",
            "汉化",
            "漢化",
            "简体中文",
            "簡體中文",
            "繁体中文",
            "繁體中文",
            "中国語",
            "中國語",
            " ChInEsE ",
        ] {
            assert_eq!(language_tag_kind(tag), Some(LanguageTagKind::Chinese));
        }
        for tag in ["日文", "日语", "日語", "日本語", "生肉", " JaPaNeSe "] {
            assert_eq!(language_tag_kind(tag), Some(LanguageTagKind::Untranslated));
        }
        for tag in [
            "",
            "日漫",
            "中文标题",
            "漢化組",
            "未汉化",
            "非生肉",
            "英語 ENG",
            "Author",
        ] {
            assert_eq!(language_tag_kind(tag), None, "{tag}");
        }
    }

    #[test]
    fn retention_keeps_both_conflicting_kinds_without_unrelated_metadata() {
        assert_eq!(
            retained_language_tags(&tags(
                &["Tag", " 中文 ", "漢化", "生肉", "日文", "chinese",]
            )),
            ["中文", "生肉"]
        );
    }

    #[test]
    fn fresh_language_wins_and_missing_language_inherits_at_most_two_labels() {
        let prior = tags(&["Old tag", "中文", "漢化", "生肉", "日文"]);
        for fresh in [tags(&["New tag", "日文"]), tags(&["中文", "生肉"])] {
            assert_eq!(inherit_language_tags(&fresh, &prior), fresh);
        }
        let incoming = (0..64).map(|i| format!("Tag {i}")).collect::<Vec<_>>();
        let inherited = inherit_language_tags(&incoming, &prior);
        assert_eq!(&inherited[..64], incoming);
        assert_eq!(&inherited[64..], ["中文", "生肉"]);
        assert_eq!(inherited.len(), 66);
        assert_eq!(inherit_language_tags(&inherited, &prior), inherited);
        let full = vec!["Other".into(); 66];
        assert_eq!(inherit_language_tags(&full, &prior), full);
    }
}
