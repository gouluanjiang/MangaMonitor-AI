//! Opt-in local Windows probes, compiled by CI but never into the application.
//! Faults/termination target only newly created marked synthetic case directories.
use crate::{AccountFollowing, FollowedAccount, Source, WorkbenchPreferences, WorkbenchStore, PRIVATE_DIRECTORY};
use crate::windows_validation_support as support;
use std::{cell::RefCell, fs, os::windows::process::CommandExt, path::{Path, PathBuf}, process::{Child, Command, Stdio}, thread, time::{Duration, Instant}};

const WORKER: &str = "MANGAMONITOR_STORAGE_VALIDATION_WORKER";
const CASE: &str = "MANGAMONITOR_STORAGE_VALIDATION_CASE";

enum Action { Error(i32), Wait }
struct Hook { root: PathBuf, phase: &'static str, action: Action }
thread_local! { static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) }; }
struct ResetHook;
impl Drop for ResetHook { fn drop(&mut self) { HOOK.with(|v| *v.borrow_mut() = None); } }

fn install(root: &Path, phase: &'static str, action: Action) -> ResetHook {
    let case = support::verify_case(root.parent().unwrap());
    assert_eq!(root.file_name().unwrap(), PRIVATE_DIRECTORY);
    HOOK.with(|v| {
        assert!(v.borrow().is_none());
        *v.borrow_mut() = Some(Hook { root: case.join(PRIVATE_DIRECTORY), phase, action });
    });
    ResetHook
}

pub(crate) fn checkpoint(root: &Path, phase: &'static str) -> crate::Result<()> {
    let hook = HOOK.with(|v| {
        let mut hook = v.borrow_mut();
        if hook.as_ref().is_some_and(|h| h.root == root && h.phase == phase) { hook.take() } else { None }
    });
    let Some(hook) = hook else { return Ok(()); };
    match hook.action {
        Action::Error(code) => {
            let injected = std::io::Error::from_raw_os_error(code);
            assert_eq!(injected.kind(), std::io::ErrorKind::PermissionDenied);
            Err(crate::StoreError::new("STORE_WRITE_FAILED"))
        }
        Action::Wait => {
            let case = support::verify_case(root.parent().unwrap());
            fs::write(case.join(format!("phase-{phase}")), b"owned child reached checkpoint").unwrap();
            // Parent terminates this owned PID. A bounded wait prevents an orphan
            // from surviving indefinitely if the controller itself fails.
            thread::sleep(Duration::from_secs(30));
            panic!("controller did not terminate the checkpoint worker");
        }
    }
}

fn following(author: &str) -> AccountFollowing {
    AccountFollowing { version: 1, accounts: vec![FollowedAccount {
        source: Source::Jm, account_key: "a".repeat(64), works: vec![], authors: vec![author.into()],
    }] }
}
fn following_path(case: &Path) -> PathBuf { case.join(PRIVATE_DIRECTORY).join("following.json") }
fn initialize(case: &Path) {
    let store = WorkbenchStore::open(case).unwrap();
    store.write_following(0, following("synthetic-baseline")).unwrap();
    store.write_preferences(0, WorkbenchPreferences::default()).unwrap();
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) { if self.0.try_wait().ok().flatten().is_none() { let _ = self.0.kill(); let _ = self.0.wait(); } }
}
fn child(case: &Path, test: &str, mode: &str) -> OwnedChild {
    support::verify_case(case);
    OwnedChild(Command::new(std::env::current_exe().unwrap())
        .args([test, "--exact", "--ignored", "--test-threads=1"])
        .env(WORKER, mode).env(CASE, case)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .creation_flags(0x0800_0000).spawn().unwrap())
}
fn wait_for(path: &Path, children: &mut [&mut OwnedChild]) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !path.exists() {
        for child in children.iter_mut() { assert!(child.0.try_wait().unwrap().is_none(), "worker exited before handshake"); }
        assert!(Instant::now() < deadline, "synthetic worker handshake timed out");
        thread::sleep(Duration::from_millis(10));
    }
}
fn finish(child: &mut OwnedChild) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() { assert!(status.success()); return; }
        assert!(Instant::now() < deadline, "synthetic worker did not finish");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "requires an explicitly marked synthetic Windows validation root"]
