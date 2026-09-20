use aes::{
    cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit},
    Aes256,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, Utc};
use hmac::{Hmac, Mac};
use reqwest::Url;
use serde_json::Value;
use sha2::Sha256;

use crate::{
    Source, SourceAccount, SourceError, SourceFolder, SourcePage, SourceResult, SourceWork,
};

pub(crate) const JM_HOST: &str = "www.cdnhth.cc";
pub(crate) const JM_API_HOSTS: &[&str] = &[
    "www.cdnhth.cc",
    "www.cdnzack.cc",
    "www.cdnhth.net",
    "www.cdnbea.net",
    "www.cdn-mspjmapiproxy.xyz",
];
pub(crate) const PICA_HOST: &str = "picaapi.picacomic.com";
pub(crate) const PICA_KEY: &str = "C69BAF41DA5ABD1FFEDC6D2FEA56B";
pub(crate) const PICA_NONCE: &str = "ptxdhmjzqtnrtwndhbxcpkjamb33w837";
// Public protocol constants, never personal account credentials.
const PICA_DIGEST: &str = r"~d}$Q7$eIni=V)9\RK/P.RM4;9[7|@/CA}b~OW!3?EV`:<>M7pddUBL5n|0/*Cn";
// Cover-only static mirrors from the existing Python pin, not dynamic trust:
// hect0x7/JMComic-Crawler-Python@9fddb0494caf0cdc812ac6cbfc1c62f4f845b058
// src/jmcomic/jm_config.py:184-192 and jm_toolkit.py:404-421. The legacy
// lanyeeee ComicCard.vue:54 endpoint remains the final candidate. API hosts,
// account headers, and the separate real-download adapters are unchanged.
pub(crate) const JM_COVER_HOSTS: &[&str] = &[
    "cdn-msp.jmapiproxy1.cc",
    "cdn-msp.jmapiproxy2.cc",
    "cdn-msp3.18comic.vip",
];
// Exact CDN identities only. storage-b is used by the pinned upstream UI:
// lanyeeee/picacomic-downloader@77c8b62ede42b3afc074506d092313816af8092d,
// src/AppContent.vue:109. Transformed paths and img redirects are evidenced in
// https://github.com/tonquer/picacg-qt/discussions/48 (2024-04-22 cover log).
// These protocol references do not grant trust to arbitrary *.picacomic.com.
const PICA_COVER_HOSTS: &[&str] = &[
    "storage1.picacomic.com",
    "s3.picacomic.com",
    "storage-b.picacomic.com",
    "img.picacomic.com",
];
pub(crate) const MAX_WORK_JSON_BYTES: usize = 64 * 1024;
const MAX_JS_COUNT: u64 = 9_007_199_254_740_991;

pub(crate) fn error(code: &'static str) -> SourceError {
    SourceError::new(code)
}

pub(crate) fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) if value.is_u64() => Some(value.to_string()),
        _ => None,
    }
}

pub(crate) fn required_text(value: &Value) -> SourceResult<String> {
    text(value)
        .filter(|s| !s.trim().is_empty() && s.len() <= 16_384)
        .ok_or(error("SOURCE_RESPONSE_INVALID"))
}

// Match JavaScript String.length so astral characters cannot pass a native
// limit and then fail the renderer's validation of the same field.
fn within_text_limit(value: &str, maximum: usize) -> bool {
    value.encode_utf16().take(maximum + 1).count() <= maximum
}

pub(crate) fn bounded_required_text(value: &Value, maximum: usize) -> SourceResult<String> {
    required_text(value).and_then(|text| {
        if within_text_limit(&text, maximum) {
            Ok(text)
        } else {
            Err(error("SOURCE_RESPONSE_INVALID"))
        }
    })
}

