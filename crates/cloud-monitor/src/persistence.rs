use crate::monitor::*;
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};

pub const FILES: [&str; 8] = [
    "authors.json",
    "catalog.json",
    "inventory_index.json",
    "pending.json",
    "review.json",
    "decisions.json",
    "scan_state.json",
    "latest.json",
];
fn read(dir: &Path, name: &str) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(dir.join(name)).map_err(|_| format!("READ_{name}"))?)
        .map_err(|_| format!("INVALID_{name}"))
}
pub fn load(dir: &Path) -> Result<State, String> {
    let mut docs = BTreeMap::new();
    for name in FILES {
        docs.insert(name, read(dir, name)?);
    }
    for (file, field) in [
        ("authors.json", "authors"),
        ("catalog.json", "records"),
        ("inventory_index.json", "works"),
        ("pending.json", "tasks"),
        ("review.json", "match_review"),
        ("review.json", "cleanup_review"),
        ("decisions.json", "positive_mappings"),
        ("decisions.json", "negative_mappings"),
        ("decisions.json", "ignored_source_records"),
        ("decisions.json", "ignored_works"),
        ("latest.json", "events"),
    ] {
        if !docs[file][field].is_array() {
            return Err(format!("INVALID_{file}_{field}"));
        }
    }
    let mut catalog = BTreeMap::new();
    let mut work_ids = std::collections::BTreeSet::new();
    for w in docs["inventory_index.json"]["works"].as_array().unwrap() {
        let id = w["work_id"].as_str().ok_or("INVALID_INVENTORY_WORK_ID")?;
        if !work_ids.insert(id)
            || !w["owned"].is_boolean()
            || !w["authors_confirmed"].is_array()
            || !w["versions"].is_array()
        {
            return Err("INVALID_INVENTORY_WORK".into());
        }
    }
    for v in docs["catalog.json"]["records"].as_array().unwrap() {
        let e: Entry = serde_json::from_value(v.clone()).map_err(|_| "INVALID_CATALOG_ENTRY")?;
        if catalog.insert(key(&e.record), e).is_some() {
            return Err("DUPLICATE_SOURCE_ID".into());
        }
    }
    let mut pending = BTreeMap::new();
    for v in docs["pending.json"]["tasks"].as_array().unwrap() {
        let t: Task = serde_json::from_value(v.clone()).map_err(|_| "INVALID_TASK")?;
        if pending.insert(t.work_id.clone(), t).is_some() {
            return Err("DUPLICATE_TASK_WORK_ID".into());
        }
    }
    let mut review = BTreeMap::new();
    for v in docs["review.json"]["match_review"].as_array().unwrap() {
        let r: Review = serde_json::from_value(v.clone()).map_err(|_| "INVALID_REVIEW")?;
        review.insert(r.review_id.clone(), r);
    }
    let decisions =
        serde_json::from_value(docs["decisions.json"].clone()).map_err(|_| "INVALID_DECISIONS")?;
    let scan = if docs["scan_state.json"].get("phase3a_scan").is_some() {
        serde_json::from_value(docs["scan_state.json"]["phase3a_scan"].clone())
            .map_err(|_| "INVALID_SCAN")?
    } else {
        Scan::default()
    };
    Ok(State {
        authors: docs["authors.json"].clone(),
        inventory: docs["inventory_index.json"].clone(),
        catalog,
        pending,
        review,
        cleanup_review: docs["review.json"]["cleanup_review"].clone(),
        decisions,
        scan,
    })
}
pub fn load_checkpoint(dir: &Path) -> Result<State, String> {
    serde_json::from_value(read(dir, "checkpoint.json")?).map_err(|_| "INVALID_CHECKPOINT".into())
}
pub fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(value).map_err(|_| "SERIALIZE_STATE")?;
    fs::write(path, data).map_err(|_| "WRITE_STATE".into())
}
pub fn save(dir: &Path, s: &State) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|_| "CREATE_OUTPUT")?;
    // A single authoritative checkpoint avoids mixed generations of eight export files.
    // Rename replaces atomically on supported local filesystems, including Windows Rust std.
    write_json(&dir.join("checkpoint.tmp"), s)?;
    fs::rename(dir.join("checkpoint.tmp"), dir.join("checkpoint.json"))
        .map_err(|_| "COMMIT_CHECKPOINT")?;
    let mut decisions = serde_json::to_value(&s.decisions).unwrap();
    decisions["schema_version"] = json!(3);
    for (name, v) in [
        ("authors.json", s.authors.clone()),
        ("inventory_index.json", s.inventory.clone()),
        (
            "catalog.json",
            json!({"schema_version":3,"records":s.catalog.values().collect::<Vec<_>>()}),
        ),
        (
            "pending.json",
            json!({"schema_version":3,"tasks":s.pending.values().collect::<Vec<_>>()}),
        ),
        (
            "review.json",
            json!({"schema_version":3,"match_review":s.review.values().collect::<Vec<_>>(),"cleanup_review":s.cleanup_review}),
        ),
        ("decisions.json", decisions),
        (
            "scan_state.json",
            json!({"schema_version":3,"phase3a_scan":s.scan}),
        ),
        (
            "latest.json",
            json!({"schema_version":3,"scan_id":s.scan.scan_id,"scan_status":if s.scan.complete{"COMPLETE"}else{"PARTIAL"},"events":s.scan.events}),
        ),
    ] {
        write_json(&dir.join(name), &v)?;
    }
    export_reviews(dir, s)?;
    write_json(
        &dir.join("sanitized-state-sample.json"),
        &json!({
            "schema_version": 1,
            "sanitized": true,
            "catalog": {"records":s.catalog.values().take(2).collect::<Vec<_>>()},
            "pending": {"tasks":s.pending.values().take(2).collect::<Vec<_>>()},
            "review": {"match_review":s.review.values().filter(|r|r.status=="REVIEW_REQUIRED").take(2).collect::<Vec<_>>()},
            "latest": {"events":s.scan.events.iter().take(2).collect::<Vec<_>>()},
            "scan_state": {
                "scan_id":s.scan.scan_id,
                "requested_mode":s.scan.requested_mode,
                "threshold":s.scan.threshold,
                "complete":s.scan.complete,
                "progress":s.scan.progress,
                "direct_failures":s.scan.direct_failures
            }
        }),
    )?;
    Ok(())
}

