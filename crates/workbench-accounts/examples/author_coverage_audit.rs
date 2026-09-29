//! Explicit local acceptance helper. Uses the same account/discovery service as
//! the desktop, never direct credentials, media routes or library mutations.
//! CI compiles/tests the helper; only an authorized local Windows run may execute it.
#[cfg(any(windows, test))]
mod contract {
    use serde::Deserialize;
    use std::path::PathBuf;
    use workbench_accounts::Source;

    #[derive(Clone, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct EvidenceWork {
        pub source: Source,
        pub work_id: String,
        pub expected_authors: Vec<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct Plan {
        pub version: u32,
        pub expected_followed_authors: usize,
        #[serde(default)]
        pub authors: Vec<String>,
        #[serde(default)]
        pub works: Vec<EvidenceWork>,
        #[serde(default = "default_recent_pages")]
        pub recent_max_pages: u64,
    }
    fn default_recent_pages() -> u64 {
        128
    }
    impl Plan {
        pub fn validate(&self) -> Result<(), String> {
            let author_valid = |s: &str| {
                !s.trim().is_empty() && s.chars().count() <= 200 && !s.chars().any(char::is_control)
            };
            if self.version != 1
                || !(1..=2000).contains(&self.expected_followed_authors)
                || self.authors.len() > self.expected_followed_authors
                || self.authors.iter().any(|s| !author_valid(s))
                || self.works.len() > 1000
                || !(2..=1000).contains(&self.recent_max_pages)
                || self.works.iter().any(|work| {
                    let id_valid = match work.source {
                        Source::Jm => {
                            !work.work_id.is_empty()
                                && work.work_id.len() <= 19
                                && work.work_id.bytes().all(|b| b.is_ascii_digit())
                                && work.work_id.parse::<u64>().is_ok_and(|n| n > 0)
                        }
                        Source::Pica => {
                            work.work_id.len() == 24
                                && work
                                    .work_id
                                    .bytes()
                                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                        }
                    };
                    !id_valid
                        || work.expected_authors.is_empty()
                        || work.expected_authors.len() > 16
                        || work.expected_authors.iter().any(|s| !author_valid(s))
                })
            {
                return Err("AUDIT_PLAN_INVALID".into());
            }
            let unique: std::collections::HashSet<_> = self
                .works
                .iter()
                .map(|work| (work.source, work.work_id.as_str()))
                .collect();
            if unique.len() != self.works.len() {
                return Err("AUDIT_PLAN_DUPLICATE_WORK".into());
            }
            Ok(())
        }
    }

