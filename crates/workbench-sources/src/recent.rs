//! One page of the source's latest listing, using existing pinned protocols.
use crate::{protocol, Source, SourcePage, SourceResult, SourceSession, WorkbenchSources};
use reqwest::Method;
use serde_json::{json, Value};

fn request_for(source: Source, page: u64) -> (Method, String, Option<Value>) {
    match source {
        // JMComic-Crawler-Python@9fddb0494caf0cdc812ac6cbfc1c62f4f845b058:
        // JmApiClient.categories_filter, CATEGORY_ALL=0, ORDER_BY_LATEST=mr.
        Source::Jm => (
            Method::GET,
            format!("/categories/filter?page={page}&order=&c=0&o=mr"),
            None,
        ),
        // picacomic-downloader@77c8b62ede42b3afc074506d092313816af8092d:
        // SearchPane allows an empty keyword/categories with TimeNewest=dd.
        // Keep the source order; this does not imply a per-record update date.
        Source::Pica => (
            Method::POST,
            format!("comics/advanced-search?page={page}"),
            Some(json!({"keyword":"","sort":"dd","categories":[]})),
        ),
    }
}

impl WorkbenchSources {
    /// Fetch exactly the explicitly requested page, without walking the site.
    pub async fn recent(&self, session: &SourceSession, page: u64) -> SourceResult<SourcePage> {
        protocol::validate_page(page)?;
        let _operation = session.operation.lock().await;
        let (method, route, payload) = request_for(session.source, page);
        let data = self
            .request(
                session.source,
                Some(session),
                method,
                &route,
                payload,
                false,
            )
            .await?;
        let (result, covers) = protocol::page(session.source, &data, page, false)?;
        session.remember_covers(covers)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_recent_routes_have_no_keyword_category_or_sort_input() {
        assert_eq!(
            request_for(Source::Jm, 2),
            (
                Method::GET,
                "/categories/filter?page=2&order=&c=0&o=mr".into(),
                None
            )
        );
        assert_eq!(
            request_for(Source::Pica, 3),
            (
                Method::POST,
                "comics/advanced-search?page=3".into(),
                Some(json!({"keyword":"","sort":"dd","categories":[]}))
            )
        );
    }
}
