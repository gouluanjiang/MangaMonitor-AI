//! Shared scan runner. It always writes to a separate staging directory.
use crate::{matcher_m2::repair_primary, monitor::*, persistence::*};
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

const DIRECT_CHECK_PENDING: &str = "DIRECT_CHECK_PENDING";
const JM_DETAIL_MAX_ATTEMPTS: usize = jm_adapter::BASELINE_DOMAINS.len();
const JM_SEARCH_MAX_ATTEMPTS: usize = JM_DETAIL_MAX_ATTEMPTS * 2;
const MAX_AUTHOR_CONCURRENCY: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Phase3A,
    Phase3B,
}

#[derive(Clone, Serialize, Deserialize)]
struct Observation {
    source: String,
    author: String,
    page: Option<SearchPage>,
    details: BTreeMap<String, Record>,
    error: Option<String>,
}

struct SourceAcquisition {
    observations: Vec<Observation>,
    traces: Vec<RequestTrace>,
}

struct AuthorLane {
    author: String,
    jm: jm_adapter::JmClient,
    pica: pica_adapter::PicaClient,
    pica_auth_error: Option<String>,
}

struct AuthorAcquisition {
    jm: SourceAcquisition,
    pica: SourceAcquisition,
}

#[derive(Default, Serialize, Deserialize)]
struct Tape {
    observations: Vec<Observation>,
    #[serde(default)]
    direct_checks: Vec<(String, Detail)>,
}

struct StageMetadata<'a> {
    base_commit: &'a str,
    requested_mode: &'a str,
    effective_requested_mode: &'a str,
    batch_index: usize,
    batch_count: usize,
    selected_authors: &'a [String],
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

    fn exhausted(&self) -> bool {
        self.used() >= self.limit
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
        .find(|w| w[0] == name)
        .map(|w| w[1].clone())
        .unwrap_or(default.into())
}

fn parse_author_concurrency(
    args: &[String],
    profile: Profile,
    replay: &str,
) -> Result<usize, String> {
    let concurrency: usize = opt(args, "--author-concurrency", "1")
        .parse()
        .map_err(|_| "AUTHOR_CONCURRENCY")?;
    if !(1..=MAX_AUTHOR_CONCURRENCY).contains(&concurrency) {
        return Err("AUTHOR_CONCURRENCY_OUT_OF_RANGE".into());
    }
    if concurrency > 1 && profile != Profile::Phase3B {
        return Err("AUTHOR_CONCURRENCY_REQUIRES_PHASE3B".into());
    }
    if concurrency > 1 && !replay.is_empty() {
        return Err("AUTHOR_CONCURRENCY_REQUIRES_LIVE_SOURCE".into());
    }
    Ok(concurrency)
}

fn cursor_complete(state: &State, source: &str, author: &str) -> bool {
    state
        .scan
        .progress
        .get(&State::cursor_key(source, author))
        .is_some_and(|cursor| {
            ["COMPLETE", "EARLY_STOP_HEURISTIC"].contains(&cursor.boundary.as_str())
        })
}

fn strategy_complete(state: &State) -> bool {
    state.scan.direct_failures.is_empty()
        && state.scan.selected_authors.iter().all(|author| {
            ["jm", "pica"]
                .iter()
                .all(|source| cursor_complete(state, source, author))
        })
}

fn planned_direct_check_keys(state: &State) -> Vec<String> {
    state
        .catalog
        .iter()
        .filter_map(|(source_key, entry)| {
            let pending = state
                .pending
                .values()
                .any(|task| task.target.source_key == *source_key && task.status == "pending");
            let serial = entry.record.metadata["finished"] == false;
            (pending || serial).then(|| source_key.clone())
        })
        .collect()
}

