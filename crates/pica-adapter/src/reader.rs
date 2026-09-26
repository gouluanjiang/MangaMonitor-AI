//! One-page reader metadata using the existing MIT-licensed Pica protocol pin
//! 77c8b62ede42b3afc074506d092313816af8092d. No whole-book enumeration or media IO.
use super::{
    media_descriptors::{parse_media_doc, PicaMediaItem},
    pagination, parse_preflight_chapter, string, valid_id, PicaClient,
};
use serde_json::Value;
use std::collections::BTreeSet;

pub const MAX_READER_PAGES: u64 = 5_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReaderPageScope {
    pub total: u64,
    pub pages: u64,
    pub limit: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReaderChapter {
    pub id: String,
    pub title: String,
    pub order: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReaderChapterPage {
    pub scope: ReaderPageScope,
    pub items: Vec<ReaderChapter>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReaderMediaPage {
    pub scope: ReaderPageScope,
    pub items: Vec<PicaMediaItem>,
}

fn envelope(value: &Value, page: u64, maximum: u64) -> Result<(ReaderPageScope, &[Value]), String> {
    let (scope, docs) = pagination::read_page(value, page, MAX_READER_PAGES, &mut None)?;
    if scope.total > maximum || scope.limit > 1_000 {
        return Err("READER_METADATA_LIMIT".into());
    }
    Ok((
        ReaderPageScope {
            total: scope.total,
            pages: scope.pages,
            limit: scope.limit,
        },
        docs,
    ))
}

fn parse_chapters(value: &Value, page: u64) -> Result<ReaderChapterPage, String> {
    let (scope, docs) = envelope(&value["eps"], page, 5_000)?;
    let mut ids = BTreeSet::new();
    let mut orders = BTreeSet::new();
    let mut items = Vec::with_capacity(docs.len());
    for doc in docs {
        let chapter = parse_preflight_chapter(doc)?;
        if !ids.insert(chapter.chapter_id.clone()) || !orders.insert(chapter.chapter_order) {
            return Err("READER_CHAPTER_DUPLICATE".into());
        }
        let title = string(&doc["title"])
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| format!("第 {} 章", chapter.chapter_order));
        if title.len() > 4096 || title.chars().any(char::is_control) {
            return Err("READER_CHAPTER_TITLE_INVALID".into());
        }
        items.push(ReaderChapter {
            id: chapter.chapter_id,
            title,
            order: chapter.chapter_order,
        });
    }
    Ok(ReaderChapterPage { scope, items })
}

fn parse_images(value: &Value, page: u64) -> Result<ReaderMediaPage, String> {
    let (scope, docs) = envelope(&value["pages"], page, 50_000)?;
    let mut ids = BTreeSet::new();
    let mut items = Vec::with_capacity(docs.len());
    for doc in docs {
        let item = parse_media_doc(doc)?;
        if !ids.insert(item.media_id.clone()) {
            return Err("DUPLICATE_PICA_MEDIA_ID".into());
        }
        if item.file_server.len() > 2048
            || item.path.len() > 2048
            || item.original_name.len() > 4096
        {
            return Err("READER_METADATA_LIMIT".into());
        }
        items.push(item);
    }
    Ok(ReaderMediaPage { scope, items })
}

impl PicaClient {
    pub async fn reader_chapters<Guard>(
        &mut self,
        work_id: &str,
        page: u64,
        mut before_request: Guard,
    ) -> Result<ReaderChapterPage, String>
    where
        Guard: FnMut() -> Result<(), String>,
    {
        if !valid_id(work_id) || !(1..=MAX_READER_PAGES).contains(&page) {
            return Err("READER_REQUEST_INVALID".into());
        }
        before_request()?;
        let value = self
            .request(
                reqwest::Method::GET,
                &format!("comics/{work_id}/eps?page={page}"),
                None,
                "reader_chapters",
                Some(page),
            )
            .await?;
        before_request()?;
        parse_chapters(&value, page)
    }

    pub async fn reader_images<Guard>(
        &mut self,
        work_id: &str,
        chapter_order: u64,
        page: u64,
        mut before_request: Guard,
    ) -> Result<ReaderMediaPage, String>
    where
        Guard: FnMut() -> Result<(), String>,
    {
        if !valid_id(work_id) || chapter_order == 0 || !(1..=MAX_READER_PAGES).contains(&page) {
            return Err("READER_REQUEST_INVALID".into());
        }
        before_request()?;
        let value = self
            .request(
                reqwest::Method::GET,
                &format!("comics/{work_id}/order/{chapter_order}/pages?page={page}"),
                None,
                "reader_images",
                Some(page),
            )
            .await?;
        before_request()?;
        parse_images(&value, page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const ID: &str = "111111111111111111111111";

    fn image(id: &str) -> Value {
        json!({"_id":id,"media":{"originalName":"001.jpg","fileServer":"https://storage-b.picacomic.com","path":"fixture/001.jpg"}})
    }

    #[tokio::test]
    async fn reader_requests_only_selected_metadata_page_and_checks_scope() {
        let mut client = PicaClient::new_for_reader("synthetic-token".into()).unwrap();
        client.script = Some(std::collections::VecDeque::from([Ok(
            json!({"pages":{"page":2,"pages":2,"total":3,"limit":2,"docs":[image(ID)]}}),
        )]));
        let result = client.reader_images(ID, 3, 2, || Ok(())).await.unwrap();
        assert_eq!(result.scope.total, 3);
        assert_eq!(result.items.len(), 1);
        assert_eq!(
            client.requested_paths,
            vec![format!("comics/{ID}/order/3/pages?page=2")]
        );
        assert!(parse_images(
            &json!({"pages":{"page":1,"pages":2,"total":3,"limit":2,"docs":[image(ID)]}}),
            1
        )
        .is_err());
    }

    #[tokio::test]
    async fn cancelled_reader_performs_no_api_request_and_discards_inflight_result() {
        let mut client = PicaClient::new_for_reader("synthetic-token".into()).unwrap();
        assert!(client
            .reader_chapters(ID, 1, || Err("READER_CLOSED".into()))
            .await
            .is_err());
        assert!(client.requested_paths.is_empty());
        client.script = Some(std::collections::VecDeque::from([Ok(
            json!({"eps":{"page":1,"pages":1,"total":1,"limit":2,"docs":[{"_id":ID,"order":1,"title":"Chapter"}]}}),
        )]));
        let mut checks = 0;
        let error = client
            .reader_chapters(ID, 1, || {
                checks += 1;
                if checks == 1 {
                    Ok(())
                } else {
                    Err("SESSION_CHANGED".into())
                }
            })
            .await
            .unwrap_err();
        assert_eq!(error, "SESSION_CHANGED");
        assert_eq!(client.requested_paths.len(), 1);
    }

    #[test]
    fn reader_does_not_hide_duplicate_chapters_or_accept_wrong_page() {
        let row = json!({"_id":ID,"order":1,"title":"Chapter"});
        let value =
            json!({"eps":{"page":1,"pages":1,"total":2,"limit":2,"docs":[row.clone(),row]}});
        assert!(parse_chapters(&value, 1).is_err());
        assert!(parse_chapters(&value, 2).is_err());
    }

    #[tokio::test]
    async fn failed_address_page_retries_that_page_without_restarting_the_chapter() {
        let mut client = PicaClient::new_for_reader("synthetic-token".into()).unwrap();
        client.script = Some(std::collections::VecDeque::from([
            Err("TIMEOUT".into()),
            Ok(json!({"pages":{"page":2,"pages":2,"total":3,"limit":2,"docs":[image(ID)]}})),
        ]));
        assert!(client.reader_images(ID, 2, 2, || Ok(())).await.is_err());
        assert_eq!(
            client
                .reader_images(ID, 2, 2, || Ok(()))
                .await
                .unwrap()
                .items
                .len(),
            1
        );
        assert_eq!(
            client.requested_paths,
            vec![format!("comics/{ID}/order/2/pages?page=2"); 2]
        );
    }
}
