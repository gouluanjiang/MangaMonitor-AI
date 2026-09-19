//! Pinned extraction of lanyeeee/jmcomic-downloader @ f0cdd724.
//! Metadata APIs remain read-only; A6.14 media transport may fetch bytes only in memory and never touches the filesystem.
//! Reliability hardening follows the MIT-licensed JMComic-Crawler-Python domain failover pattern,
//! but remains restricted to the pinned baseline domains and fail-closed protocol semantics.
pub mod media_descriptors;
#[cfg(test)]
mod media_fetch;

use aes::{
    cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit},
    Aes256,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::Rng;
use serde_json::Value;
use state_model::{authors, fields, number, string, whitelist, Record, RequestTrace, SearchPage};
use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const UPSTREAM_COMMIT: &str = "f0cdd724af6892002f2fb7be883b88832cebe7e9";
pub const CRAWLER_RELIABILITY_COMMIT: &str = "9fddb0494caf0cdc812ac6cbfc1c62f4f845b058";
pub const DEFAULT_DOMAIN: &str = "www.cdnhth.cc";
pub const BASELINE_DOMAINS: &[&str] = &[
    "www.cdnhth.cc",
    "www.cdnzack.cc",
    "www.cdnhth.net",
    "www.cdnbea.net",
    "www.cdn-mspjmapiproxy.xyz",
];
/// Maximum in-memory response size for encrypted JSON and metadata bodies.
pub const MAX_METADATA_BYTES: u64 = 8 * 1024 * 1024;

fn checked_metadata_len(current: usize, incoming: usize) -> Result<usize, String> {
    let next = current
        .checked_add(incoming)
        .ok_or("METADATA_RESPONSE_SIZE_OVERFLOW")?;
    if u64::try_from(next).map_err(|_| "METADATA_RESPONSE_SIZE_OVERFLOW")? > MAX_METADATA_BYTES {
        return Err("METADATA_RESPONSE_TOO_LARGE".into());
    }
    Ok(next)
}

