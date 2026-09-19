//! Pinned extraction of lanyeeee/picacomic-downloader @ 77c8b62e.
//! Metadata APIs remain read-only; A6.14 media transport may fetch bytes only in memory and never touches the filesystem.
#[cfg(test)]
mod download_tests;
pub mod media_descriptors;
#[cfg(test)]
mod media_fetch;
mod pagination;

use hmac::{Hmac, Mac};
use rand::Rng;
use serde_json::{json, Value};
use sha2::Sha256;
use state_model::{authors, fields, number, string, whitelist, Record, RequestTrace, SearchPage};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const UPSTREAM_COMMIT: &str = "77c8b62ede42b3afc074506d092313816af8092d";
pub const HOST: &str = "https://picaapi.picacomic.com/";
// Public upstream protocol constants, NOT personal authentication credentials.
const API_KEY: &str = "C69BAF41DA5ABD1FFEDC6D2FEA56B";
const NONCE: &str = "ptxdhmjzqtnrtwndhbxcpkjamb33w837";
const DIGEST_KEY: &str = r"~d}$Q7$eIni=V)9\RK/P.RM4;9[7|@/CA}b~OW!3?EV`:<>M7pddUBL5n|0/*Cn";
/// Maximum in-memory response size for API metadata and error envelopes.
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

