//! A6.14 exact JM media-byte transport.
//!
//! This fetcher owns no JM API/session credentials and never touches the
//! filesystem. It performs exactly one GET for the exact A6.13 media URL,
//! disables redirects/retries, bounds the response body in memory, and returns
//! raw bytes only. Scramble processing remains a higher-layer responsibility.

use reqwest::{header::USER_AGENT, redirect::Policy, Client, Request, StatusCode, Url};
use std::time::Duration;

use crate::media_descriptors::IMAGE_DOMAIN;

pub const MAX_MEDIA_BYTES: u64 = 128 * 1024 * 1024;
const REQUEST_TIMEOUT_SECS: u64 = 60;
const CONNECT_TIMEOUT_SECS: u64 = 15;
const PINNED_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";

#[derive(Clone)]
pub struct JmMediaFetcher {
    client: Client,
}

fn transport_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "JM_MEDIA_FETCH_TIMEOUT".into()
    } else if error.is_connect() {
        "JM_MEDIA_FETCH_CONNECT_ERROR".into()
    } else {
        "JM_MEDIA_FETCH_TRANSPORT_ERROR".into()
    }
}

fn validate_url_exact(url: &str) -> Result<Url, String> {
    let parsed = Url::parse(url).map_err(|_| "INVALID_JM_MEDIA_URL")?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some(IMAGE_DOMAIN)
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

fn next_body_len(current: u64, incoming: u64) -> Result<u64, String> {
    let next = current
        .checked_add(incoming)
        .ok_or("JM_MEDIA_SIZE_OVERFLOW")?;
    if next > MAX_MEDIA_BYTES {
        return Err("JM_MEDIA_RESPONSE_TOO_LARGE".into());
    }
    Ok(next)
}

fn append_bounded(body: &mut Vec<u8>, chunk: &[u8]) -> Result<(), String> {
    let current = u64::try_from(body.len()).map_err(|_| "JM_MEDIA_SIZE_OVERFLOW")?;
    let incoming = u64::try_from(chunk.len()).map_err(|_| "JM_MEDIA_SIZE_OVERFLOW")?;
    next_body_len(current, incoming)?;
    body.extend_from_slice(chunk);
    Ok(())
}

impl JmMediaFetcher {
    pub fn new() -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .redirect(Policy::none())
            .build()
            .map_err(|_| "JM_MEDIA_CLIENT_INIT")?;
        Ok(Self { client })
    }

    fn build_request(&self, url: &str) -> Result<Request, String> {
        let parsed = validate_url_exact(url)?;
        let request = self
            .client
            .get(parsed)
            .header(USER_AGENT, PINNED_USER_AGENT)
            .build()
            .map_err(|_| "JM_MEDIA_REQUEST_BUILD_FAILED")?;
        if request.url().as_str() != url {
            return Err("JM_MEDIA_REQUEST_URL_MISMATCH".into());
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
            return Err("JM_MEDIA_RESPONSE_URL_MISMATCH".into());
        }
        if response.status() != StatusCode::OK {
            return Err(format!("JM_MEDIA_HTTP_{}", response.status().as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_MEDIA_BYTES)
        {
            return Err("JM_MEDIA_RESPONSE_TOO_LARGE".into());
        }

        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
            append_bounded(&mut body, &chunk)?;
        }
        if body.is_empty() {
            return Err("JM_MEDIA_RESPONSE_EMPTY".into());
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_is_exact_and_contains_no_api_or_session_credentials() {
        let fetcher = JmMediaFetcher::new().unwrap();
        let url = "https://cdn-msp2.jmapiproxy2.cc/media/photos/123456/001.gif";
        let request = fetcher.build_request(url).unwrap();
        assert_eq!(request.method(), reqwest::Method::GET);
        assert_eq!(request.url().as_str(), url);
        assert_eq!(request.headers().get(USER_AGENT).unwrap(), PINNED_USER_AGENT);
        for sensitive in [
            "authorization",
            "cookie",
            "proxy-authorization",
            "token",
            "tokenparam",
            "api-key",
            "signature",
            "nonce",
        ] {
            assert!(request.headers().get(sensitive).is_none(), "{sensitive}");
        }
    }

    #[test]
    fn query_credentials_redirectable_host_or_normalization_drift_fail_closed() {
        assert!(validate_url_exact(
            "https://cdn-msp2.jmapiproxy2.cc/media/photos/123456/001.gif?ts=1"
        )
        .is_err());
        assert!(validate_url_exact(
            "https://user:pass@cdn-msp2.jmapiproxy2.cc/media/photos/123456/001.gif"
        )
        .is_err());
        assert!(validate_url_exact(
            "https://evil.invalid/media/photos/123456/001.gif"
        )
        .is_err());
        assert!(validate_url_exact(
            "https://cdn-msp2.jmapiproxy2.cc/media/photos/123456/page 1.gif"
        )
        .is_err());
    }

    #[test]
    fn size_limit_arithmetic_fails_before_an_oversized_append() {
        assert_eq!(next_body_len(0, 3).unwrap(), 3);
        assert_eq!(next_body_len(MAX_MEDIA_BYTES - 1, 1).unwrap(), MAX_MEDIA_BYTES);
        assert_eq!(
            next_body_len(MAX_MEDIA_BYTES, 1).unwrap_err(),
            "JM_MEDIA_RESPONSE_TOO_LARGE"
        );
    }
}
