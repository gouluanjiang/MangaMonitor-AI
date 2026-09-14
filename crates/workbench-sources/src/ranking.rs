//! Read-only lists from the already pinned lanyeeee JM/Pica protocols.
use crate::{
    protocol, RankOption, RankOptions, Source, SourcePage, SourceResult, SourceSession,
    WorkbenchSources,
};
use reqwest::{Method, Url};
use serde_json::Value;

fn choice(id: &str, label: &str) -> RankOption {
    RankOption {
        id: id.into(),
        label: label.into(),
    }
}
fn token(value: &str) -> SourceResult<()> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    {
        return Err(protocol::error("SOURCE_RANK_INVALID"));
    }
    Ok(())
}
fn options(value: &Value, time: bool) -> SourceResult<Vec<RankOption>> {
    let values = value
        .as_array()
        .ok_or(protocol::error("SOURCE_RESPONSE_INVALID"))?;
    if values.len() > 1000 {
        return Err(protocol::error("SOURCE_RESPONSE_INVALID"));
    }
    let mut result = vec![];
    for row in values {
        let id = protocol::required_text(&row["id"])?;
        token(&id)?;
        let mut label = protocol::bounded_required_text(&row["title"], 2000)?;
        if time && row["time"].as_str().is_some_and(|v| !v.is_empty()) {
            label.push_str(" · ");
            label.push_str(&protocol::bounded_required_text(&row["time"], 2000)?);
        }
        if result.iter().any(|option: &RankOption| option.id == id) {
            return Err(protocol::error("SOURCE_RESPONSE_INVALID"));
        }
        result.push(choice(&id, &label));
    }
    Ok(result)
}
impl WorkbenchSources {
    pub async fn ranking_options(&self, session: &SourceSession) -> SourceResult<RankOptions> {
        if session.source == Source::Pica {
            return Ok(RankOptions {
                categories: vec![],
                periods: vec![
                    choice("week", "周榜"),
                    choice("day", "日榜"),
                    choice("month", "月榜"),
                ],
            });
        }
        let _operation = session.operation.lock().await;
        let data = self
            .request(Source::Jm, Some(session), Method::GET, "/week", None, false)
            .await?;
        Ok(RankOptions {
            categories: options(&data["categories"], true)?,
            periods: options(&data["type"], false)?,
        })
    }
    pub async fn ranking(
        &self,
        session: &SourceSession,
        category: Option<&str>,
        period: &str,
    ) -> SourceResult<SourcePage> {
        let route = match session.source {
            Source::Jm => {
                let category = category.ok_or(protocol::error("SOURCE_RANK_INVALID"))?;
                token(category)?;
                token(period)?;
                let mut url = Url::parse(&format!("https://{}/week/filter", protocol::JM_HOST))
                    .map_err(|_| protocol::error("SOURCE_CLIENT_FAILED"))?;
                url.query_pairs_mut()
                    .append_pair("id", category)
                    .append_pair("type", period);
                format!("/week/filter?{}", url.query().unwrap_or_default())
            }
            Source::Pica => {
                if category.is_some() {
                    return Err(protocol::error("SOURCE_RANK_INVALID"));
                }
                let tt = match period {
                    "day" => "H24",
                    "week" => "D7",
                    "month" => "D30",
                    _ => return Err(protocol::error("SOURCE_RANK_INVALID")),
                };
                format!("comics/leaderboard?tt={tt}&ct=VC")
            }
        };
        let _operation = session.operation.lock().await;
        let data = self
            .request(
                session.source,
                Some(session),
                Method::GET,
                &route,
                None,
                false,
            )
            .await?;
        let records = data[if session.source == Source::Jm {
            "list"
        } else {
            "comics"
        }]
        .as_array()
        .ok_or(protocol::error("SOURCE_RESPONSE_INVALID"))?;
        if records.len() > 1000 {
            return Err(protocol::error("SOURCE_RESPONSE_INVALID"));
        }
        let mut items = vec![];
        let mut covers = vec![];
        let mut ids = std::collections::HashSet::new();
        for row in records {
            let (work, cover) = protocol::work(session.source, row, false)?;
            if !ids.insert(work.work_id.clone()) {
                return Err(protocol::error("SOURCE_PAGINATION_INVALID"));
            }
            covers.push((work.work_id.clone(), cover));
            items.push(work);
        }
        let total = if session.source == Source::Jm {
            protocol::count(&data["total"])?
        } else {
            Some(records.len() as u64)
        };
        // Neither upstream endpoint offers pagination. A short JM response must
        // remain visibly partial rather than inventing another page or total.
        let complete = total == Some(records.len() as u64);
        if total.is_some_and(|n| n < records.len() as u64) {
            return Err(protocol::error("SOURCE_PAGINATION_INVALID"));
        }
        session.remember_covers(covers)?;
        Ok(SourcePage {
            page: 1,
            total,
            pages: if complete { Some(1) } else { None },
            has_more: if complete { Some(false) } else { None },
            folders: vec![],
            items,
        })
    }
}