async fn read_bounded_response(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
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
pub struct PicaPreflightChapter {
    pub chapter_id: String,
    pub chapter_order: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PicaChapterEnumeration {
    pub total_pages: u64,
    pub successful_pages: Vec<u64>,
    pub chapters: Vec<PicaPreflightChapter>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PicaImageEnumeration {
    pub total_pages: u64,
    pub successful_pages: Vec<u64>,
    pub expected_images: u64,
}

pub struct PicaClient {
    client: reqwest::Client,
    token: String,
    pub traces: Vec<RequestTrace>,
    pacing: MetadataPacing,
    #[cfg(test)]
    script: Option<std::collections::VecDeque<Result<Value, String>>>,
    #[cfg(test)]
    requested_paths: Vec<String>,
}

#[derive(Clone, Copy)]
enum MetadataPacing {
    Monitor,
    AuthorizedDownload,
}

impl MetadataPacing {
    fn delay_ms(self, method: &reqwest::Method, operation: &str) -> u64 {
        if matches!(self, Self::AuthorizedDownload)
            && method == reqwest::Method::GET
            && matches!(
                operation,
                "preflight_chapters" | "preflight_images" | "live_media_descriptors"
            )
        {
            0
        } else {
            rand::thread_rng().gen_range(1000..=3000)
        }
    }
}

impl PicaClient {
    pub fn new(token: String) -> Result<Self, String> {
        Self::with_pacing(token, MetadataPacing::Monitor)
    }

    /// Only the explicitly authorized download enumerators select immediate
    /// metadata pacing. Monitoring, login and unrelated APIs retain the delay.
    pub fn new_for_download(token: String) -> Result<Self, String> {
        Self::with_pacing(token, MetadataPacing::AuthorizedDownload)
    }

    fn with_pacing(token: String, pacing: MetadataPacing) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "CLIENT_INIT")?;
        Ok(Self {
            client,
            token,
            traces: vec![],
            pacing,
            #[cfg(test)]
            script: None,
            #[cfg(test)]
            requested_paths: Vec::new(),
        })
    }
    async fn request(
        &mut self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
        operation: &str,
        page: Option<u64>,
    ) -> Result<Value, String> {
        #[cfg(test)]
        if let Some(result) = self.scripted_response(path) {
            return result;
        }
        self.request_live(method, path, body, operation, page).await
    }

    #[cfg(test)]
    fn scripted_response(&mut self, path: &str) -> Option<Result<Value, String>> {
        self.requested_paths.push(path.to_owned());
        // Always intercept unit-test requests, including an absent or exhausted
        // script. No forgotten fixture can fall through to a real source.
        Some(
            self.script
                .as_mut()
                .and_then(std::collections::VecDeque::pop_front)
                .unwrap_or_else(|| Err("TEST_SOURCE_REQUEST_FORBIDDEN".into())),
        )
    }

    async fn request_live(
        &mut self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
        operation: &str,
        page: Option<u64>,
    ) -> Result<Value, String> {
        let delay_ms = self.pacing.delay_ms(&method, operation);
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        let started = Instant::now();
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "CLOCK_ERROR")?
            .as_secs()
            .to_string();
        let signature = signature(path, method.as_str(), &time);
        let mut request = self
            .client
            .request(method, format!("{HOST}{path}"))
            .header("api-key", API_KEY)
            .header("accept", "application/vnd.picacomic.com.v1+json")
            .header("app-channel", "2")
            .header("time", time)
            .header("nonce", NONCE)
            .header("app-version", "2.2.1.2.3.3")
            .header("app-uuid", "defaultUuid")
            .header("app-platform", "android")
            .header("app-build-version", "44")
            .header("content-type", "application/json; charset=UTF-8")
            .header("user-agent", "okhttp/3.8.1")
            .header("authorization", &self.token)
            .header("image-quality", "original")
            .header("signature", signature);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await;
        let mut http_status = None;
        let result = async {
            let response = response.map_err(|e| {
                if e.is_timeout() {
                    "TIMEOUT"
                } else if e.is_connect() {
                    "CONNECT_ERROR"
                } else {
                    "TRANSPORT_ERROR"
                }
            })?;
            http_status = Some(response.status().as_u16());
            if operation == "detail" && response.status().as_u16() == 404 {
                let body = read_json_response(response).await?;
                if certified_unavailable(&body) {
                    return Err("CONFIRMED_UNAVAILABLE".into());
                }
                return Err("HTTP_404".into());
            }
            if !response.status().is_success() {
                return Err(format!("HTTP_{}", response.status().as_u16()));
            }
            let value = read_json_response(response).await?;
            if number(&value["code"]) != Some(200) {
                return Err(format!(
                    "API_CODE_{}",
                    number(&value["code"])
                        .map(|n| n.to_string())
                        .unwrap_or("UNKNOWN".into())
                ));
            }
            value.get("data").cloned().ok_or("MISSING_DATA".into())
        }
        .await;
        self.traces.push(RequestTrace {
            operation: operation.into(),
            page,
            delay_ms,
            elapsed_ms: started.elapsed().as_millis() as u64,
            http_status,
            outcome: result
                .as_ref()
                .map(|_| "OK".into())
                .unwrap_or_else(|e| e.clone()),
        });
        result
    }
    pub async fn login(&mut self, email: &str, password: &str) -> Result<(), String> {
        let data = self
            .request(
                reqwest::Method::POST,
                "auth/sign-in",
                Some(json!({"email":email,"password":password})),
                "login",
                None,
            )
            .await?;
        self.token = data["token"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("MISSING_TOKEN")?
            .into();
        Ok(())
    }
    pub async fn search(&mut self, author: &str, page: u64) -> Result<SearchPage, String> {
        let data = self
            .request(
                reqwest::Method::POST,
                &format!("comics/advanced-search?page={page}"),
                Some(json!({"keyword":author,"sort":"dd","categories":[]})),
                "search",
                Some(page),
            )
            .await?;
        let body = &data["comics"];
        let docs = body["docs"].as_array().ok_or("MISSING_SEARCH_DOCS")?;
        let records = docs
            .iter()
            .map(parse_record)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SearchPage {
            page,
            reported_total: number(&body["total"]),
            reported_pages: number(&body["pages"]),
            reported_limit: number(&body["limit"]),
            response_fields: fields(body),
            record_fields: docs.first().map(fields).unwrap_or_default(),
            records,
            redirect_to_detail: false,
        })
    }
    pub async fn detail(&mut self, id: &str) -> Result<(Record, Vec<String>), String> {
        if !valid_id(id) {
            return Err("INVALID_PICA_ID".into());
        }
        let data = self
            .request(
                reqwest::Method::GET,
                &format!("comics/{id}"),
                None,
                "detail",
                None,
            )
            .await?;
        let record = parse_record(&data["comic"])?;
        if record.source_work_id != id {
            return Err("DETAIL_ID_MISMATCH".into());
        }
        Ok((record, fields(&data["comic"])))
    }
    // Metadata only. Any missing page is fatal; unlike the upstream GUI aggregation
    // helper, no failed page is silently discarded.
    pub async fn chapters(&mut self, id: &str, max_pages: u64) -> Result<Value, String> {
        if !valid_id(id) {
            return Err("INVALID_PICA_ID".into());
        }
        let mut result = vec![];
        for page in 1..=max_pages {
            let data = self
                .request(
                    reqwest::Method::GET,
                    &format!("comics/{id}/eps?page={page}"),
                    None,
                    "chapters",
                    Some(page),
                )
                .await?;
            let eps = &data["eps"];
            let pages = number(&eps["pages"]).ok_or("MISSING_CHAPTER_PAGES")?;
            let docs = eps["docs"].as_array().ok_or("MISSING_CHAPTER_DOCS")?;
            if docs.is_empty() && page < pages {
                return Err("INCOMPLETE_CHAPTER_PAGINATION".into());
            }
            result.extend(
                docs.iter()
                    .map(|v| whitelist(v, &["_id", "id", "title", "order", "updated_at"])),
            );
            if page >= pages {
                return Ok(Value::Array(result));
            }
        }
        Err("CHAPTER_PAGINATION_BUDGET_EXHAUSTED".into())
    }

    /// Enumerate every chapter page and return an exact ordered scope. The
    /// caller supplies a hard page budget; reaching it before the server's
    /// reported final page fails closed.
    pub async fn preflight_chapters(
        &mut self,
        id: &str,
        max_pages: u64,
    ) -> Result<PicaChapterEnumeration, String> {
        self.preflight_chapters_with_guard(id, max_pages, || Ok(()))
            .await
    }

    pub async fn preflight_chapters_with_guard<Guard>(
        &mut self,
        id: &str,
        max_pages: u64,
        mut before_request: Guard,
    ) -> Result<PicaChapterEnumeration, String>
    where
        Guard: FnMut() -> Result<(), String>,
    {
        if !valid_id(id) {
            return Err("INVALID_PICA_ID".into());
        }
        if max_pages == 0 {
            return Err("INVALID_PICA_PREFLIGHT_PAGE_BUDGET".into());
        }
        let mut page_scope = None;
        let mut successful_pages = Vec::new();
        let mut chapters = Vec::new();
        for page in 1..=max_pages {
            before_request()?;
            let data = self
                .request(
                    reqwest::Method::GET,
                    &format!("comics/{id}/eps?page={page}"),
                    None,
                    "preflight_chapters",
                    Some(page),
                )
                .await?;
            let eps = &data["eps"];
            let (scope, docs) = pagination::read_page(eps, page, max_pages, &mut page_scope)?;
            successful_pages.push(page);
            for doc in docs {
                chapters.push(parse_preflight_chapter(doc)?);
            }
            if page == scope.pages {
                // Pinned utils.rs sorts the complete chapter set by order.
                // Sorting does not excuse duplicate IDs/orders or missing pages.
                chapters.sort_by_key(|chapter| chapter.chapter_order);
                validate_preflight_chapters(&chapters)?;
                if chapters.len() as u64 != scope.total {
                    return Err("PICA_PAGINATION_INCOMPLETE".into());
                }
                return Ok(PicaChapterEnumeration {
                    total_pages: scope.pages,
                    successful_pages,
                    chapters,
                });
            }
        }
        Err("PICA_PREFLIGHT_CHAPTER_PAGINATION_BUDGET_EXHAUSTED".into())
    }

    /// Enumerate all image metadata pages for one chapter. This counts the
    /// image objects that the pinned worker would subsequently download, but
    /// performs no request to any image media URL.
    pub async fn preflight_chapter_images(
        &mut self,
        comic_id: &str,
        chapter_order: u64,
        max_pages: u64,
    ) -> Result<PicaImageEnumeration, String> {
        self.preflight_chapter_images_with_guard(comic_id, chapter_order, max_pages, || Ok(()))
            .await
    }

    pub async fn preflight_chapter_images_with_guard<Guard>(
        &mut self,
        comic_id: &str,
        chapter_order: u64,
        max_pages: u64,
        mut before_request: Guard,
    ) -> Result<PicaImageEnumeration, String>
    where
        Guard: FnMut() -> Result<(), String>,
    {
        if !valid_id(comic_id) || chapter_order == 0 || max_pages == 0 {
            return Err("INVALID_PICA_IMAGE_PREFLIGHT_INPUT".into());
        }
        let mut page_scope = None;
        let mut successful_pages = Vec::new();
        let mut image_ids = BTreeSet::new();
        let mut expected_images = 0u64;
        for page in 1..=max_pages {
            before_request()?;
            let data = self
                .request(
                    reqwest::Method::GET,
                    &format!("comics/{comic_id}/order/{chapter_order}/pages?page={page}"),
                    None,
                    "preflight_images",
                    Some(page),
                )
                .await?;
            let pages = &data["pages"];
            let (scope, docs) = pagination::read_page(pages, page, max_pages, &mut page_scope)?;
            successful_pages.push(page);
            for doc in docs {
                let image_id = validate_preflight_image_doc(doc)?;
                if !image_ids.insert(image_id) {
                    return Err("INVALID_PICA_PREFLIGHT_IMAGE_SET".into());
                }
                expected_images = expected_images
                    .checked_add(1)
                    .ok_or("PICA_PREFLIGHT_IMAGE_COUNT_OVERFLOW")?;
            }
            if page == scope.pages {
                if expected_images != scope.total {
                    return Err("PICA_PAGINATION_INCOMPLETE".into());
                }
                return Ok(PicaImageEnumeration {
                    total_pages: scope.pages,
                    successful_pages,
                    expected_images,
                });
            }
        }
        Err("PICA_PREFLIGHT_IMAGE_PAGINATION_BUDGET_EXHAUSTED".into())
    }
}

