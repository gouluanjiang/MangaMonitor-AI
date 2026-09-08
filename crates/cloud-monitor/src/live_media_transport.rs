//! Private A6.14 media-byte HTTP transport.
//!
//! This module is intentionally not exported from the crate API. The only
//! supported caller is the guarded `live_media_fetch` bridge, which is then
//! wrapped by A6.12's per-fetch/per-write authorization generation checks.
//! These clients own no source API/session credentials, perform one exact GET,
//! disable redirects/retries, bound each response in memory, and never touch
//! the filesystem.

use reqwest::{header::USER_AGENT, redirect::Policy, Client, Request, StatusCode, Url};
use std::time::Duration;

pub(crate) const MAX_MEDIA_BYTES: u64 = 128 * 1024 * 1024;
const REQUEST_TIMEOUT_SECS: u64 = 60;
const CONNECT_TIMEOUT_SECS: u64 = 15;
const JM_PINNED_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";
const PICA_STORAGE_SUFFIX: &str = ".picacomic.com";

fn transport_error(prefix: &str, error: reqwest::Error) -> String {
    if error.is_timeout() {
        format!("{prefix}_MEDIA_FETCH_TIMEOUT")
    } else if error.is_connect() {
        format!("{prefix}_MEDIA_FETCH_CONNECT_ERROR")
    } else {
        format!("{prefix}_MEDIA_FETCH_TRANSPORT_ERROR")
    }
}

fn next_body_len(prefix: &str, current: u64, incoming: u64) -> Result<u64, String> {
    let next = current
        .checked_add(incoming)
        .ok_or_else(|| format!("{prefix}_MEDIA_SIZE_OVERFLOW"))?;
    if next > MAX_MEDIA_BYTES {
        return Err(format!("{prefix}_MEDIA_RESPONSE_TOO_LARGE"));
    }
    Ok(next)
}

fn append_bounded(prefix: &str, body: &mut Vec<u8>, chunk: &[u8]) -> Result<(), String> {
    let current = u64::try_from(body.len()).map_err(|_| format!("{prefix}_MEDIA_SIZE_OVERFLOW"))?;
    let incoming =
        u64::try_from(chunk.len()).map_err(|_| format!("{prefix}_MEDIA_SIZE_OVERFLOW"))?;
    next_body_len(prefix, current, incoming)?;
    body.extend_from_slice(chunk);
    Ok(())
}

fn base_client(prefix: &str) -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        // A redirect would be an additional source fetch without a fresh A6.10
        // generation check and would break the descriptor's exact URL binding.
        .redirect(Policy::none())
        .build()
        .map_err(|_| format!("{prefix}_MEDIA_CLIENT_INIT"))
}