pub(crate) async fn read_bounded_response(
    mut response: reqwest::Response,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_METADATA_BYTES)
    {
        return Err("METADATA_RESPONSE_TOO_LARGE".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "METADATA_RESPONSE_READ_FAILED")?
    {
        checked_metadata_len(body.len(), chunk.len())?;
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

async fn read_json_response(response: reqwest::Response) -> Result<Value, String> {
    let body = read_bounded_response(response).await?;
    serde_json::from_slice(&body).map_err(|_| "INVALID_JSON".into())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JmPreflightChapter {
    pub chapter_id: String,
    pub chapter_order: u64,
}

#[derive(Clone, Copy)]
enum MetadataPacing {
    Monitor,
    AuthorizedDownload,
}

impl MetadataPacing {
    fn delay_range(self, path: &str) -> std::ops::RangeInclusive<u64> {
        match (self, path) {
            (Self::AuthorizedDownload, "/album" | "/chapter") => 0..=0,
            _ => 1000..=3000,
        }
    }
}

pub struct JmClient {
    client: reqwest::Client,
    domains: Vec<String>,
    pacing: MetadataPacing,
    pub traces: Vec<RequestTrace>,
}
impl JmClient {
    pub fn new(domain: &str) -> Result<Self, String> {
        Self::with_pacing(domain, MetadataPacing::Monitor)
    }

    /// For explicitly authorized manual downloads: only album/chapter metadata
    /// skips monitor pacing. The caller still owns the current approval gate.
    pub fn new_for_download(domain: &str) -> Result<Self, String> {
        Self::with_pacing(domain, MetadataPacing::AuthorizedDownload)
    }

    fn with_pacing(domain: &str, pacing: MetadataPacing) -> Result<Self, String> {
        if !BASELINE_DOMAINS.contains(&domain) {
            return Err("DOMAIN_NOT_IN_PINNED_BASELINE".into());
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .cookie_store(true)
            .build()
            .map_err(|_| "CLIENT_INIT")?;
        Ok(Self {
            client,
            domains: ordered_domains(domain),
            pacing,
            traces: vec![],
        })
    }
    pub fn domain(&self) -> &str {
        &self.domains[0]
    }
    async fn request(
        &mut self,
        path: &str,
        query: &[(&str, String)],
        page: Option<u64>,
    ) -> Result<Value, String> {
        let domains = self.domains.clone();
        let mut last_retryable_error = None;

        for domain in domains {
            let delay_ms = rand::thread_rng().gen_range(self.pacing.delay_range(path));
            if delay_ms != 0 {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
            let started = Instant::now();
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| "CLOCK_ERROR")?
                .as_secs();
            let token = format!("{:x}", md5::compute(format!("{ts}18comicAPP")));
            let response = self
                .client
                .get(format!("https://{domain}{path}"))
                .query(query)
                .header("token", token)
                .header("tokenparam", format!("{ts},2.0.13"))
                .header("user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36")
                .send()
                .await;
            let mut http_status = None;
            let result = async {
                let response = response.map_err(|e| transport_code(&e))?;
                let status = response.status().as_u16();
                http_status = Some(status);
                if !response.status().is_success() {
                    return Err(format!("HTTP_{status}"));
                }
                let body = read_json_response(response).await?;
                if number(&body["code"]) != Some(200) {
                    return Err(format!(
                        "API_CODE_{}",
                        number(&body["code"])
                            .map(|n| n.to_string())
                            .unwrap_or("UNKNOWN".into())
                    ));
                }
                decode(ts, body["data"].as_str().ok_or("MISSING_ENCRYPTED_DATA")?)
            }
            .await;
            let retryable = match (&result, http_status) {
                (Err(code), None) => is_retryable_transport(code),
                (Err(_), Some(status)) => retryable_http_status(status),
                _ => false,
            };
            self.traces.push(RequestTrace {
                operation: path.into(),
                page,
                delay_ms,
                elapsed_ms: started.elapsed().as_millis() as u64,
                http_status,
                outcome: result
                    .as_ref()
                    .map(|_| "OK".into())
                    .unwrap_or_else(|e| e.clone()),
            });
            match result {
                Ok(value) => return Ok(value),
                Err(error) if retryable => last_retryable_error = Some(error),
                Err(error) => return Err(error),
            }
        }

        Err(last_retryable_error.unwrap_or_else(|| "JM_ALL_PINNED_DOMAINS_FAILED".into()))
    }
    pub async fn search(&mut self, author: &str, page: u64) -> Result<SearchPage, String> {
        let body = self
            .request(
                "/search",
                &[
                    ("main_tag", "0".into()),
                    ("search_query", author.into()),
                    ("page", page.to_string()),
                    ("o", "mr".into()),
                ],
                Some(page),
            )
            .await?;
        if let Some(id) = string(&body["redirect_aid"]) {
            let (record, record_fields) = self.detail(&id).await?;
            return Ok(SearchPage {
                page,
                reported_total: Some(1),
                reported_pages: Some(1),
                reported_limit: None,
                response_fields: fields(&body),
                record_fields,
                records: vec![record],
                redirect_to_detail: true,
            });
        }
        let content = body["content"].as_array().ok_or("MISSING_SEARCH_CONTENT")?;
        let records = content
            .iter()
            .map(parse_record)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SearchPage {
            page,
            reported_total: number(&body["total"]),
            reported_pages: None,
            reported_limit: None,
            response_fields: fields(&body),
            record_fields: content.first().map(fields).unwrap_or_default(),
            records,
            redirect_to_detail: false,
        })
    }
    pub async fn detail(&mut self, id: &str) -> Result<(Record, Vec<String>), String> {
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
            return Err("INVALID_JM_ID".into());
        }
        let body = self.request("/album", &[("id", id.into())], None).await?;
        let record = parse_record(&body)?;
        if record.source_work_id != id {
            return Err("DETAIL_ID_MISMATCH".into());
        }
        Ok((record, fields(&body)))
    }

    /// Enumerate the complete chapter scope using the pinned `/album` shape.
    /// This mirrors the upstream chapter construction rule, including the
    /// single-chapter fallback when `series` is empty, but fails closed instead
    /// of silently dropping malformed series IDs.
    pub async fn preflight_chapters(
        &mut self,
        id: &str,
    ) -> Result<Vec<JmPreflightChapter>, String> {
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
            return Err("INVALID_JM_ID".into());
        }
        let body = self.request("/album", &[("id", id.into())], None).await?;
        let body_id = string(&body["id"]).ok_or("MISSING_JM_ID")?;
        if body_id != id {
            return Err("PREFLIGHT_DETAIL_ID_MISMATCH".into());
        }
        parse_preflight_chapters(&body, id)
    }

    /// Count the exact image entries the pinned JM worker would schedule from
    /// `/chapter`: GIF and WEBP entries only. No image URL is fetched here.
    pub async fn preflight_chapter_image_count(&mut self, chapter_id: &str) -> Result<u64, String> {
        if chapter_id.is_empty() || !chapter_id.bytes().all(|b| b.is_ascii_digit()) {
            return Err("INVALID_JM_CHAPTER_ID".into());
        }
        let body = self
            .request("/chapter", &[("id", chapter_id.into())], None)
            .await?;
        let returned_id = string(&body["id"]).ok_or("MISSING_JM_CHAPTER_ID")?;
        if returned_id != chapter_id {
            return Err("PREFLIGHT_CHAPTER_ID_MISMATCH".into());
        }
        count_preflight_images(&body)
    }
}

fn ordered_domains(primary: &str) -> Vec<String> {
    std::iter::once(primary)
        .chain(
            BASELINE_DOMAINS
                .iter()
                .copied()
                .filter(|domain| *domain != primary),
        )
        .map(str::to_owned)
        .collect()
}

fn transport_code(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "TIMEOUT".into()
    } else if error.is_connect() {
        "CONNECT_ERROR".into()
    } else {
        "TRANSPORT_ERROR".into()
    }
}

