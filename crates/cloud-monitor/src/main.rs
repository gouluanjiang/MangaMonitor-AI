//! Phase 1A smoke only. No scheduling, pending execution, inventory mutation or images.
use serde_json::{json, Value};
use state_model::{Record, RequestTrace, SearchPage};
use std::{collections::HashSet, path::PathBuf, time::Instant};

enum Client {
    Jm(jm_adapter::JmClient),
    Pica(pica_adapter::PicaClient),
}
impl Client {
    async fn search(&mut self, author: &str, page: u64) -> Result<SearchPage, String> {
        match self {
            Self::Jm(c) => c.search(author, page).await,
            Self::Pica(c) => c.search(author, page).await,
        }
    }
    async fn detail(&mut self, id: &str) -> Result<(Record, Vec<String>), String> {
        match self {
            Self::Jm(c) => c.detail(id).await,
            Self::Pica(c) => c.detail(id).await,
        }
    }
    fn traces(&self) -> &Vec<RequestTrace> {
        match self {
            Self::Jm(c) => &c.traces,
            Self::Pica(c) => &c.traces,
        }
    }
}

fn timestamp(v: &Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        return Some(n);
    }
    let s = v.as_str()?;
    if let Ok(n) = s.parse::<i64>() {
        return Some(n);
    }
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(d.timestamp());
    }
    for fmt in ["%Y-%m-%d %H:%M:%S"] {
        if let Ok(d) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(d.and_utc().timestamp());
        }
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d.and_hms_opt(0, 0, 0)?.and_utc().timestamp());
    }
    None
}
fn ordering(records: &[Record], field: &str) -> Value {
    let vals: Vec<_> = records
        .iter()
        .filter_map(|r| timestamp(&r.metadata[field]))
        .collect();
    let inversions = vals.windows(2).filter(|p| p[0] < p[1]).count();
    let distinct = vals.iter().collect::<HashSet<_>>().len();
    json!({"field":field,"sampled_records":records.len(),"parseable_values":vals.len(),"distinct_values":distinct,
        "ascending_inversions":inversions,"first":vals.first(),"last":vals.last(),
        "evidence":if vals.len() != records.len() || vals.len()<2 || distinct<2 {"INCONCLUSIVE"} else if inversions==0 {"OBSERVED_NONINCREASING"} else {"NOT_NONINCREASING"}})
}