    pub struct Args {
        pub profile: PathBuf,
        pub output: PathBuf,
        pub plan: PathBuf,
        pub mode: String,
    }
    impl Args {
        pub fn parse(values: Vec<String>) -> Result<Self, String> {
            let mut profile = None;
            let mut output = None;
            let mut plan = None;
            let mut mode = None;
            let mut authorized = false;
            let mut iter = values.into_iter();
            while let Some(arg) = iter.next() {
                if arg == "--live-author-audit" {
                    if authorized {
                        return Err("AUDIT_ARGUMENT_INVALID".into());
                    }
                    authorized = true;
                    continue;
                }
                let slot = match arg.as_str() {
                    "--profile" => &mut profile,
                    "--output" => &mut output,
                    "--plan" => &mut plan,
                    "--mode" => &mut mode,
                    _ => return Err("AUDIT_ARGUMENT_INVALID".into()),
                };
                if slot.is_some() {
                    return Err("AUDIT_ARGUMENT_INVALID".into());
                }
                *slot = Some(
                    iter.next()
                        .filter(|value| !value.starts_with("--") && !value.is_empty())
                        .ok_or("AUDIT_ARGUMENT_INVALID")?,
                );
            }
            if !authorized {
                return Err("AUDIT_LOCAL_AUTHORIZATION_REQUIRED".into());
            }
            let mode = mode.ok_or("AUDIT_MODE_REQUIRED")?;
            if !matches!(
                mode.as_str(),
                "status" | "snapshot" | "evidence" | "authors" | "retry" | "recent"
            ) {
                return Err("AUDIT_MODE_INVALID".into());
            }
            Ok(Self {
                profile: profile.ok_or("AUDIT_PROFILE_REQUIRED")?.into(),
                output: output.ok_or("AUDIT_OUTPUT_REQUIRED")?.into(),
                plan: plan.ok_or("AUDIT_PLAN_REQUIRED")?.into(),
                mode,
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn execution_requires_explicit_scope_and_never_accepts_secrets_or_urls() {
            assert!(Args::parse(vec![]).is_err());
            for arg in ["--password", "--token", "--url", "--download"] {
                assert!(Args::parse(vec![
                    "--live-author-audit".into(),
                    arg.into(),
                    "fixture".into()
                ])
                .is_err());
            }
            let args = Args::parse(
                [
                    "--live-author-audit",
                    "--profile",
                    "profile",
                    "--output",
                    "output",
                    "--plan",
                    "plan",
                    "--mode",
                    "snapshot",
                ]
                .map(str::to_owned)
                .into(),
            )
            .unwrap();
            assert_eq!(args.mode, "snapshot");
            assert_eq!(args.profile, PathBuf::from("profile"));
            assert_eq!(args.output, PathBuf::from("output"));
            assert_eq!(args.plan, PathBuf::from("plan"));
        }
        #[test]
        fn private_plan_is_bounded_and_ids_are_not_arbitrary_requests() {
            let value = serde_json::json!({"version":1,"expectedFollowedAuthors":2,"authors":[],"works":[
                {"source":"JM","workId":"123","expectedAuthors":["Fixture writer"]}],"recentMaxPages":8});
            let valid: Plan = serde_json::from_value(value.clone()).unwrap();
            assert!(valid.validate().is_ok());
            let mut invalid = value.clone();
            invalid["works"][0]["workId"] = serde_json::json!("https://example.invalid/private");
            assert!(serde_json::from_value::<Plan>(invalid)
                .unwrap()
                .validate()
                .is_err());
            let mut duplicate = value;
            let extra = duplicate["works"][0].clone();
            duplicate["works"].as_array_mut().unwrap().push(extra);
            assert!(serde_json::from_value::<Plan>(duplicate)
                .unwrap()
                .validate()
                .is_err());
        }
    }
}

#[cfg(windows)]
mod local {
    use super::contract::{Args, Plan};
    use serde::Serialize;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::{
        collections::BTreeMap,
        fs,
        path::Path,
        process::Command,
        sync::Arc,
        time::{Duration, Instant},
    };
    use workbench_accounts::{
        AccountService, AccountState, DiscoveryMode, DiscoveryPhase, DiscoveryScope,
        DiscoverySnapshot, QueryKind, Source,
    };
    use workbench_credentials::WindowsVault;
    use workbench_sources::WorkbenchSources;
    use workbench_storage::{DiscoveryRangeState, PRIVATE_DIRECTORY};

    type Service = AccountService<WorkbenchSources, WindowsVault>;
    type Result<T> = std::result::Result<T, String>;
    const MAX_PLAN_BYTES: u64 = 2 * 1024 * 1024;
    const MAX_RUN: Duration = Duration::from_secs(12 * 60 * 60);

    fn is_ci() -> bool {
        ["CI", "GITHUB_ACTIONS", "TF_BUILD", "BUILDKITE"]
            .iter()
            .any(|name| std::env::var_os(name).is_some())
    }
    fn require_app_closed() -> Result<()> {
        let output = Command::new("tasklist.exe")
            .args(["/FO", "CSV", "/NH"])
            .output()
            .map_err(|_| "AUDIT_PROCESS_CHECK_FAILED")?;
        if !output.status.success() {
            return Err("AUDIT_PROCESS_CHECK_FAILED".into());
        }
        for row in String::from_utf8_lossy(&output.stdout).lines() {
            let image = row
                .split(',')
                .next()
                .unwrap_or_default()
                .trim_matches('"')
                .to_ascii_lowercase();
            if image == "mangamonitor-workbench-preview.exe" || image == "mangamonitor.exe" {
                return Err("AUDIT_CLOSE_DESKTOP_APP_REQUIRED".into());
            }
        }
        Ok(())
    }
    fn private_output(path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .ok_or("AUDIT_OUTPUT_PARENT_REQUIRED")?
            .canonicalize()
            .map_err(|_| "AUDIT_OUTPUT_PARENT_REQUIRED")?;
        if parent.ancestors().any(|ancestor| {
            let git = ancestor.join(".git");
            git.is_file() || git.join("HEAD").is_file()
        }) {
            return Err("AUDIT_OUTPUT_MUST_NOT_BE_REPOSITORY".into());
        }
        // Each run receives a new evidence directory. Never overwrite a prior report.
        fs::create_dir(path).map_err(|_| "AUDIT_NEW_OUTPUT_DIRECTORY_REQUIRED".to_owned())
    }
    fn redact(value: &mut Value) {
        match value {
            Value::Object(object) => {
                for key in ["sessionId", "accountId", "displayName", "accountKey"] {
                    object.remove(key);
                }
                for value in object.values_mut() {
                    redact(value);
                }
            }
            Value::Array(values) => {
                for value in values {
                    redact(value);
                }
            }
            _ => {}
        }
    }
    fn save(output: &Path, name: &str, value: &impl Serialize) -> Result<()> {
        let mut value = serde_json::to_value(value).map_err(|_| "AUDIT_REPORT_ENCODE_FAILED")?;
        redact(&mut value);
        fs::write(
            output.join(name),
            serde_json::to_vec_pretty(&value).map_err(|_| "AUDIT_REPORT_ENCODE_FAILED")?,
        )
        .map_err(|_| "AUDIT_REPORT_WRITE_FAILED".to_owned())
    }
    fn protected(root: &Path) -> Result<BTreeMap<String, Option<String>>> {
        let mut hashes = BTreeMap::new();
        for name in [
            "library.json",
            "downloads.json",
            "following.json",
            "author-query-policies.json",
        ] {
            let path = root.join(PRIVATE_DIRECTORY).join(name);
            let hash = match fs::read(path) {
                Ok(bytes) => Some(format!("{:x}", Sha256::digest(bytes))),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => return Err("AUDIT_PROTECTED_READ_FAILED".into()),
            };
            hashes.insert(name.into(), hash);
        }
        Ok(hashes)
    }
    fn scope(scopes: &[DiscoveryScope], source: Source) -> Result<&DiscoveryScope> {
        scopes
            .iter()
            .find(|scope| scope.source == source)
            .ok_or("AUDIT_SOURCE_NOT_CONNECTED".into())
    }
    async fn snapshot(
        service: &Arc<Service>,
        scopes: &[DiscoveryScope],
        output: &Path,
        name: &str,
    ) -> Result<DiscoverySnapshot> {
        let snapshot = service
            .discovery_read_view(scopes.to_vec(), true)
            .await
            .map_err(|e| e.code.to_owned())?;
        save(output, name, &snapshot)?;
        Ok(snapshot)
    }
    fn unfinished(snapshot: &DiscoverySnapshot) -> Vec<&workbench_storage::DiscoveryAuthorRange> {
        snapshot
            .authors
            .iter()
            .filter(|range| range.state != DiscoveryRangeState::Complete)
            .collect()
    }
    async fn evidence(
        service: &Arc<Service>,
        scopes: &[DiscoveryScope],
        plan: &Plan,
        output: &Path,
    ) -> Result<Value> {
        if plan.works.is_empty() {
            return Err("AUDIT_EVIDENCE_WORKS_REQUIRED".into());
        }
        let mut rows = vec![];
        for work in &plan.works {
            require_app_closed()?;
            let auth = scope(scopes, work.source)?;
            let response = service
                .query(
                    work.source,
                    &auth.session_id,
                    QueryKind::Detail,
                    &work.work_id,
                    None,
                    1,
                )
                .await;
            match response {
                Ok(response)=>{
                    let mut memberships=vec![];
                    for author in &work.expected_authors {
                        let result=service.source_author_known_works(work.source,&auth.session_id,author).await;
                        memberships.push(match result {
                            Ok(known)=>json!({"author":author,"present":known.items.iter().any(|item|item.work_id==work.work_id),
                                "discoveryRevision":known.discovery_revision,"historyComplete":known.history_complete,
                                "observationErrorCode":known.observation_error_code}),
                            Err(error)=>json!({"author":author,"present":false,"errorCode":error.code}),
                        });
                    }
                    let pass=response.observation_error_code.is_none() && !memberships.is_empty()
                        && memberships.iter().all(|membership|membership["present"]==true && membership["observationErrorCode"].is_null());
                    rows.push(json!({"source":work.source,"workId":work.work_id,"passed":pass,
                        "detail":response.page,"observationErrorCode":response.observation_error_code,"memberships":memberships}));
                }
                Err(error)=>rows.push(json!({"source":work.source,"workId":work.work_id,"passed":false,"errorCode":error.code})),
            }
            save(output, "evidence-progress.private.json", &rows)?;
            println!(
                "{}",
                json!({"mode":"evidence","checked":rows.len(),"total":plan.works.len()})
            );
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        let final_snapshot =
            snapshot(service, scopes, output, "catalog-after.private.json").await?;
        let confirmed = service
            .discovery_read_view(scopes.to_vec(), false)
            .await
            .map_err(|e| e.code.to_owned())?;
        for row in &mut rows {
            let saved = final_snapshot.records.iter().find(|record| {
                serde_json::to_value(record.work.source).ok().as_ref() == Some(&row["source"])
                    && record.work.work_id == row["workId"].as_str().unwrap_or_default()
            });
            let projected = confirmed.records.iter().any(|record| {
                serde_json::to_value(record.work.source).ok().as_ref() == Some(&row["source"])
                    && record.work.work_id == row["workId"].as_str().unwrap_or_default()
            });
            row["savedInCatalog"] = json!(saved.is_some());
            row["visibleInAllAuthorResults"] = json!(projected);
            row["passed"] = json!(
                row["passed"] == true
                    && saved.is_some()
                    && projected
                    && final_snapshot.observation_error_code.is_none()
                    && confirmed.observation_error_code.is_none()
            );
        }
        save(output, "evidence-final.private.json", &rows)?;
        Ok(
            json!({"mode":"evidence","passed":rows.iter().all(|row|row["passed"]==true),
            "checked":rows.len(),"passing":rows.iter().filter(|row|row["passed"]==true).count(),
            "nativeUiAcceptance":false,"inventoryMutation":false,
            "observationErrorCode":final_snapshot.observation_error_code,"projectedObservationErrorCode":confirmed.observation_error_code}),
        )
    }
    async fn authors(
        service: &Arc<Service>,
        scopes: &[DiscoveryScope],
        plan: &Plan,
        output: &Path,
        retry: bool,
    ) -> Result<Value> {
        let started = if retry {
            service
                .discovery_start_unfinished(scopes.to_vec(), plan.authors.clone())
                .await
        } else {
            service
                .discovery_start_with_mode(
                    scopes.to_vec(),
                    plan.authors.clone(),
                    DiscoveryMode::Full,
                )
                .await
        }
        .map_err(|e| e.code.to_owned())?;
        let timer = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if let Err(error) = require_app_closed() {
                let _ = service.discovery_cancel(&started.run_id);
                return Err(error);
            }
            if timer.elapsed() > MAX_RUN {
                let _ = service.discovery_cancel(&started.run_id);
                return Err("AUDIT_RUN_TIME_LIMIT".into());
            }
            let progress = service
                .discovery_progress(scopes.to_vec())
                .await
                .map_err(|e| e.code.to_owned())?;
            save(output, "author-progress.private.json", &progress)?;
            let run = progress
                .run
                .as_ref()
                .filter(|run| run.id == started.run_id)
                .ok_or("AUDIT_RUN_LOST")?;
            println!(
                "{}",
                json!({"mode":"authors","phase":run.phase,"requests":run.requests_used,
                "completedScopes":run.completed_scopes,"totalScopes":run.total_scopes,"errorCode":run.error_code})
            );
            if run.phase != DiscoveryPhase::Checking {
                let final_snapshot =
                    snapshot(service, scopes, output, "catalog-after.private.json").await?;
                save(
                    output,
                    "unfinished.private.json",
                    &unfinished(&final_snapshot),
                )?;
                return Ok(
                    json!({"mode":"authors","phase":run.phase,"passed":run.phase==DiscoveryPhase::Complete && run.storage_warning_code.is_none() && final_snapshot.observation_error_code.is_none(),
                    "observationErrorCode":final_snapshot.observation_error_code,
                    "requests":run.requests_used,"completedScopes":run.completed_scopes,"totalScopes":run.total_scopes,
                    "unfinished":unfinished(&final_snapshot).len(),"errorCode":run.error_code,"storageWarningCode":run.storage_warning_code}),
                );
            }
        }
    }
    async fn recent(
        service: &Arc<Service>,
        scopes: &[DiscoveryScope],
        plan: &Plan,
        output: &Path,
    ) -> Result<Value> {
        let started = service
            .recent_check_start(scopes.to_vec(), Some(plan.recent_max_pages))
            .await
            .map_err(|e| e.code.to_owned())?;
        let timer = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if let Err(error) = require_app_closed() {
                let _ = service.recent_check_cancel(&started.id);
                return Err(error);
            }
            if timer.elapsed() > MAX_RUN {
                let _ = service.recent_check_cancel(&started.id);
                return Err("AUDIT_RUN_TIME_LIMIT".into());
            }
            let run = service
                .recent_check_progress()
                .map_err(|e| e.code.to_owned())?
                .filter(|run| run.id == started.id)
                .ok_or("AUDIT_RUN_LOST")?;
            save(output, "recent-progress.private.json", &run)?;
            println!(
                "{}",
                json!({"mode":"recent","phase":run.phase,"pagesRead":run.pages_read,"recordsRead":run.records_read,"errorCode":run.error_code})
            );
            if run.phase != "checking" {
                for auth in scopes {
                    let history = service
                        .source_recent_history(auth.source, &auth.session_id)
                        .await
                        .map_err(|e| e.code.to_owned())?;
                    save(
                        output,
                        &format!("recent-{}.private.json", auth.source.as_str()),
                        &history,
                    )?;
                }
                let snapshot =
                    snapshot(service, scopes, output, "catalog-after.private.json").await?;
                return Ok(
                    json!({"mode":"recent","passed":run.phase=="complete" && snapshot.observation_error_code.is_none(),
                    "observationErrorCode":snapshot.observation_error_code,"run":run,"fullSiteCoverage":false}),
                );
            }
        }
    }
    async fn execute(service: &Arc<Service>, args: &Args, plan: &Plan) -> Result<Value> {
        let accounts = service.accounts(false).await;
        save(
            &args.output,
            "accounts.private.json",
            &accounts
                .iter()
                .map(|account| {
                    json!({
            "source":account.source,"state":account.state,"errorCode":account.error_code})
                })
                .collect::<Vec<_>>(),
        )?;
        if accounts
            .iter()
            .any(|account| account.state != AccountState::Connected)
        {
            return Err("AUDIT_BOTH_SOURCES_MUST_BE_CONNECTED".into());
        }
        let scopes: Vec<_> = accounts
            .into_iter()
            .map(|account| {
                Ok(DiscoveryScope {
                    source: account.source,
                    session_id: account.session_id.ok_or("AUDIT_SOURCE_NOT_CONNECTED")?,
                })
            })
            .collect::<Result<_>>()?;
        let before = snapshot(
            service,
            &scopes,
            &args.output,
            "catalog-before.private.json",
        )
        .await?;
        if before.followed_authors.len() != plan.expected_followed_authors {
            return Err("AUDIT_FOLLOWED_SCOPE_CHANGED".into());
        }
        if plan
            .authors
            .iter()
            .any(|author| !before.followed_authors.contains(author))
        {
            return Err("AUDIT_PLAN_AUTHOR_NOT_FOLLOWED".into());
        }
        if before
            .run
            .as_ref()
            .is_some_and(|run| run.phase == DiscoveryPhase::Checking)
        {
            return Err("AUDIT_DISCOVERY_ALREADY_RUNNING".into());
        }
        let initial_warning = before.observation_error_code.clone();
        let mut report = match args.mode.as_str() {
            "status" | "snapshot" => {
                let projected = service
                    .discovery_read_view(scopes.clone(), false)
                    .await
                    .map_err(|e| e.code.to_owned())?;
                save(&args.output, "confirmed-catalog.private.json", &projected)?;
                save(
                    &args.output,
                    "unfinished.private.json",
                    &unfinished(&before),
                )?;
                Ok(
                    json!({"mode":args.mode,"passed":before.observation_error_code.is_none() && projected.observation_error_code.is_none(),
                    "observationErrorCode":before.observation_error_code,"projectedObservationErrorCode":projected.observation_error_code,
                    "followedAuthors":before.followed_authors.len(),
                    "savedRecords":before.records.len(),"confirmedRecords":projected.records.len(),
                    "unfinishedScopes":unfinished(&before).len(),"fullSiteCoverage":false}),
                )
            }
            "evidence" => evidence(service, &scopes, plan, &args.output).await,
            "authors" | "retry" => {
                authors(service, &scopes, plan, &args.output, args.mode == "retry").await
            }
            "recent" => recent(service, &scopes, plan, &args.output).await,
            _ => Err("AUDIT_MODE_INVALID".into()),
        }?;
        if let Some(warning) = initial_warning {
            report["passed"] = json!(false);
            report["initialObservationErrorCode"] = json!(warning);
        }
        Ok(report)
    }
    pub async fn run() -> Result<bool> {
        if is_ci() {
            return Err("AUDIT_LOCAL_RUNTIME_REQUIRED".into());
        }
        let args = Args::parse(std::env::args().skip(1).collect())?;
        if !args.profile.join(PRIVATE_DIRECTORY).is_dir() {
            return Err("AUDIT_EXISTING_PROFILE_REQUIRED".into());
        }
        if fs::metadata(&args.plan)
            .map_err(|_| "AUDIT_PLAN_READ_FAILED")?
            .len()
            > MAX_PLAN_BYTES
        {
            return Err("AUDIT_PLAN_TOO_LARGE".into());
        }
        let plan: Plan =
            serde_json::from_slice(&fs::read(&args.plan).map_err(|_| "AUDIT_PLAN_READ_FAILED")?)
                .map_err(|_| "AUDIT_PLAN_INVALID")?;
        plan.validate()?;
        require_app_closed()?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(
                args.profile
                    .join(PRIVATE_DIRECTORY)
                    .join(".author-coverage-audit.lock"),
            )
            .map_err(|_| "AUDIT_LOCK_FAILED")?;
        lock.try_lock().map_err(|_| "AUDIT_ALREADY_RUNNING")?;
        private_output(&args.output)?;
        let before = protected(&args.profile)?;
        save(&args.output, "protected-before.private.json", &before)?;
        let sources = WorkbenchSources::new().map_err(|e| e.code.to_owned())?;
        let service = Arc::new(AccountService::new(
            sources,
            WindowsVault::new(),
            args.profile.clone(),
        ));
        let result = execute(&service, &args, &plan).await;
        drop(service);
        let after = protected(&args.profile)?;
        save(&args.output, "protected-after.private.json", &after)?;
        let mut report = match result {
            Ok(report) => report,
            Err(error) => json!({"mode":args.mode,"passed":false,"errorCode":error}),
        };
        report["protectedMetadataUnchanged"] = json!(before == after);
        report["nativeUiAcceptance"] = json!(false);
        let passed = report["passed"] == true && before == after;
        report["passed"] = json!(passed);
        save(&args.output, "result.private.json", &report)?;
        println!("{}", report);
        drop(lock);
        Ok(passed)
    }
}

#[cfg(windows)]
#[tokio::main]
async fn main() {
    let code = match local::run().await {
        Ok(true) => 0,
        Ok(false) => 2,
        Err(error) => {
            eprintln!("{error}");
            2
        }
    };
    std::process::exit(code);
}
#[cfg(not(windows))]
fn main() {
    eprintln!("AUDIT_WINDOWS_LOCAL_RUNTIME_REQUIRED");
    std::process::exit(2);
}
