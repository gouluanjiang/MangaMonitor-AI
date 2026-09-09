//! Credential-free thumbnail transport. Redirects are followed manually only
//! after each destination passes the source-specific origin/path allowlist.
use crate::{
    bounded_bytes, check_live_environment, protocol, thumbnail, transport_error, CoverLookup,
    Source, SourceError, SourceResult, SourceSession, WorkbenchSources,
};
use protocol::error;
use reqwest::{
    header::{ACCEPT, LOCATION, USER_AGENT},
    Url,
};
use std::{collections::HashSet, time::Duration};

// One shared budget for redirects and fixed mirror failover, never four GETs
// per mirror. The public thumbnail entry point also enforces 30 seconds total.
const MAX_COVER_REQUESTS: usize = 4;
const JM_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
// Public browser identity from the same Python pin's jm_config.py:211-215.
// No account identity, app Referer, or cookie is sent to any cover origin.
const JM_COVER_USER_AGENT: &str = "Mozilla/5.0 (Linux; Android 9; V1938CT Build/PQ3A.190705.11211812; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/91.0.4472.114 Safari/537.36";
const COVER_ACCEPT: &str = "image/jpeg,image/png,image/webp,image/gif";

pub(super) enum CoverResponse {
    Redirect(String),
    Missing,
    HttpFailure(u16),
    Bytes(Vec<u8>),
}

fn http_error(status: u16) -> SourceError {
    error(match status {
        401 | 403 => "SOURCE_COVER_ACCESS_DENIED",
        429 => "SOURCE_COVER_RATE_LIMITED",
        500..=599 => "SOURCE_COVER_SERVER_ERROR",
        _ => "SOURCE_COVER_UNAVAILABLE",
    })
}

fn retryable_transport(cause: &SourceError) -> bool {
    matches!(cause.code, "SOURCE_CONNECTION_FAILED" | "SOURCE_TIMEOUT")
}

fn lookup(session: &SourceSession, id: &str) -> SourceResult<CoverLookup> {
    Ok(session
        .covers
        .lock()
        .map_err(|_| error("SOURCE_SESSION_FAILED"))?
        .lookup(id))
}

impl WorkbenchSources {
    async fn cover_descriptor(
        &self,
        session: &SourceSession,
        id: &str,
    ) -> SourceResult<Option<String>> {
        match lookup(session, id)? {
            CoverLookup::Unknown => return Err(error("WORK_NOT_LOADED")),
            CoverLookup::Missing => return Ok(None),
            CoverLookup::Ready(url) => return Ok(Some(url)),
            CoverLookup::Evicted => (),
        }
        // Recheck under the metadata lock so simultaneous cover callers perform
        // at most one recovery detail read. Recovery grants no favorite action.
        let _operation = session.operation.lock().await;
        match lookup(session, id)? {
            CoverLookup::Unknown => return Err(error("WORK_NOT_LOADED")),
            CoverLookup::Missing => return Ok(None),
            CoverLookup::Ready(url) => return Ok(Some(url)),
            CoverLookup::Evicted => (),
        }
        self.detail_inner(session, id).await?;
        match lookup(session, id)? {
            CoverLookup::Missing => Ok(None),
            CoverLookup::Ready(url) => Ok(Some(url)),
            _ => Err(error("SOURCE_COVER_UNAVAILABLE")),
        }
    }

    pub(super) async fn thumbnail_inner(
        &self,
        session: &SourceSession,
        id: &str,
    ) -> SourceResult<Option<String>> {
        let Some(url) = self.cover_descriptor(session, id).await? else {
            return Ok(None);
        };
        let original = Url::parse(&url).map_err(|_| error("SOURCE_COVER_INVALID"))?;
        let candidates = protocol::cover_candidates(session.source, &original)?;
        let mut url = candidates[0].clone();
        let mut visited = HashSet::new();
        let mut last_error = None;
        for attempt in 0..MAX_COVER_REQUESTS {
            protocol::validate_cover_url(session.source, &url)?;
            if !visited.insert(url.to_string()) {
                return Err(error("SOURCE_REDIRECT_REFUSED"));
            }
            match self.send_cover_request(&url).await {
                Err(cause) if session.source == Source::Jm && retryable_transport(&cause) => {
                    last_error = Some(cause);
                }
                Err(cause) => return Err(cause),
                Ok(CoverResponse::Missing) => (),
                Ok(CoverResponse::HttpFailure(status))
                    if session.source == Source::Jm && matches!(status, 502..=504) =>
                {
                    last_error = Some(http_error(status));
                }
                Ok(CoverResponse::HttpFailure(status)) => return Err(http_error(status)),
                Ok(CoverResponse::Redirect(location)) => {
                    if attempt + 1 == MAX_COVER_REQUESTS {
                        return Err(error("SOURCE_REDIRECT_REFUSED"));
                    }
                    url = protocol::cover_redirect(session.source, &url, &location)?;
                    continue;
                }
                Ok(CoverResponse::Bytes(bytes)) => {
                    if bytes.len() > thumbnail::MAX_COVER_BYTES {
                        return Err(error("SOURCE_RESPONSE_TOO_LARGE"));
                    }
                    return tokio::task::spawn_blocking(move || thumbnail::data_url(&bytes))
                        .await
                        .map_err(|_| error("SOURCE_COVER_INVALID"))?
                        .map(Some);
                }
            }
            // A rejected/absent image does not broaden URL trust. Only transport
            // failures, 502-504, or 404 reach another fixed JM candidate; 403,
            // 429, invalid redirects, oversized responses and decoding errors
            // return immediately. Pica has one metadata-supplied candidate.
            let Some(next) = candidates
                .iter()
                .find(|candidate| !visited.contains(candidate.as_str()))
            else {
                return Err(last_error.unwrap_or(error("SOURCE_COVER_NOT_FOUND")));
            };
            url = next.clone();
        }
        Err(last_error.unwrap_or(error("SOURCE_COVER_UNAVAILABLE")))
    }

    pub(super) fn cover_request(&self, url: &Url) -> reqwest::RequestBuilder {
        // No SourceSession argument, cookie jar, Authorization, Referer, or
        // forwarded headers; each hop is a fresh GET on this separate client.
        let request = self.covers.get(url.clone());
        if url
            .host_str()
            .is_some_and(|host| protocol::JM_COVER_HOSTS.contains(&host))
        {
            request
                .timeout(JM_REQUEST_TIMEOUT)
                .header(USER_AGENT, JM_COVER_USER_AGENT)
                .header(ACCEPT, COVER_ACCEPT)
        } else {
            request
        }
    }

    async fn send_cover_request(&self, url: &Url) -> SourceResult<CoverResponse> {
        #[cfg(test)]
        if let Some(script) = &self.cover_script {
            self.cover_recorded.lock().unwrap().push(url.clone());
            return script
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected extra cover request");
        }
        check_live_environment()?;
        let response = self
            .cover_request(url)
            .send()
            .await
            .map_err(transport_error)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(CoverResponse::Missing);
        }
        if matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or(error("SOURCE_REDIRECT_REFUSED"))?;
            return Ok(CoverResponse::Redirect(location.to_owned()));
        }
        if response.status().is_redirection() {
            return Err(error("SOURCE_REDIRECT_REFUSED"));
        }
        if !response.status().is_success() {
            return Ok(CoverResponse::HttpFailure(response.status().as_u16()));
        }
        Ok(CoverResponse::Bytes(
            bounded_bytes(response, thumbnail::MAX_COVER_BYTES).await?,
        ))
    }
}
