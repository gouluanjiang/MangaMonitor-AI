//! Synthetic-only sustained storage workload. Creates its own marked temporary
//! roots; accepts no application/profile path. Never contacts a source or opens media.
use serde_json::json;
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use workbench_storage::{
    BrowsingBaseline, BrowsingSurface, DiscoveryAccount, DiscoveryDocument, DiscoveryPagePatch,
    DiscoveryRecord, DiscoveryWork, DownloadsDocument, HistoryIdentity, LibraryDocument, Source,
    WorkbenchPreferences, WorkbenchStore, PRIVATE_DIRECTORY,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const MARKER: &str = ".synthetic-long-regression";
const ACCOUNT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}
fn random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}
fn event(file: &mut File, value: serde_json::Value) -> Result<()> {
    writeln!(file, "{value}")?;
    file.flush()?;
    Ok(())
}
fn history_id(n: u64) -> HistoryIdentity {
    HistoryIdentity::Source {
        source: Source::Jm,
        work_id: n.to_string(),
    }
}
fn record(n: usize) -> DiscoveryRecord {
    DiscoveryRecord {
        work: DiscoveryWork {
            source: if n.is_multiple_of(2) {
                Source::Jm
            } else {
                Source::Pica
            },
            work_id: if n.is_multiple_of(2) {
                (n + 1).to_string()
            } else {
                format!("{:024x}", n + 1)
            },
            title: format!("Synthetic {}", n / 2),
            authors: vec![format!("Synthetic author {}", n % 32)],
            description: None,
            tags: vec![],
            favorite: None,
            chapter_count: Some(1),
            page_count: Some(3),
            source_updated_at: None,
            cover_available: false,
        },
        matched_authors: vec![format!("Synthetic author {}", n % 32)],
        author_verified: true,
        observed_at: 1,
        metadata_detail_at: None,
        scan_id: "b".repeat(64),
        first_discovered_run_id: None,
    }
}
fn seed(root: &Path, count: usize) -> Result<WorkbenchStore> {
    fs::write(root.join(MARKER), "generated isolated test data only")?;
    let store = WorkbenchStore::open(root)?;
    store.write_preferences(0, WorkbenchPreferences::default())?;
    store.write_library(0, LibraryDocument::default())?;
    store.write_downloads(0, DownloadsDocument::default())?;
    store.write_discovery(
        0,
        DiscoveryDocument {
            version: 2,
            accounts: vec![DiscoveryAccount {
                account_key: ACCOUNT.into(),
                authors: vec![],
                records: (0..count).map(record).collect(),
                last_check: None,
            }],
        },
    )?;
    store.record_viewing_history(history_id(1), "Synthetic sentinel".into())?;
    store.checkpoint_discovery_for_following(1, 0)?;
    Ok(store)
}
fn crash_worker(root: &Path) -> Result<()> {
    if fs::read_to_string(root.join(MARKER))? != "generated isolated test data only" {
        return Err("synthetic marker missing".into());
    }
    let store = WorkbenchStore::open(root)?;
    println!("READY");
    std::io::stdout().flush()?;
    loop {
        let old = store.read_preferences()?;
        store.write_preferences(old.revision, old.value)?;
    }
}
fn crash_round(out: &Path, state: &mut u64) -> Result<serde_json::Value> {
    let root = tempfile::Builder::new()
        .prefix("crash-only-")
        .tempdir_in(out)?
        .keep();
    let store = seed(&root, 32)?;
    let before = fs::read(root.join(PRIVATE_DIRECTORY).join("viewing-history.json"))?;
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--crash-worker")
        .arg(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut ready = String::new();
    BufReader::new(child.stdout.take().ok_or("no child stdout")?).read_line(&mut ready)?;
    if ready.trim() != "READY" {
        let _ = child.kill();
        let _ = child.wait();
        return Err("crash child not ready".into());
    }
    thread::sleep(Duration::from_millis(1 + random(state) % 35));
    child.kill()?;
    let status = child.wait()?;
    let reopened = WorkbenchStore::open(&root)?;
    let saved = reopened.read_preferences()?;
    assert!(saved.revision >= 1);
    assert_eq!(
        before,
        fs::read(root.join(PRIVATE_DIRECTORY).join("viewing-history.json"))?
    );
    reopened.write_preferences(saved.revision, saved.value)?;
    assert_eq!(store.read_discovery()?.value.accounts[0].records.len(), 32);
    drop(reopened);
    drop(store);
    fs::remove_dir_all(&root)?;
    Ok(
        json!({"killed": !status.success(), "recoveredRevision": saved.revision, "unrelatedBytesPreserved": true}),
    )
}
fn corrupt_round(out: &Path, step: u64) -> Result<serde_json::Value> {
    let root = tempfile::Builder::new()
        .prefix("corruption-only-")
        .tempdir_in(out)?
        .keep();
    let store = seed(&root, 32)?;
    let path = root.join(PRIVATE_DIRECTORY).join("viewing-history.json");
    let good = fs::read(&path)?;
    let bad = match step % 3 {
        0 => b"{truncated".to_vec(),
        1 => String::from_utf8(good.clone())?.replace("\"version\":1", "\"version\":999").into_bytes(),
        _ => b"{\"schemaVersion\":1,\"revision\":0,\"value\":{\"version\":1,\"enabled\":true,\"entries\":[]}}".to_vec(),
    };
    assert_ne!(good, bad);
    fs::write(&path, &bad)?;
    let read_error = store
        .read_viewing_history()
        .err()
        .ok_or("DATA SAFETY: corrupt history became readable")?;
    let write_error = store
        .record_viewing_history(history_id(2), "Must refuse".into())
        .err()
        .ok_or("DATA SAFETY: corrupt history overwritten")?;
    assert_eq!(fs::read(&path)?, bad, "DATA SAFETY: corrupt bytes changed");
    fs::write(&path, &good)?;
    assert_eq!(store.read_viewing_history()?.value.entries.len(), 1);
    assert_eq!(store.read_discovery()?.value.accounts[0].records.len(), 32);
    drop(store);
    fs::remove_dir_all(&root)?;
    Ok(
        json!({"readError":read_error.code, "writeError":write_error.code, "preserved":true, "restored":true}),
    )
}
fn proc_sample() -> serde_json::Value {
    #[cfg(target_os = "linux")]
    {
        json!({"status":fs::read_to_string("/proc/self/status").ok().map(|s| s.lines().filter(|l| l.starts_with("VmRSS:") || l.starts_with("VmHWM:") || l.starts_with("Threads:")).collect::<Vec<_>>().join("\n")),
        "stat":fs::read_to_string("/proc/self/stat").ok(), "fds":fs::read_dir("/proc/self/fd").ok().map(|d| d.count())})
    }
    #[cfg(not(target_os = "linux"))]
    {
        json!({"unavailable":"native resource sampler currently Linux-only"})
    }
}
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--crash-worker") {
        return crash_worker(Path::new(args.get(2).ok_or("child root missing")?));
    }
    let out = PathBuf::from(args.get(1).ok_or("output directory required")?);
    let seconds: u64 = args.get(2).ok_or("duration seconds required")?.parse()?;
    if !(1..=21600).contains(&seconds) {
        return Err("duration outside 1..21600".into());
    }
    let mut state: u64 = args
        .get(3)
        .map(String::as_str)
        .unwrap_or("20261002")
        .parse()?;
    let seed_value = state;
    fs::create_dir_all(&out)?;
    let root = tempfile::Builder::new()
        .prefix("synthetic-storage-")
        .tempdir_in(&out)?
        .keep();
    let small_root = root.join("small");
    let large_root = root.join("large");
    fs::create_dir_all(&small_root)?;
    fs::create_dir_all(&large_root)?;
    // Setup and large seed serialization are excluded from the timed phase.
    let stores = [seed(&small_root, 32)?, seed(&large_root, 20000)?];
    let paths = [&small_root, &large_root];
    let mut log = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(out.join("operations.ndjson"))?;
    let mut metrics = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(out.join("metrics.ndjson"))?;
    let started = now();
    let clock = Instant::now();
    let mut step = 0_u64;
    let mut next_sample = 0_u64;
    let mut counts = [0_u64; 10];
    println!(
        "{}",
        json!({"event":"STORAGE_STARTED","startedUnixMs":started,"seconds":seconds,"seed":seed_value,"pid":std::process::id(),"root":root})
    );
    while clock.elapsed().as_secs() < seconds {
        step += 1;
        let index = (random(&mut state) % 2) as usize;
        let store = &stores[index];
        let count = [32, 20000][index];
        let mode = (random(&mut state) % 10) as usize;
        let op = Instant::now();
        event(
            &mut log,
            json!({"event":"begin","step":step,"mode":mode,"rootSize":count,"rng":state,"at":now()}),
        )?;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || -> Result<serde_json::Value> {
                match mode {
                    0 => {
                        let ids: Vec<_> = (0..4).map(|n| 10 + (step % 64) * 4 + n).collect();
                        thread::scope(|scope| {
                            let jobs: Vec<_> = ids
                                .iter()
                                .map(|n| {
                                    scope.spawn(move || {
                                        let other = WorkbenchStore::open(paths[index]).unwrap();
                                        other
                                            .record_viewing_history(
                                                history_id(*n),
                                                format!("Synthetic {n}"),
                                            )
                                            .unwrap();
                                    })
                                })
                                .collect();
                            for job in jobs {
                                job.join().unwrap();
                            }
                        });
                        let history = store.read_viewing_history()?.value;
                        assert!(history.entries.len() <= 100);
                        let identities: HashSet<_> =
                            history.entries.iter().map(|e| &e.identity).collect();
                        assert_eq!(identities.len(), history.entries.len());
                        for id in ids {
                            assert!(identities.contains(&history_id(id)));
                        }
                    }
                    1 => {
                        let doc = store.read_discovery()?;
                        assert_eq!(doc.value.accounts[0].records.len(), count);
                        let other = WorkbenchStore::open(paths[index])?;
                        assert_eq!(doc, other.read_discovery()?);
                    }
                    2 => {
                        let current = store.read_discovery()?;
                        let n = (random(&mut state) as usize) % count;
                        let mut next = record(n);
                        next.work.title = format!("Synthetic {n} observation {step}");
                        let revision = store.apply_discovery_patch_for_following(
                            current.revision,
                            0,
                            DiscoveryPagePatch {
                                account_key: ACCOUNT.into(),
                                authors: vec![],
                                records: vec![next.clone()],
                                retain_authors: None,
                                last_check: None,
                            },
                        )?;
                        if step.is_multiple_of(7) {
                            store.checkpoint_discovery_for_following(revision, 0)?;
                        }
                        let result = store.read_discovery()?;
                        assert_eq!(result.revision, revision);
                        assert_eq!(result.value.accounts[0].records.len(), count);
                        assert!(result.value.accounts[0].records.iter().any(|r| r == &next));
                    }
                    3 => {
                        let old = store.read_preferences()?;
                        let other = WorkbenchStore::open(paths[index])?;
                        other.write_preferences(old.revision, old.value.clone())?;
                        assert_eq!(
                            store
                                .write_preferences(old.revision, old.value)
                                .unwrap_err()
                                .code,
                            "REVISION_CONFLICT"
                        );
                        assert_eq!(store.read_preferences()?.revision, old.revision + 1);
                    }
                    4 => {
                        store.set_viewing_history_enabled(false)?;
                        let before = serde_json::to_vec(&store.read_viewing_history()?.value)?;
                        store.record_viewing_history(history_id(500), "Disabled".into())?;
                        assert_eq!(
                            before,
                            serde_json::to_vec(&store.read_viewing_history()?.value)?
                        );
                        if step.is_multiple_of(13) {
                            store.clear_viewing_history()?;
                        }
                        store.set_viewing_history_enabled(true)?;
                    }
                    5 => return corrupt_round(&out, step),
                    6 => return crash_round(&out, &mut state),
                    7 => {
                        let n = 1 + random(&mut state) % 100;
                        let baseline = BrowsingBaseline {
                            known_ids: vec![n.to_string()],
                            head_ids: vec![n.to_string()],
                            reached_end: false,
                        };
                        store.write_browsing_markers(
                            ACCOUNT,
                            BrowsingSurface::Recent,
                            baseline.clone(),
                            || Ok(()),
                        )?;
                        assert_eq!(
                            WorkbenchStore::open(paths[index])?
                                .read_browsing_markers(ACCOUNT, BrowsingSurface::Recent)?
                                .value
                                .baseline,
                            Some(baseline)
                        );
                        assert!(store
                            .read_browsing_markers(ACCOUNT, BrowsingSurface::Authors)?
                            .value
                            .baseline
                            .is_none());
                    }
                    8 => {
                        let library = store.read_library_shared()?;
                        let downloads = store.read_downloads_shared()?;
                        assert_eq!(*library, store.read_library()?);
                        assert_eq!(*downloads, store.read_downloads()?);
                        assert_eq!(library.revision, 1);
                        assert_eq!(downloads.revision, 1);
                    }
                    _ => {
                        let work = record(0).work;
                        let account = store.edit_special_follows(ACCOUNT, (0, 0), |a| {
                            a.set_author(&work.authors[0], true);
                            a.observe(
                                &work.authors[0],
                                work.source,
                                &"c".repeat(64),
                                std::slice::from_ref(&work),
                                true,
                                step,
                                None,
                            );
                        })?;
                        assert_eq!(account.authors.len(), 1);
                        assert!(
                            account.updates.is_empty(),
                            "repeated initial known work became new"
                        );
                        assert!(
                            store.read_successful_scan(ACCOUNT)?.is_none(),
                            "special observation changed manual scan receipt"
                        );
                    }
                }
                Ok(json!({"verified":true}))
            },
        ))
        .unwrap_or_else(|_| {
            Err("assertion/panic; preserved synthetic roots and preceding operation log".into())
        });
        match outcome {
            Ok(value) => {
                counts[mode] += 1;
                event(
                    &mut log,
                    json!({"event":"pass","step":step,"mode":mode,"ms":op.elapsed().as_millis(),"detail":value}),
                )?;
            }
            Err(error) => {
                event(
                    &mut log,
                    json!({"event":"FAIL_STOP_AFFECTED_STORAGE","step":step,"mode":mode,"error":error.to_string(),"root":root}),
                )?;
                fs::write(out.join("summary.json"), json!({"complete":false,"startedUnixMs":started,"endedUnixMs":now(),"elapsedMs":clock.elapsed().as_millis(),"operations":step,"counts":counts,"failure":error.to_string(),"preservedRoot":root}).to_string())?;
                return Err(error);
            }
        }
        if clock.elapsed().as_secs() >= next_sample {
            let checkpoint = json!({"at":now(),"startedUnixMs":started,"elapsedMs":clock.elapsed().as_millis(),"operations":step,"counts":counts,"randomState":state,"pid":std::process::id(),"resources":proc_sample(),"root":root,"complete":false});
            event(&mut metrics, checkpoint.clone())?;
            fs::write(out.join("checkpoint.tmp"), checkpoint.to_string())?;
            #[cfg(windows)]
            if out.join("checkpoint.json").exists() {
                fs::remove_file(out.join("checkpoint.json"))?;
            }
            fs::rename(out.join("checkpoint.tmp"), out.join("checkpoint.json"))?;
            println!(
                "{}",
                json!({"event":"STORAGE_CHECKPOINT","elapsedSeconds":clock.elapsed().as_secs(),"operations":step})
            );
            next_sample = clock.elapsed().as_secs() + 60;
        }
        thread::sleep(Duration::from_millis(100 + random(&mut state) % 150));
    }
    fs::write(out.join("summary.json"), json!({"complete":true,"startedUnixMs":started,"endedUnixMs":now(),"continuousMs":clock.elapsed().as_millis(),"operations":step,"counts":counts,"seed":seed_value,"root":root,"pid":std::process::id()}).to_string())?;
    println!(
        "{}",
        json!({"event":"STORAGE_ENDED","operations":step,"continuousMs":clock.elapsed().as_millis()})
    );
    Ok(())
}
