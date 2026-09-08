//! Three-author JM+Pica bounded-concurrency acquisition probe.
//!
//! This stage deliberately keeps the existing Phase3B writer as the only durable-state
//! authority. Exactly three confirmed authors are active concurrently. Inside each author,
//! JM and Pica run concurrently, while pagination and detail fetches remain sequential within
//! each source. The emitted tape is reordered deterministically by author, then JM before Pica,
//! before replay through the existing Phase3B writer.

use chrono::Utc;
use cloud_monitor::{monitor::*, persistence::*};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use state_model::{Record, RequestTrace, SearchPage};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Instant,
};

const AUTHOR_CONCURRENCY: usize = 3;

#[derive(Clone, Serialize, Deserialize)]
struct Observation {
    source: String,
    author: String,
    page: Option<SearchPage>,
    details: BTreeMap<String, Record>,
    error: Option<String>,
}

#[derive(Serialize)]
struct Tape {
    observations: Vec<Observation>,
    direct_checks: Vec<Value>,
}

struct SourceResult {
    observations: Vec<Observation>,
    traces: Vec<RequestTrace>,
    boundary: String,
    elapsed_ms: u128,
}

struct AuthorResult {
    author: String,
    jm: SourceResult,
    pica: SourceResult,
    elapsed_ms: u128,
}

#[derive(Clone)]
struct RequestBudget {
    used: Arc<AtomicUsize>,
    limit: usize,
}

struct Reservation {
    budget: RequestBudget,
    reserved: usize,
    committed: bool,
}

impl RequestBudget {
    fn new(limit: usize) -> Result<Self, String> {
        if limit == 0 {
            return Err("INVALID_REQUEST_BUDGET".into());
        }
        Ok(Self {
            used: Arc::new(AtomicUsize::new(0)),
            limit,
        })
    }

    fn reserve(&self, maximum_attempts: usize) -> Option<Reservation> {
        if maximum_attempts == 0 || maximum_attempts > self.limit {
            return None;
        }
        self.used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(maximum_attempts)
                    .filter(|next| *next <= self.limit)
            })
            .ok()?;
        Some(Reservation {
            budget: self.clone(),
            reserved: maximum_attempts,
            committed: false,
        })
    }

    fn used(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }
}

impl Reservation {
    fn commit(mut self, actual_attempts: usize) -> Result<(), String> {
        if actual_attempts > self.reserved {
            return Err("REQUEST_BUDGET_ACCOUNTING_OVERFLOW".into());
        }
        let unused = self.reserved - actual_attempts;
        if unused > 0 {
            self.budget.used.fetch_sub(unused, Ordering::AcqRel);
        }
        self.committed = true;
        Ok(())
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if !self.committed {
            self.budget.used.fetch_sub(self.reserved, Ordering::AcqRel);
        }
    }
}

fn opt(args: &[String], name: &str, default: &str) -> String {
    args.windows(2)
        .find(|window| window[0] == name)
        .map(|window| window[1].clone())
        .unwrap_or_else(|| default.into())
}

