//! Offline migration/IO diagnostic for an explicitly marked isolated profile copy.
//! No source clients, credentials, downloads or library mutation are available.
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Instant};
use workbench_storage::{
    discovery_record_matches_author, DiscoveryPagePatch, WorkbenchStore, PRIVATE_DIRECTORY,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::args_os().nth(1).ok_or("isolated root required")?);
    if !root.join(".mangamonitor-catalog-probe").is_file() {
        return Err("isolated-copy marker required".into());
    }
    let marker = std::fs::read_to_string(root.join(".mangamonitor-catalog-probe"))?;
    if marker.trim() != "isolated metadata copy; no live application profile" {
        return Err("invalid isolated-copy marker".into());
    }
    let legacy_path = root.join(PRIVATE_DIRECTORY).join("discovery.json");
    let legacy_before = std::fs::read(&legacy_path)?;
    let store = WorkbenchStore::open(&root)?;
    let start = Instant::now();
    let before = store.read_discovery()?;
    let initial_read_ms = start.elapsed().as_millis();
    let encoded = serde_json::to_vec(&before.value)?;
    let content_hash = format!("{:x}", Sha256::digest(&encoded));
    let records: usize = before.value.accounts.iter().map(|a| a.records.len()).sum();
    let confirmed: usize = before
        .value
        .accounts
        .iter()
        .flat_map(|a| &a.records)
        .filter(|r| discovery_record_matches_author(r))
        .count();
    let account = before.value.accounts.first().ok_or("empty catalog")?;
    let patch = DiscoveryPagePatch {
        account_key: account.account_key.clone(),
        authors: vec![],
        records: account.records.iter().take(20).cloned().collect(),
        retain_authors: None,
    };
    let patch_bytes = serde_json::to_vec(&patch)?.len();
    let following_revision = store.read_following()?.revision;
    let mut revision = before.revision;
    let mut commits_ms = Vec::new();
    // Upsert identical metadata: the copy's revision changes, its content does not.
    for _ in 0..4 {
        let start = Instant::now();
        revision = store.apply_discovery_patch_for_following(
            revision,
            following_revision,
            patch.clone(),
        )?;
        commits_ms.push(start.elapsed().as_millis());
    }
    let start = Instant::now();
    store.checkpoint_discovery_for_following(revision, following_revision)?;
    let checkpoint_ms = start.elapsed().as_millis();
    drop(store);
    let reopened = WorkbenchStore::open(&root)?.read_discovery()?;
    if reopened.revision != revision || reopened.value != before.value {
        return Err("catalog content changed during round trip".into());
    }
    if std::fs::read(&legacy_path)? != legacy_before {
        return Err("legacy JSON changed".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "records": records,
            "confirmedRecords": confirmed,
            "otherRecords": records - confirmed,
            "legacyBytes": legacy_before.len(),
            "catalogContentSha256": content_hash,
            "pageRecords": patch.records.len(),
            "pagePayloadBytes": patch_bytes,
            "initialReadMs": initial_read_ms,
            "commitMs": commits_ms,
            "checkpointMs": checkpoint_ms,
            "roundTripUnchanged": true,
            "legacyUnchanged": true
        }))?
    );
    Ok(())
}