pub(crate) fn count(value: &Value) -> SourceResult<Option<u64>> {
    if value.is_null() {
        return Ok(None);
    }
    let result = value.as_u64().or_else(|| {
        value.as_str().and_then(|s| {
            (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                .then(|| s.parse().ok())
                .flatten()
        })
    });
    result
        .filter(|value| *value <= MAX_JS_COUNT)
        .map(Some)
        .ok_or(error("SOURCE_RESPONSE_INVALID"))
}

fn strings(source: Source, value: &Value) -> SourceResult<Vec<String>> {
    if value.is_null() {
        return Ok(vec![]);
    }
    if let Some(s) = value.as_str() {
        return Ok(if s.trim().is_empty() {
            vec![]
        } else {
            vec![bounded_required_text(value, 2000)?]
        });
    }
    let values = value.as_array().ok_or(error("SOURCE_RESPONSE_INVALID"))?;
    if values.len() > 64 {
        return Err(error("SOURCE_RESPONSE_INVALID"));
    }
    if source == Source::Jm {
        // JM detail metadata can contain blank author/tag placeholders. Check
        // the raw array and every string before omitting only those placeholders;
        // filtering must not bypass the input bounds or coerce unknown shapes.
        let mut retained = Vec::with_capacity(values.len());
        for value in values {
            let text = value.as_str().ok_or(error("SOURCE_RESPONSE_INVALID"))?;
            if !within_text_limit(text, 2000) {
                return Err(error("SOURCE_RESPONSE_INVALID"));
            }
            if !text.trim().is_empty() {
                retained.push(text.to_owned());
            }
        }
        return Ok(retained);
    }
    values
        .iter()
        .map(|value| bounded_required_text(value, 2000))
        .collect()
}

pub(crate) fn optional_bool(value: &Value) -> SourceResult<Option<bool>> {
    if value.is_null() {
        Ok(None)
    } else {
        value
            .as_bool()
            .map(Some)
            .ok_or(error("SOURCE_RESPONSE_INVALID"))
    }
}

pub fn parse_work_id(source: Source, input: &str) -> SourceResult<String> {
    let input = input.trim();
    if input.len() > 2048 {
        return Err(error("SOURCE_WORK_ID_INVALID"));
    }
    if valid_id(source, input) {
        return Ok(normalize_id(source, input));
    }
    // These are parsed locally. The pasted URL is never requested.
    let url = Url::parse(input).map_err(|_| error("SOURCE_WORK_ID_INVALID"))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return Err(error("SOURCE_WORK_ID_INVALID"));
    }
    let host = url.host_str().ok_or(error("SOURCE_WORK_ID_INVALID"))?;
    let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
    let id = match source {
        Source::Jm
            if JM_API_HOSTS.contains(&host)
                || ["18comic.vip", "www.18comic.vip"].contains(&host) =>
        {
            if parts.len() == 2 && parts[0] == "album" && url.query().is_none() {
                parts[1].to_owned()
            } else if JM_API_HOSTS.contains(&host) && url.path() == "/album" {
                let pairs: Vec<_> = url.query_pairs().collect();
                if pairs.len() != 1 || pairs[0].0 != "id" {
                    return Err(error("SOURCE_WORK_ID_INVALID"));
                }
                pairs[0].1.to_string()
            } else {
                return Err(error("SOURCE_WORK_ID_INVALID"));
            }
        }
        Source::Pica
            if host == PICA_HOST
                && parts.len() == 2
                && parts[0] == "comics"
                && url.query().is_none() =>
        {
            parts[1].to_owned()
        }
        _ => return Err(error("SOURCE_WORK_ID_INVALID")),
    };
    if !valid_id(source, &id) {
        return Err(error("SOURCE_WORK_ID_INVALID"));
    }
    Ok(normalize_id(source, &id))
}

fn normalize_id(source: Source, id: &str) -> String {
    match source {
        Source::Jm => id.trim_start_matches('0').to_owned(),
        Source::Pica => id.to_ascii_lowercase(),
    }
}

fn valid_id(source: Source, id: &str) -> bool {
    match source {
        Source::Jm => {
            !id.is_empty()
                && id.len() <= 19
                && id.bytes().all(|b| b.is_ascii_digit())
                && id.parse::<u64>().is_ok_and(|id| id > 0)
        }
        Source::Pica => id.len() == 24 && id.bytes().all(|b| b.is_ascii_hexdigit()),
    }
}

pub(crate) fn validate_page(page: u64) -> SourceResult<()> {
    if (1..=100_000).contains(&page) {
        Ok(())
    } else {
        Err(error("SOURCE_PAGE_INVALID"))
    }
}

pub(crate) fn account(source: Source, data: &Value) -> SourceResult<SourceAccount> {
    let (value, id, name) = match source {
        Source::Jm => (data, "uid", "username"),
        Source::Pica => (&data["user"], "_id", "name"),
    };
    Ok(SourceAccount {
        source,
        account_id: required_text(&value[id])?,
        display_name: required_text(&value[name])?,
    })
}

