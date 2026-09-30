//! Synthetic old-format fixtures. No existing profile or source requests are used.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};
use tempfile::TempDir;
use workbench_storage::{
    DiscoveryPagePatch, DiscoveryRecord, DiscoveryWork, Source, WorkbenchStore, PRIVATE_DIRECTORY,
};

fn record(id: &str) -> DiscoveryRecord {
    DiscoveryRecord {
        work: DiscoveryWork {
            source: Source::Jm,
            work_id: id.into(),
            title: format!("Synthetic work {id}"),
            authors: vec!["Synthetic author".into()],
            description: None,
            tags: vec![],
            favorite: None,
            chapter_count: Some(1),
            page_count: Some(1),
            source_updated_at: None,
            cover_available: false,
        },
        matched_authors: vec!["Synthetic author".into()],
        author_verified: true,
        observed_at: 10,
        metadata_detail_at: None,
        scan_id: "a".repeat(64),
        first_discovered_run_id: None,
    }
}

fn patch(id: &str) -> DiscoveryPagePatch {
    DiscoveryPagePatch {
        account_key: "b".repeat(64),
        authors: vec![],
        records: vec![record(id)],
        retain_authors: None,
        last_check: None,
    }
}

fn catalog(version: u32, ids: &[&str]) -> Value {
    json!({
        "version": version,
        "accounts": [{
            "accountKey": "b".repeat(64),
            "authors": [],
            "records": ids.iter().map(|id| record(id)).collect::<Vec<_>>()
        }]
    })
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_json(root: &Path, name: &str, value: &Value) -> Vec<u8> {
    let bytes = serde_json::to_vec(value).unwrap();
    fs::write(root.join(name), &bytes).unwrap();
    bytes
}

// Versions are independently chosen for the legacy base, checkpoint value,
// immutable page envelope, and manifest. Hashes always match the fixture bytes,
// so a future-version rejection cannot be accidentally satisfied by corruption.
fn old_journal(root: &Path, with_checkpoint: bool, versions: [u32; 4]) -> u64 {
    let base = write_json(
        root,
        "discovery.json",
        &json!({
            "schemaVersion": 1,
            "revision": 1,
            "value": catalog(versions[0], &["101"])
        }),
    );
    let checkpoint = if with_checkpoint {
        let bytes = serde_json::to_vec(&json!({
            "revision": 2,
            "value": catalog(versions[1], &["101", "102"])
        }))
        .unwrap();
        let sha256 = hash(&bytes);
        fs::write(
            root.join(format!("discovery-checkpoint-{sha256}.json")),
            &bytes,
        )
        .unwrap();
        json!({"revision": 2, "sha256": sha256, "bytes": bytes.len()})
    } else {
        Value::Null
    };
    let previous = if with_checkpoint { 2 } else { 1 };
    let revision = previous + 1;
    let page = serde_json::to_vec(&json!({
        "version": versions[2],
        "previousRevision": previous,
        "previousSha256": null,
        "revision": revision,
        "patch": patch("103")
    }))
    .unwrap();
    let head = hash(&page);
    fs::write(root.join(format!("discovery-page-{head}.json")), &page).unwrap();
    write_json(
        root,
        "discovery-journal.json",
        &json!({
            "version": versions[3],
            "baseRevision": 1,
            "baseSha256": hash(&base),
            "revision": revision,
            "headSha256": head,
            "checkpoint": checkpoint,
            "patchCount": 1,
            "journalBytes": page.len()
        }),
    );
    fs::write(
        root.join("discovery-journal-required.json"),
        b"{\"version\":1}",
    )
    .unwrap();
    revision
}

fn journal_bytes(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(root)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                entry.path(),
            )
        })
        .filter(|(name, _)| name.starts_with("discovery"))
        .map(|(name, path)| (name, fs::read(path).unwrap()))
        .collect()
}

fn manifest(root: &Path) -> Value {
    serde_json::from_slice(&fs::read(root.join("discovery-journal.json")).unwrap()).unwrap()
}

