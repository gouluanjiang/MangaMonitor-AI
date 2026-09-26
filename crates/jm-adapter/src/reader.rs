//! Interactive metadata extraction from the existing MIT-licensed
//! lanyeeee/jmcomic-downloader protocol pin f0cdd724af6892002f2fb7be883b88832cebe7e9.
//! No image transport, download authorization or filesystem operations.

use super::{parse_preflight_chapters, string, JmClient};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReaderChapter {
    pub id: String,
    pub title: String,
    pub order: u64,
}

fn parse_chapters(body: &Value, work_id: &str) -> Result<Vec<ReaderChapter>, String> {
    if string(&body["id"]).as_deref() != Some(work_id) {
        return Err("READER_WORK_ID_MISMATCH".into());
    }
    let chapters = parse_preflight_chapters(body, work_id)?;
    if chapters.len() > 5_000 {
        return Err("READER_CHAPTER_LIMIT".into());
    }
    let series = body["series"].as_array().ok_or("MISSING_JM_SERIES")?;
    chapters
        .into_iter()
        .enumerate()
        .map(|(index, chapter)| {
            if chapter.chapter_id.len() > 20 {
                return Err("INVALID_JM_CHAPTER_ID".into());
            }
            let title = series
                .get(index)
                .and_then(|item| string(&item["name"]))
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| format!("第 {} 章", chapter.chapter_order));
            if title.len() > 4096 || title.chars().any(char::is_control) {
                return Err("READER_CHAPTER_TITLE_INVALID".into());
            }
            Ok(ReaderChapter {
                id: chapter.chapter_id,
                title,
                order: chapter.chapter_order,
            })
        })
        .collect()
}

impl JmClient {
    pub async fn reader_chapters<Guard>(
        &mut self,
        work_id: &str,
        mut before_request: Guard,
    ) -> Result<Vec<ReaderChapter>, String>
    where
        Guard: FnMut() -> Result<(), String>,
    {
        if work_id.is_empty() || work_id.len() > 20 || !work_id.bytes().all(|b| b.is_ascii_digit())
        {
            return Err("INVALID_JM_ID".into());
        }
        let body = self
            .request_with_guard(
                "/album",
                &[("id", work_id.into())],
                None,
                &mut before_request,
            )
            .await?;
        before_request()?;
        parse_chapters(&body, work_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reader_keeps_source_chapter_order_and_rejects_wrong_book_and_duplicates() {
        let body =
            json!({"id":"12","series":[{"id":"14","name":"Second"},{"id":"13","name":"First"}]});
        let chapters = parse_chapters(&body, "12").unwrap();
        assert_eq!(
            chapters[0],
            ReaderChapter {
                id: "14".into(),
                title: "Second".into(),
                order: 1
            }
        );
        assert!(parse_chapters(&body, "99").is_err());
        assert!(
            parse_chapters(&json!({"id":"12","series":[{"id":"13"},{"id":"13"}]}), "12").is_err()
        );
        assert_eq!(
            parse_chapters(&json!({"id":"12","series":[]}), "12").unwrap()[0].id,
            "12"
        );
    }
}