async fn budgeted_jm_search(
    client: &mut jm_adapter::JmClient,
    budget: &RequestBudget,
    author: &str,
    page: u64,
) -> Result<SearchPage, String> {
    let reservation = budget
        .reserve(JM_SEARCH_MAX_ATTEMPTS)
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
        .reserve(JM_DETAIL_MAX_ATTEMPTS)
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

async fn prepare_author_lane(
    state: &State,
    author: String,
    budget: &RequestBudget,
) -> Result<AuthorLane, String> {
    let jm = jm_adapter::JmClient::new(jm_adapter::DEFAULT_DOMAIN)?;
    let token = env::var("PICA_TOKEN").unwrap_or_default();
    let mut pica = pica_adapter::PicaClient::new(token.clone())?;
    let mut pica_auth_error = None;
    if !cursor_complete(state, "pica", &author) && token.is_empty() {
        pica_auth_error = match (env::var("PICA_EMAIL"), env::var("PICA_PASSWORD")) {
            (Ok(email), Ok(password)) if !email.is_empty() && !password.is_empty() => {
                budgeted_pica_login(&mut pica, budget, &email, &password)
                    .await
                    .err()
            }
            _ => Some("MISSING_PICA_AUTH".into()),
        };
    }
    Ok(AuthorLane {
        author,
        jm,
        pica,
        pica_auth_error,
    })
}

async fn acquire_jm_source(
    mut state: State,
    author: String,
    mut client: jm_adapter::JmClient,
    budget: RequestBudget,
) -> Result<SourceAcquisition, String> {
    let cursor_key = State::cursor_key("jm", &author);
    if cursor_complete(&state, "jm", &author) {
        return Ok(SourceAcquisition {
            observations: Vec::new(),
            traces: client.traces,
        });
    }
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
                apply(&mut state, &observation)?;
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
                apply(&mut state, &observation)?;
                observations.push(observation);
                if state.scan.progress[&cursor_key].boundary != "CHECKPOINT" {
                    break;
                }
                page += 1;
            }
        }
    }
    Ok(SourceAcquisition {
        observations,
        traces: client.traces,
    })
}

async fn acquire_pica_source(
    mut state: State,
    author: String,
    mut client: pica_adapter::PicaClient,
    auth_error: Option<String>,
    budget: RequestBudget,
) -> Result<SourceAcquisition, String> {
    let cursor_key = State::cursor_key("pica", &author);
    if cursor_complete(&state, "pica", &author) {
        return Ok(SourceAcquisition {
            observations: Vec::new(),
            traces: client.traces,
        });
    }
    if let Some(error) = auth_error {
        let observation = Observation {
            source: "pica".into(),
            author,
            page: None,
            details: BTreeMap::new(),
            error: Some(error),
        };
        apply(&mut state, &observation)?;
        return Ok(SourceAcquisition {
            observations: vec![observation],
            traces: client.traces,
        });
    }
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
                apply(&mut state, &observation)?;
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
                apply(&mut state, &observation)?;
                observations.push(observation);
                if state.scan.progress[&cursor_key].boundary != "CHECKPOINT" {
                    break;
                }
                page += 1;
            }
        }
    }
    Ok(SourceAcquisition {
        observations,
        traces: client.traces,
    })
}

async fn acquire_author(
    state: State,
    lane: AuthorLane,
    budget: RequestBudget,
) -> Result<AuthorAcquisition, String> {
    let AuthorLane {
        author,
        jm,
        pica,
        pica_auth_error,
    } = lane;
    let (jm, pica) = tokio::join!(
        acquire_jm_source(state.clone(), author.clone(), jm, budget.clone()),
        acquire_pica_source(state, author, pica, pica_auth_error, budget)
    );
    Ok(AuthorAcquisition { jm: jm?, pica: pica? })
}

async fn acquire_author_wave(
    state: State,
    authors: &[String],
    budget: RequestBudget,
) -> Result<Vec<AuthorAcquisition>, String> {
    if authors.is_empty() || authors.len() > MAX_AUTHOR_CONCURRENCY {
        return Err("AUTHOR_CONCURRENCY_OUT_OF_RANGE".into());
    }
    let mut lanes = Vec::with_capacity(authors.len());
    for author in authors {
        lanes.push(prepare_author_lane(&state, author.clone(), &budget).await?);
    }
    let mut lanes = lanes.into_iter();
    match authors.len() {
        1 => Ok(vec![acquire_author(state, lanes.next().unwrap(), budget).await?]),
        2 => {
            let lane0 = lanes.next().unwrap();
            let lane1 = lanes.next().unwrap();
            let (result0, result1) = tokio::join!(
                acquire_author(state.clone(), lane0, budget.clone()),
                acquire_author(state, lane1, budget)
            );
            Ok(vec![result0?, result1?])
        }
        3 => {
            let lane0 = lanes.next().unwrap();
            let lane1 = lanes.next().unwrap();
            let lane2 = lanes.next().unwrap();
            let (result0, result1, result2) = tokio::join!(
                acquire_author(state.clone(), lane0, budget.clone()),
                acquire_author(state.clone(), lane1, budget.clone()),
                acquire_author(state, lane2, budget)
            );
            Ok(vec![result0?, result1?, result2?])
        }
        _ => unreachable!(),
    }
}