fn is_retryable_transport(code: &str) -> bool {
    matches!(code, "TIMEOUT" | "CONNECT_ERROR" | "TRANSPORT_ERROR")
}

fn retryable_http_status(status: u16) -> bool {
    status == 403 || matches!(status, 408 | 425 | 429) || (500..=599).contains(&status)
}

fn parse_preflight_chapters(v: &Value, work_id: &str) -> Result<Vec<JmPreflightChapter>, String> {
    let series = v["series"].as_array().ok_or("MISSING_JM_SERIES")?;
    if series.is_empty() {
        return Ok(vec![JmPreflightChapter {
            chapter_id: work_id.into(),
            chapter_order: 1,
        }]);
    }
    let mut ids = BTreeSet::new();
    let mut chapters = Vec::with_capacity(series.len());
    for (index, item) in series.iter().enumerate() {
        let chapter_id = string(&item["id"]).ok_or("MISSING_JM_CHAPTER_ID")?;
        if chapter_id.is_empty()
            || !chapter_id.bytes().all(|b| b.is_ascii_digit())
            || !ids.insert(chapter_id.clone())
        {
            return Err("INVALID_JM_PREFLIGHT_CHAPTERS".into());
        }
        let chapter_order = u64::try_from(index + 1).map_err(|_| "JM_CHAPTER_ORDER_OVERFLOW")?;
        chapters.push(JmPreflightChapter {
            chapter_id,
            chapter_order,
        });
    }
    Ok(chapters)
}