// The pinned JM search/weekly schemas expose update_at as Unix seconds;
// Pica search/detail schemas expose updated_at as an ISO date-time. Never
// substitute addtime/adddate/created_at or the moment this response was read.
// Invalid optional dates leave the otherwise readable work available.
fn source_update_date(source: Source, data: &Value) -> Option<String> {
    let value = &data[match source {
        Source::Jm => "update_at",
        Source::Pica => "updated_at",
    }];
    if source == Source::Jm {
        if let Some(seconds) = value.as_i64() {
            if seconds <= 0 {
                return None;
            }
            let timestamp = DateTime::<Utc>::from_timestamp(seconds, 0)?;
            return valid_update_timestamp(timestamp)
                .then(|| timestamp.to_rfc3339_opts(SecondsFormat::Millis, true));
        }
    }
    let text = value.as_str()?;
    if text.len() > 64 || text.chars().any(char::is_control) {
        return None;
    }
    if text.len() == 10 {
        let date = NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()?;
        return (date.format("%Y-%m-%d").to_string() == text
            && (1900..=9999).contains(&date.year())
            && date != NaiveDate::from_ymd_opt(1970, 1, 1)?)
        .then(|| text.to_owned());
    }
    let timestamp = DateTime::parse_from_rfc3339(text).ok()?.with_timezone(&Utc);
    valid_update_timestamp(timestamp)
        .then(|| timestamp.to_rfc3339_opts(SecondsFormat::Millis, true))
}

fn valid_update_timestamp(timestamp: DateTime<Utc>) -> bool {
    (1900..=9999).contains(&timestamp.year())
        && timestamp.date_naive() != NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()
        // JavaScript Date cannot represent chrono's leap-second extension.
        && timestamp.timestamp_subsec_nanos() < 1_000_000_000
}

pub(crate) fn work(
    source: Source,
    data: &Value,
    favorite_listing: bool,
) -> SourceResult<(SourceWork, Option<String>)> {
    parse_work(source, data, favorite_listing, false)
}

pub(crate) fn listing_work(
    source: Source,
    data: &Value,
    favorite_listing: bool,
) -> SourceResult<(SourceWork, Option<String>)> {
    parse_work(source, data, favorite_listing, true)
}

fn parse_work(
    source: Source,
    data: &Value,
    favorite_listing: bool,
    listing: bool,
) -> SourceResult<(SourceWork, Option<String>)> {
    let id_field = match source {
        Source::Jm => "id",
        Source::Pica => "_id",
    };
    let id = required_text(&data[id_field])?;
    if !valid_id(source, &id) {
        return Err(error("SOURCE_RESPONSE_INVALID"));
    }
    let work_id = normalize_id(source, &id);
    let title_value = &data[match source {
        Source::Jm => "name",
        Source::Pica => "title",
    }];
    let missing_title = listing
        && source == Source::Jm
        && title_value
            .as_str()
            .is_some_and(|name| name.trim().is_empty());
    // JM can retain a valid catalog ID whose metadata has become blank. Keep
    // its position/count without inventing an author or reading every detail.
    // The strict detail path still rejects it before download preparation.
    let title = if missing_title {
        if !within_text_limit(title_value.as_str().unwrap(), 2000) {
            return Err(error("SOURCE_RESPONSE_INVALID"));
        }
        format!("来源作品信息缺失（JM{work_id}）")
    } else {
        bounded_required_text(title_value, 2000)?
    };
    let cover = if missing_title {
        None
    } else {
        cover_url(source, &work_id, data)
    };
    let favorite = if favorite_listing {
        Some(true)
    } else {
        optional_bool(
            &data[match source {
                Source::Jm => "is_favorite",
                Source::Pica => "isFavourite",
            }],
        )?
    };
    let (chapter_count, page_count) = match source {
        Source::Jm => (None, count(&data["total_photos"])?),
        Source::Pica => (
            count(&data["epsCount"])?,
            // Pica can report a negative page count even in a readable favorite.
            // Keep the work with an unknown count; never invent zero or abs(count).
            if data["pagesCount"].as_i64().is_some_and(|count| count < 0) {
                None
            } else {
                count(&data["pagesCount"])?
            },
        ),
    };
    let work = SourceWork {
        source,
        work_id,
        title,
        authors: strings(source, &data["author"])?,
        description: if data["description"].is_null() {
            None
        } else {
            let text = data["description"]
                .as_str()
                .ok_or(error("SOURCE_RESPONSE_INVALID"))?;
            if !within_text_limit(text, 10_000) {
                return Err(error("SOURCE_RESPONSE_INVALID"));
            }
            Some(text.to_owned())
        },
        tags: strings(source, &data["tags"])?,
        favorite,
        chapter_count,
        page_count,
        source_updated_at: source_update_date(source, data),
        cover_available: cover.is_some(),
    };
    // Bound the actual IPC representation, including JSON string escaping.
    let serialized = serde_json::to_vec(&work).map_err(|_| error("SOURCE_RESPONSE_INVALID"))?;
    if serialized.len() > MAX_WORK_JSON_BYTES {
        return Err(error("SOURCE_RESPONSE_INVALID"));
    }
    Ok((work, cover))
}