fn read_three_authors(path: &Path) -> Result<Vec<String>, String> {
    let value: Value = serde_json::from_slice(&fs::read(path).map_err(|_| "AUTHORS_READ")?)
        .map_err(|_| "AUTHORS_JSON")?;
    let authors = if let Ok(names) = serde_json::from_value::<Vec<String>>(value.clone()) {
        names
    } else {
        value["authors"]
            .as_array()
            .ok_or("AUTHORS_JSON")?
            .iter()
            .filter(|author| author["enabled"] != false)
            .map(|author| {
                author["name"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or("AUTHORS_JSON")
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    if authors.len() != AUTHOR_CONCURRENCY || authors.iter().any(String::is_empty) {
        return Err("THREE_AUTHOR_ACQUIRE_REQUIRES_EXACTLY_THREE_AUTHORS".into());
    }
    let unique: BTreeSet<_> = authors.iter().collect();
    if unique.len() != AUTHOR_CONCURRENCY {
        return Err("THREE_AUTHOR_ACQUIRE_REQUIRES_UNIQUE_AUTHORS".into());
    }
    Ok(authors)
}

fn apply_observation(state: &mut State, observation: &Observation) -> Result<(), String> {
    let cursor_key = State::cursor_key(&observation.source, &observation.author);
    if state.scan.progress.get(&cursor_key).is_some_and(|cursor| {
        ["COMPLETE", "EARLY_STOP_HEURISTIC"].contains(&cursor.boundary.as_str())
    }) {
        return Ok(());
    }
    if let Some(page) = &observation.page {
        for record in &page.records {
            if state.needs_detail(record) {
                if let Some(detail) = observation.details.get(&key(record)) {
                    state.accept(record, detail)?;
                    state.note_search_query(record, &observation.author);
                } else if observation.error.is_none() {
                    return Err("ACQUIRE_MISSING_DETAIL".into());
                }
            } else {
                state.observe_unchanged(record);
                state.note_search_query(record, &observation.author);
            }
        }
        if observation.error.is_none() {
            state.page_boundary(&observation.source, &observation.author, page);
        }
    }
    if let Some(error) = &observation.error {
        state.source_error(&observation.source, &observation.author, error);
    }
    Ok(())
}

async fn budgeted_jm_search(
    client: &mut jm_adapter::JmClient,
    budget: &RequestBudget,
    author: &str,
    page: u64,
) -> Result<SearchPage, String> {
    // Search can use the full pinned-domain failover set and then perform a second
    // failover-capable detail request when JM returns redirect_aid.
    let reservation = budget
        .reserve(jm_adapter::BASELINE_DOMAINS.len() * 2)
        .ok_or("REQUEST_BUDGET_CHECKPOINT")?;
    let before = client.traces.len();
    let result = client.search(author, page).await;
    reservation.commit(client.traces.len() - before)?;
    result
}

async fn budgeted_jm_detail(
    client: &mut jm_adapter::JmClient,
    budget: &RequestBudget,
    id: &str,
) -> Result<(Record, Vec<String>), String> {
    let reservation = budget
        .reserve(jm_adapter::BASELINE_DOMAINS.len())
        .ok_or("REQUEST_BUDGET_CHECKPOINT")?;
    let before = client.traces.len();
    let result = client.detail(id).await;
    reservation.commit(client.traces.len() - before)?;
    result
}

async fn budgeted_pica_login(
    client: &mut pica_adapter::PicaClient,
    budget: &RequestBudget,
    email: &str,
    password: &str,
) -> Result<(), String> {
    let reservation = budget.reserve(1).ok_or("REQUEST_BUDGET_CHECKPOINT")?;
    let before = client.traces.len();
    let result = client.login(email, password).await;
    reservation.commit(client.traces.len() - before)?;
    result
}

async fn budgeted_pica_search(
    client: &mut pica_adapter::PicaClient,
    budget: &RequestBudget,
    author: &str,
    page: u64,
) -> Result<SearchPage, String> {
    let reservation = budget.reserve(1).ok_or("REQUEST_BUDGET_CHECKPOINT")?;
    let before = client.traces.len();
    let result = client.search(author, page).await;
    reservation.commit(client.traces.len() - before)?;
    result
}

async fn budgeted_pica_detail(
    client: &mut pica_adapter::PicaClient,
    budget: &RequestBudget,
    id: &str,
) -> Result<(Record, Vec<String>), String> {
    let reservation = budget.reserve(1).ok_or("REQUEST_BUDGET_CHECKPOINT")?;
    let before = client.traces.len();
    let result = client.detail(id).await;
    reservation.commit(client.traces.len() - before)?;
    result
}

async fn scan_jm(
    mut state: State,
    author: String,
    mut client: jm_adapter::JmClient,
    budget: RequestBudget,
) -> Result<SourceResult, String> {
    let started = Instant::now();
    let cursor_key = State::cursor_key("jm", &author);
    let mut page = state
        .scan
        .progress
        .get(&cursor_key)
        .map(|cursor| cursor.next_page.max(1))
        .unwrap_or(1);
    let mut observations = Vec::new();

    loop {
        let found = budgeted_jm_search(&mut client, &budget, &author, page).await;
        let mut observation = Observation {
            source: "jm".into(),
            author: author.clone(),
            page: None,
            details: BTreeMap::new(),
            error: None,
        };
        match found {
            Err(error) => {
                observation.error = Some(error);
                apply_observation(&mut state, &observation)?;
                observations.push(observation);
                break;
            }
            Ok(search_page) => {
                let mut failed = None;
                for record in &search_page.records {
                    if state.needs_detail(record) {
                        match budgeted_jm_detail(&mut client, &budget, &record.source_work_id).await {
                            Ok((detail, _)) => {
                                observation.details.insert(key(record), detail);
                            }
                            Err(error) => {
                                failed = Some(error);
                                break;
                            }
                        }
                    }
                }
                observation.page = Some(search_page);
                observation.error = failed;
                apply_observation(&mut state, &observation)?;
                observations.push(observation);
                if state.scan.progress[&cursor_key].boundary != "CHECKPOINT" {
                    break;
                }
                page += 1;
            }
        }
    }

    Ok(SourceResult {
        observations,
        traces: client.traces,
        boundary: state.scan.progress[&cursor_key].boundary.clone(),
        elapsed_ms: started.elapsed().as_millis(),
    })
}

async fn scan_pica(
    mut state: State,
    author: String,
    mut client: pica_adapter::PicaClient,
    budget: RequestBudget,
) -> Result<SourceResult, String> {
    let started = Instant::now();
    let cursor_key = State::cursor_key("pica", &author);
    let mut page = state
        .scan
        .progress
        .get(&cursor_key)
        .map(|cursor| cursor.next_page.max(1))
        .unwrap_or(1);
    let mut observations = Vec::new();

    loop {
        let found = budgeted_pica_search(&mut client, &budget, &author, page).await;
        let mut observation = Observation {
            source: "pica".into(),
            author: author.clone(),
            page: None,
            details: BTreeMap::new(),
            error: None,
        };
        match found {
            Err(error) => {
                observation.error = Some(error);
                apply_observation(&mut state, &observation)?;
                observations.push(observation);
                break;
            }
            Ok(search_page) => {
                let mut failed = None;
                for record in &search_page.records {
                    if state.needs_detail(record) {
                        match budgeted_pica_detail(&mut client, &budget, &record.source_work_id).await {
                            Ok((detail, _)) => {
                                observation.details.insert(key(record), detail);
                            }
                            Err(error) => {
                                failed = Some(error);
                                break;
                            }
                        }
                    }
                }
                observation.page = Some(search_page);
                observation.error = failed;
                apply_observation(&mut state, &observation)?;
                observations.push(observation);
                if state.scan.progress[&cursor_key].boundary != "CHECKPOINT" {
                    break;
                }
                page += 1;
            }
        }
    }

    Ok(SourceResult {
        observations,
        traces: client.traces,
        boundary: state.scan.progress[&cursor_key].boundary.clone(),
        elapsed_ms: started.elapsed().as_millis(),
    })
}

async fn scan_author(
    state: State,
    author: String,
    jm: jm_adapter::JmClient,
    pica: pica_adapter::PicaClient,
    budget: RequestBudget,
) -> Result<AuthorResult, String> {
    let started = Instant::now();
    let (jm_result, pica_result) = tokio::join!(
        scan_jm(state.clone(), author.clone(), jm, budget.clone()),
        scan_pica(state, author.clone(), pica, budget)
    );
    Ok(AuthorResult {
        author,
        jm: jm_result?,
        pica: pica_result?,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

async fn authenticate_pica_clients(
    budget: &RequestBudget,
) -> Result<[pica_adapter::PicaClient; AUTHOR_CONCURRENCY], String> {
    let token = env::var("PICA_TOKEN").unwrap_or_default();
    let mut pica0 = pica_adapter::PicaClient::new(token.clone())?;
    let mut pica1 = pica_adapter::PicaClient::new(token.clone())?;
    let mut pica2 = pica_adapter::PicaClient::new(token.clone())?;
    if !token.is_empty() {
        return Ok([pica0, pica1, pica2]);
    }
    let email = env::var("PICA_EMAIL").map_err(|_| "MISSING_PICA_AUTH")?;
    let password = env::var("PICA_PASSWORD").map_err(|_| "MISSING_PICA_AUTH")?;
    if email.is_empty() || password.is_empty() {
        return Err("MISSING_PICA_AUTH".into());
    }
    // Authenticate sequentially on purpose. The three-author experiment is about scan
    // concurrency, not an authentication burst. This keeps login pressure conservative.
    budgeted_pica_login(&mut pica0, budget, &email, &password).await?;
    budgeted_pica_login(&mut pica1, budget, &email, &password).await?;
    budgeted_pica_login(&mut pica2, budget, &email, &password).await?;
    Ok([pica0, pica1, pica2])
}

async fn run() -> Result<(), String> {
    let started = Instant::now();
    let args: Vec<String> = env::args().collect();
    let input = PathBuf::from(opt(&args, "--state", "monitor-state"));
    let output = PathBuf::from(opt(
        &args,
        "--output",
        "reports/phase3b-three-author-dual-source-acquire",
    ));
    fs::create_dir_all(&output).map_err(|_| "OUTPUT_DIR")?;
    let input_abs = fs::canonicalize(&input).map_err(|_| "INPUT_DIR")?;
    let output_abs = fs::canonicalize(&output).map_err(|_| "OUTPUT_DIR")?;
    if input_abs == output_abs
        || input_abs.starts_with(&output_abs)
        || output_abs.starts_with(&input_abs)
    {
        return Err("OUTPUT_MUST_BE_SEPARATE_FROM_INPUT".into());
    }

    let authors = read_three_authors(&PathBuf::from(opt(
        &args,
        "--authors",
        "three-author.json",
    )))?;
    let requested_mode = opt(&args, "--mode", "full");
    let mode = match requested_mode.as_str() {
        "full" => "full",
        "incremental" | "monthly" => "incremental",
        _ => return Err("INVALID_MODE".into()),
    };
    let threshold: usize = opt(&args, "--threshold", "5")
        .parse()
        .map_err(|_| "THRESHOLD")?;
    if threshold == 0 {
        return Err("THRESHOLD".into());
    }
    let request_limit: usize = opt(&args, "--max-requests", "400")
        .parse()
        .map_err(|_| "BUDGET")?;
    let budget = RequestBudget::new(request_limit)?;

    let mut state = load(&input)?;
    let confirmed = state.author_names();
    if authors.iter().any(|author| !confirmed.contains(author)) {
        return Err("AUTHOR_NOT_IN_CONFIRMED_INPUT".into());
    }
    let scan_id = Utc::now().to_rfc3339();
    state.begin_with_event_history(
        &scan_id,
        &scan_id,
        authors.clone(),
        mode,
        threshold,
        false,
    )?;

    let [pica0, pica1, pica2] = authenticate_pica_clients(&budget).await?;
    let jm0 = jm_adapter::JmClient::new(jm_adapter::DEFAULT_DOMAIN)?;
    let jm1 = jm_adapter::JmClient::new(jm_adapter::DEFAULT_DOMAIN)?;
    let jm2 = jm_adapter::JmClient::new(jm_adapter::DEFAULT_DOMAIN)?;
    let author0 = authors[0].clone();
    let author1 = authors[1].clone();
    let author2 = authors[2].clone();

    let multi_started = Instant::now();
    let (result0, result1, result2) = tokio::join!(
        scan_author(state.clone(), author0, jm0, pica0, budget.clone()),
        scan_author(state.clone(), author1, jm1, pica1, budget.clone()),
        scan_author(state, author2, jm2, pica2, budget.clone())
    );
    let results = [result0?, result1?, result2?];
    let multi_author_wall_ms = multi_started.elapsed().as_millis();

    let observations: Vec<Observation> = results
        .iter()
        .flat_map(|result| {
            result
                .jm
                .observations
                .iter()
                .cloned()
                .chain(result.pica.observations.iter().cloned())
        })
        .collect();
    let tape = Tape {
        observations,
        direct_checks: Vec::new(),
    };
    write_json(&output.join("observations.json"), &tape)?;

    let requests = budget.used();
    let trace_count: usize = results
        .iter()
        .map(|result| result.jm.traces.len() + result.pica.traces.len())
        .sum();
    if requests != trace_count {
        return Err("REQUEST_BUDGET_TRACE_MISMATCH".into());
    }

    let sum_author_wall_ms: u128 = results.iter().map(|result| result.elapsed_ms).sum();
    let multi_author_overlap_observed = multi_author_wall_ms < sum_author_wall_ms;
    let all_dual_source_overlap_observed = results.iter().all(|result| {
        result.elapsed_ms < result.jm.elapsed_ms.saturating_add(result.pica.elapsed_ms)
    });
    let all_boundaries_complete = results
        .iter()
        .all(|result| result.jm.boundary == "COMPLETE" && result.pica.boundary == "COMPLETE");

    let per_author: Vec<Value> = results
        .iter()
        .map(|result| {
            json!({
                "author": result.author,
                "jm_boundary": result.jm.boundary,
                "pica_boundary": result.pica.boundary,
                "jm_requests": result.jm.traces.len(),
                "pica_requests": result.pica.traces.len(),
                "jm_elapsed_ms": result.jm.elapsed_ms,
                "pica_elapsed_ms": result.pica.elapsed_ms,
                "author_wall_elapsed_ms": result.elapsed_ms,
                "dual_source_overlap_observed": result.elapsed_ms < result.jm.elapsed_ms.saturating_add(result.pica.elapsed_ms),
                "requests_trace": {"jm": result.jm.traces, "pica": result.pica.traces}
            })
        })
        .collect();

    let report = json!({
        "schema_version": 1,
        "authors": authors,
        "author_concurrency": AUTHOR_CONCURRENCY,
        "requested_mode": requested_mode,
        "effective_mode": mode,
        "request_budget": request_limit,
        "requests": requests,
        "all_boundaries_complete": all_boundaries_complete,
        "all_dual_source_overlap_observed": all_dual_source_overlap_observed,
        "multi_author_overlap_observed": multi_author_overlap_observed,
        "multi_author_wall_elapsed_ms": multi_author_wall_ms,
        "sum_author_wall_elapsed_ms": sum_author_wall_ms,
        "total_elapsed_ms": started.elapsed().as_millis(),
        "per_author": per_author,
        "durable_state_mutated": false,
        "inventory_mutation_authorized": false,
        "task_completion_authorized": false,
        "promotion_authorized": false,
        "replacement_authorized": false,
        "physical_delete_authorized": false,
        "production_enablement_authorized": false
    });
    write_json(&output.join("acquisition-report.json"), &report)?;
    println!("{report}");
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("PHASE3B_THREE_AUTHOR_DUAL_SOURCE_ACQUIRE_ERROR: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_budget_never_exceeds_limit_and_releases_unused_attempts() {
        let budget = RequestBudget::new(6).unwrap();
        let first = budget.reserve(5).unwrap();
        assert!(budget.reserve(2).is_none());
        first.commit(1).unwrap();
        assert_eq!(budget.used(), 1);
        let second = budget.reserve(5).unwrap();
        second.commit(5).unwrap();
        assert_eq!(budget.used(), 6);
        assert!(budget.reserve(1).is_none());
    }

    #[test]
    fn three_author_selection_is_exact_and_unique() {
        let root = std::env::temp_dir().join(format!(
            "three-author-authors-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("authors.json");
        write_json(
            &path,
            &json!({"authors":[
                {"name":"A","enabled":true},
                {"name":"B","enabled":true},
                {"name":"C","enabled":true}
            ]}),
        )
        .unwrap();
        assert_eq!(read_three_authors(&path).unwrap(), vec!["A", "B", "C"]);

        write_json(
            &path,
            &json!({"authors":[{"name":"A","enabled":true},{"name":"B","enabled":true}]}),
        )
        .unwrap();
        assert_eq!(
            read_three_authors(&path).unwrap_err(),
            "THREE_AUTHOR_ACQUIRE_REQUIRES_EXACTLY_THREE_AUTHORS"
        );

        write_json(
            &path,
            &json!({"authors":[
                {"name":"A","enabled":true},
                {"name":"A","enabled":true},
                {"name":"C","enabled":true}
            ]}),
        )
        .unwrap();
        assert_eq!(
            read_three_authors(&path).unwrap_err(),
            "THREE_AUTHOR_ACQUIRE_REQUIRES_UNIQUE_AUTHORS"
        );
    }
}
