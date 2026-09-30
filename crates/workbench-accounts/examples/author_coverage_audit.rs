//! Explicit local acceptance helper. Uses the same account/discovery service as
//! the desktop, never direct credentials, media routes or library mutations.
//! CI compiles/tests the helper; only an authorized local Windows run may execute it.
//! Optional `--cancel-file` names a new absolute local path outside Git/profile.
//! Create that empty file to request a stop, then wait for process completion and
//! protected-after/result receipts before shutting down. Never force-kill a write.
#[cfg(any(windows, test))]
mod contract {
    use serde::Deserialize;
    use std::{
        fs,
        future::Future,
        path::{Path, PathBuf},
        sync::Arc,
        time::{Duration, Instant},
    };
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
        pub cancel_file: Option<PathBuf>,
    }
    impl Args {
        pub fn parse(values: Vec<String>) -> Result<Self, String> {
            let mut profile = None;
            let mut output = None;
            let mut plan = None;
            let mut mode = None;
            let mut cancel_file = None;
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
                    "--cancel-file" => &mut cancel_file,
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
                cancel_file: cancel_file.map(PathBuf::from),
            })
        }
    }

    const READ_BUSY_DELAYS_MS: [u64; 5] = [250, 500, 1000, 2000, 4000];

    // Only local reads may use this wrapper. Starting a run or querying a source
    // must happen exactly once, even when its surrounding evidence read is busy.
    pub async fn retry_busy_read<T, F, Fut>(read: F) -> Result<T, String>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = workbench_accounts::Result<T>>,
    {
        retry_busy_read_with_wait(read, tokio::time::sleep).await
    }

    async fn retry_busy_read_with_wait<T, F, Fut, W, Wait>(
        mut read: F,
        mut wait: W,
    ) -> Result<T, String>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = workbench_accounts::Result<T>>,
        W: FnMut(Duration) -> Wait,
        Wait: Future<Output = ()>,
    {
        let mut delays = READ_BUSY_DELAYS_MS.into_iter().enumerate();
        loop {
            match read().await {
                Err(error) if error.code == "BUSY" => {
                    let Some((attempt, delay)) = delays.next() else {
                        return Err(error.code.to_owned());
                    };
                    println!("AUDIT_READ_BUSY_RETRY attempt={}", attempt + 1);
                    wait(Duration::from_millis(delay)).await;
                }
                result => return result.map_err(|error| error.code.to_owned()),
            }
        }
    }

    pub struct CancelFile {
        path: PathBuf,
    }

    fn is_link(metadata: &fs::Metadata) -> bool {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes() & 0x400 != 0
        }
        #[cfg(not(windows))]
        {
            metadata.file_type().is_symlink()
        }
    }

    impl CancelFile {
        pub fn new(path: &Path, profile: &Path) -> Result<Self, String> {
            if !path.is_absolute() || path.file_name().is_none() {
                return Err("AUDIT_CANCEL_FILE_ABSOLUTE_PATH_REQUIRED".into());
            }
            #[cfg(windows)]
            if !matches!(
                path.components().next(),
                Some(std::path::Component::Prefix(prefix))
                    if matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_))
            ) {
                return Err("AUDIT_CANCEL_FILE_LOCAL_PATH_REQUIRED".into());
            }
            let parent = path.parent().ok_or("AUDIT_CANCEL_FILE_PARENT_REQUIRED")?;
            for ancestor in parent.ancestors() {
                let metadata = fs::symlink_metadata(ancestor)
                    .map_err(|_| "AUDIT_CANCEL_FILE_PARENT_REQUIRED")?;
                if is_link(&metadata) {
                    return Err("AUDIT_CANCEL_FILE_LINK_NOT_ALLOWED".into());
                }
            }
            let parent = parent
                .canonicalize()
                .map_err(|_| "AUDIT_CANCEL_FILE_PARENT_REQUIRED")?;
            let profile = profile
                .canonicalize()
                .map_err(|_| "AUDIT_EXISTING_PROFILE_REQUIRED")?;
            if parent.starts_with(&profile)
                || parent
                    .ancestors()
                    .any(|ancestor| ancestor.join(".git").exists())
            {
                return Err("AUDIT_CANCEL_FILE_MUST_BE_PRIVATE".into());
            }
            let path = parent.join(path.file_name().unwrap());
            match fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self { path }),
                Ok(_) => Err("AUDIT_CANCEL_FILE_ALREADY_EXISTS".into()),
                Err(_) => Err("AUDIT_CANCEL_FILE_READ_FAILED".into()),
            }
        }

        pub fn check(&self) -> Result<(), String> {
            match fs::symlink_metadata(&self.path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Ok(metadata) if metadata.is_file() && !is_link(&metadata) => {
                    Err("AUDIT_CANCEL_REQUESTED".into())
                }
                Ok(_) => Err("AUDIT_CANCEL_FILE_INVALID".into()),
                Err(_) => Err("AUDIT_CANCEL_FILE_READ_FAILED".into()),
            }
        }
    }

    // This standalone helper owns the sole non-worker Arc. Both service workers
    // retain their Arc while awaiting source calls and blocking store tasks.
    // A cancelled phase alone is not a completion signal. Never abort a worker
    // or add a detached owner here; cooperative settlement has no forced timeout.
    pub async fn wait_for_workers<T>(service: &Arc<T>) {
        let mut notice = None;
        while Arc::strong_count(service) > 1 {
            if notice.is_none_or(|at: Instant| at.elapsed() >= Duration::from_secs(10)) {
                println!("AUDIT_SETTLING waiting_for_source_and_store=true");
                notice = Some(Instant::now());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
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
            assert!(args.cancel_file.is_none());
            let mut values: Vec<_> = [
                "--live-author-audit",
                "--profile",
                "profile",
                "--output",
                "output",
                "--plan",
                "plan",
                "--mode",
                "snapshot",
                "--cancel-file",
                "marker",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect();
            assert_eq!(
                Args::parse(values.clone()).unwrap().cancel_file,
                Some(PathBuf::from("marker"))
            );
            values.extend(["--cancel-file".into(), "duplicate".into()]);
            assert!(Args::parse(values).is_err());
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

        #[tokio::test]
        async fn local_read_retries_are_bounded_and_preserve_other_errors() {
            use std::{cell::Cell, future::ready};
            use workbench_accounts::AccountError;

            let reads = Cell::new(0);
            let waits = Cell::new(0);
            let exhausted = retry_busy_read_with_wait(
                || {
                    reads.set(reads.get() + 1);
                    ready(Err::<(), _>(AccountError::new("BUSY")))
                },
                |_| {
                    waits.set(waits.get() + 1);
                    ready(())
                },
            )
            .await;
            assert_eq!(exhausted.unwrap_err(), "BUSY");
            assert_eq!(reads.get(), 6);
            assert_eq!(waits.get(), 5);

            reads.set(0);
            let recovered = retry_busy_read_with_wait(
                || {
                    reads.set(reads.get() + 1);
                    ready(if reads.get() == 1 {
                        Err(AccountError::new("BUSY"))
                    } else {
                        Ok("saved fixture")
                    })
                },
                |_| ready(()),
            )
            .await;
            assert_eq!(recovered.unwrap(), "saved fixture");
            assert_eq!(reads.get(), 2);

            reads.set(0);
            let rejected = retry_busy_read(|| {
                reads.set(reads.get() + 1);
                ready(Err::<(), _>(AccountError::new("SESSION_CHANGED")))
            })
            .await;
            assert_eq!(rejected.unwrap_err(), "SESSION_CHANGED");
            assert_eq!(reads.get(), 1);
        }

        #[test]
        fn cancel_file_requires_a_new_absolute_private_path() {
            let temp = tempfile::tempdir().unwrap();
            let profile = temp.path().join("profile");
            fs::create_dir(&profile).unwrap();
            let path = temp.path().join("stop.request");
            let marker = CancelFile::new(&path, &profile).unwrap();
            assert!(marker.check().is_ok());
            fs::write(&path, b"").unwrap();
            assert_eq!(marker.check().unwrap_err(), "AUDIT_CANCEL_REQUESTED");
            assert!(CancelFile::new(&path, &profile).is_err());
            assert!(CancelFile::new(Path::new("relative.request"), &profile).is_err());
            assert!(CancelFile::new(&profile.join("stop.request"), &profile).is_err());

            let repo = temp.path().join("fixture-repo");
            fs::create_dir(&repo).unwrap();
            fs::write(repo.join(".git"), b"fixture git marker").unwrap();
            assert!(CancelFile::new(&repo.join("stop.request"), &profile).is_err());
        }

        #[cfg(unix)]
        #[test]
        fn cancel_file_rejects_linked_parents_and_replaced_markers() {
            let temp = tempfile::tempdir().unwrap();
            let profile = temp.path().join("profile");
            let private = temp.path().join("private");
            fs::create_dir(&profile).unwrap();
            fs::create_dir(&private).unwrap();
            let link = temp.path().join("linked-private");
            std::os::unix::fs::symlink(&private, &link).unwrap();
            assert!(CancelFile::new(&link.join("stop.request"), &profile).is_err());
            let path = private.join("stop.request");
            let marker = CancelFile::new(&path, &profile).unwrap();
            std::os::unix::fs::symlink(private.join("missing"), &path).unwrap();
            assert_eq!(marker.check().unwrap_err(), "AUDIT_CANCEL_FILE_INVALID");
        }

        #[tokio::test]
        async fn early_cancel_phase_cannot_finish_before_nested_store_work() {
            use std::sync::atomic::{AtomicBool, Ordering};
            let service = Arc::new(AtomicBool::new(false));
            let worker_service = Arc::clone(&service);
            let (source_release, source_wait) = tokio::sync::oneshot::channel();
            let (phase_changed, phase_wait) = tokio::sync::oneshot::channel();
            let (store_started, store_wait) = tokio::sync::oneshot::channel();
            let (store_release, store_gate) = std::sync::mpsc::channel();
            let worker = tokio::spawn(async move {
                // Service cancellation can already report terminal here.
                worker_service.store(true, Ordering::SeqCst);
                phase_changed.send(()).unwrap();
                source_wait.await.unwrap();
                tokio::task::spawn_blocking(move || {
                    store_started.send(()).unwrap();
                    store_gate.recv().unwrap();
                })
                .await
                .unwrap();
                drop(worker_service);
            });
            let settled = wait_for_workers(&service);
            tokio::pin!(settled);
            phase_wait.await.unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(20), &mut settled)
                    .await
                    .is_err()
            );
            assert!(service.load(Ordering::SeqCst));
            source_release.send(()).unwrap();
            store_wait.await.unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(20), &mut settled)
                    .await
                    .is_err()
            );
            store_release.send(()).unwrap();
            settled.await;
            worker.await.unwrap();
            assert_eq!(Arc::strong_count(&service), 1);
        }
    }
}

