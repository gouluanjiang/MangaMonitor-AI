use cloud_monitor::persistence;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}, process::Command};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture() -> PathBuf {
    root().join("fixtures/matcher-m3/phase3b-old-state")
}

fn unique(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "mangamonitor-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn state_bytes(dir: &Path) -> BTreeMap<&'static str, Vec<u8>> {
    persistence::FILES
        .iter()
        .map(|name| (*name, fs::read(dir.join(name)).unwrap()))
        .collect()
}

fn bundle_bytes(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        files.insert(
            entry.file_name().to_string_lossy().into_owned(),
            fs::read(entry.path()).unwrap(),
        );
    }
    files
}

fn invoke(output: &Path, batch_size: usize) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_assistant-export"))
        .current_dir(root())
        .args(["--state"])
        .arg(fixture())
        .args(["--output"])
        .arg(output)
        .args(["--batch-size", &batch_size.to_string()])
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .output()
        .unwrap()
}

#[test]
fn assistant_export_is_complete_deterministic_bounded_and_read_only() {
    let input = fixture();
    let before = state_bytes(&input);
    let first = unique("assistant-export-first");
    let second = unique("assistant-export-second");

    let run = invoke(&first, 100);
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let run = invoke(&second, 100);
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));

    let first_bytes = bundle_bytes(&first);
    let second_bytes = bundle_bytes(&second);
    assert_eq!(first_bytes, second_bytes);
    assert_eq!(before, state_bytes(&input));

    let manifest: Value = serde_json::from_slice(&first_bytes["manifest.json"]).unwrap();
    assert_eq!(manifest["bundle"], "mangamonitor-assistant-read-only");
    assert_eq!(manifest["review_total"], 215);
    assert_eq!(manifest["review_batch_size"], 100);
    assert_eq!(manifest["review_batch_count"], 3);

    let expected_batches = [
        ("review-batch-000000.json", 0u64, 100u64),
        ("review-batch-000100.json", 100u64, 100u64),
        ("review-batch-000200.json", 200u64, 15u64),
    ];
    let mut review_ids = std::collections::BTreeSet::new();
    for (name, offset, returned) in expected_batches {
        let value: Value = serde_json::from_slice(&first_bytes[name]).unwrap();
        assert_eq!(value["offset"], offset);
        assert_eq!(value["returned"], returned);
        assert!(value["returned"].as_u64().unwrap() <= 100);
        for item in value["items"].as_array().unwrap() {
            assert!(review_ids.insert(item["review_id"].as_str().unwrap().to_owned()));
        }
    }
    assert_eq!(review_ids.len(), 215);

    for name in persistence::FILES {
        let bytes = &before[name];
        let expected = format!("{:x}", Sha256::digest(bytes));
        assert_eq!(manifest["input_state_sha256"][name], expected);
    }

    fs::remove_dir_all(first).unwrap();
    fs::remove_dir_all(second).unwrap();
}

#[test]
fn assistant_export_refuses_existing_output_and_invalid_batch_size() {
    let output = unique("assistant-export-existing");
    fs::create_dir_all(&output).unwrap();
    let run = invoke(&output, 25);
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("ASSISTANT_EXPORT_OUTPUT_EXISTS"));
    fs::remove_dir_all(output).unwrap();

    let zero = unique("assistant-export-zero");
    let run = invoke(&zero, 0);
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("INVALID_ASSISTANT_EXPORT_BATCH_SIZE"));
    assert!(!zero.exists());
}