pub async fn run(args: Vec<String>, profile: Profile) -> Result<(), String> {
    let started = Instant::now();
    let input = PathBuf::from(opt(&args, "--state", "state"));
    let output = PathBuf::from(opt(
        &args,
        "--output",
        if profile == Profile::Phase3A { "reports/phase3a" } else { "reports/phase3b-staging" },
    ));
    fs::create_dir_all(&output).map_err(|_| "OUTPUT_DIR")?;
    let input_abs = fs::canonicalize(&input).map_err(|_| "INPUT_DIR")?;
    let output_abs = fs::canonicalize(&output).map_err(|_| "OUTPUT_DIR")?;
    if input_abs == output_abs || input_abs.starts_with(&output_abs) || output_abs.starts_with(&input_abs) {
        return Err("OUTPUT_MUST_BE_SEPARATE_FROM_INPUT".into());
    }

    let all_authors = read_author_selection(&PathBuf::from(opt(&args, "--authors", "fixtures/phase3a-authors.json")))?;
    if all_authors.iter().collect::<std::collections::BTreeSet<_>>().len() != all_authors.len() {
        return Err("REQUIRE_UNIQUE_AUTHORS".into());
    }
    if profile == Profile::Phase3A && !(5..=10).contains(&all_authors.len()) {
        return Err("REQUIRE_5_TO_10_UNIQUE_AUTHORS".into());
    }

    let batch_size: usize = opt(&args, "--batch-size", if profile == Profile::Phase3A { "10" } else { "200" })
        .parse().map_err(|_| "BATCH_SIZE")?;
    let batch_index: usize = opt(&args, "--batch-index", "0").parse().map_err(|_| "BATCH_INDEX")?;
    if batch_size == 0 || batch_size > 200 { return Err("BATCH_SIZE_OUT_OF_RANGE".into()); }
    let start = batch_index.checked_mul(batch_size).ok_or("BATCH_INDEX")?;
    let selected: Vec<String> = all_authors.iter().skip(start).take(batch_size).cloned().collect();
    if selected.is_empty() { return Err("EMPTY_AUTHOR_BATCH".into()); }
    if profile == Profile::Phase3A && (batch_index != 0 || selected.len() != all_authors.len()) {
        return Err("REQUIRE_5_TO_10_UNIQUE_AUTHORS".into());
    }

    let requested_mode = opt(&args, "--mode", "full");
    let mode = match requested_mode.as_str() {
        "full" => "full",
        "incremental" => "incremental",
        "monthly" if profile == Profile::Phase3B => "incremental",
        _ => return Err("INVALID_MODE".into()),
    };
    let expected_base = opt(&args, "--expected-base-sha", "");
    let actual_base = opt(&args, "--actual-base-sha", &env::var("GITHUB_SHA").unwrap_or_default());
    if profile == Profile::Phase3B && !expected_base.is_empty() && expected_base != actual_base {
        return Err("BASE_COMMIT_MISMATCH".into());
    }

    let batch_count = all_authors.len().div_ceil(batch_size);
    let stage_metadata = StageMetadata {
        base_commit: &expected_base,
        requested_mode: &requested_mode,
        effective_requested_mode: mode,
        batch_index,
        batch_count,
        selected_authors: &selected,
    };
    let resume = args.iter().any(|s| s == "--resume");
    let mut s = if resume { load_checkpoint(&output)? } else { load(&input)? };
    let replay = opt(&args, "--replay", "");
    let author_concurrency = parse_author_concurrency(&args, profile, &replay)?;
    if profile == Profile::Phase3B && !resume && !replay.is_empty() && !s.scan.complete && !strategy_complete(&s)
        && !s.scan.scan_id.is_empty() && !s.scan.selected_authors.is_empty() {
        return Err("INCOMPLETE_SCAN_REQUIRES_RESUME".into());
    }

    let repair_overlay = opt(&args, "--repair-overlay", "");
    if !repair_overlay.is_empty() {
        if profile != Profile::Phase3B { return Err("REPAIR_OVERLAY_REQUIRES_PHASE3B".into()); }
        let manifest: Value = serde_json::from_slice(&fs::read(&repair_overlay).map_err(|_| "REPAIR_OVERLAY_READ")?)
            .map_err(|_| "INVALID_REPAIR_OVERLAY")?;
        let audit = repair_primary(&mut s.inventory, &manifest)?;
        s.scan.inventory_repairs = vec![audit];
    }
    if selected.iter().any(|a| !s.author_names().contains(a)) { return Err("AUTHOR_NOT_IN_CONFIRMED_INPUT".into()); }

    let before = s.clone();
    let threshold: usize = opt(&args, "--threshold", "5").parse().map_err(|_| "THRESHOLD")?;
    if resume {
        if s.scan.selected_authors != selected || s.scan.requested_mode != mode || s.scan.threshold != threshold {
            return Err("RESUME_OPTIONS_MISMATCH".into());
        }
    } else {
        let preserve_latest = profile == Profile::Phase3B && args.iter().any(|value| value == "--continue-cycle");
        s.begin_with_event_history(&now(), &now(), selected.clone(), mode, threshold, preserve_latest)?;
    }
    stage_save(&output, &s, profile, &stage_metadata)?;

    let context_before = hash(&(&before.pending, &before.review));
    let mut tape = Tape::default();
    let mut requests = 0usize;
    let mut traces = json!({});

    if !replay.is_empty() {
        tape = serde_json::from_slice(&fs::read(&replay).map_err(|_| "TAPE_READ")?).map_err(|_| "INVALID_TAPE")?;
        for o in &tape.observations {
            apply(&mut s, o)?;
            stage_save(&output, &s, profile, &stage_metadata)?;
        }
        for (k, result) in &tape.direct_checks {
            s.direct(k, result.clone(), &s.scan.scan_id.clone())?;
            if matches!(result, &Detail::ExplicitUnavailable) { s.scan.direct_failures.remove(k); }
        }
    } else {
        if profile == Profile::Phase3B && !args.iter().any(|value| value == "--live-source") {
            return Err("LIVE_SOURCE_REQUIRES_EXPLICIT_FLAG".into());
        }
        if resume && output.join("observations.json").exists() {
            tape = serde_json::from_slice(&fs::read(output.join("observations.json")).map_err(|_| "RESUME_TAPE_READ")?)
                .map_err(|_| "INVALID_RESUME_TAPE")?;
        }

        let request_limit: usize = opt(&args, "--max-requests", "400").parse().map_err(|_| "BUDGET")?;
        let budget = RequestBudget::new(request_limit)?;

        if !resume {
            for source_key in planned_direct_check_keys(&before) {
                s.scan.direct_failures.insert(source_key, DIRECT_CHECK_PENDING.into());
            }
            stage_save(&output, &s, profile, &stage_metadata)?;
        }

        if author_concurrency == 1 {
            let mut jm = jm_adapter::JmClient::new(jm_adapter::DEFAULT_DOMAIN)?;
            let mut pica = pica_adapter::PicaClient::new(env::var("PICA_TOKEN").unwrap_or_default())?;
            let login = match (env::var("PICA_EMAIL"), env::var("PICA_PASSWORD")) {
                (Ok(email), Ok(password)) if !email.is_empty() && !password.is_empty() => budgeted_pica_login(&mut pica, &budget, &email, &password).await,
                _ if env::var("PICA_TOKEN").is_ok_and(|token| !token.is_empty()) => Ok(()),
                _ => Err("MISSING_PICA_AUTH".into()),
            };

            for author in &selected {
                for source in ["jm", "pica"] {
                    let ck = State::cursor_key(source, author);
                    if cursor_complete(&s, source, author) { continue; }
                    if source == "pica" {
                        if let Err(code) = &login {
                            s.source_error(source, author, code);
                            tape.observations.push(Observation { source: source.into(), author: author.clone(), page: None, details: BTreeMap::new(), error: Some(code.clone()) });
                            continue;
                        }
                    }

                    let mut page = s.scan.progress.get(&ck).map(|c| c.next_page.max(1)).unwrap_or(1);
                    loop {
                        if budget.exhausted() { s.source_error(source, author, "REQUEST_BUDGET_CHECKPOINT"); break; }
                        let found = if source == "jm" {
                            budgeted_jm_search(&mut jm, &budget, author, page).await
                        } else {
                            budgeted_pica_search(&mut pica, &budget, author, page).await
                        };
                        let mut o = Observation { source: source.into(), author: author.clone(), page: None, details: BTreeMap::new(), error: None };
                        match found {
                            Err(code) => { o.error = Some(code); apply(&mut s, &o)?; tape.observations.push(o); break; }
                            Ok(search_page) => {
                                let mut failed = None;
                                for record in &search_page.records {
                                    if s.needs_detail(record) {
                                        let detail = if source == "jm" {
                                            budgeted_jm_detail(&mut jm, &budget, &record.source_work_id).await
                                        } else {
                                            budgeted_pica_detail(&mut pica, &budget, &record.source_work_id).await
                                        };
                                        match detail {
                                            Ok((detail_record, _)) => { s.accept(record, &detail_record)?; s.note_search_query(record, author); o.details.insert(key(record), detail_record); }
                                            Err(code) => { failed = Some(code); break; }
                                        }
                                    } else {
                                        s.observe_unchanged(record);
                                        s.note_search_query(record, author);
                                    }
                                }
                                o.page = Some(search_page.clone());
                                o.error = failed;
                                if let Some(code) = &o.error { s.source_error(source, author, code); } else { s.page_boundary(source, author, &search_page); }
                                tape.observations.push(o);
                                stage_save(&output, &s, profile, &stage_metadata)?;
                                write_json(&output.join("observations.json"), &tape)?;
                                let boundary = &s.scan.progress[&ck].boundary;
                                if boundary != "CHECKPOINT" { break; }
                                page += 1;
                            }
                        }
                    }
                }
            }

            let direct_keys: Vec<String> = s.scan.direct_failures.keys().cloned().collect();
            for source_key in direct_keys {
                if budget.exhausted() { break; }
                let Some(entry) = s.catalog.get(&source_key).cloned() else { continue; };
                if entry.record.source == "pica" && login.is_err() { continue; }
                let result = if entry.record.source == "jm" {
                    budgeted_jm_detail(&mut jm, &budget, &entry.record.source_work_id).await
                } else {
                    budgeted_pica_detail(&mut pica, &budget, &entry.record.source_work_id).await
                };
                let outcome = match result {
                    Ok((record, _)) => Detail::Available(Box::new(record)),
                    Err(code) if entry.record.source == "pica" && code == "CONFIRMED_UNAVAILABLE" => Detail::ExplicitUnavailable,
                    Err(code) => Detail::SourceError(code),
                };
                s.direct(&source_key, outcome.clone(), &s.scan.scan_id.clone())?;
                if matches!(outcome, Detail::ExplicitUnavailable) { s.scan.direct_failures.remove(&source_key); }
                tape.direct_checks.push((source_key, outcome));
                stage_save(&output, &s, profile, &stage_metadata)?;
                write_json(&output.join("observations.json"), &tape)?;
            }

            requests = jm.traces.len() + pica.traces.len();
            if requests != budget.used() || requests > request_limit { return Err("REQUEST_BUDGET_TRACE_MISMATCH".into()); }
            traces = json!({"jm":jm.traces,"pica":pica.traces});
        } else {
            let mut jm_traces = Vec::<RequestTrace>::new();
            let mut pica_traces = Vec::<RequestTrace>::new();
            for author_wave in selected.chunks(author_concurrency) {
                let results = acquire_author_wave(s.clone(), author_wave, budget.clone()).await?;
                for result in results {
                    jm_traces.extend(result.jm.traces);
                    pica_traces.extend(result.pica.traces);
                    for observation in result.jm.observations.into_iter().chain(result.pica.observations) {
                        apply(&mut s, &observation)?;
                        tape.observations.push(observation);
                        stage_save(&output, &s, profile, &stage_metadata)?;
                        write_json(&output.join("observations.json"), &tape)?;
                    }
                }
            }

            let mut direct_jm = jm_adapter::JmClient::new(jm_adapter::DEFAULT_DOMAIN)?;
            let mut direct_pica = pica_adapter::PicaClient::new(env::var("PICA_TOKEN").unwrap_or_default())?;
            let direct_login = match (env::var("PICA_EMAIL"), env::var("PICA_PASSWORD")) {
                (Ok(email), Ok(password)) if !email.is_empty() && !password.is_empty() => budgeted_pica_login(&mut direct_pica, &budget, &email, &password).await,
                _ if env::var("PICA_TOKEN").is_ok_and(|token| !token.is_empty()) => Ok(()),
                _ => Err("MISSING_PICA_AUTH".into()),
            };
            let direct_keys: Vec<String> = s.scan.direct_failures.keys().cloned().collect();
            for source_key in direct_keys {
                if budget.exhausted() { break; }
                let Some(entry) = s.catalog.get(&source_key).cloned() else { continue; };
                if entry.record.source == "pica" && direct_login.is_err() { continue; }
                let result = if entry.record.source == "jm" {
                    budgeted_jm_detail(&mut direct_jm, &budget, &entry.record.source_work_id).await
                } else {
                    budgeted_pica_detail(&mut direct_pica, &budget, &entry.record.source_work_id).await
                };
                let outcome = match result {
                    Ok((record, _)) => Detail::Available(Box::new(record)),
                    Err(code) if entry.record.source == "pica" && code == "CONFIRMED_UNAVAILABLE" => Detail::ExplicitUnavailable,
                    Err(code) => Detail::SourceError(code),
                };
                s.direct(&source_key, outcome.clone(), &s.scan.scan_id.clone())?;
                if matches!(outcome, Detail::ExplicitUnavailable) { s.scan.direct_failures.remove(&source_key); }
                tape.direct_checks.push((source_key, outcome));
                stage_save(&output, &s, profile, &stage_metadata)?;
                write_json(&output.join("observations.json"), &tape)?;
            }
            jm_traces.extend(direct_jm.traces);
            pica_traces.extend(direct_pica.traces);
            requests = jm_traces.len() + pica_traces.len();
            if requests != budget.used() || requests > request_limit { return Err("REQUEST_BUDGET_TRACE_MISMATCH".into()); }
            traces = json!({"jm":jm_traces,"pica":pica_traces});
        }
    }

    s.finish();
    let strategy_done = strategy_complete(&s);
    stage_save(&output, &s, profile, &stage_metadata)?;
    write_json(&output.join("observations.json"), &tape)?;

    let mut event_kinds = BTreeMap::<String, usize>::new();
    for event in &s.scan.events {
        if let Some(kind) = event["kind"].as_str() { *event_kinds.entry(kind.to_owned()).or_default() += 1; }
    }
    let mut review_reasons = BTreeMap::<String, usize>::new();
    for review in s.review.values().filter(|review| review.status == "REVIEW_REQUIRED") {
        *review_reasons.entry(review.reason.clone()).or_default() += 1;
    }
    let new_review_events = event_kinds.get("NEW_REVIEW").copied().unwrap_or(0);
    let direct_checks_pending = s.scan.direct_failures.values().filter(|code| code.as_str() == DIRECT_CHECK_PENDING).count();
    let report = json!({
        "phase":if profile==Profile::Phase3A{"3A_ARTIFACT_ONLY"}else{"3B_STAGED_PRODUCTION"},
        "git_commit":env::var("GITHUB_SHA").ok(),
        "github_run_id":env::var("GITHUB_RUN_ID").ok(),
        "base_commit":if expected_base.is_empty(){Value::Null}else{json!(expected_base)},
        "requested_mode":requested_mode,
        "effective_requested_mode":mode,
        "batch_index":batch_index,
        "batch_size":batch_size,
        "batch_count":batch_count,
        "all_author_count":all_authors.len(),
        "author_concurrency":if replay.is_empty(){author_concurrency}else{1},
        "source_concurrency":if replay.is_empty() && author_concurrency>1{2}else{1},
        "durable_writer_serialized":true,
        "requests":requests,
        "elapsed_ms":started.elapsed().as_millis(),
        "replay":!replay.is_empty(),
        "selected_authors":selected,
        "complete":s.scan.complete,
        "coverage_complete":s.scan.complete,
        "strategy_complete":strategy_done,
        "catalog_records":s.catalog.len(),
        "pending":s.pending.len(),
        "review":s.review.values().filter(|r|r.status=="REVIEW_REQUIRED").count(),
        "review_reasons":review_reasons,
        "new_events":s.scan.events.len(),
        "event_kinds":event_kinds,
        "new_review_events":new_review_events,
        "review_migration":s.scan.review_migration,
        "business_state_unchanged":context_before==hash(&(&s.pending,&s.review)),
        "reanalyzed":s.catalog.values().map(|e|e.analysis_count).sum::<u64>()-before.catalog.values().map(|e|e.analysis_count).sum::<u64>(),
        "matcher_version":rules_core::title_m2::RULE_VERSION,
        "inventory_repairs":s.scan.inventory_repairs,
        "inactive_records":s.catalog.values().filter(|e|!e.active).count(),
        "unavailable_streak_nonzero":s.catalog.values().filter(|e|e.unavailable_streak>0).count(),
        "source_error_boundaries":s.scan.progress.values().filter(|cursor|cursor.boundary=="SOURCE_ERROR").count(),
        "direct_source_failures":s.scan.direct_failures.len(),
        "direct_checks_pending":direct_checks_pending,
        "boundaries":s.scan.progress,
        "image_requests":0,
        "input_state_modified":false,
        "requests_trace":traces
    });
    write_json(&output.join("scan-report.json"), &report)?;

    let changed: Vec<_> = s.catalog.iter().filter(|(k, e)| before.catalog.get(*k).map(|old| hash(old) != hash(e)).unwrap_or(true)).map(|(k, _)| k.clone()).collect();
    write_json(&output.join("state-diff.json"), &json!({
        "catalog_changed_keys":changed,
        "pending_before":before.pending.len(),
        "pending_after":s.pending.len(),
        "review_before":before.review.len(),
        "review_after":s.review.len(),
        "events":s.scan.events,
        "deletion_authorized":false
    }))?;
    println!("{}", json!({
        "requests":requests,
        "complete":s.scan.complete,
        "strategy_complete":strategy_done,
        "events":s.scan.events.len(),
        "catalog":s.catalog.len(),
        "author_concurrency":if replay.is_empty(){author_concurrency}else{1}
    }));
    if args.iter().any(|a| a == "--assert-idempotent") && (context_before != hash(&(&s.pending, &s.review)) || !s.scan.events.is_empty()) {
        return Err("IDEMPOTENCE_FAILED".into());
    }
    Ok(())
}