#[cfg(windows)]
mod local {
    use super::contract::{retry_busy_read, wait_for_workers, Args, CancelFile, Plan};
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

    enum ActiveRun {
        Authors(String),
        Recent(String),
    }

    struct Control {
        cancel_file: Option<CancelFile>,
        active_run: Option<ActiveRun>,
        scopes: Vec<DiscoveryScope>,
    }

    impl Control {
        fn check_cancel(&self) -> Result<()> {
            self.cancel_file.as_ref().map_or(Ok(()), CancelFile::check)
        }

        fn cancel(&self, service: &Service) -> Result<()> {
            match &self.active_run {
                Some(ActiveRun::Authors(id)) => service.discovery_cancel(id).map(|_| ()),
                Some(ActiveRun::Recent(id)) => service.recent_check_cancel(id).map(|_| ()),
                None => return Ok(()),
            }
            .map_err(|error| error.code.to_owned())
        }
    }

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
        let snapshot = read_catalog(service, scopes, true).await?;
        save(output, name, &snapshot)?;
        Ok(snapshot)
    }

    async fn read_catalog(
        service: &Service,
        scopes: &[DiscoveryScope],
        include_other: bool,
    ) -> Result<DiscoverySnapshot> {
        retry_busy_read(|| async {
            let snapshot = service
                .discovery_read_view(scopes.to_vec(), include_other)
                .await?;
            // Observation replay reports local storage contention as a warning.
            // It is still a read retry; no source request is repeated here.
            if snapshot.observation_error_code.as_deref() == Some("BUSY") {
                return Err(workbench_accounts::AccountError::new("BUSY"));
            }
            Ok(snapshot)
        })
        .await
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
        control: &Control,
    ) -> Result<Value> {
        if plan.works.is_empty() {
            return Err("AUDIT_EVIDENCE_WORKS_REQUIRED".into());
        }
        let mut rows = vec![];
        for work in &plan.works {
            control.check_cancel()?;
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
                        let result=retry_busy_read(|| service.source_author_known_works(work.source,&auth.session_id,author)).await;
                        memberships.push(match result {
                            Ok(known)=>json!({"author":author,"present":known.items.iter().any(|item|item.work_id==work.work_id),
                                "discoveryRevision":known.discovery_revision,"historyComplete":known.history_complete,
                                "observationErrorCode":known.observation_error_code}),
                            Err(error)=>json!({"author":author,"present":false,"errorCode":error}),
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
        control.check_cancel()?;
        let final_snapshot =
            snapshot(service, scopes, output, "catalog-after.private.json").await?;
        let confirmed = read_catalog(service, scopes, false).await?;
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
        control: &mut Control,
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
        control.active_run = Some(ActiveRun::Authors(started.run_id.clone()));
        let timer = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            control.check_cancel()?;
            require_app_closed()?;
            if timer.elapsed() > MAX_RUN {
                return Err("AUDIT_RUN_TIME_LIMIT".into());
            }
            let progress = retry_busy_read(|| service.discovery_progress(scopes.to_vec())).await?;
            control.check_cancel()?;
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
                wait_for_workers(service).await;
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
        control: &mut Control,
    ) -> Result<Value> {
        let started = service
            .recent_check_start(scopes.to_vec(), Some(plan.recent_max_pages))
            .await
            .map_err(|e| e.code.to_owned())?;
        control.active_run = Some(ActiveRun::Recent(started.id.clone()));
        let timer = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            control.check_cancel()?;
            require_app_closed()?;
            if timer.elapsed() > MAX_RUN {
                return Err("AUDIT_RUN_TIME_LIMIT".into());
            }
            let run = retry_busy_read(|| std::future::ready(service.recent_check_progress()))
                .await?
                .filter(|run| run.id == started.id)
                .ok_or("AUDIT_RUN_LOST")?;
            save(output, "recent-progress.private.json", &run)?;
            println!(
                "{}",
                json!({"mode":"recent","phase":run.phase,"pagesRead":run.pages_read,"recordsRead":run.records_read,"errorCode":run.error_code})
            );
            if run.phase != "checking" {
                wait_for_workers(service).await;
                for auth in scopes {
                    let history = retry_busy_read(|| {
                        service.source_recent_history(auth.source, &auth.session_id)
                    })
                    .await?;
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
    async fn execute(
        service: &Arc<Service>,
        args: &Args,
        plan: &Plan,
        control: &mut Control,
    ) -> Result<Value> {
        control.check_cancel()?;
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
        control.scopes = scopes.clone();
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
        control.check_cancel()?;
        let mut report = match args.mode.as_str() {
            "status" | "snapshot" => {
                let projected = read_catalog(service, &scopes, false).await?;
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
            "evidence" => evidence(service, &scopes, plan, &args.output, control).await,
            "authors" | "retry" => {
                authors(
                    service,
                    &scopes,
                    plan,
                    &args.output,
                    args.mode == "retry",
                    control,
                )
                .await
            }
            "recent" => recent(service, &scopes, plan, &args.output, control).await,
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
        let mut control = Control {
            cancel_file: args
                .cancel_file
                .as_ref()
                .map(|path| CancelFile::new(path, &args.profile))
                .transpose()?,
            active_run: None,
            scopes: vec![],
        };
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
        let mut result = execute(&service, &args, &plan, &mut control).await;
        if result.is_ok() {
            if let Err(error) = control.check_cancel() {
                result = Err(error);
            }
        }
        let stop_requested = result.is_err() && control.active_run.is_some();
        let cancel_error = if stop_requested {
            println!("AUDIT_STOP_REQUESTED waiting_for_worker_settlement=true");
            control.cancel(&service).err()
        } else {
            None
        };
        // Every exit after accepting a run passes through this barrier, including
        // failed polling/report writes. Dropping the caller's Arc is not a join.
        wait_for_workers(&service).await;
        let mut report = match result {
            Ok(report) => report,
            Err(error) => json!({"mode":args.mode,"passed":false,"errorCode":error}),
        };
        if stop_requested {
            report["stopRequested"] = json!(true);
            report["cancelErrorCode"] = json!(cancel_error);
            let saved_progress = match &control.active_run {
                Some(ActiveRun::Authors(_)) => {
                    match retry_busy_read(|| service.discovery_progress(control.scopes.clone()))
                        .await
                    {
                        Ok(progress) => save(
                            &args.output,
                            "author-progress-settled.private.json",
                            &progress,
                        ),
                        Err(error) => Err(error),
                    }
                }
                Some(ActiveRun::Recent(_)) => {
                    match retry_busy_read(|| std::future::ready(service.recent_check_progress()))
                        .await
                    {
                        Ok(progress) => save(
                            &args.output,
                            "recent-progress-settled.private.json",
                            &progress,
                        ),
                        Err(error) => Err(error),
                    }
                }
                None => Ok(()),
            };
            report["settledProgressErrorCode"] = json!(saved_progress.err());
            report["settledSnapshotErrorCode"] = json!(snapshot(
                &service,
                &control.scopes,
                &args.output,
                "catalog-after-stop.private.json",
            )
            .await
            .err());
        }
        drop(service);
        let after = protected(&args.profile)?;
        save(&args.output, "protected-after.private.json", &after)?;
        report["workersSettled"] = json!(true);
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
async fn main() -> std::process::ExitCode {
    match local::run().await {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::from(2),
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::from(2)
        }
    }
}
#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("AUDIT_WINDOWS_LOCAL_RUNTIME_REQUIRED");
    std::process::ExitCode::from(2)
}