fn parse_preflight_chapter(v: &Value) -> Result<PicaPreflightChapter, String> {
    let chapter_id = string(&v["_id"]).ok_or("MISSING_PICA_CHAPTER_ID")?;
    if !valid_id(&chapter_id) {
        return Err("INVALID_PICA_CHAPTER_ID".into());
    }
    let chapter_order = number(&v["order"]).ok_or("MISSING_PICA_CHAPTER_ORDER")?;
    if chapter_order == 0 {
        return Err("INVALID_PICA_CHAPTER_ORDER".into());
    }
    Ok(PicaPreflightChapter {
        chapter_id,
        chapter_order,
    })
}

fn validate_preflight_chapters(chapters: &[PicaPreflightChapter]) -> Result<(), String> {
    if chapters.is_empty() {
        return Err("PICA_PREFLIGHT_CHAPTERS_EMPTY".into());
    }
    let mut ids = BTreeSet::new();
    let mut orders = BTreeSet::new();
    let mut previous_order = 0u64;
    for chapter in chapters {
        if chapter.chapter_order <= previous_order
            || !ids.insert(chapter.chapter_id.clone())
            || !orders.insert(chapter.chapter_order)
        {
            return Err("PICA_PREFLIGHT_CHAPTERS_NOT_CANONICAL".into());
        }
        previous_order = chapter.chapter_order;
    }
    Ok(())
}