#[test]
fn legacy_journal_reads_without_rewriting_and_append_keeps_mixed_version_chain() {
    for with_checkpoint in [false, true] {
        let directory = TempDir::new().unwrap();
        let store = WorkbenchStore::open(directory.path()).unwrap();
        let root = directory.path().join(PRIVATE_DIRECTORY);
        let revision = old_journal(&root, with_checkpoint, [1; 4]);
        let original = journal_bytes(&root);
        let old_manifest = manifest(&root);

        let before = store.read_discovery().unwrap();
        assert_eq!(before.revision, revision);
        assert_eq!(before.value.version, 2);
        let mut ids = if with_checkpoint {
            vec!["101", "102", "103"]
        } else {
            vec!["101", "103"]
        };
        assert_eq!(
            before.value.accounts[0].records,
            ids.iter().map(|id| record(id)).collect::<Vec<_>>()
        );
        assert_eq!(journal_bytes(&root), original);
        assert_eq!(
            WorkbenchStore::open(directory.path())
                .unwrap()
                .read_discovery()
                .unwrap(),
            before
        );
        assert_eq!(journal_bytes(&root), original);

        assert_eq!(
            store
                .apply_discovery_patch_for_following(revision, 0, patch("104"))
                .unwrap(),
            revision + 1
        );
        let next = manifest(&root);
        assert_eq!(next["version"], 2);
        assert_eq!(next["patchCount"], 2);
        assert_eq!(next["baseSha256"], old_manifest["baseSha256"]);
        assert_eq!(next["checkpoint"], old_manifest["checkpoint"]);
        let new_page: Value = serde_json::from_slice(
            &fs::read(root.join(format!(
                "discovery-page-{}.json",
                next["headSha256"].as_str().unwrap()
            )))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(new_page["version"], 2);
        assert_eq!(new_page["previousSha256"], old_manifest["headSha256"]);
        for (name, bytes) in &original {
            if name != "discovery-journal.json" {
                assert_eq!(fs::read(root.join(name)).unwrap(), *bytes);
            }
        }
        ids.push("104");
        let mixed = WorkbenchStore::open(directory.path())
            .unwrap()
            .read_discovery()
            .unwrap();
        assert_eq!(mixed.revision, revision + 1);
        assert_eq!(mixed.value.version, 2);
        assert_eq!(
            mixed.value.accounts[0].records,
            ids.iter().map(|id| record(id)).collect::<Vec<_>>()
        );

        // Only the explicit checkpoint operation replaces/retires old journal
        // artifacts; the legacy base remains byte-identical even then.
        store
            .checkpoint_discovery_for_following(revision + 1, 0)
            .unwrap();
        let compacted = manifest(&root);
        assert_eq!(compacted["version"], 2);
        assert_eq!(compacted["patchCount"], 0);
        let checkpoint: Value = serde_json::from_slice(
            &fs::read(root.join(format!(
                "discovery-checkpoint-{}.json",
                compacted["checkpoint"]["sha256"].as_str().unwrap()
            )))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(checkpoint["value"]["version"], 2);
        assert_eq!(
            fs::read(root.join("discovery.json")).unwrap(),
            original["discovery.json"]
        );
        assert_eq!(
            WorkbenchStore::open(directory.path())
                .unwrap()
                .read_discovery()
                .unwrap(),
            mixed
        );
    }
}

#[test]
fn future_version_at_any_journal_layer_is_rejected_without_replacement() {
    for layer in 0..4 {
        let directory = TempDir::new().unwrap();
        let store = WorkbenchStore::open(directory.path()).unwrap();
        let root = directory.path().join(PRIVATE_DIRECTORY);
        let mut versions = [1; 4];
        versions[layer] = 3;
        let revision = old_journal(&root, true, versions);
        let original = journal_bytes(&root);
        assert_eq!(
            store.read_discovery().unwrap_err().code,
            "UNSUPPORTED_SCHEMA",
            "layer {layer}"
        );
        assert_eq!(
            store
                .apply_discovery_patch_for_following(revision, 0, patch("104"))
                .unwrap_err()
                .code,
            "UNSUPPORTED_SCHEMA",
            "layer {layer}"
        );
        assert_eq!(
            store
                .checkpoint_discovery_for_following(revision, 0)
                .unwrap_err()
                .code,
            "UNSUPPORTED_SCHEMA",
            "layer {layer}"
        );
        assert_eq!(journal_bytes(&root), original, "layer {layer}");
    }
}