fn count_preflight_images(v: &Value) -> Result<u64, String> {
    let images = v["images"].as_array().ok_or("MISSING_JM_CHAPTER_IMAGES")?;
    if images.iter().any(|image| !image.is_string()) {
        return Err("INVALID_JM_CHAPTER_IMAGES".into());
    }
    let count = images
        .iter()
        .filter_map(Value::as_str)
        .filter(|filename| {
            Path::new(filename)
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "gif" | "webp"))
        })
        .count();
    if count == 0 {
        return Err("JM_PREFLIGHT_IMAGES_EMPTY".into());
    }
    u64::try_from(count).map_err(|_| "JM_PREFLIGHT_IMAGE_COUNT_OVERFLOW".into())
}

fn parse_record(v: &Value) -> Result<Record, String> {
    let id = string(&v["id"]).ok_or("MISSING_JM_ID")?;
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err("INVALID_JM_ID".into());
    }
    let title = v["name"].as_str().ok_or("MISSING_TITLE")?.to_owned();
    let mut metadata = whitelist(
        v,
        &[
            "update_at",
            "adddate",
            "addtime",
            "total_photos",
            "description",
            "tags",
            "works",
            "actors",
            "series_id",
            "category",
            "category_sub",
        ],
    );
    if let Some(series) = v["series"].as_array() {
        metadata["series"] = Value::Array(
            series
                .iter()
                .map(|s| whitelist(s, &["id", "name", "sort"]))
                .collect(),
        );
    }
    Ok(Record::new(
        "jm",
        id,
        authors(&v["author"]),
        title,
        metadata,
    ))
}
fn decode(ts: u64, data: &str) -> Result<Value, String> {
    let mut bytes = STANDARD.decode(data).map_err(|_| "BASE64_ERROR")?;
    if bytes.is_empty() || bytes.len() % 16 != 0 {
        return Err("AES_BLOCK_ERROR".into());
    }
    let key = format!("{:x}", md5::compute(format!("{ts}185Hcomic3PAPP7R")));
    let cipher = Aes256::new_from_slice(key.as_bytes()).map_err(|_| "AES_KEY_ERROR")?;
    for block in bytes.as_chunks_mut::<16>().0 {
        cipher.decrypt_block(GenericArray::from_mut_slice(block));
    }
    let padding = *bytes.last().ok_or("EMPTY_AES_DATA")? as usize;
    if padding == 0
        || padding > 16
        || bytes[bytes.len() - padding..]
            .iter()
            .any(|b| *b as usize != padding)
    {
        return Err("AES_PADDING_ERROR".into());
    }
    bytes.truncate(bytes.len() - padding);
    serde_json::from_slice(&bytes).map_err(|_| "DECRYPTED_JSON_ERROR".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_client_retains_monitor_delay_for_every_metadata_route() {
        let client = JmClient::new(DEFAULT_DOMAIN).unwrap();
        for path in [
            "/album",
            "/chapter",
            "/search",
            "/chapter_view_template",
            "/unknown",
        ] {
            assert_eq!(client.pacing.delay_range(path), 1000..=3000, "{path}");
        }
    }

    #[test]
    fn authorized_download_pacing_is_immediate_only_for_exact_album_and_chapter_routes() {
        let client = JmClient::new_for_download(DEFAULT_DOMAIN).unwrap();
        for path in ["/album", "/chapter"] {
            assert_eq!(client.pacing.delay_range(path), 0..=0, "{path}");
        }
        for path in [
            "/search",
            "/chapter_view_template",
            "/album/",
            "/chapter/",
            "/album?id=1",
            "/ALBUM",
            "/login",
            "",
        ] {
            assert_eq!(client.pacing.delay_range(path), 1000..=3000, "{path}");
        }
    }

    #[test]
    fn both_pacing_constructors_keep_the_same_pinned_domain_boundary() {
        for create in [JmClient::new, JmClient::new_for_download] {
            for domain in [
                "",
                "example.invalid",
                "https://www.cdnhth.cc",
                "www.cdnhth.cc:443",
                "WWW.CDNHTH.CC",
                "www.cdnhth.cc/album",
            ] {
                assert!(
                    matches!(create(domain), Err(code) if code == "DOMAIN_NOT_IN_PINNED_BASELINE")
                );
            }
            let client = create("www.cdnzack.cc").unwrap();
            assert_eq!(client.domain(), "www.cdnzack.cc");
            assert_eq!(client.domains, ordered_domains("www.cdnzack.cc"));
            assert!(client.traces.is_empty());
        }
    }

    #[test]
    fn identity_is_namespaced_and_checked() {
        let record = parse_record(&serde_json::json!({"id":123,"name":"title","author":["one","two"],"thumb":"secret-image-url"})).unwrap();
        assert_eq!(record.source, "jm");
        assert_eq!(record.source_work_id, "123");
        assert_eq!(record.author.len(), 2);
        assert!(record.metadata.get("thumb").is_none());
        assert!(parse_record(&serde_json::json!({"id":"abc","name":"title"})).is_err());
    }
    #[test]
    fn corrupt_protocol_payload_is_an_error() {
        assert!(decode(1, "AA==").is_err());
    }
    #[test]
    fn pinned_domain_failover_keeps_primary_first_and_never_expands_trust() {
        let ordered = ordered_domains("www.cdnzack.cc");
        assert_eq!(ordered[0], "www.cdnzack.cc");
        assert_eq!(ordered.len(), BASELINE_DOMAINS.len());
        assert_eq!(
            ordered.iter().collect::<BTreeSet<_>>().len(),
            BASELINE_DOMAINS.len()
        );
        assert!(ordered
            .iter()
            .all(|domain| BASELINE_DOMAINS.contains(&domain.as_str())));
    }
    #[test]
    fn only_transient_or_domain_level_http_failures_are_failover_candidates() {
        for status in [403, 408, 425, 429, 500, 502, 503, 599] {
            assert!(retryable_http_status(status), "{status}");
        }
        for status in [400, 401, 404, 409, 422] {
            assert!(!retryable_http_status(status), "{status}");
        }
        for code in ["TIMEOUT", "CONNECT_ERROR", "TRANSPORT_ERROR"] {
            assert!(is_retryable_transport(code));
        }
        assert!(!is_retryable_transport("INVALID_JSON"));
    }
    #[test]
    fn metadata_body_limit_is_enforced_before_append() {
        assert!(checked_metadata_len(MAX_METADATA_BYTES as usize, 1).is_err());
        assert_eq!(checked_metadata_len(4, 5).unwrap(), 9);
    }
    #[test]
    fn preflight_chapters_match_pinned_series_and_fallback_rules() {
        let chapters = parse_preflight_chapters(
            &serde_json::json!({"series":[{"id":"101"},{"id":"102"}]}),
            "99",
        )
        .unwrap();
        assert_eq!(chapters[0].chapter_id, "101");
        assert_eq!(chapters[0].chapter_order, 1);
        assert_eq!(chapters[1].chapter_id, "102");
        assert_eq!(chapters[1].chapter_order, 2);

        let fallback = parse_preflight_chapters(&serde_json::json!({"series":[]}), "99").unwrap();
        assert_eq!(
            fallback,
            vec![JmPreflightChapter {
                chapter_id: "99".into(),
                chapter_order: 1
            }]
        );

        assert!(parse_preflight_chapters(
            &serde_json::json!({"series":[{"id":"101"},{"id":"101"}]}),
            "99"
        )
        .is_err());
    }
    #[test]
    fn preflight_image_count_matches_pinned_worker_extension_filter() {
        let count = count_preflight_images(&serde_json::json!({
            "images":["001.webp","002.GIF","003.jpg","bad"]
        }))
        .unwrap();
        assert_eq!(count, 2);
        assert!(count_preflight_images(&serde_json::json!({"images":["001.jpg"]})).is_err());
        assert!(count_preflight_images(&serde_json::json!({"images":["001.webp",7]})).is_err());
    }
}
