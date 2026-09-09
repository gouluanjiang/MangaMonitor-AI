//! Credential-free thumbnail transport. Redirects are followed manually only
//! after each destination passes the source-specific origin/path allowlist.
use crate::{
    bounded_bytes, check_live_environment, protocol, thumbnail, transport_error, CoverLookup,
    SourceResult, SourceSession, WorkbenchSources,
};
use protocol::error;
use reqwest::{header::LOCATION, Url};
use std::collections::HashSet;

const MAX_REDIRECTS: usize = 3;

pub(super) enum CoverResponse {
    Redirect(String),
    Missing,
    Bytes(Vec<u8>),
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
        let mut url = Url::parse(&url).map_err(|_| error("SOURCE_COVER_INVALID"))?;
        let mut visited = HashSet::new();
        for hop in 0..=MAX_REDIRECTS {
            protocol::validate_cover_url(session.source, &url)?;
            if !visited.insert(url.to_string()) {
                return Err(error("SOURCE_REDIRECT_REFUSED"));
            }
            match self.send_cover_request(&url).await? {
                CoverResponse::Missing => return Ok(None),
                CoverResponse::Redirect(location) => {
                    if hop == MAX_REDIRECTS {
                        return Err(error("SOURCE_REDIRECT_REFUSED"));
                    }
                    url = protocol::cover_redirect(session.source, &url, &location)?;
                }
                CoverResponse::Bytes(bytes) => {
                    if bytes.len() > thumbnail::MAX_COVER_BYTES {
                        return Err(error("SOURCE_RESPONSE_TOO_LARGE"));
                    }
                    return tokio::task::spawn_blocking(move || thumbnail::data_url(&bytes))
                        .await
                        .map_err(|_| error("SOURCE_COVER_INVALID"))?
                        .map(Some);
                }
            }
        }
        Err(error("SOURCE_REDIRECT_REFUSED"))
    }

    pub(super) fn cover_request(&self, url: &Url) -> reqwest::RequestBuilder {
        // No SourceSession argument, cookie jar, Authorization, Referer, or
        // forwarded headers; each hop is a fresh GET on this separate client.
        self.covers.get(url.clone())
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
            return Err(error("SOURCE_COVER_UNAVAILABLE"));
        }
        Ok(CoverResponse::Bytes(
            bounded_bytes(response, thumbnail::MAX_COVER_BYTES).await?,
        ))
    }
}