fn stage_save(output: &Path, state: &State, profile: Profile, metadata: &StageMetadata<'_>) -> Result<(), String> {
    save(output, state)?;
    if profile == Profile::Phase3B {
        write_json(&output.join("state-manifest.json"), &json!({
            "schema_version": 1,
            "base_commit": metadata.base_commit,
            "state_hash": hash(state),
            "scan_id": state.scan.scan_id,
            "complete": state.scan.complete,
            "coverage_complete": state.scan.complete,
            "strategy_complete": strategy_complete(state),
            "requested_mode": metadata.requested_mode,
            "effective_requested_mode": metadata.effective_requested_mode,
            "batch_index": metadata.batch_index,
            "batch_count": metadata.batch_count,
            "selected_authors": metadata.selected_authors
        }))?;
    }
    Ok(())
}

fn read_author_selection(path: &Path) -> Result<Vec<String>, String> {
    let value: Value = serde_json::from_slice(&fs::read(path).map_err(|_| "AUTHORS_READ")?).map_err(|_| "AUTHORS_JSON")?;
    if let Ok(names) = serde_json::from_value::<Vec<String>>(value.clone()) { return Ok(names); }
    value["authors"].as_array().ok_or_else(|| String::from("AUTHORS_JSON"))?.iter()
        .filter(|author| author["enabled"] != false)
        .map(|author| author["name"].as_str().map(str::to_owned).ok_or_else(|| String::from("AUTHORS_JSON")))
        .collect()
}

