//! Shared JM/Pica external naming. Source identity remains in the metadata.
use workbench_storage::JmDownloadMetadata;

fn comparable(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}
fn annotation(value: &str) -> bool {
    let value = value.to_lowercase();
    [
        "漢化",
        "汉化",
        "翻译",
        "翻譯",
        "翻訳",
        "个人整合",
        "個人整合",
    ]
    .iter()
    .any(|part| value.contains(part))
        || ["digital", "dl版", "sample", "見本", "无字", "無字"].contains(&value.as_str())
}
fn brackets(value: &str) -> impl Iterator<Item = (usize, usize, &str)> {
    value.match_indices('[').filter_map(|(start, _)| {
        let tail = &value[start + 1..];
        let end = tail.find(']')?;
        let content = &tail[..end];
        (!content.contains('[')).then_some((start, start + end + 2, content))
    })
}
fn strip_prefixes(mut title: &str) -> &str {
    loop {
        title = title.trim();
        if let Some((_, end, credit)) = brackets(title).next().filter(|v| v.0 == 0) {
            let normalized = credit.to_ascii_lowercase();
            let valid = normalized.strip_prefix("jm").is_some_and(|id| {
                let id = id.trim_start_matches([':', '-']);
                !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())
            }) || normalized.strip_prefix("pica").is_some_and(|id| {
                let id = id.trim_start_matches([':', '-']);
                id.len() == 24 && id.bytes().all(|b| b.is_ascii_hexdigit())
            });
            if valid {
                title = &title[end..];
                continue;
            }
        }
        if let Some(rest) = title.strip_prefix('(') {
            if let Some((event, rest)) = rest.split_once(')') {
                if event.strip_prefix(['C', 'c']).is_some_and(|n| {
                    (2..=4).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit())
                }) {
                    title = rest;
                    continue;
                }
            }
        }
        return title;
    }
}

pub(crate) fn zip_name(metadata: &JmDownloadMetadata) -> String {
    let title = strip_prefixes(&metadata.title);
    let mut authors = Vec::new();
    for author in &metadata.authors {
        if !author.trim().is_empty() && !authors.contains(&author.trim()) {
            authors.push(author.trim());
        }
    }
    let verified = brackets(title).find(|(_, _, credit)| {
        let credit = comparable(credit);
        authors.iter().any(|author| {
            let author = comparable(author);
            credit == author || credit.ends_with(&format!("({author})"))
        })
    });
    let literal = {
        let mut candidate = None;
        let mut offset = 0;
        for (start, end, credit) in brackets(title) {
            if !title[offset..start].trim().is_empty() {
                break;
            }
            if !annotation(credit) {
                candidate = Some((start, end, credit));
                break;
            }
            offset = end;
        }
        candidate
    };
    let name = if let Some((start, end, _)) = verified.or(literal) {
        let credit = &title[start..end];
        let remainder = format!("{} {}", &title[..start], &title[end..]);
        let mut remainder = remainder.trim();
        let mut annotations = Vec::new();
        while let Some((_, end, value)) = brackets(remainder).next().filter(|v| v.0 == 0) {
            if !annotation(value) {
                break;
            }
            annotations.push(remainder[..end].to_owned());
            remainder = remainder[end..].trim();
        }
        let mut parts = vec![credit.to_owned(), remainder.to_owned()];
        parts.extend(annotations);
        parts
            .into_iter()
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    } else if !authors.is_empty() {
        format!("[{}] {title}", authors.join("、"))
    } else {
        title.to_owned()
    };
    let name: String = name
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| match c {
            '<' => '＜',
            '>' => '＞',
            ':' => '：',
            '"' => '＂',
            '/' => '／',
            '\\' => '＼',
            '|' => '｜',
            '?' => '？',
            '*' => '＊',
            c => c,
        })
        .collect();
    let mut name = name.trim().trim_end_matches('.').trim_end().to_owned();
    let reserved = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if name.is_empty() {
        name = "作品".into();
    }
    if ["CON", "PRN", "AUX", "NUL"].contains(&reserved.as_str())
        || (reserved.len() == 4
            && (reserved.starts_with("COM") || reserved.starts_with("LPT"))
            && matches!(reserved.as_bytes()[3], b'1'..=b'9'))
    {
        name.insert(0, '_');
    }
    if name.encode_utf16().count() > 176 {
        let suffix = format!(" [{}]", &crate::hash(name.as_bytes())[..8]);
        let mut short = String::new();
        let mut length = suffix.encode_utf16().count();
        for c in name.chars() {
            if length + c.len_utf16() > 176 {
                break;
            }
            short.push(c);
            length += c.len_utf16();
        }
        name = short.trim_end().to_owned() + &suffix;
    }
    name + ".zip"
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item(title: &str, authors: &[&str]) -> JmDownloadMetadata {
        JmDownloadMetadata {
            work_id: "123".into(),
            title: title.into(),
            authors: authors.iter().map(|v| (*v).into()).collect(),
            tags: vec![],
            description: None,
        }
    }
    #[test]
    fn common_naming_keeps_creator_title_and_translation_edition() {
        let value = item(
            "(C106) [翻译组] [Digital] [Circle (Author)] Example [中国翻訳] [DL版]",
            &["Author"],
        );
        assert_eq!(
            zip_name(&value),
            "[Circle (Author)] Example [中国翻訳] [DL版] [翻译组] [Digital].zip"
        );
        let mut pica = value.clone();
        pica.work_id = "0123456789abcdef01234567".into();
        assert_eq!(zip_name(&value), zip_name(&pica));
        assert_eq!(
            value.title,
            "(C106) [翻译组] [Digital] [Circle (Author)] Example [中国翻訳] [DL版]"
        );
    }
    #[test]
    fn source_prefix_is_removed_and_missing_creator_is_not_invented() {
        assert_eq!(
            zip_name(&item(
                "[Pica0123456789abcdef01234567] (C107) Title",
                &["Artist"]
            )),
            "[Artist] Title.zip"
        );
        assert_eq!(
            zip_name(&item("[JM123] (C106) Title [DL版]", &[])),
            "Title [DL版].zip"
        );
        assert_eq!(
            zip_name(&item("(Original) Title", &[])),
            "(Original) Title.zip"
        );
    }
    #[test]
    fn external_name_is_safe_without_losing_unicode_or_same_length_identity() {
        assert_eq!(
            zip_name(&item("Title: A/B?", &["Artist"])),
            "[Artist] Title： A／B？.zip"
        );
        assert_eq!(zip_name(&item("CON", &[])), "_CON.zip");
        let first = zip_name(&item(&format!("{}A", "長😀".repeat(120)), &["Artist"]));
        let second = zip_name(&item(&format!("{}B", "長😀".repeat(120)), &["Artist"]));
        assert!(first.encode_utf16().count() <= 180);
        assert_ne!(first, second);
    }
}