async fn execute_exact(
    prefix: &str,
    client: &Client,
    request: Request,
    exact_url: &str,
) -> Result<Vec<u8>, String> {
    if request.url().as_str() != exact_url {
        return Err(format!("{prefix}_MEDIA_REQUEST_URL_MISMATCH"));
    }
    let mut response = client
        .execute(request)
        .await
        .map_err(|error| transport_error(prefix, error))?;
    if response.url().as_str() != exact_url {
        return Err(format!("{prefix}_MEDIA_RESPONSE_URL_MISMATCH"));
    }
    if response.status() != StatusCode::OK {
        return Err(format!(
            "{prefix}_MEDIA_HTTP_{}",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MEDIA_BYTES)
    {
        return Err(format!("{prefix}_MEDIA_RESPONSE_TOO_LARGE"));
    }

    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| transport_error(prefix, error))?
    {
        append_bounded(prefix, &mut body, &chunk)?;
    }
    if body.is_empty() {
        return Err(format!("{prefix}_MEDIA_RESPONSE_EMPTY"));
    }
    Ok(body)
}

#[derive(Clone)]
pub(crate) struct JmTransport {
    client: Client,
}

fn validate_jm_url_exact(url: &str) -> Result<Url, String> {
    let parsed = Url::parse(url).map_err(|_| "INVALID_JM_MEDIA_URL")?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some(jm_adapter::media_descriptors::IMAGE_DOMAIN)
        || parsed.port().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.path().starts_with("/media/photos/")
    {
        return Err("INVALID_JM_MEDIA_URL".into());
    }
    if parsed.as_str() != url {
        return Err("JM_MEDIA_URL_NORMALIZATION_DRIFT".into());
    }
    Ok(parsed)
}

impl JmTransport {
    pub(crate) fn new() -> Result<Self, String> {
        Ok(Self {
            client: base_client("JM")?,
        })
    }

    fn build_request(&self, url: &str) -> Result<Request, String> {
        let parsed = validate_jm_url_exact(url)?;
        self.client
            .get(parsed)
            .header(USER_AGENT, JM_PINNED_USER_AGENT)
            .build()
            .map_err(|_| "JM_MEDIA_REQUEST_BUILD_FAILED".into())
    }

    /// Exactly one query-free request. The pinned GUI's empty response fallback
    /// that appends `?ts=...` is deliberately not copied because it would be a
    /// different URL and an additional fetch without a fresh generation check.
    pub(crate) async fn fetch_exact(&self, url: &str) -> Result<Vec<u8>, String> {
        let request = self.build_request(url)?;
        execute_exact("JM", &self.client, request, url).await
    }
}

#[derive(Clone)]
pub(crate) struct PicaTransport {
    client: Client,
}

fn audited_pica_storage_host(host: &str) -> bool {
    // Pinned upstream concretely contains storage-b.picacomic.com. Keep only
    // storage-prefixed origins under the provider-owned domain; arbitrary
    // metadata-controlled hosts, literal IPs and localhost are not fetchable.
    host.starts_with("storage") && host.ends_with(PICA_STORAGE_SUFFIX)
}

fn validate_pica_url_exact(url: &str) -> Result<Url, String> {
    let parsed = Url::parse(url).map_err(|_| "INVALID_PICA_MEDIA_URL")?;
    let host = parsed.host_str().ok_or("INVALID_PICA_MEDIA_URL")?;
    if parsed.scheme() != "https"
        || !audited_pica_storage_host(host)
        || parsed.port().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.path().starts_with("/static/")
    {
        return Err("INVALID_PICA_MEDIA_URL".into());
    }
    if parsed.as_str() != url {
        return Err("PICA_MEDIA_URL_NORMALIZATION_DRIFT".into());
    }
    Ok(parsed)
}

impl PicaTransport {
    /// There is intentionally no token/credential parameter. This client is
    /// isolated from `PicaClient`, which owns API authorization state.
    pub(crate) fn new() -> Result<Self, String> {
        Ok(Self {
            client: base_client("PICA")?,
        })
    }

    fn build_request(&self, url: &str) -> Result<Request, String> {
        let parsed = validate_pica_url_exact(url)?;
        self.client
            .get(parsed)
            .build()
            .map_err(|_| "PICA_MEDIA_REQUEST_BUILD_FAILED".into())
    }

    pub(crate) async fn fetch_exact(&self, url: &str) -> Result<Vec<u8>, String> {
        let request = self.build_request(url)?;
        execute_exact("PICA", &self.client, request, url).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jm_request_is_exact_and_contains_no_source_credentials() {
        let transport = JmTransport::new().unwrap();
        let url = "https://cdn-msp2.jmapiproxy2.cc/media/photos/123456/001.gif";
        let request = transport.build_request(url).unwrap();
        assert_eq!(request.method(), reqwest::Method::GET);
        assert_eq!(request.url().as_str(), url);
        assert_eq!(request.headers().get(USER_AGENT).unwrap(), JM_PINNED_USER_AGENT);
        assert_no_sensitive_headers(&request);
    }

    #[test]
    fn jm_query_credentials_wrong_host_or_normalization_drift_fail_closed() {
        for url in [
            "https://cdn-msp2.jmapiproxy2.cc/media/photos/123456/001.gif?ts=1",
            "https://user:pass@cdn-msp2.jmapiproxy2.cc/media/photos/123456/001.gif",
            "https://evil.invalid/media/photos/123456/001.gif",
            "https://cdn-msp2.jmapiproxy2.cc/media/photos/123456/page 1.gif",
        ] {
            assert!(validate_jm_url_exact(url).is_err(), "{url}");
        }
    }

    #[test]
    fn pica_request_is_exact_and_contains_no_api_credentials() {
        let transport = PicaTransport::new().unwrap();
        let url = "https://storage-b.picacomic.com/static/media/path/001.png";
        let request = transport.build_request(url).unwrap();
        assert_eq!(request.method(), reqwest::Method::GET);
        assert_eq!(request.url().as_str(), url);
        assert_no_sensitive_headers(&request);
    }

    #[test]
    fn pica_metadata_controlled_ssrf_and_credential_forms_fail_closed() {
        for url in [
            "https://storage-b.picacomic.com/static/media/001.jpg?token=secret",
            "https://user:pass@storage-b.picacomic.com/static/media/001.jpg",
            "https://storage-b.picacomic.com:8443/static/media/001.jpg",
            "https://storage-b.picacomic.com/media/001.jpg",
            "https://evil.invalid/static/media/001.jpg",
            "https://localhost/static/media/001.jpg",
            "https://127.0.0.1/static/media/001.jpg",
            "https://[::1]/static/media/001.jpg",
            "https://picaapi.picacomic.com/static/media/001.jpg",
            "https://storage-b.picacomic.com/static/media/page 1.jpg",
        ] {
            assert!(validate_pica_url_exact(url).is_err(), "{url}");
        }
        assert!(validate_pica_url_exact(
            "https://storage-b.picacomic.com/static/media/001.jpg"
        )
        .is_ok());
    }

    #[test]
    fn fixed_size_limit_arithmetic_fails_before_oversized_append() {
        assert_eq!(next_body_len("JM", 0, 3).unwrap(), 3);
        assert_eq!(
            next_body_len("PICA", MAX_MEDIA_BYTES - 1, 1).unwrap(),
            MAX_MEDIA_BYTES
        );
        assert_eq!(
            next_body_len("PICA", MAX_MEDIA_BYTES, 1).unwrap_err(),
            "PICA_MEDIA_RESPONSE_TOO_LARGE"
        );
    }

    fn assert_no_sensitive_headers(request: &Request) {
        for sensitive in [
            "authorization",
            "cookie",
            "proxy-authorization",
            "api-key",
            "signature",
            "time",
            "nonce",
            "token",
            "tokenparam",
        ] {
            assert!(request.headers().get(sensitive).is_none(), "{sensitive}");
        }
    }
}