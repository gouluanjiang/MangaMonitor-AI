use cloud_monitor::monitor::hash;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn invoke(out: &Path, resume: Option<&Path>) -> std::process::Output {
    let root = root();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_matcher-m2"));
    cmd.current_dir(&root)
        .args([
            "--state",
            "monitor-state",
            "--export",
            "evidence/phase3b-run-33998989019/review-export.json",
            "--observations",
            "fixtures/matcher-m2/phase3b-observations.json",
            "--repair",
            "fixtures/matcher-m2/inventory-primary-repair.json",
            "--output",
        ])
        .arg(out)
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1");
    if let Some(p) = resume {
        cmd.arg("--resume").arg(p);
    }
    cmd.output().unwrap()
}
#[test]
fn full_215_cli_replay_is_offline_idempotent_and_keeps_seed_unchanged() {
    let root = root();
    let before = hash(&std::fs::read(root.join("monitor-state/inventory_index.json")).unwrap());
    let temp = std::env::temp_dir().join(format!(
        "manga-m2-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp).unwrap();
    let first = temp.join("first");
    let second = temp.join("second");
    let r = invoke(&first, None);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let r = invoke(&second, Some(&first));
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let summary: Value =
        serde_json::from_slice(&std::fs::read(second.join("summary.json")).unwrap()).unwrap();
    assert_eq!(summary["count"], 215);
    assert_eq!(summary["source_requests"], 0);
    assert_eq!(summary["image_requests"], 0);
    assert_eq!(summary["business_state_unchanged"], true);
    assert_eq!(summary["reanalyzed"], 0);
    assert_eq!(summary["new_events"], 0);
    assert_eq!(summary["after"]["AUTO_EXISTING"], 0);
    assert_eq!(summary["after"]["PROVEN_NEW"], 0);
    assert_eq!(summary["after"]["REVIEW_REQUIRED"], 215);
    assert_eq!(
        std::fs::read(first.join("checkpoint.json")).unwrap(),
        std::fs::read(second.join("checkpoint.json")).unwrap()
    );
    assert_eq!(
        before,
        hash(&std::fs::read(root.join("monitor-state/inventory_index.json")).unwrap())
    );
}
#[test]
fn cli_rejects_source_flags_and_writes_into_seed() {
    let r = Command::new(env!("CARGO_BIN_EXE_matcher-m2"))
        .args(["--live-source", "true"])
        .output()
        .unwrap();
    assert!(!r.status.success());
    let forbidden = root().join("monitor-state/m2-forbidden-output");
    assert!(!forbidden.exists());
    let r = invoke(&forbidden, None);
    assert!(!r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("OUTPUT_MUST_BE_SEPARATE"));
    assert!(!forbidden.exists());
}
