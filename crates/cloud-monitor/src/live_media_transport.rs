//! Private A6.14 media-byte HTTP transport.
//!
//! This module is intentionally not exported from the crate API. The only
//! supported caller is the guarded `live_media_fetch` bridge, which is then
//! wrapped by A6.12's per-fetch/per-write authorization generation checks.
//! These clients own no source API/session credentials, disable automatic
//! redirects/retries, bound each response in memory, and never touch files.
//! Desktop Pica may follow at most two same-origin /static/ redirects; every
//! physical GET first obtains a fresh grant from the staging coordinator.

use reqwest::{
    header::{LOCATION, USER_AGENT},
    redirect::Policy,
    Client, Request, StatusCode, Url,
};
use std::{collections::HashSet, future::Future, time::Duration};

pub(crate) const MAX_MEDIA_BYTES: u64 = 128 * 1024 * 1024;
const REQUEST_TIMEOUT_SECS: u64 = 60;
const CONNECT_TIMEOUT_SECS: u64 = 15;
const JM_PINNED_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";
const PICA_STORAGE_SUFFIX: &str = ".picacomic.com";
const MAX_PICA_REDIRECTS: usize = 2;

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
        // The client must not follow redirects without a fresh generation
        // check. Pica's explicit redirect loop performs that check per GET.
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
    let response = client
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
    read_bounded_body(prefix, response).await
}

async fn read_bounded_body(
    prefix: &str,
    mut response: reqwest::Response,
) -> Result<Vec<u8>, String> {
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

    /// The descriptor keeps its original source URL, so existing checkpoints
    /// remain valid. Each transport hop is exact, credential-free and bound to
    /// that URL's origin; no URL is inferred, rewritten or tried as a fallback.
    pub(crate) async fn fetch_with_redirects<Guard, Grant>(
        &self,
        url: &str,
        before_request: Guard,
    ) -> Result<Vec<u8>, String>
    where
        Guard: FnMut() -> Grant,
        Grant: Future<Output = Result<(), String>>,
    {
        self.fetch_redirect_chain(url, |request| self.send_pica_once(request), before_request)
            .await
    }

    async fn send_pica_once(&self, request: Request) -> Result<PicaHop, String> {
        let exact_url = request.url().clone();
        let response = self
            .client
            .execute(request)
            .await
            .map_err(|error| transport_error("PICA", error))?;
        if response.url() != &exact_url {
            return Err("PICA_MEDIA_RESPONSE_URL_MISMATCH".into());
        }
        if let Some(location) = pica_redirect_location(response.status(), response.headers())? {
            // A redirect's HTML body is never decoded as an image or saved.
            return Ok(PicaHop::Redirect(location));
        }
        read_bounded_body("PICA", response)
            .await
            .map(PicaHop::Bytes)
    }

    // The injected dispatcher is used only by in-module synthetic tests. They
    // exercise the same request builder, redirect loop and per-request grants.
    async fn fetch_redirect_chain<Dispatch, Response, Guard, Grant>(
        &self,
        url: &str,
        mut dispatch: Dispatch,
        mut before_request: Guard,
    ) -> Result<Vec<u8>, String>
    where
        Dispatch: FnMut(Request) -> Response,
        Response: Future<Output = Result<PicaHop, String>>,
        Guard: FnMut() -> Grant,
        Grant: Future<Output = Result<(), String>>,
    {
        let initial = validate_pica_url_exact(url)?;
        let mut current = initial.clone();
        let mut seen = HashSet::from([current.to_string()]);
        for hop in 0..=MAX_PICA_REDIRECTS {
            let request = self.build_request(current.as_str())?;
            // The worker cannot reuse a previous grant for a redirect. Nothing
            // else is awaited between this grant and the physical request.
            before_request().await?;
            match dispatch(request).await? {
                PicaHop::Bytes(bytes) => return Ok(bytes),
                PicaHop::Redirect(location) => {
                    if hop == MAX_PICA_REDIRECTS {
                        return Err("PICA_MEDIA_REDIRECT_LIMIT".into());
                    }
                    let next = pica_redirect_url(&initial, &location)?;
                    if !seen.insert(next.to_string()) {
                        return Err("PICA_MEDIA_REDIRECT_LOOP".into());
                    }
                    current = next;
                }
            }
        }
        Err("PICA_MEDIA_REDIRECT_LIMIT".into())
    }
}

enum PicaHop {
    Redirect(String),
    Bytes(Vec<u8>),
}

