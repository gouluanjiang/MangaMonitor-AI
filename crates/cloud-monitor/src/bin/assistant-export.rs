use cloud_monitor::{assistant, persistence};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}};

const BUNDLE_SCHEMA_VERSION: u64 = 1;

fn usage() -> ! {
    eprintln!(
        "usage: assistant-export --state <dir> --output <new-dir> [--batch-size N]"
    );
    std::process::exit(2);
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| "ASSISTANT_EXPORT_SERIALIZE")?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|_| format!("ASSISTANT_EXPORT_WRITE:{}", path.display()))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn state_hashes(state_dir: &Path) -> Result<(BTreeMap<String, String>, String), String> {
    let mut hashes = BTreeMap::new();
    let mut aggregate = Sha256::new();
    for name in persistence::FILES {
        let bytes = fs::read(state_dir.join(name))
            .map_err(|_| format!("ASSISTANT_EXPORT_READ:{name}"))?;
        let hash = sha256(&bytes);
        aggregate.update(name.as_bytes());
        aggregate.update(b":");
        aggregate.update(hash.as_bytes());
        aggregate.update(b"\n");
        hashes.insert(name.to_owned(), hash);
    }
    Ok((hashes, format!("{:x}", aggregate.finalize())))
}

fn export(state_dir: &Path, output: &Path, batch_size: usize) -> Result<(), String> {
    if batch_size == 0 || batch_size > assistant::MAX_REVIEW_BATCH {
        return Err("INVALID_ASSISTANT_EXPORT_BATCH_SIZE".into());
    }
    if output.exists() {
        return Err("ASSISTANT_EXPORT_OUTPUT_EXISTS".into());
    }

    let state = persistence::load(state_dir)?;
    let (input_hashes, aggregate_state_hash) = state_hashes(state_dir)?;
    fs::create_dir_all(output).map_err(|_| "ASSISTANT_EXPORT_CREATE_OUTPUT")?;

    let scan = assistant::scan_summary(&state);
    let review = assistant::review_backlog_summary(&state);
    let pending = assistant::pending_task_summary(&state);
    let collection = assistant::collection_summary(&state)?;

    let mut files = vec![
        "scan-summary.json".to_owned(),
        "review-summary.json".to_owned(),
        "pending.json".to_owned(),
        "collection.json".to_owned(),
    ];
    write_json(&output.join("scan-summary.json"), &scan)?;
    write_json(&output.join("review-summary.json"), &review)?;
    write_json(&output.join("pending.json"), &pending)?;
    write_json(&output.join("collection.json"), &collection)?;

    let total = review["total"]
        .as_u64()
        .ok_or("INVALID_ASSISTANT_REVIEW_TOTAL")? as usize;
    let mut offset = 0usize;
    let mut batch_count = 0usize;
    while offset < total {
        let batch = assistant::review_batch(&state, offset, batch_size)?;
        let returned = batch["returned"]
            .as_u64()
            .ok_or("INVALID_ASSISTANT_REVIEW_RETURNED")? as usize;
        if returned == 0 {
            return Err("ASSISTANT_EXPORT_ZERO_PROGRESS".into());
        }
        let name = format!("review-batch-{offset:06}.json");
        write_json(&output.join(&name), &batch)?;
        files.push(name);
        batch_count += 1;
        offset = offset.saturating_add(returned);
    }
    files.sort();

    let mut all_files = files.clone();
    all_files.push("manifest.json".to_owned());
    all_files.sort();
    let manifest = json!({
        "schema_version": BUNDLE_SCHEMA_VERSION,
        "assistant_view_schema_version": assistant::ASSISTANT_VIEW_SCHEMA_VERSION,
        "bundle": "mangamonitor-assistant-read-only",
        "source_scan_id": state.scan.scan_id,
        "source_scan_status": if state.scan.complete { "COMPLETE" } else { "PARTIAL" },
        "review_total": total,
        "review_batch_size": batch_size,
        "review_batch_count": batch_count,
        "input_state_sha256": input_hashes,
        "aggregate_state_sha256": aggregate_state_hash,
        "files": all_files,
    });
    write_json(&output.join("manifest.json"), &manifest)?;
    Ok(())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut state: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut batch_size = 50usize;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => state = args.next().map(PathBuf::from),
            "--output" => output = args.next().map(PathBuf::from),
            "--batch-size" => {
                batch_size = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_else(|| usage())
            }
            _ => usage(),
        }
    }

    let state = state.unwrap_or_else(|| usage());
    let output = output.unwrap_or_else(|| usage());
    if let Err(error) = export(&state, &output, batch_size) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
