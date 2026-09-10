use crate::{error, Result};
use quick_xml::{events::Event, Reader};
use std::collections::{HashMap, HashSet};
use workbench_storage::{LibraryEvidence, LibraryItem, LibraryReference, Source};

pub(crate) const MAX_METADATA_BYTES: usize = 256 * 1024;

#[derive(Default)]
pub(crate) struct Metadata {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub reference: Option<LibraryReference>,
    pub conflict: bool,
}

fn clean(value: &str, maximum: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(maximum)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn list(value: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    value
        .split([',', ';'])
        .map(|v| clean(v, 200))
        .filter(|v| !v.is_empty() && seen.insert(v.clone()))
        .take(50)
        .collect()
}

pub(crate) fn filename_reference(name: &str) -> (Option<LibraryReference>, bool) {
    let mut references = Vec::new();
    for part in name.split('[').skip(1) {
        let Some((token, _)) = part.split_once(']') else {
            continue;
        };
        // A hyphen is valid in Windows filenames; a colon is not. Retain the
        // existing token spelling for metadata supplied as plain text.
        let Some((source, id)) = token.split_once('-').or_else(|| token.split_once(':')) else {
            continue;
        };
        let source = match source {
            "JM" => Source::Jm,
            "Pica" => Source::Pica,
            _ => continue,
        };
        let reference = LibraryReference {
            source,
            work_id: id.to_ascii_lowercase(),
        };
        if reference.is_valid() {
            references.push(reference);
        }
    }
    let conflict = references
        .first()
        .is_some_and(|first| references.iter().any(|v| v != first));
    (
        if conflict {
            None
        } else {
            references.into_iter().next()
        },
        conflict,
    )
}

pub(crate) fn apply(item: &mut LibraryItem, metadata: Metadata) {
    if let Some(title) = metadata.title.filter(|v| !v.is_empty()) {
        item.title = title;
    }
    if !metadata.authors.is_empty() {
        item.authors = metadata.authors;
    }
    if metadata.description.is_some() {
        item.description = metadata.description;
    }
    if !metadata.tags.is_empty() {
        item.tags = metadata.tags;
    }
    if metadata.conflict
        || item
            .source_ref
            .as_ref()
            .zip(metadata.reference.as_ref())
            .is_some_and(|(a, b)| a != b)
    {
        item.source_ref = None;
        item.identity_evidence = None;
        item.error_code = Some("LIBRARY_IDENTITY_CONFLICT".into());
    } else if let Some(reference) = metadata.reference {
        if item.error_code.as_deref() != Some("LIBRARY_IDENTITY_CONFLICT") {
            item.source_ref = Some(reference);
            item.identity_evidence = Some(LibraryEvidence::Metadata);
        }
    }
}

pub(crate) fn downloader_json(bytes: &[u8]) -> Result<Metadata> {
    if bytes.len() > MAX_METADATA_BYTES {
        return Err(error("LIBRARY_METADATA_LIMIT"));
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| error("LIBRARY_METADATA_INVALID"))?;
    let object = value.as_object().ok_or(error("LIBRARY_METADATA_INVALID"))?;
    // These fields identify the explicitly supported downloader document shape.
    // An arbitrary JSON object containing just `id` is never source evidence.
    let jm = object
        .get("id")
        .and_then(serde_json::Value::as_u64)
        .filter(|v| *v > 0);
    let pica = object.get("id").and_then(serde_json::Value::as_str);
    let (name, reference) = if let Some(id) = jm {
        let name = object
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or(error("LIBRARY_METADATA_INVALID"))?;
        if !object
            .get("author")
            .is_some_and(serde_json::Value::is_array)
        {
            return Err(error("LIBRARY_METADATA_INVALID"));
        }
        (
            name,
            LibraryReference {
                source: Source::Jm,
                work_id: id.to_string(),
            },
        )
    } else if let Some(id) = pica {
        let reference = LibraryReference {
            source: Source::Pica,
            work_id: id.to_ascii_lowercase(),
        };
        let name = object
            .get("title")
            .and_then(serde_json::Value::as_str)
            .ok_or(error("LIBRARY_METADATA_INVALID"))?;
        if !reference.is_valid()
            || !object
                .get("author")
                .is_some_and(serde_json::Value::is_string)
            || object
                .get("pagesCount")
                .and_then(serde_json::Value::as_i64)
                .is_none()
        {
            return Err(error("LIBRARY_METADATA_INVALID"));
        }
        (name, reference)
    } else {
        return Err(error("LIBRARY_METADATA_INVALID"));
    };
    if !reference.is_valid() {
        return Err(error("LIBRARY_METADATA_INVALID"));
    }
    for key in ["chapterInfos", "tags"] {
        if !object.get(key).is_some_and(serde_json::Value::is_array) {
            return Err(error("LIBRARY_METADATA_INVALID"));
        }
    }
    let names = |key: &str, maximum: usize| -> Vec<String> {
        let mut seen = HashSet::new();
        object[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                entry
                    .as_str()
                    .or_else(|| entry.get("name").and_then(serde_json::Value::as_str))
            })
            .map(|v| clean(v, 200))
            .filter(|v| !v.is_empty() && seen.insert(v.clone()))
            .take(maximum)
            .collect()
    };
    Ok(Metadata {
        title: Some(clean(name, 1024)),
        authors: object
            .get("author")
            .and_then(serde_json::Value::as_str)
            .map(list)
            .unwrap_or_else(|| names("author", 50)),
        tags: names("tags", 100),
        description: object
            .get("description")
            .and_then(serde_json::Value::as_str)
            .map(|v| clean(v, 4096))
            .filter(|v| !v.is_empty()),
        reference: Some(reference),
        conflict: false,
    })
}

