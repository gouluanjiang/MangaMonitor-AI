//! Pinned Pica media-descriptor enumeration for A6.13.
//!
//! This module re-reads every image metadata page for one chapter and retains
//! the exact media identity/file-server/path ordering used by the pinned
//! downloader. It never downloads image bytes or touches the filesystem.

use super::{number, string, valid_id, PicaClient};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PicaMediaItem {
    pub media_id: String,
    pub original_name: String,
    pub file_server: String,
    pub path: String,
    pub source_format: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PicaChapterMediaEnumeration {
    pub chapter_order: u64,
    pub total_pages: u64,
    pub successful_pages: Vec<u64>,
    pub media: Vec<PicaMediaItem>,
}

fn source_format_from_path(path: &str) -> Result<String, String> {
    let tail = path
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .ok_or("INVALID_PICA_MEDIA_PATH")?;
    let (_, extension) = tail
        .rsplit_once('.')
        .ok_or("INVALID_PICA_MEDIA_PATH")?;
    let extension = extension.to_ascii_lowercase();
    if !matches!(extension.as_str(), "gif" | "webp" | "jpg" | "jpeg" | "png") {
        return Err("UNSUPPORTED_PICA_MEDIA_FORMAT".into());
    }
    Ok(extension)
}

fn parse_media_doc(value: &Value) -> Result<PicaMediaItem, String> {
    let media_id = string(&value["_id"]).ok_or("MISSING_PICA_IMAGE_ID")?;
    if !valid_id(&media_id) {
        return Err("INVALID_PICA_IMAGE_ID".into());
    }
    let media = value["media"]
        .as_object()
        .ok_or("MISSING_PICA_IMAGE_MEDIA")?;
    let original_name = media
        .get("originalName")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("INVALID_PICA_IMAGE_MEDIA")?
        .to_owned();
    let path = media
        .get("path")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("INVALID_PICA_IMAGE_MEDIA")?
        .to_owned();
    let file_server = media
        .get("fileServer")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("INVALID_PICA_IMAGE_MEDIA")?
        .trim_end_matches('/')
        .to_owned();
    if !file_server.starts_with("https://")
        || file_server.contains('?')
        || file_server.contains('#')
        || file_server["https://".len()..]
            .split('/')
            .next()
            .unwrap_or_default()
            .contains('@')
        || path.starts_with('/')
        || path.contains("..")
        || path.contains('\\')
        || path.contains('?')
        || path.contains('#')
    {
        return Err("INVALID_PICA_IMAGE_MEDIA".into());
    }
    let source_format = source_format_from_path(&path)?;
    Ok(PicaMediaItem {
        media_id,
        original_name,
        file_server,
        path,
        source_format,
    })
}

impl PicaClient {
    /// Re-enumerate every live media metadata page for one already-proven
    /// chapter order. Any missing/changed page count, empty page, malformed
    /// media object, or duplicate media ID fails closed.
    pub async fn live_chapter_media(
        &mut self,
        comic_id: &str,
        chapter_order: u64,
        max_pages: u64,
    ) -> Result<PicaChapterMediaEnumeration, String> {
        self.live_chapter_media_with_guard(comic_id, chapter_order, max_pages, || Ok(()))
            .await
    }

    /// Guarded A6.13 enumeration. `before_request` is invoked immediately
    /// before every image-metadata pagination request so the caller can prove
    /// the A6.10 generation is still current for each source fetch.
    pub async fn live_chapter_media_with_guard<Guard>(
        &mut self,
        comic_id: &str,
        chapter_order: u64,
        max_pages: u64,
        mut before_request: Guard,
    ) -> Result<PicaChapterMediaEnumeration, String>
    where
        Guard: FnMut() -> Result<(), String>,
    {
        if !valid_id(comic_id) || chapter_order == 0 || max_pages == 0 {
            return Err("INVALID_PICA_MEDIA_ENUMERATION_INPUT".into());
        }
        let mut total_pages = None;
        let mut successful_pages = Vec::new();
        let mut media_ids = BTreeSet::new();
        let mut media = Vec::new();

        for page in 1..=max_pages {
            before_request()?;
            let data = self
                .request(
                    reqwest::Method::GET,
                    &format!("comics/{comic_id}/order/{chapter_order}/pages?page={page}"),
                    None,
                    "live_media_descriptors",
                    Some(page),
                )
                .await?;
            let pages = &data["pages"];
            let reported_pages = number(&pages["pages"]).ok_or("MISSING_IMAGE_PAGES")?;
            if reported_pages == 0 {
                return Err("PICA_MEDIA_PAGES_EMPTY".into());
            }
            if total_pages
                .replace(reported_pages)
                .is_some_and(|previous| previous != reported_pages)
            {
                return Err("PICA_MEDIA_PAGE_COUNT_CHANGED".into());
            }
            let docs = pages["docs"].as_array().ok_or("MISSING_IMAGE_DOCS")?;
            if docs.is_empty() {
                return Err("PICA_MEDIA_PAGE_EMPTY".into());
            }
            successful_pages.push(page);
            for doc in docs {
                let item = parse_media_doc(doc)?;
                if !media_ids.insert(item.media_id.clone()) {
                    return Err("DUPLICATE_PICA_MEDIA_ID".into());
                }
                media.push(item);
            }
            if page == reported_pages {
                if media.is_empty() {
                    return Err("PICA_MEDIA_ENUMERATION_EMPTY".into());
                }
                return Ok(PicaChapterMediaEnumeration {
                    chapter_order,
                    total_pages: reported_pages,
                    successful_pages,
                    media,
                });
            }
            if page > reported_pages {
                return Err("PICA_MEDIA_PAGINATION_OVERRUN".into());
            }
        }
        Err("PICA_MEDIA_PAGINATION_BUDGET_EXHAUSTED".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_media_doc_preserves_pinned_url_inputs() {
        let item = parse_media_doc(&serde_json::json!({
            "_id":"333333333333333333333333",
            "media":{
                "originalName":"page 1.PNG",
                "path":"media/path/001.PNG",
                "fileServer":"https://storage.example.invalid/"
            }
        }))
        .unwrap();
        assert_eq!(item.media_id, "333333333333333333333333");
        assert_eq!(item.file_server, "https://storage.example.invalid");
        assert_eq!(item.path, "media/path/001.PNG");
        assert_eq!(item.source_format, "png");
    }

    #[test]
    fn unsafe_server_path_or_format_fails_closed() {
        assert!(parse_media_doc(&serde_json::json!({
            "_id":"333333333333333333333333",
            "media":{"originalName":"1.jpg","path":"../1.jpg","fileServer":"https://storage.example.invalid"}
        }))
        .is_err());
        assert!(parse_media_doc(&serde_json::json!({
            "_id":"333333333333333333333333",
            "media":{"originalName":"1.jpg","path":"1.jpg","fileServer":"https://user:pass@storage.example.invalid"}
        }))
        .is_err());
        assert!(parse_media_doc(&serde_json::json!({
            "_id":"333333333333333333333333",
            "media":{"originalName":"1.bmp","path":"1.bmp","fileServer":"https://storage.example.invalid"}
        }))
        .is_err());
    }
}