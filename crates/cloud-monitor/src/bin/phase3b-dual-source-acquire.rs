//! Single-author JM+Pica acquisition probe.
//!
//! This is deliberately separate from the production Phase3B state writer. It fetches
//! one confirmed author from JM and Pica concurrently, emits the ordinary Phase3B replay
//! tape shape, and never mutates durable monitor state. The existing Phase3B runner remains
//! the deterministic authority that applies the tape.

use chrono::Utc;
use cloud_monitor::{monitor::*, persistence::*};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use state_model::{Record, RequestTrace, SearchPage};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Instant,
};

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

fn read_one_author(path: &Path) -> Result<String, String> {
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
    if authors.len() != 1 || authors[0].is_empty() {
        return Err("DUAL_SOURCE_ACQUIRE_REQUIRES_ONE_AUTHOR".into());
    }
    Ok(authors[0].clone())
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
    // Search may consume the pinned failover set and then another failover-capable
    // detail request when JM returns redirect_aid.
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
    login_error: Option<String>,
) -> Result<SourceResult, String> {
    let started = Instant::now();
    let cursor_key = State::cursor_key("pica", &author);
    let mut observations = Vec::new();

    if let Some(error) = login_error {
        let observation = Observation {
            source: "pica".into(),
            author,
            page: None,
            details: BTreeMap::new(),
            error: Some(error),
        };
        apply_observation(&mut state, &observation)?;
        observations.push(observation);
        return Ok(SourceResult {
            observations,
            traces: client.traces,
            boundary: state.scan.progress[&cursor_key].boundary.clone(),
            elapsed_ms: started.elapsed().as_millis(),
        });
    }

    let mut page = state
        .scan
        .progress
        .get(&cursor_key)
        .map(|cursor| cursor.next_page.max(1))
        .unwrap_or(1);
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

async fn run() -> Result<(), String> {
    let started = Instant::now();
    let args: Vec<String> = env::args().collect();
    let input = PathBuf::from(opt(&args, "--state", "monitor-state"));
    let output = PathBuf::from(opt(
        &args,
        "--output",
        "reports/phase3b-dual-source-acquire",
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

    let author = read_one_author(&PathBuf::from(opt(
        &args,
        "--authors",
        "dual-source-author.json",
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
    let request_limit: usize = opt(&args, "--max-requests", "200")
        .parse()
        .map_err(|_| "BUDGET")?;
    let budget = RequestBudget::new(request_limit)?;

    let mut state = load(&input)?;
    if !state.author_names().contains(&author) {
        return Err("AUTHOR_NOT_IN_CONFIRMED_INPUT".into());
    }
    let scan_id = Utc::now().to_rfc3339();
    state.begin_with_event_history(
        &scan_id,
        &scan_id,
        vec![author.clone()],
        mode,
        threshold,
        false,
    )?;

    let jm = jm_adapter::JmClient::new(jm_adapter::DEFAULT_DOMAIN)?;
    let mut pica = pica_adapter::PicaClient::new(env::var("PICA_TOKEN").unwrap_or_default())?;
    let login = match (env::var("PICA_EMAIL"), env::var("PICA_PASSWORD")) {
        (Ok(email), Ok(password)) if !email.is_empty() && !password.is_empty() => {
            budgeted_pica_login(&mut pica, &budget, &email, &password).await
        }
        _ if env::var("PICA_TOKEN").is_ok_and(|token| !token.is_empty()) => Ok(()),
        _ => Err("MISSING_PICA_AUTH".into()),
    };

    let concurrent_started = Instant::now();
    let (jm_result, pica_result) = tokio::join!(
        scan_jm(state.clone(), author.clone(), jm, budget.clone()),
        scan_pica(
            state,
            author.clone(),
            pica,
            budget.clone(),
            login.err()
        )
    );
    let jm_result = jm_result?;
    let pica_result = pica_result?;
    let concurrent_wall_ms = concurrent_started.elapsed().as_millis();

    let mut observations = jm_result.observations;
    observations.extend(pica_result.observations);
    let tape = Tape {
        observations,
        direct_checks: Vec::new(),
    };
    write_json(&output.join("observations.json"), &tape)?;

    let requests = budget.used();
    let trace_count = jm_result.traces.len() + pica_result.traces.len();
    if requests != trace_count {
        return Err("REQUEST_BUDGET_TRACE_MISMATCH".into());
    }
    let overlap_observed = concurrent_wall_ms < jm_result.elapsed_ms + pica_result.elapsed_ms;
    let report = json!({
        "schema_version": 1,
        "author": author,
        "requested_mode": requested_mode,
        "effective_mode": mode,
        "request_budget": request_limit,
        "requests": requests,
        "jm_requests": jm_result.traces.len(),
        "pica_requests": pica_result.traces.len(),
        "jm_boundary": jm_result.boundary,
        "pica_boundary": pica_result.boundary,
        "jm_elapsed_ms": jm_result.elapsed_ms,
        "pica_elapsed_ms": pica_result.elapsed_ms,
        "dual_source_wall_elapsed_ms": concurrent_wall_ms,
        "overlap_observed": overlap_observed,
        "total_elapsed_ms": started.elapsed().as_millis(),
        "durable_state_mutated": false,
        "inventory_mutation_authorized": false,
        "task_completion_authorized": false,
        "promotion_authorized": false,
        "replacement_authorized": false,
        "physical_delete_authorized": false,
        "production_enablement_authorized": false,
        "requests_trace": {"jm": jm_result.traces, "pica": pica_result.traces}
    });
    write_json(&output.join("acquisition-report.json"), &report)?;
    println!("{report}");
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("PHASE3B_DUAL_SOURCE_ACQUIRE_ERROR: {error}");
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
    fn one_author_selection_is_fail_closed() {
        let root = std::env::temp_dir().join(format!(
            "dual-source-authors-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("authors.json");
        write_json(
            &path,
            &json!({"authors":[{"name":"A","enabled":true},{"name":"B","enabled":true}]}),
        )
        .unwrap();
        assert_eq!(
            read_one_author(&path).unwrap_err(),
            "DUAL_SOURCE_ACQUIRE_REQUIRES_ONE_AUTHOR"
        );
    }
}