fn concurrent_different_values_and_stale_revision() {
    if let Ok(mode) = std::env::var(WORKER) {
        assert!(matches!(mode.as_str(), "writer-a" | "writer-b"));
        let case = support::verify_case(&PathBuf::from(std::env::var_os(CASE).unwrap()));
        let store = WorkbenchStore::open(&case).unwrap();
        let revision = store.read_following().unwrap().revision;
        fs::write(case.join(format!("ready-{mode}")), b"ready").unwrap();
        wait_for(&case.join("start-writers"), &mut []);
        let result = store.write_following(revision, following(&mode));
        let value = match result { Ok(doc) => serde_json::json!({"success":true,"revision":doc.revision,"author":mode}), Err(e) => serde_json::json!({"success":false,"code":e.code}) };
        fs::write(case.join(format!("result-{mode}.json")), serde_json::to_vec(&value).unwrap()).unwrap();
        return;
    }
    let directory = support::new_case("storage-cas");
    let case = directory.path();
    initialize(case);
    let untouched = fs::read(case.join(PRIVATE_DIRECTORY).join("preferences.json")).unwrap();
    let test = "windows_local_validation::concurrent_different_values_and_stale_revision";
    let mut a = child(case, test, "writer-a");
    let mut b = child(case, test, "writer-b");
    wait_for(&case.join("ready-writer-a"), &mut [&mut a, &mut b]);
    wait_for(&case.join("ready-writer-b"), &mut [&mut a, &mut b]);
    fs::write(case.join("start-writers"), b"go").unwrap();
    finish(&mut a); finish(&mut b);
    let results: Vec<serde_json::Value> = ["writer-a", "writer-b"].into_iter().map(|mode|
        serde_json::from_slice(&fs::read(case.join(format!("result-{mode}.json"))).unwrap()).unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r["success"] == true).count(), 1);
    assert_eq!(results.iter().find(|r| r["success"] == false).unwrap()["code"], "REVISION_CONFLICT");
    let winner = results.iter().find(|r| r["success"] == true).unwrap()["author"].as_str().unwrap();
    let reopened = WorkbenchStore::open(case).unwrap();
    assert_eq!(reopened.read_following().unwrap().value, following(winner));
    assert_eq!(reopened.write_following(1, following("stale-overwrite")).unwrap_err().code, "REVISION_CONFLICT");
    assert_eq!(reopened.read_following().unwrap().value, following(winner));
    assert_eq!(fs::read(case.join(PRIVATE_DIRECTORY).join("preferences.json")).unwrap(), untouched);
    println!("WINDOWS_VALIDATION storage-cas: two owned processes, one commit, stale different value rejected");
}

#[test]
#[ignore = "requires an explicitly marked synthetic Windows validation root"]
fn permission_denied_preserves_document_then_retries() {
    let directory = support::new_case("storage-permission");
    let case = directory.path(); initialize(case);
    let original = fs::read(following_path(case)).unwrap();
    let store = WorkbenchStore::open(case).unwrap();
    {
        let _fault = install(&store.root, "before-write", Action::Error(5));
        assert_eq!(store.write_following(1, following("synthetic-new")).unwrap_err().code, "STORE_WRITE_FAILED");
    }
    assert_eq!(fs::read(following_path(case)).unwrap(), original);
    assert_eq!(store.read_following().unwrap().revision, 1);
    store.write_following(1, following("synthetic-new")).unwrap();
    assert_eq!(WorkbenchStore::open(case).unwrap().read_following().unwrap().value, following("synthetic-new"));
    println!("WINDOWS_VALIDATION storage-permission: injected AccessDenied, no ACL changes, retry succeeded");
}

#[test]
#[ignore = "requires an explicitly marked synthetic Windows validation root"]
fn process_exit_at_known_commit_phases_recovers() {
    if let Ok(mode) = std::env::var(WORKER) {
        let phase = match mode.as_str() { "before-write" => "before-write", "before-rename" => "before-rename", "after-rename" => "after-rename", _ => panic!("unknown child phase") };
        let case = support::verify_case(&PathBuf::from(std::env::var_os(CASE).unwrap()));
        let store = WorkbenchStore::open(&case).unwrap();
        let _hook = install(&store.root, phase, Action::Wait);
        store.write_following(1, following("synthetic-new")).unwrap();
        panic!("worker was not stopped at its selected phase");
    }
    for phase in ["before-write", "before-rename", "after-rename"] {
        let directory = support::new_case("storage-crash");
        let case = directory.path(); initialize(case);
        let before = fs::read(following_path(case)).unwrap();
        let test = "windows_local_validation::process_exit_at_known_commit_phases_recovers";
        let mut worker = child(case, test, phase);
        let owned_pid = worker.0.id();
        wait_for(&case.join(format!("phase-{phase}")), &mut [&mut worker]);
        worker.0.kill().unwrap(); let status = worker.0.wait().unwrap(); assert!(!status.success());
        let reopened = WorkbenchStore::open(case).unwrap();
        let saved = reopened.read_following().unwrap();
        if phase == "after-rename" {
            assert_eq!(saved.revision, 2); assert_eq!(saved.value, following("synthetic-new"));
        } else {
            assert_eq!(saved.revision, 1); assert_eq!(saved.value, following("synthetic-baseline"));
            assert_eq!(fs::read(following_path(case)).unwrap(), before);
        }
        reopened.write_following(saved.revision, following("synthetic-retry")).unwrap();
        assert_eq!(WorkbenchStore::open(case).unwrap().read_following().unwrap().value, following("synthetic-retry"));
        println!("WINDOWS_VALIDATION storage-exit phase={phase} owned_pid={owned_pid}: recovered and retry committed");
    }
}