fn apply(s: &mut State, o: &Observation) -> Result<(), String> {
    let ck = State::cursor_key(&o.source, &o.author);
    if s.scan.progress.get(&ck).is_some_and(|c| ["COMPLETE", "EARLY_STOP_HEURISTIC"].contains(&c.boundary.as_str())) { return Ok(()); }
    if let Some(p) = &o.page {
        for r in &p.records {
            if s.needs_detail(r) {
                if let Some(d) = o.details.get(&key(r)) {
                    s.accept(r, d)?;
                    s.note_search_query(r, &o.author);
                } else if o.error.is_none() { return Err("REPLAY_MISSING_DETAIL".into()); }
            } else {
                s.observe_unchanged(r);
                s.note_search_query(r, &o.author);
            }
        }
        if o.error.is_none() { s.page_boundary(&o.source, &o.author, p); }
    }
    if let Some(code) = &o.error { s.source_error(&o.source, &o.author, code); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn empty_state() -> State {
        State {
            authors: json!({"authors":[]}), inventory: json!({"works":[]}), catalog: BTreeMap::new(), pending: BTreeMap::new(), review: BTreeMap::new(), cleanup_review: json!([]), decisions: Decisions::default(), scan: Scan::default(),
        }
    }

    #[test]
    fn strategy_completion_accepts_incremental_early_stop_without_claiming_coverage() {
        let mut state = empty_state();
        state.scan.selected_authors = vec!["Writer".into()];
        for source in ["jm", "pica"] {
            state.scan.progress.insert(State::cursor_key(source, "Writer"), Cursor { boundary: "EARLY_STOP_HEURISTIC".into(), ..Cursor::default() });
        }
        state.finish();
        assert!(!state.scan.complete);
        assert!(strategy_complete(&state));
        state.scan.direct_failures.insert("jm:1".into(), DIRECT_CHECK_PENDING.into());
        assert!(!strategy_complete(&state));
    }

    #[test]
    fn request_budget_reservation_is_fail_closed_and_releases_unused_attempts() {
        assert_eq!(JM_SEARCH_MAX_ATTEMPTS, jm_adapter::BASELINE_DOMAINS.len() * 2);
        let budget = RequestBudget::new(12).unwrap();
        let search = budget.reserve(JM_SEARCH_MAX_ATTEMPTS).unwrap();
        assert!(budget.reserve(JM_DETAIL_MAX_ATTEMPTS).is_none());
        search.commit(2).unwrap();
        assert_eq!(budget.used(), 2);
        let detail = budget.reserve(JM_DETAIL_MAX_ATTEMPTS).unwrap();
        detail.commit(1).unwrap();
        assert_eq!(budget.used(), 3);
        assert!(budget.used() <= 12);
    }

    #[test]
    fn author_concurrency_is_bounded_and_phase3b_live_only() {
        let args = vec!["phase3b".to_string(), "--author-concurrency".to_string(), "3".to_string()];
        assert_eq!(parse_author_concurrency(&args, Profile::Phase3B, "").unwrap(), 3);
        assert_eq!(parse_author_concurrency(&args, Profile::Phase3A, "").unwrap_err(), "AUTHOR_CONCURRENCY_REQUIRES_PHASE3B");
        assert_eq!(parse_author_concurrency(&args, Profile::Phase3B, "tape.json").unwrap_err(), "AUTHOR_CONCURRENCY_REQUIRES_LIVE_SOURCE");
        let invalid = vec!["phase3b".to_string(), "--author-concurrency".to_string(), "4".to_string()];
        assert_eq!(parse_author_concurrency(&invalid, Profile::Phase3B, "").unwrap_err(), "AUTHOR_CONCURRENCY_OUT_OF_RANGE");
    }
}
