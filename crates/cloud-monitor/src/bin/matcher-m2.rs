//! Offline-only executable. No adapter/client path, credentials, or source flags.
use cloud_monitor::{matcher_m2::*, monitor::*, persistence::*};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use state_model::Record;
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};
fn read(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
fn sha(path: &Path) -> Result<String, String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(fs::read(path).map_err(|e| e.to_string())?)
    ))
}
fn run() -> Result<(), String> {
    let args: Vec<_> = env::args().skip(1).collect();
    let mut opts = BTreeMap::new();
    if args.len() % 2 != 0 {
        return Err("EXPECTED_NAMED_PATH_ARGUMENTS".into());
    }
    for pair in args.chunks(2) {
        if ![
            "--state",
            "--export",
            "--observations",
            "--repair",
            "--output",
            "--resume",
        ]
        .contains(&pair[0].as_str())
            || opts
                .insert(pair[0].clone(), PathBuf::from(&pair[1]))
                .is_some()
        {
            return Err("UNSUPPORTED_OR_DUPLICATE_OPTION".into());
        }
    }
    for required in [
        "--state",
        "--export",
        "--observations",
        "--repair",
        "--output",
    ] {
        if !opts.contains_key(required) {
            return Err(format!("MISSING_{required}"));
        }
    }
    let input = fs::canonicalize(&opts["--state"]).map_err(|e| e.to_string())?;
    let config = read(
        &input
            .parent()
            .ok_or("STATE_PARENT")?
            .join("monitor-config.json"),
    )?;
    if config["production_enabled"] != false {
        return Err("REQUIRE_PRODUCTION_DISABLED".into());
    }
    // No overwriting input, its ancestors, or an existing unrelated output directory.
    let out = &opts["--output"];
    if out.exists() {
        return Err("OUTPUT_MUST_BE_NEW_DIRECTORY".into());
    }
    let parent = out.parent().ok_or("OUTPUT_PARENT")?;
    let resolved = fs::canonicalize(parent)
        .map_err(|e| e.to_string())?
        .join(out.file_name().ok_or("OUTPUT_NAME")?);
    if resolved == input || resolved.starts_with(&input) || input.starts_with(&resolved) {
        return Err("OUTPUT_MUST_BE_SEPARATE".into());
    }
    let export = read(&opts["--export"])?;
    let tape = read(&opts["--observations"])?;
    let mut records = BTreeMap::new();
    for o in tape["observations"]
        .as_array()
        .ok_or("OBSERVATIONS_ARRAY")?
    {
        if !o["error"].is_null() {
            return Err("SOURCE_ERROR_IN_BASELINE_TAPE".into());
        }
        for (key_in_tape, v) in o["details"].as_object().ok_or("OBSERVATION_DETAILS")? {
            let r: Record = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
            if key(&r) != *key_in_tape {
                return Err("TAPE_SOURCE_KEY_MISMATCH".into());
            }
            if let Some(old) = records.insert(key(&r), r.clone()) {
                if hash(&(&old.author, &old.raw_title, &old.metadata))
                    != hash(&(&r.author, &r.raw_title, &r.metadata))
                {
                    return Err("CONFLICTING_REPLAY_DETAILS".into());
                }
            }
        }
    }
    let mut before = BTreeMap::<String, usize>::new();
    let mut baseline = BTreeMap::new();
    let items = export["items"].as_array().ok_or("EXPORT_ITEMS")?;
    if items.len() != 215 || records.len() != 215 {
        return Err("REQUIRE_PHASE3B_215_RECORDS".into());
    }
    for item in items {
        let k = item["source_key"].as_str().ok_or("EXPORT_KEY")?;
        let r = records.get(k).ok_or("MISSING_CAPTURED_RECORD")?;
        if json!(r.raw_title) != item["raw_title"]
            || json!(r.author) != item["raw_authors"]
            || r.metadata != item["metadata"]
        {
            return Err(format!("EXPORT_TAPE_MISMATCH:{k}"));
        }
        let reason = item["reason"].as_str().ok_or("EXPORT_REASON")?.to_string();
        *before.entry(reason.clone()).or_default() += 1;
        if baseline.insert(k.to_string(), reason).is_some() {
            return Err("DUPLICATE_EXPORT_KEY".into());
        }
    }
    let (mut state, mut audit) = if let Some(resume) = opts.get("--resume") {
        (
            load_checkpoint(resume)?,
            serde_json::from_value::<ReplayAudit>(read(&resume.join("identity-audit.json"))?)
                .map_err(|e| e.to_string())?,
        )
    } else {
        (load(&input)?, ReplayAudit::default())
    };
    let repair = repair_primary(&mut state.inventory, &read(&opts["--repair"])?)?;
    let before_hash = hash(&state);
    let event_count = state.scan.events.len();
    let changed = replay(
        &mut state,
        &records.into_values().collect::<Vec<_>>(),
        &mut audit,
        &[],
    );
    let unchanged = before_hash == hash(&state);
    let mut reasons = BTreeMap::<String, usize>::new();
    let mut rules = BTreeMap::<String, usize>::new();
    let mut dispositions = BTreeMap::<String, usize>::new();
    for name in [
        "AUTO_EXISTING",
        "AUTHORITATIVE_EXISTING",
        "PROVEN_NEW",
        "REVIEW_REQUIRED",
        "IGNORED",
    ] {
        dispositions.insert(name.into(), 0);
    }
    for o in audit.outcomes.values() {
        *dispositions.entry(o.disposition.clone()).or_default() += 1;
        if o.disposition == "REVIEW_REQUIRED" {
            *reasons.entry(o.reason.clone()).or_default() += 1;
        } else {
            *rules.entry(o.reason.clone()).or_default() += 1;
        }
    }
    let rows: Vec<_> = audit
        .outcomes
        .values()
        .map(|o| json!({"before_reason":baseline[&o.source_key],"after":o}))
        .collect();
    let summary = json!({"rule_version":rules_core::title_m2::RULE_VERSION,"count":215,"before":before,
        "after":dispositions,"review_reasons":reasons,"automatic_or_authoritative_rules":rules,
        "reanalyzed":changed,"business_state_unchanged":unchanged,"new_events":state.scan.events.len()-event_count,
        "source_requests":0,"image_requests":0,"downloads":0,"deletion_authorized":false,"production_enabled":false,
        "observations_sha256":sha(&opts["--observations"])?,"export_sha256":sha(&opts["--export"])?,
        "inventory_repair":repair,"replay_scope":"Captured detail records from full Phase3B observations; identity/downstream state replay, not pagination rescan"});
    if opts.contains_key("--resume") && (!unchanged || changed != 0) {
        return Err("REPLAY_NOT_IDEMPOTENT".into());
    }
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    save(out, &state)?;
    write_json(&out.join("identity-audit.json"), &audit)?;
    write_json(&out.join("before-after.json"), &rows)?;
    write_json(&out.join("summary.json"), &summary)?;
    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