pub(crate) fn page(
    source: Source,
    data: &Value,
    requested: u64,
    favorites: bool,
) -> SourceResult<(SourcePage, Vec<(String, String)>)> {
    validate_page(requested)?;
    let body = match source {
        Source::Jm => data,
        Source::Pica => &data["comics"],
    };
    let records = body[match source {
        Source::Jm if favorites => "list",
        Source::Jm => "content",
        Source::Pica => "docs",
    }]
    .as_array()
    .ok_or(error("SOURCE_RESPONSE_INVALID"))?;
    if records.len() > 1000 {
        return Err(error("SOURCE_RESPONSE_INVALID"));
    }
    let total = count(&body["total"])?;
    if total.is_some_and(|total| total < records.len() as u64)
        || (records.is_empty() && total.is_some_and(|total| total > 0))
    {
        return Err(error("SOURCE_PAGINATION_INVALID"));
    }
    let (pages, has_more) = match source {
        Source::Jm => (None, if total == Some(0) { Some(false) } else { None }),
        Source::Pica => {
            let page = count(&body["page"])?.ok_or(error("SOURCE_PAGINATION_INVALID"))?;
            let pages = count(&body["pages"])?.ok_or(error("SOURCE_PAGINATION_INVALID"))?;
            let limit = count(&body["limit"])?.ok_or(error("SOURCE_PAGINATION_INVALID"))?;
            let total = total.ok_or(error("SOURCE_PAGINATION_INVALID"))?;
            if page != requested
                || limit == 0
                || records.len() as u64 > limit
                || records.len() as u64 > total
                || (total > 0
                    && (pages != total.div_ceil(limit) || page > pages || records.is_empty()))
                || (total == 0 && (!records.is_empty() || pages > 1))
            {
                return Err(error("SOURCE_PAGINATION_INVALID"));
            }
            (Some(pages), Some(page < pages))
        }
    };
    let mut items = Vec::with_capacity(records.len());
    let mut covers = Vec::new();
    let mut ids = std::collections::HashMap::new();
    for record in records {
        let (item, cover) = listing_work(source, record, favorites)?;
        // Preserve identical Pica favorite entries for pagination accounting.
        // Conflicting records, search results and JM remain strict.
        if ids
            .insert(item.work_id.clone(), record)
            .is_some_and(|previous| source != Source::Pica || !favorites || previous != record)
        {
            return Err(error("SOURCE_PAGINATION_INVALID"));
        }
        if let Some(cover) = cover {
            covers.push((item.work_id.clone(), cover));
        }
        items.push(item);
    }
    let mut folders = vec![];
    if source == Source::Jm && favorites {
        let values = data["folder_list"]
            .as_array()
            .ok_or(error("SOURCE_RESPONSE_INVALID"))?;
        if values.len() > 1000 {
            return Err(error("SOURCE_RESPONSE_INVALID"));
        }
        for folder in values {
            let folder_id = required_text(&folder["FID"])?;
            validate_folder(&folder_id)?;
            folders.push(SourceFolder {
                id: folder_id,
                name: bounded_required_text(&folder["name"], 2000)?,
                // The pinned folder schema establishes no per-folder count.
                count: None,
            });
        }
    }
    Ok((
        SourcePage {
            page: requested,
            total,
            pages,
            has_more,
            folders,
            items,
        },
        covers,
    ))
}

pub(crate) fn validate_folder(folder: &str) -> SourceResult<()> {
    if !folder.is_empty() && folder.len() <= 19 && folder.bytes().all(|b| b.is_ascii_digit()) {
        Ok(())
    } else {
        Err(error("SOURCE_FOLDER_INVALID"))
    }
}

fn cover_url(source: Source, id: &str, data: &Value) -> Option<String> {
    match source {
        Source::Jm => Some(format!(
            "https://{}/media/albums/{id}_3x4.jpg",
            JM_COVER_HOSTS[0]
        )),
        Source::Pica => {
            let server = Url::parse(data["thumb"]["fileServer"].as_str()?).ok()?;
            if server.scheme() != "https"
                || !PICA_COVER_HOSTS.contains(&server.host_str()?)
                || !server.username().is_empty()
                || server.password().is_some()
                || server.port().is_some()
                || server.query().is_some()
                || server.fragment().is_some()
                || server.path() != "/"
            {
                return None;
            }
            let path = data["thumb"]["path"].as_str()?;
            if path.starts_with('/') || path.len() > 1024 || !valid_cover_path(path) {
                return None;
            }
            let url = Url::parse(&format!("https://{}/static/{path}", server.host_str()?)).ok()?;
            validate_cover_url(source, &url).ok()?;
            Some(url.to_string())
        }
    }
}

