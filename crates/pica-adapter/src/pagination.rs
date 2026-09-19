//! Completeness checks for the pinned Pica `Pagination<T>` envelope.
//! Upstream: picacomic-downloader@77c8b62e, responses/mod.rs (total, limit,
//! page, pages, docs). `number` also accepts the numeric strings used by APIs.

use super::number;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PageScope {
    pub total: u64,
    pub pages: u64,
    limit: u64,
}

pub(crate) fn read_page<'a>(
    value: &'a Value,
    requested: u64,
    max_pages: u64,
    previous: &mut Option<PageScope>,
) -> Result<(PageScope, &'a [Value]), String> {
    let invalid = || "PICA_PAGINATION_INVALID".to_owned();
    let scope = PageScope {
        total: number(&value["total"]).ok_or_else(invalid)?,
        pages: number(&value["pages"]).ok_or_else(invalid)?,
        limit: number(&value["limit"]).ok_or_else(invalid)?,
    };
    if requested == 0
        || number(&value["page"]) != Some(requested)
        || scope.total == 0
        || scope.limit == 0
        || scope.pages == 0
        || scope.pages > max_pages
        || requested > scope.pages
        || scope.total.div_ceil(scope.limit) != scope.pages
    {
        return Err(invalid());
    }
    if previous.is_some_and(|previous| previous != scope) {
        return Err("PICA_PAGINATION_CHANGED".into());
    }
    let docs = value["docs"].as_array().ok_or_else(invalid)?;
    let offset = (requested - 1)
        .checked_mul(scope.limit)
        .ok_or_else(invalid)?;
    let remaining = scope.total.checked_sub(offset).ok_or_else(invalid)?;
    if u64::try_from(docs.len()).map_err(|_| invalid())? != remaining.min(scope.limit) {
        return Err("PICA_PAGINATION_INCOMPLETE".into());
    }
    *previous = Some(scope);
    Ok((scope, docs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numeric_strings_and_the_short_final_page_keep_one_complete_scope() {
        let mut previous = None;
        let first = json!({"total":"3","limit":"2","page":"1","pages":"2","docs":[{},{}]});
        read_page(&first, 1, 2, &mut previous).unwrap();
        let last = json!({"total":3,"limit":2,"page":2,"pages":2,"docs":[{}]});
        assert_eq!(read_page(&last, 2, 2, &mut previous).unwrap().0.total, 3);
    }

    #[test]
    fn incomplete_changed_and_wrong_page_envelopes_cannot_certify_completion() {
        let valid = json!({"total":3,"limit":2,"page":1,"pages":2,"docs":[{},{}]});
        let mut previous = None;
        read_page(&valid, 1, 2, &mut previous).unwrap();
        for invalid in [
            json!({"total":3,"limit":2,"page":2,"pages":2,"docs":[]}),
            json!({"total":4,"limit":2,"page":2,"pages":2,"docs":[{},{}]}),
            json!({"total":3,"limit":2,"page":1,"pages":2,"docs":[{}]}),
            json!({"total":3,"limit":3,"page":2,"pages":1,"docs":[{}]}),
        ] {
            assert!(read_page(&invalid, 2, 2, &mut previous).is_err());
        }
        let partial = json!({"total":2,"limit":2,"page":1,"pages":1,"docs":[{}]});
        assert_eq!(
            read_page(&partial, 1, 1, &mut None).unwrap_err(),
            "PICA_PAGINATION_INCOMPLETE"
        );
    }
}
