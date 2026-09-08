//! A6.14 exact Pica media-byte transport.
//!
//! This type deliberately cannot accept a Pica API token. It owns a separate
//! credential-free HTTP client, performs exactly one GET for an audited Pica
//! storage host, disables redirects/retries, bounds the response body, and
//! never touches the filesystem.

use reqwest::{redirect::Policy, Client, Request, StatusCode, Url};
use std::time::Duration;

pub const MAX_MEDIA_BYTES: u64 = 128 * 1024 * 1024;
const REQUEST_TIMEOUT_SECS: u64 = 60;
const CONNECT_TIMEOUT_SECS: u64 = 15;
const PICA_STORAGE_SUFFIX: &str = ".picacomic.com";

#[derive(Clone)]
pub struct PicaMediaFetcher {
    client: Client,
}

fn transport_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "PICA_MEDIA_FETCH_TIMEOUT".into()
    } else if error.is_connect() {
        "PICA_MEDIA_FETCH_CONNECT_ERROR".into()
    } else {
        "PICA_MEDIA_FETCH_TRANSPORT_ERROR".into()
    }
}

fn audited_storage_host(host: &str) -> bool {
    host.starts_with("storage")
        && host.ends_with(PICA_STORAGE_SUFFIX)
        && host.len() > "storage.picacomic.com".len()
}

fn validate_url_exact(url: &str) -> Result<Url, String> {
    let parsed = Url::parse(url).map_err(|_| "INVALID_PICA_MEDIA_URL")?;
    let host = parsed.host_str().ok_or("INVALID_PICA_MEDIA_URL")?;
    if parsed.scheme() != "https"
        || !audited_storage_host(host)
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

fn next_body_len(current: u64, incoming: u64) -> Result<u64, String> {
    let next = current
        .checked_add(incoming)
        .ok_or("PICA_MEDIA_SIZE_OVERFLOW")?;
    if next > MAX_MEDIA_BYTES {
        return Err("PICA_MEDIA_RESPONSE_TOO_LARGE".into());
    }
    Ok(next)
}

fn append_bounded(body: &mut Vec<u8>, chunk: &[u8]) -> Result<(), String> {
    let current = u64::try_from(body.len()).map_err(|_| "PICA_MEDIA_SIZE_OVERFLOW")?;
    let incoming = u64::try_from(chunk.len()).map_err(|_| "PICA_MEDIA_SIZE_OVERFLOW")?;
    next_body_len(current, incoming)?;
    body.extend_from_slice(chunk);
    Ok(())
}

impl PicaMediaFetcher {
    pub fn new() -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .redirect(Policy::none())
            .build()
            .map_err(|_| "PICA_MEDIA_CLIENT_INIT")?;
        Ok(Self { client })
    }

    fn build_request(&self, url: &str) -> Result<Request, String> {
        let parsed = validate_url_exact(url)?;
        let request = self
            .client
            .get(parsed)
            .build()
            .map_err(|_| "PICA_MEDIA_REQUEST_BUILD_FAILED")?;
        if request.url().as_str() != url {
            return Err("PICA_MEDIA_REQUEST_URL_MISMATCH".into());
        }
        Ok(request)
    }

    /// Legacy adapter-local transport retained only for regression coverage.
    /// External crates must enter live media transfer through cloud-monitor's
    /// A6.14 guarded bridge so A6.10 generation checks cannot be bypassed.
    #[allow(dead_code)]
    pub(crate) async fn fetch_exact(&self, url: &str) -> Result<Vec<u8>, String> {
        let request = self.build_request(url)?;
        let mut response = self.client.execute(request).await.map_err(transport_error)?;

        if response.url().as_str() != url {
            return Err("PICA_MEDIA_RESPONSE_URL_MISMATCH".into());
        }
        if response.status() != StatusCode::OK {
            return Err(format!("PICA_MEDIA_HTTP_{}", response.status().as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_MEDIA_BYTES)
        {
            return Err("PICA_MEDIA_RESPONSE_TOO_LARGE".into());
        }

        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
            append_bounded(&mut body, &chunk)?;
        }
        if body.is_empty() {
            return Err("PICA_MEDIA_RESPONSE_EMPTY".into());
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_is_exact_and_contains_no_pica_api_credentials() {
        let fetcher = PicaMediaFetcher::new().unwrap();
        let url = "https://storage-b.picacomic.com/static/media/path/001.png";
        let request = fetcher.build_request(url).unwrap();
        assert_eq!(request.method(), reqwest::Method::GET);
        assert_eq!(request.url().as_str(), url);
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

    #[test]
    fn metadata_controlled_ssrf_and_credential_forms_fail_closed() {
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
            assert!(validate_url_exact(url).is_err(), "{url}");
        }
        assert!(validate_url_exact(
            "https://storage-b.picacomic.com/static/media/001.jpg"
        )
        .is_ok());
        assert!(validate_url_exact(
            "https://storage2.picacomic.com/static/media/001.jpg"
        )
        .is_ok());
    }

    #[test]
    fn size_limit_arithmetic_fails_before_an_oversized_append() {
        assert_eq!(next_body_len(0, 3).unwrap(), 3);
        assert_eq!(next_body_len(MAX_MEDIA_BYTES - 1, 1).unwrap(), MAX_MEDIA_BYTES);
        assert_eq!(
            next_body_len(MAX_MEDIA_BYTES, 1).unwrap_err(),
            "PICA_MEDIA_RESPONSE_TOO_LARGE"
        );
    }
}