fn valid_cover_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 2048
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_/.:=+".contains(&b))
        && !path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
}

pub(crate) fn validate_cover_url(source: Source, url: &Url) -> SourceResult<()> {
    let allowed_host = match source {
        Source::Jm => url
            .host_str()
            .is_some_and(|host| JM_COVER_HOSTS.contains(&host)),
        Source::Pica => url
            .host_str()
            .is_some_and(|host| PICA_COVER_HOSTS.contains(&host)),
    };
    let path = url.path().strip_prefix('/').unwrap_or_default();
    let allowed_path = match source {
        Source::Jm => path
            .strip_prefix("media/albums/")
            .and_then(|name| name.strip_suffix("_3x4.jpg"))
            .is_some_and(|id| valid_id(Source::Jm, id)),
        Source::Pica => {
            let parts: Vec<_> = path.split('/').collect();
            path.starts_with("static/")
                || (parts.len() == 4 && parts[1].starts_with("rs:") && parts[2].starts_with("g:"))
        }
    };
    if url.scheme() != "https"
        || !allowed_host
        || !allowed_path
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !valid_cover_path(path)
    {
        return Err(error("SOURCE_REDIRECT_REFUSED"));
    }
    Ok(())
}

pub(crate) fn cover_redirect(source: Source, current: &Url, location: &str) -> SourceResult<Url> {
    // Check before Url::join normalizes dot segments or backslashes. No URL
    // escapes, credentials, new schemes, fragments, or query-token forwarding.
    if location.is_empty()
        || location.len() > 4096
        || location.chars().any(char::is_control)
        || location.contains('%')
        || location.contains('\\')
        || location.split('/').any(|part| part == "." || part == "..")
    {
        return Err(error("SOURCE_REDIRECT_REFUSED"));
    }
    let next = current
        .join(location)
        .map_err(|_| error("SOURCE_REDIRECT_REFUSED"))?;
    validate_cover_url(source, &next)?;
    if source == Source::Jm && next.path() != current.path() {
        return Err(error("SOURCE_REDIRECT_REFUSED"));
    }
    Ok(next)
}

pub(crate) fn cover_candidates(source: Source, original: &Url) -> SourceResult<Vec<Url>> {
    validate_cover_url(source, original)?;
    if source == Source::Pica {
        return Ok(vec![original.clone()]);
    }
    JM_COVER_HOSTS
        .iter()
        .map(|host| {
            let mut candidate = original.clone();
            candidate
                .set_host(Some(host))
                .map_err(|_| error("SOURCE_COVER_INVALID"))?;
            validate_cover_url(source, &candidate)?;
            Ok(candidate)
        })
        .collect()
}

pub(crate) fn pica_signature(route: &str, method: &str, timestamp: u64) -> SourceResult<String> {
    let message = format!("{route}{timestamp}{PICA_NONCE}{method}{PICA_KEY}").to_lowercase();
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(PICA_DIGEST.as_bytes())
        .map_err(|_| error("SOURCE_CLIENT_FAILED"))?;
    mac.update(message.as_bytes());
    Ok(mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub(crate) fn decode_jm(timestamp: u64, encoded: &str) -> SourceResult<Value> {
    let mut bytes = STANDARD
        .decode(encoded)
        .map_err(|_| error("SOURCE_RESPONSE_INVALID"))?;
    if bytes.is_empty() || bytes.len() % 16 != 0 {
        return Err(error("SOURCE_RESPONSE_INVALID"));
    }
    let key = format!("{:x}", md5::compute(format!("{timestamp}185Hcomic3PAPP7R")));
    let cipher =
        Aes256::new_from_slice(key.as_bytes()).map_err(|_| error("SOURCE_CLIENT_FAILED"))?;
    for block in bytes.as_chunks_mut::<16>().0 {
        cipher.decrypt_block(GenericArray::from_mut_slice(block));
    }
    let padding = usize::from(*bytes.last().ok_or(error("SOURCE_RESPONSE_INVALID"))?);
    if !(1..=16).contains(&padding)
        || bytes[bytes.len() - padding..]
            .iter()
            .any(|b| usize::from(*b) != padding)
    {
        return Err(error("SOURCE_RESPONSE_INVALID"));
    }
    bytes.truncate(bytes.len() - padding);
    serde_json::from_slice(&bytes).map_err(|_| error("SOURCE_RESPONSE_INVALID"))
}