fn validate_preflight_image_doc(v: &Value) -> Result<String, String> {
    let image_id = string(&v["_id"]).ok_or("MISSING_PICA_IMAGE_ID")?;
    if !valid_id(&image_id) {
        return Err("INVALID_PICA_IMAGE_ID".into());
    }
    let media = v["media"].as_object().ok_or("MISSING_PICA_IMAGE_MEDIA")?;
    for key in ["originalName", "path", "fileServer"] {
        if media
            .get(key)
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err("INVALID_PICA_IMAGE_MEDIA".into());
        }
    }
    Ok(image_id)
}

fn valid_id(id: &str) -> bool {
    id.len() == 24 && id.bytes().all(|b| b.is_ascii_hexdigit())
}
// Observed on the valid detail route for the absent all-zero object ID in Phase 3A.
// A generic HTTP 404, auth error or empty response is not this certificate.
fn certified_unavailable(body: &Value) -> bool {
    number(&body["code"]) == Some(404)
        && string(&body["error"]).as_deref() == Some("1007")
        && body["message"] == "not found"
}
fn signature(path: &str, method: &str, time: &str) -> String {
    let data = format!("{path}{time}{NONCE}{method}{API_KEY}").to_lowercase();
    let mut mac =
        Hmac::<Sha256>::new_from_slice(DIGEST_KEY.as_bytes()).expect("HMAC accepts any key size");
    mac.update(data.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn parse_record(v: &Value) -> Result<Record, String> {
    let id = string(&v["_id"]).ok_or("MISSING_PICA_ID")?;
    if !valid_id(&id) {
        return Err("INVALID_PICA_ID".into());
    }
    Ok(Record::new(
        "pica",
        id,
        authors(&v["author"]),
        v["title"].as_str().ok_or("MISSING_TITLE")?.into(),
        whitelist(
            v,
            &[
                "created_at",
                "updated_at",
                "pagesCount",
                "epsCount",
                "finished",
                "categories",
                "tags",
                "description",
                "chineseTeam",
                "allowDownload",
            ],
        ),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_observed_not_found_envelope_certifies_unavailable() {
        assert!(certified_unavailable(
            &json!({"code":404,"error":"1007","message":"not found"})
        ));
        assert!(!certified_unavailable(
            &json!({"code":404,"message":"not found"})
        ));
        assert!(!certified_unavailable(
            &json!({"code":401,"error":"1007","message":"not found"})
        ));
    }
    #[test]
    fn metadata_excludes_secrets_and_image_urls() {
        let r = parse_record(&json!({"_id":"0123456789abcdef01234567","title":"title","author":"writer","_creator":{"token":"secret"},"thumb":"image"})).unwrap();
        assert_eq!(r.author, vec!["writer"]);
        assert!(!r.metadata.to_string().contains("secret"));
    }
    #[test]
    fn metadata_body_limit_is_enforced_before_append() {
        assert!(checked_metadata_len(MAX_METADATA_BYTES as usize, 1).is_err());
        assert_eq!(checked_metadata_len(4, 5).unwrap(), 9);
    }
    #[test]
    fn signature_is_deterministic_and_path_sensitive() {
        assert_eq!(
            signature("comics/x", "GET", "1"),
            signature("comics/x", "GET", "1")
        );
        assert_ne!(
            signature("comics/x", "GET", "1"),
            signature("comics/y", "GET", "1")
        );
    }
    #[test]
    fn preflight_chapter_parser_requires_canonical_pinned_shape() {
        let chapters = vec![
            parse_preflight_chapter(&json!({"_id":"111111111111111111111111","order":1})).unwrap(),
            parse_preflight_chapter(&json!({"_id":"222222222222222222222222","order":2})).unwrap(),
        ];
        validate_preflight_chapters(&chapters).unwrap();
        let duplicate = vec![
            chapters[0].clone(),
            PicaPreflightChapter {
                chapter_id: chapters[0].chapter_id.clone(),
                chapter_order: 2,
            },
        ];
        assert!(validate_preflight_chapters(&duplicate).is_err());
        assert!(parse_preflight_chapter(&json!({"_id":"bad","order":1})).is_err());
        assert!(
            parse_preflight_chapter(&json!({"_id":"111111111111111111111111","order":0})).is_err()
        );
    }
    #[test]
    fn preflight_image_parser_requires_pinned_media_shape() {
        let valid = json!({
            "_id":"333333333333333333333333",
            "media":{"originalName":"1.jpg","path":"path","fileServer":"https://example.invalid"}
        });
        assert_eq!(
            validate_preflight_image_doc(&valid).unwrap(),
            "333333333333333333333333"
        );
        assert!(validate_preflight_image_doc(&json!({
            "_id":"333333333333333333333333","media":{"path":"x"}
        }))
        .is_err());
        assert!(validate_preflight_image_doc(&json!({
            "_id":"bad","media":{"originalName":"1","path":"x","fileServer":"y"}
        }))
        .is_err());
    }
}