fn csv_cell(value: &str) -> String {
    // Spreadsheet applications may evaluate cells beginning with one of these
    // characters as formulas. Prefixing a single quote keeps the displayed
    // value while making CSV exports inert when opened in a spreadsheet.
    let formula_leading = value
        .chars()
        .skip_while(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'))
        .next()
        .is_some_and(|ch| matches!(ch, '=' | '+' | '-' | '@'));
    let safe = if formula_leading {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    format!("\"{}\"", safe.replace('"', "\"\""))
}

pub fn review_export(s: &State) -> Value {
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    let items: Vec<Value> = s
        .review
        .values()
        .filter(|r| r.status == "REVIEW_REQUIRED")
        .filter_map(|review| {
            let entry = s.catalog.get(&review.source_key)?;
            *reasons.entry(review.reason.clone()).or_default() += 1;
            Some(json!({
                "review_id": review.review_id,
                "status": review.status,
                "reason": review.reason,
                "source": entry.record.source,
                "source_work_id": entry.record.source_work_id,
                "source_key": review.source_key,
                "raw_title": entry.record.raw_title,
                "raw_authors": entry.record.author,
                "author_evidence": entry.author_evidence,
                "search_queries": entry.search_queries,
                "candidate_work_ids": review.candidates,
                "version_evidence": version(&entry.record),
                "metadata": entry.record.metadata,
                "search_fingerprint": entry.search_fingerprint,
                "detail_fingerprint": entry.detail_fingerprint,
                "first_seen": entry.record.first_seen,
                "last_seen": entry.record.last_seen,
                "last_checked": entry.record.last_checked,
                "processing_result": entry.record.processing_result
                ,"matcher_version": review.matcher_version
                ,"identity_evidence": review.identity_evidence
                ,"provenance": review.provenance
            }))
        })
        .collect();
    json!({
        "schema_version": 2,
        "sanitized": true,
        "count": items.len(),
        "reason_counts": reasons,
        "items": items
    })
}

pub fn export_reviews(dir: &Path, s: &State) -> Result<(), String> {
    let export = review_export(s);
    write_json(&dir.join("review-export.json"), &export)?;
    write_json(
        &dir.join("review-reason-summary.json"),
        &json!({"schema_version":1,"count":export["count"],"reason_counts":export["reason_counts"]}),
    )?;
    let mut rows = vec!["review_id,status,reason,source,source_work_id,source_key,raw_title,raw_authors,author_evidence,search_queries,candidate_work_ids,version_evidence,metadata,search_fingerprint,detail_fingerprint,first_seen,last_seen,last_checked,processing_result,matcher_version,identity_evidence,provenance".to_owned()];
    for item in export["items"].as_array().into_iter().flatten() {
        let fields = [
            item["review_id"].as_str().unwrap_or_default().to_owned(),
            item["status"].as_str().unwrap_or_default().to_owned(),
            item["reason"].as_str().unwrap_or_default().to_owned(),
            item["source"].as_str().unwrap_or_default().to_owned(),
            item["source_work_id"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            item["source_key"].as_str().unwrap_or_default().to_owned(),
            item["raw_title"].as_str().unwrap_or_default().to_owned(),
            serde_json::to_string(&item["raw_authors"]).unwrap(),
            serde_json::to_string(&item["author_evidence"]).unwrap(),
            serde_json::to_string(&item["search_queries"]).unwrap(),
            serde_json::to_string(&item["candidate_work_ids"]).unwrap(),
            serde_json::to_string(&item["version_evidence"]).unwrap(),
            serde_json::to_string(&item["metadata"]).unwrap(),
            item["search_fingerprint"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            item["detail_fingerprint"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            item["first_seen"].as_str().unwrap_or_default().to_owned(),
            item["last_seen"].as_str().unwrap_or_default().to_owned(),
            item["last_checked"].as_str().unwrap_or_default().to_owned(),
            item["processing_result"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            item["matcher_version"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            serde_json::to_string(&item["identity_evidence"]).unwrap(),
            serde_json::to_string(&item["provenance"]).unwrap(),
        ];
        rows.push(
            fields
                .iter()
                .map(|v| csv_cell(v))
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    fs::write(
        dir.join("review-export.csv"),
        format!("{}\n", rows.join("\n")),
    )
    .map_err(|_| String::from("WRITE_REVIEW_CSV"))?;
    let mut reason_csv = vec!["reason,count".to_owned()];
    if let Some(counts) = export["reason_counts"].as_object() {
        for (reason, count) in counts {
            reason_csv.push(format!(
                "{},{}",
                csv_cell(reason),
                count.as_u64().unwrap_or(0)
            ));
        }
    }
    fs::write(
        dir.join("review-reason-summary.csv"),
        format!("{}\n", reason_csv.join("\n")),
    )
    .map_err(|_| String::from("WRITE_REASON_CSV"))
}