fn pica_redirect_location(
    status: StatusCode,
    headers: &reqwest::header::HeaderMap,
) -> Result<Option<String>, String> {
    if matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308) {
        return headers
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(|value| Some(value.to_owned()))
            .ok_or_else(|| "PICA_MEDIA_REDIRECT_LOCATION_INVALID".into());
    }
    if status != StatusCode::OK {
        return Err(format!("PICA_MEDIA_HTTP_{}", status.as_u16()));
    }
    Ok(None)
}

fn pica_redirect_url(initial: &Url, location: &str) -> Result<Url, String> {
    if location.is_empty()
        || location.len() > 4096
        || !location.is_ascii()
        || location
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == b' ')
        || location.contains(['\\', '?', '#'])
        || location.to_ascii_lowercase().contains("%2f")
        || location.to_ascii_lowercase().contains("%5c")
    {
        return Err("PICA_MEDIA_REDIRECT_LOCATION_INVALID".into());
    }
    let exact = if location.starts_with("/static/") {
        format!("{}{location}", initial.origin().ascii_serialization())
    } else if location.starts_with("https://") {
        location.to_owned()
    } else {
        return Err("PICA_MEDIA_REDIRECT_REJECTED".into());
    };
    let next = validate_pica_url_exact(&exact).map_err(|_| "PICA_MEDIA_REDIRECT_REJECTED")?;
    if next.origin() != initial.origin() {
        return Err("PICA_MEDIA_REDIRECT_REJECTED".into());
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, future::ready, io::Cursor};

    const PICA_START: &str = "https://storage-b.picacomic.com/static/tobs/fixture/page.jpg";

    #[tokio::test]
    async fn pica_301_downloads_valid_image_with_a_fresh_grant_for_each_request() {
        let transport = PicaTransport::new().unwrap();
        let mut encoded = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut encoded, image::ImageFormat::Jpeg)
            .unwrap();
        let expected = encoded.into_inner();
        let grants = Cell::new(0);
        let requests = Cell::new(0);
        let bytes = transport
            .fetch_redirect_chain(
                PICA_START,
                |request| {
                    assert_no_sensitive_headers(&request);
                    assert_eq!(request.method(), reqwest::Method::GET);
                    let hop = requests.get();
                    assert_eq!(grants.get(), hop + 1);
                    requests.set(hop + 1);
                    let response = if hop == 0 {
                        assert_eq!(request.url().as_str(), PICA_START);
                        let mut headers = reqwest::header::HeaderMap::new();
                        headers.insert(LOCATION, "/static/fixture/page.jpg".parse().unwrap());
                        PicaHop::Redirect(
                            pica_redirect_location(StatusCode::MOVED_PERMANENTLY, &headers)
                                .unwrap()
                                .unwrap(),
                        )
                    } else {
                        assert_eq!(
                            request.url().as_str(),
                            "https://storage-b.picacomic.com/static/fixture/page.jpg"
                        );
                        PicaHop::Bytes(expected.clone())
                    };
                    ready(Ok(response))
                },
                || {
                    grants.set(grants.get() + 1);
                    ready(Ok(()))
                },
            )
            .await
            .unwrap();
        assert_eq!(bytes, expected);
        crate::media_validation::validate("jpg", &bytes).unwrap();
        assert_eq!(requests.get(), 2);
        assert_eq!(grants.get(), 2);
    }

    #[tokio::test]
    async fn pica_pause_after_redirect_stops_before_the_next_physical_get() {
        let transport = PicaTransport::new().unwrap();
        let requests = Cell::new(0);
        let grants = Cell::new(0);
        let error = transport
            .fetch_redirect_chain(
                PICA_START,
                |_| {
                    requests.set(requests.get() + 1);
                    ready(Ok(PicaHop::Redirect("/static/fixture/page.jpg".into())))
                },
                || {
                    grants.set(grants.get() + 1);
                    ready(if grants.get() == 1 {
                        Ok(())
                    } else {
                        Err("DOWNLOAD_PAUSED".into())
                    })
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error, "DOWNLOAD_PAUSED");
        assert_eq!(requests.get(), 1);
        assert_eq!(grants.get(), 2);
    }

    #[tokio::test]
    async fn pica_unsafe_redirects_fail_before_another_request_or_grant() {
        for location in [
            "https://storage1.picacomic.com/static/fixture.jpg",
            "https://storage-b.picacomic.com.evil.invalid/static/fixture.jpg",
            "https://127.0.0.1/static/fixture.jpg",
            "http://storage-b.picacomic.com/static/fixture.jpg",
            "https://user:secret@storage-b.picacomic.com/static/fixture.jpg",
            "https://storage-b.picacomic.com:8443/static/fixture.jpg",
            "/static/fixture.jpg?token=secret",
            "/static/fixture.jpg#fragment",
            "/static/../private/fixture.jpg",
            "/static/%2e%2e/private/fixture.jpg",
            "/static/page%2fsecret.jpg",
            "/static/page%5csecret.jpg",
            "/static/page 1.jpg",
            "/static/fixture.jpg\r\nCookie: secret",
            "//storage-b.picacomic.com/static/fixture.jpg",
            "/private/fixture.jpg",
            "../fixture.jpg",
            "",
        ] {
            let transport = PicaTransport::new().unwrap();
            let requests = Cell::new(0);
            let grants = Cell::new(0);
            let result = transport
                .fetch_redirect_chain(
                    PICA_START,
                    |_| {
                        requests.set(requests.get() + 1);
                        ready(Ok(PicaHop::Redirect(location.into())))
                    },
                    || {
                        grants.set(grants.get() + 1);
                        ready(Ok(()))
                    },
                )
                .await;
            assert!(result.is_err(), "{location}");
            assert_eq!(requests.get(), 1, "{location}");
            assert_eq!(grants.get(), 1, "{location}");
        }
    }

    #[tokio::test]
    async fn pica_redirect_loops_and_long_chains_have_bounded_physical_counts() {
        let transport = PicaTransport::new().unwrap();
        let mut requests = 0;
        let mut grants = 0;
        let error = transport
            .fetch_redirect_chain(
                PICA_START,
                |_| {
                    requests += 1;
                    ready(Ok(PicaHop::Redirect(PICA_START.into())))
                },
                || {
                    grants += 1;
                    ready(Ok(()))
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error, "PICA_MEDIA_REDIRECT_LOOP");
        assert_eq!((requests, grants), (1, 1));
        requests = 0;
        grants = 0;
        let error = transport
            .fetch_redirect_chain(
                PICA_START,
                |_| {
                    requests += 1;
                    ready(Ok(PicaHop::Redirect(format!("/static/{requests}.jpg"))))
                },
                || {
                    grants += 1;
                    ready(Ok(()))
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error, "PICA_MEDIA_REDIRECT_LIMIT");
        assert_eq!((requests, grants), (3, 3));
    }

    #[tokio::test]
    async fn pica_transport_failure_does_not_retry_or_invent_another_url() {
        let transport = PicaTransport::new().unwrap();
        let mut requests = 0;
        let error = transport
            .fetch_redirect_chain(
                PICA_START,
                |_| {
                    requests += 1;
                    ready(Err("PICA_MEDIA_HTTP_503".into()))
                },
                || ready(Ok(())),
            )
            .await
            .unwrap_err();
        assert_eq!(error, "PICA_MEDIA_HTTP_503");
        assert_eq!(requests, 1);
    }

    #[test]
    fn pica_http_redirect_headers_are_required_and_error_bodies_are_not_images() {
        let mut headers = reqwest::header::HeaderMap::new();
        for status in [301, 302, 303, 307, 308] {
            let status = StatusCode::from_u16(status).unwrap();
            assert_eq!(
                pica_redirect_location(status, &headers).unwrap_err(),
                "PICA_MEDIA_REDIRECT_LOCATION_INVALID"
            );
        }
        headers.insert(LOCATION, "/static/fixture/page.jpg".parse().unwrap());
        for status in [301, 302, 303, 307, 308] {
            assert_eq!(
                pica_redirect_location(StatusCode::from_u16(status).unwrap(), &headers).unwrap(),
                Some("/static/fixture/page.jpg".into())
            );
        }
        assert!(pica_redirect_location(StatusCode::OK, &headers)
            .unwrap()
            .is_none());
        for status in [304, 401, 403, 404, 429, 500, 503] {
            assert_eq!(
                pica_redirect_location(StatusCode::from_u16(status).unwrap(), &headers)
                    .unwrap_err(),
                format!("PICA_MEDIA_HTTP_{status}")
            );
        }
    }

    #[test]
    fn jm_request_is_exact_and_contains_no_source_credentials() {
        let transport = JmTransport::new().unwrap();
        let url = "https://cdn-msp2.jmapiproxy2.cc/media/photos/123456/001.gif";
        let request = transport.build_request(url).unwrap();
        assert_eq!(request.method(), reqwest::Method::GET);
        assert_eq!(request.url().as_str(), url);
        assert_eq!(
            request.headers().get(USER_AGENT).unwrap(),
            JM_PINNED_USER_AGENT
        );
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
        assert!(
            validate_pica_url_exact("https://storage-b.picacomic.com/static/media/001.jpg").is_ok()
        );
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