async fn smoke(
    source: &str,
    authors: &[String],
    max_pages: u64,
    details_per_author: usize,
    jm_domain: &str,
) -> Value {
    let start = Instant::now();
    let mut auth = json!({"mode":"not_required","login_endpoint":"NOT_NEEDED"});
    let mut client = if source == "jm" {
        match jm_adapter::JmClient::new(jm_domain) {
            Ok(c) => Client::Jm(c),
            Err(e) => return json!({"source":source,"status":"FAIL","error":e}),
        }
    } else {
        let token = std::env::var("PICA_TOKEN").unwrap_or_default();
        let email = std::env::var("PICA_EMAIL").unwrap_or_default();
        let password = std::env::var("PICA_PASSWORD").unwrap_or_default();
        let mut pica = match pica_adapter::PicaClient::new(token.clone()) {
            Ok(c) => c,
            Err(e) => return json!({"source":source,"status":"FAIL","error":e}),
        };
        if !email.is_empty() && !password.is_empty() {
            match pica.login(&email, &password).await {
                Ok(()) => auth = json!({"mode":"email_password","login_endpoint":"PASS"}),
                Err(e) => {
                    return json!({"source":source,"status":"FAIL","auth":{"mode":"email_password","login_endpoint":"FAIL","error":e},"requests":pica.traces.len(),"elapsed_ms":start.elapsed().as_millis(),"request_traces":pica.traces})
                }
            }
        } else if !token.is_empty() {
            auth =
                json!({"mode":"existing_token","login_endpoint":"NOT_TESTED_NO_ACCOUNT_PASSWORD"});
        } else {
            let probe = pica.search(&authors[0], 1).await;
            return json!({"source":source,"status":"BLOCKED","auth":{"mode":"none","login_endpoint":"NOT_TESTED_CREDENTIALS_MISSING","unauthenticated_search":probe.as_ref().map(|_|"UNEXPECTED_SUCCESS".to_owned()).unwrap_or_else(|e|e.clone())},"requests":pica.traces.len(),"elapsed_ms":start.elapsed().as_millis(),"request_traces":pica.traces});
        }
        Client::Pica(pica)
    };
    let mut author_reports = vec![];
    let mut source_failed = false;
    let mut pagination_demonstrated = false;
    for author in authors {
        let author_start = Instant::now();
        let before = client.traces().len();
        let mut pages = vec![];
        let mut records = vec![];
        let mut ids = HashSet::new();
        let mut first_ids = vec![];
        let mut duplicates = vec![];
        let mut errors = vec![];
        let mut exhausted = false;
        for page in 1..=max_pages {
            match client.search(author, page).await {
                Ok(result) => {
                    if page == 1 {
                        first_ids = result
                            .records
                            .iter()
                            .map(|r| r.source_work_id.clone())
                            .collect();
                    }
                    let unique_before = ids.len();
                    for r in &result.records {
                        if !ids.insert(r.source_work_id.clone()) {
                            duplicates.push(r.source_work_id.clone());
                        }
                    }
                    if page > 1 && ids.len() > unique_before {
                        pagination_demonstrated = true;
                    }
                    let zero_or_end = result.records.is_empty()
                        || result.redirect_to_detail
                        || result.reported_pages.is_some_and(|n| page >= n)
                        || result.reported_total.is_some_and(|n| ids.len() as u64 >= n);
                    let repeated =
                        page > 1 && !result.records.is_empty() && ids.len() == unique_before;
                    records.extend(result.records.clone());
                    pages.push(serde_json::to_value(result).expect("serializable page"));
                    if repeated {
                        errors.push("PAGINATION_NO_NEW_IDS".to_owned());
                        break;
                    }
                    if zero_or_end {
                        exhausted = true;
                        break;
                    }
                }
                Err(e) => {
                    errors.push(format!("SEARCH_PAGE_{page}:{e}"));
                    break;
                }
            }
        }
        let mut details = vec![];
        if !records.is_empty() {
            let mut detail_targets = vec![&records[0]];
            if let Some(gap) = records.iter().skip(1).find(|r| r.author.is_empty()) {
                detail_targets.push(gap);
            } else if let Some(gap) = records
                .iter()
                .skip(1)
                .find(|r| !r.author.iter().any(|a| a == author))
            {
                detail_targets.push(gap);
            }
            for r in records.iter().skip(1) {
                if detail_targets.len() >= details_per_author {
                    break;
                }
                if !detail_targets
                    .iter()
                    .any(|a| a.source_work_id == r.source_work_id)
                {
                    detail_targets.push(r);
                }
            }
            for r in detail_targets.into_iter().take(details_per_author) {
                match client.detail(&r.source_work_id).await {
                    Ok((detail,available_fields))=>details.push(json!({"record":detail,"available_fields":available_fields,"id_matches_search":true})),
                    Err(e)=>errors.push(format!("DETAIL:{}:{e}",r.source_work_id)),
                }
            }
        }
        let mut repeat = json!({"status":"NOT_TESTED"});
        if errors.is_empty() {
            match client.search(author, 1).await {
                Ok(p) => {
                    let repeat_ids: Vec<_> =
                        p.records.iter().map(|r| r.source_work_id.clone()).collect();
                    repeat = json!({"status":if repeat_ids==first_ids {"IDENTICAL_IDS_AND_ORDER"} else {"CHANGED_DURING_TEST"},"first_ids":first_ids,"repeat_ids":repeat_ids});
                }
                Err(e) => errors.push(format!("REPEAT_PAGE_1:{e}")),
            }
        }
        if !errors.is_empty() {
            source_failed = true;
        }
        let authors_empty = records.iter().filter(|r| r.author.is_empty()).count();
        let query_exact = records
            .iter()
            .filter(|r| r.author.iter().any(|a| a == author))
            .count();
        author_reports.push(json!({"author_query":author,"status":if errors.is_empty(){"PASS"}else if pages.is_empty(){"FAIL"}else{"PARTIAL"},
            "requests":client.traces().len()-before,"elapsed_ms":author_start.elapsed().as_millis(),
            "pagination":{"exhausted":exhausted,"bounded_sample":!exhausted,"max_pages":max_pages,"duplicate_ids":duplicates,"unique_ids":ids.len()},
            "ordering_observations":[ordering(&records,"adddate"),ordering(&records,"update_at"),ordering(&records,"updated_at"),ordering(&records,"created_at")],
            "author_field":{"empty":authors_empty,"exact_query_matches":query_exact,"sampled_records":records.len(),"automatic_author_inference":false},
            "repeat_page_one":repeat,"pages":pages,"details":details,"errors":errors}));
        println!(
            "{}",
            json!({"event":"author_complete","source":source,"requests":client.traces().len()-before,"ok":errors.is_empty()})
        );
        // Authentication or connection-wide failures should not cause pointless repeated probing.
        if pages.is_empty()
            && errors.iter().any(|e| {
                e.contains("HTTP_401") || e.contains("CONNECT_ERROR") || e.contains("TIMEOUT")
            })
        {
            break;
        }
    }
    let complete = author_reports.len() == authors.len() && !source_failed;
    json!({"source":source,"status":if complete{"PASS"}else{"PARTIAL_OR_FAIL"},
        "meaning":"PASS means planned bounded smoke requests succeeded, not complete author history or proven semantic sort",
        "auth":auth,"upstream_commit":if source=="jm"{jm_adapter::UPSTREAM_COMMIT}else{pica_adapter::UPSTREAM_COMMIT},
        "endpoint":if source=="jm"{jm_domain}else{pica_adapter::HOST},"sort_parameter":if source=="jm"{"mr"}else{"dd"},
        "all_requested_authors_tested":author_reports.len()==authors.len(),"pagination_with_new_ids_observed":pagination_demonstrated,
        "requests":client.traces().len(),"elapsed_ms":start.elapsed().as_millis(),
        "intentional_delay_ms":client.traces().iter().map(|t|t.delay_ms).sum::<u64>(),
        "authors":author_reports,"request_traces":client.traces()})
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), String> {
    let mut author_file = PathBuf::from("fixtures/smoke_authors.json");
    let mut output = PathBuf::from("reports/local-smoke.json");
    let mut source = "both".to_owned();
    let mut max_pages = 3;
    let mut details = 2;
    let mut domain = jm_adapter::DEFAULT_DOMAIN.to_owned();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" {
            println!("cloud-monitor [--authors FILE] [--report FILE] [--source both|jm|pica] [--max-pages 1..10] [--details-per-author 1..3] [--jm-domain PINNED_DOMAIN]\nRead-only metadata smoke. Credentials: PICA_TOKEN or PICA_EMAIL/PICA_PASSWORD.");
            return Ok(());
        }
        let val = args.next().ok_or("MISSING_ARGUMENT_VALUE")?;
        match arg.as_str() {
            "--authors" => author_file = val.into(),
            "--report" => output = val.into(),
            "--source" => source = val,
            "--max-pages" => max_pages = val.parse::<u64>().map_err(|_| "INVALID_MAX_PAGES")?,
            "--details-per-author" => {
                details = val.parse::<usize>().map_err(|_| "INVALID_DETAIL_BUDGET")?
            }
            "--jm-domain" => domain = val,
            _ => return Err("UNKNOWN_ARGUMENT".into()),
        }
    }
    if !(1..=10).contains(&max_pages) || !(1..=3).contains(&details) {
        return Err("SMOKE_BUDGET_OUT_OF_RANGE".into());
    }
    if !["both", "jm", "pica"].contains(&source.as_str()) {
        return Err("INVALID_SOURCE".into());
    }
    let bytes = std::fs::read(author_file).map_err(|_| "AUTHOR_FILE_READ_ERROR")?;
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    let authors: Vec<String> =
        serde_json::from_slice(bytes).map_err(|_| "AUTHOR_FILE_INVALID_JSON")?;
    if !(3..=5).contains(&authors.len()) || authors.iter().any(|a| a.trim().is_empty()) {
        return Err("SMOKE_REQUIRES_3_TO_5_AUTHORS".into());
    }
    let start = Instant::now();
    let started_at = chrono::Utc::now().to_rfc3339();
    let mut results = vec![];
    for name in ["jm", "pica"] {
        if source == "both" || source == name {
            results.push(smoke(name, &authors, max_pages, details, &domain).await);
        }
    }
    let ok = results.iter().all(|r| r["status"] == "PASS");
    let report = json!({"schema_version":1,"phase":"1A_READ_ONLY","started_at":started_at,"finished_at":chrono::Utc::now().to_rfc3339(),
        "environment":if std::env::var("GITHUB_ACTIONS").as_deref()==Ok("true"){"github_actions_linux"}else{std::env::consts::OS},
        "github_run_id":std::env::var("GITHUB_RUN_ID").ok(),"git_commit":std::env::var("GITHUB_SHA").ok(),
        "elapsed_ms":start.elapsed().as_millis(),"total_requests":results.iter().filter_map(|r|r["requests"].as_u64()).sum::<u64>(),
        "image_requests":0,"inventory_modified":false,"sources":results});
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "REPORT_DIRECTORY_ERROR")?;
    }
    std::fs::write(
        output,
        serde_json::to_vec_pretty(&report).map_err(|_| "REPORT_SERIALIZE_ERROR")?,
    )
    .map_err(|_| "REPORT_WRITE_ERROR")?;
    println!(
        "{}",
        json!({"event":"smoke_complete","all_planned_requests_succeeded":ok,"total_requests":report["total_requests"],"elapsed_ms":report["elapsed_ms"]})
    );
    if ok {
        Ok(())
    } else {
        Err("SMOKE_INCOMPLETE_SEE_SANITIZED_REPORT".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_timestamps_do_not_verify_sort() {
        assert_eq!(ordering(&[], "created_at")["evidence"], "INCONCLUSIVE");
    }
    #[test]
    fn inversions_are_detected() {
        let r = |n| Record::new("jm", "1".into(), vec![], "t".into(), json!({"update_at":n}));
        assert_eq!(
            ordering(&[r(1), r(2)], "update_at")["evidence"],
            "NOT_NONINCREASING"
        );
    }
}