pub(crate) fn comic_info(bytes: &[u8]) -> Result<Metadata> {
    if bytes.len() > MAX_METADATA_BYTES {
        return Err(error("LIBRARY_METADATA_LIMIT"));
    }
    let xml = std::str::from_utf8(bytes).map_err(|_| error("LIBRARY_METADATA_INVALID"))?;
    let mut reader = Reader::from_str(xml);
    let mut values = HashMap::new();
    let mut depth = 0usize;
    let mut root_seen = false;
    let mut events = 0usize;
    loop {
        events += 1;
        if events > 4096 {
            return Err(error("LIBRARY_METADATA_LIMIT"));
        }
        match reader
            .read_event()
            .map_err(|_| error("LIBRARY_METADATA_INVALID"))?
        {
            Event::DocType(_) => return Err(error("LIBRARY_METADATA_INVALID")),
            Event::Start(element) => {
                depth += 1;
                if depth > 16 {
                    return Err(error("LIBRARY_METADATA_LIMIT"));
                }
                if depth == 1 {
                    if root_seen || element.name().as_ref() != b"ComicInfo" {
                        return Err(error("LIBRARY_METADATA_INVALID"));
                    }
                    root_seen = true;
                }
                if depth == 2
                    && matches!(
                        element.name().as_ref(),
                        b"Title"
                            | b"Writer"
                            | b"Penciller"
                            | b"Summary"
                            | b"Genre"
                            | b"Tags"
                            | b"Source"
                            | b"WorkId"
                            | b"Web"
                    )
                {
                    let name = String::from_utf8(element.name().as_ref().to_vec())
                        .map_err(|_| error("LIBRARY_METADATA_INVALID"))?;
                    let text = reader
                        .read_text(element.name())
                        .map_err(|_| error("LIBRARY_METADATA_INVALID"))?;
                    let text_bytes = text.into_inner();
                    let raw = std::str::from_utf8(&text_bytes)
                        .map_err(|_| error("LIBRARY_METADATA_INVALID"))?;
                    if raw.contains('<') {
                        return Err(error("LIBRARY_METADATA_INVALID"));
                    }
                    let decoded = quick_xml::escape::unescape(raw)
                        .map_err(|_| error("LIBRARY_METADATA_INVALID"))?;
                    if values.insert(name, decoded.into_owned()).is_some() {
                        return Err(error("LIBRARY_METADATA_INVALID"));
                    }
                    depth -= 1;
                }
            }
            Event::End(_) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or(error("LIBRARY_METADATA_INVALID"))?;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !root_seen || depth != 0 {
        return Err(error("LIBRARY_METADATA_INVALID"));
    }
    let get = |key: &str| values.get(key).map(String::as_str).unwrap_or("");
    let source = match get("Source") {
        "JM" => Some(Source::Jm),
        "Pica" => Some(Source::Pica),
        _ => None,
    };
    let mut reference = source
        .map(|source| LibraryReference {
            source,
            work_id: get("WorkId").to_ascii_lowercase(),
        })
        .filter(LibraryReference::is_valid);
    let web = get("Web");
    let web_reference = [
        "https://18comic.vip/album/",
        "https://18comic.org/album/",
        "https://18comic.com/album/",
    ]
    .iter()
    .find_map(|prefix| web.strip_prefix(prefix))
    .map(|id| LibraryReference {
        source: Source::Jm,
        work_id: id.trim_end_matches('/').into(),
    })
    .filter(LibraryReference::is_valid);
    let conflict = reference
        .as_ref()
        .zip(web_reference.as_ref())
        .is_some_and(|(a, b)| a != b);
    if reference.is_none() {
        reference = web_reference;
    }
    let summary = clean(get("Summary"), 4096);
    Ok(Metadata {
        title: Some(clean(get("Title"), 1024)),
        authors: list(if get("Writer").is_empty() {
            get("Penciller")
        } else {
            get("Writer")
        }),
        description: (!summary.is_empty()).then_some(summary),
        tags: list(&format!("{},{}", get("Genre"), get("Tags"))),
        reference,
        conflict,
    })
}
