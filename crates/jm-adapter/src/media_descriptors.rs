//! Pinned JM media-descriptor enumeration for A6.13.
//!
//! This module performs metadata/source-control reads only: `/chapter` and
//! `/chapter_view_template`. It never downloads image bytes or touches the
//! filesystem. Unlike the upstream GUI, an unparseable scramble ID is fatal;
//! the upstream fallback value is not accepted as proof of the live source.

use super::{
    is_retryable_transport, retryable_http_status, string, transport_code, JmClient,
};
use serde_json::Value;
use state_model::RequestTrace;
use std::{
    collections::BTreeSet,
    path::Path,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub const IMAGE_DOMAIN: &str = "cdn-msp2.jmapiproxy2.cc";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JmMediaItem {
    pub filename: String,
    pub source_format: String,
    pub block_num: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JmChapterMediaEnumeration {
    pub chapter_id: String,
    pub scramble_id: u64,
    pub media: Vec<JmMediaItem>,
}

fn valid_chapter_id(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn parse_scramble_id(body: &str) -> Result<u64, String> {
    let value = body
        .split("var scramble_id = ")
        .nth(1)
        .and_then(|tail| tail.split(';').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("JM_MEDIA_SCRAMBLE_ID_MISSING")?;
    let scramble_id = value
        .parse::<u64>()
        .map_err(|_| "JM_MEDIA_SCRAMBLE_ID_INVALID")?;
    if scramble_id == 0 {
        return Err("JM_MEDIA_SCRAMBLE_ID_INVALID".into());
    }
    Ok(scramble_id)
}

fn scramble_token(ts: u64) -> String {
    // The upstream client uses a distinct secret for `/chapter_view_template`.
    // Using the ordinary API token here can fail even when `/search` and `/album`
    // still work, so keep this protocol distinction explicit and pinned.
    format!("{:x}", md5::compute(format!("{ts}18comicAPPContent")))
}

fn block_num(scramble_id: u64, chapter_id: u64, filename_stem: &str) -> u64 {
    if chapter_id < scramble_id {
        0
    } else if chapter_id < 268_850 {
        10
    } else {
        let modulus = if chapter_id < 421_926 { 10 } else { 8 };
        let digest = format!("{:x}", md5::compute(format!("{chapter_id}{filename_stem}")));
        let last_ascii = u64::from(*digest.as_bytes().last().expect("md5 hex is non-empty"));
        (last_ascii % modulus) * 2 + 2
    }
}

fn parse_media(
    chapter_id: &str,
    scramble_id: u64,
    body: &Value,
) -> Result<Vec<JmMediaItem>, String> {
    let returned_id = string(&body["id"]).ok_or("MISSING_JM_CHAPTER_ID")?;
    if returned_id != chapter_id {
        return Err("JM_MEDIA_CHAPTER_ID_MISMATCH".into());
    }
    let numeric_chapter_id = chapter_id
        .parse::<u64>()
        .map_err(|_| "INVALID_JM_CHAPTER_ID")?;
    let images = body["images"]
        .as_array()
        .ok_or("MISSING_JM_CHAPTER_IMAGES")?;
    if images.iter().any(|image| !image.is_string()) {
        return Err("INVALID_JM_CHAPTER_IMAGES".into());
    }

    let mut filenames = BTreeSet::new();
    let mut media = Vec::new();
    for image in images {
        let filename = image.as_str().expect("validated string above");
        let path = Path::new(filename);
        if path.file_name().and_then(|value| value.to_str()) != Some(filename)
            || filename.contains('/')
            || filename.contains('\\')
            || filename.contains("..")
        {
            return Err("INVALID_JM_MEDIA_FILENAME".into());
        }
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            // Matches pinned worker semantics: entries without a usable
            // extension are not scheduled.
            continue;
        };
        let source_format = extension.to_ascii_lowercase();
        if !matches!(source_format.as_str(), "gif" | "webp") {
            // Matches pinned worker scheduling: unsupported formats are skipped.
            continue;
        }
        if !filenames.insert(filename.to_ascii_lowercase()) {
            return Err("DUPLICATE_JM_MEDIA_FILENAME".into());
        }
        let block_num = if source_format == "gif" {
            0
        } else {
            let stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .filter(|value| !value.is_empty())
                .ok_or("INVALID_JM_MEDIA_FILENAME")?;
            block_num(scramble_id, numeric_chapter_id, stem)
        };
        media.push(JmMediaItem {
            filename: filename.into(),
            source_format,
            block_num,
        });
    }
    if media.is_empty() {
        return Err("JM_MEDIA_ENUMERATION_EMPTY".into());
    }
    Ok(media)
}

impl JmClient {
    async fn live_scramble_id(&mut self, chapter_id: &str) -> Result<u64, String> {
        if !valid_chapter_id(chapter_id) {
            return Err("INVALID_JM_CHAPTER_ID".into());
        }

        let domains = self.domains.clone();
        let mut last_retryable_error = None;
        for domain in domains {
            let started = Instant::now();
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| "CLOCK_ERROR")?
                .as_secs();
            let token = scramble_token(ts);
            let response = self
                .client
                .get(format!("https://{domain}/chapter_view_template"))
                .query(&[
                    ("id", chapter_id.to_string()),
                    ("v", ts.to_string()),
                    ("mode", "vertical".into()),
                    ("page", "0".into()),
                    ("app_img_shunt", "1".into()),
                    ("express", "off".into()),
                ])
                .header("token", token)
                .header("tokenparam", format!("{ts},2.0.13"))
                .header("user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36")
                .send()
                .await;

            let mut http_status = None;
            let result = async {
                let response = response.map_err(|error| transport_code(&error))?;
                let status = response.status().as_u16();
                http_status = Some(status);
                if !response.status().is_success() {
                    return Err(format!("HTTP_{status}"));
                }
                let body = String::from_utf8(super::read_bounded_response(response).await.map_err(
                    |error| if error == "METADATA_RESPONSE_TOO_LARGE" {
                        "JM_SCRAMBLE_RESPONSE_TOO_LARGE"
                    } else {
                        "INVALID_JM_SCRAMBLE_BODY"
                    },
                )?)
                .map_err(|_| "INVALID_JM_SCRAMBLE_BODY")?;
                parse_scramble_id(&body)
            }
            .await;
            let retryable = match (&result, http_status) {
                (Err(code), None) => is_retryable_transport(code),
                (Err(_), Some(status)) => retryable_http_status(status),
                _ => false,
            };
            self.traces.push(RequestTrace {
                operation: "/chapter_view_template".into(),
                page: None,
                delay_ms: 0,
                elapsed_ms: started.elapsed().as_millis() as u64,
                http_status,
                outcome: result
                    .as_ref()
                    .map(|_| "OK".into())
                    .unwrap_or_else(|error| error.clone()),
            });
            match result {
                Ok(value) => return Ok(value),
                Err(error) if retryable => last_retryable_error = Some(error),
                Err(error) => return Err(error),
            }
        }

        Err(last_retryable_error.unwrap_or_else(|| "JM_ALL_PINNED_DOMAINS_FAILED".into()))
    }

    /// Re-read the exact pinned source controls required to materialize the
    /// A6.11 descriptors for one already-proven chapter.
    pub async fn live_chapter_media(
        &mut self,
        chapter_id: &str,
    ) -> Result<JmChapterMediaEnumeration, String> {
        self.live_chapter_media_with_guard(chapter_id, || Ok(())).await
    }

    /// Guarded A6.13 enumeration. `before_request` is invoked immediately
    /// before both source reads (`/chapter_view_template` and `/chapter`) so a
    /// caller can prove the A6.10 generation is still current for every fetch.
    pub async fn live_chapter_media_with_guard<Guard>(
        &mut self,
        chapter_id: &str,
        mut before_request: Guard,
    ) -> Result<JmChapterMediaEnumeration, String>
    where
        Guard: FnMut() -> Result<(), String>,
    {
        if !valid_chapter_id(chapter_id) {
            return Err("INVALID_JM_CHAPTER_ID".into());
        }
        before_request()?;
        let scramble_id = self.live_scramble_id(chapter_id).await?;
        before_request()?;
        let body = self
            .request("/chapter", &[("id", chapter_id.into())], None)
            .await?;
        let media = parse_media(chapter_id, scramble_id, &body)?;
        Ok(JmChapterMediaEnumeration {
            chapter_id: chapter_id.into(),
            scramble_id,
            media,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scramble_id_has_no_upstream_fallback() {
        assert_eq!(
            parse_scramble_id("var scramble_id = 220980;").unwrap(),
            220_980
        );
        assert!(parse_scramble_id("no scramble variable").is_err());
        assert!(parse_scramble_id("var scramble_id = nope;").is_err());
        assert!(parse_scramble_id("var scramble_id = 0;").is_err());
    }

    #[test]
    fn scramble_endpoint_uses_the_distinct_upstream_content_token_secret() {
        let ts = 123_u64;
        assert_eq!(
            scramble_token(ts),
            format!("{:x}", md5::compute("12318comicAPPContent"))
        );
        assert_ne!(
            scramble_token(ts),
            format!("{:x}", md5::compute("12318comicAPP"))
        );
    }

    #[test]
    fn exact_worker_filter_order_and_block_numbers_are_preserved() {
        let media = parse_media(
            "300000",
            200000,
            &serde_json::json!({
                "id":"300000",
                "images":["001.webp","002.GIF","003.jpg","004.webp"]
            }),
        )
        .unwrap();
        assert_eq!(media.len(), 3);
        assert_eq!(media[0].filename, "001.webp");
        assert_eq!(media[1].filename, "002.GIF");
        assert_eq!(media[1].source_format, "gif");
        assert_eq!(media[1].block_num, 0);
        assert_eq!(media[2].filename, "004.webp");
        assert!(media[0].block_num >= 2);
        assert_eq!(media[0].block_num % 2, 0);
    }

    #[test]
    fn uppercase_webp_uses_the_exact_upstream_file_stem() {
        let media = parse_media(
            "123456",
            100000,
            &serde_json::json!({"id":"123456","images":["Page01.WEBP"]}),
        )
        .unwrap();
        assert_eq!(media[0].filename, "Page01.WEBP");
        assert_eq!(media[0].source_format, "webp");
        assert_eq!(media[0].block_num, 10);
    }

    #[test]
    fn malformed_or_duplicate_scheduled_filenames_fail_closed() {
        assert!(parse_media(
            "123456",
            220980,
            &serde_json::json!({"id":"123456","images":["001.webp","001.WEBP"]})
        )
        .is_err());
        assert!(parse_media(
            "123456",
            220980,
            &serde_json::json!({"id":"123456","images":["../001.webp"]})
        )
        .is_err());
        assert!(parse_media(
            "123456",
            220980,
            &serde_json::json!({"id":"999999","images":["001.webp"]})
        )
        .is_err());
    }
}
