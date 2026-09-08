//! Deterministic staging logic for assistant-managed author registry changes.
//!
//! This module does not perform filesystem, network, Git, or source-site I/O.

use rules_core::conservative_title;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub fn canonical_author_key(value: &str) -> String {
    let normalized = conservative_title(value.trim());
    // Keep this closed equivalence aligned with the imported production author
    // evidence rule. It is intentionally not a general CJK conversion.
    match normalized.as_str() {
        "10駅" | "10驛" => "10驛".into(),
        _ => normalized,
    }
}

fn user_author_id(canonical_key: &str) -> String {
    let hash = format!("{:x}", Sha256::digest(canonical_key.as_bytes()));
    format!("AUTHOR_USER_{}", hash[..16].to_ascii_uppercase())
}

fn validate_registry(document: &Value) -> Result<BTreeMap<String, usize>, String> {
    let authors = document["authors"]
        .as_array()
        .ok_or("INVALID_ASSISTANT_AUTHORS_ARRAY")?;
    let mut canonical = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for (index, author) in authors.iter().enumerate() {
        let id = author["author_id"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .ok_or("INVALID_ASSISTANT_AUTHOR_ID")?;
        let name = author["name"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .ok_or("INVALID_ASSISTANT_AUTHOR_NAME")?;
        if !author["enabled"].is_boolean() {
            return Err("INVALID_ASSISTANT_AUTHOR_ENABLED".into());
        }
        if !ids.insert(id.to_owned()) {
            return Err("DUPLICATE_ASSISTANT_AUTHOR_ID".into());
        }
        let key = canonical_author_key(name);
        if key.is_empty() {
            return Err("INVALID_ASSISTANT_AUTHOR_CANONICAL_KEY".into());
        }
        if canonical.insert(key, index).is_some() {
            return Err("AMBIGUOUS_ASSISTANT_AUTHOR_CANONICAL_KEY".into());
        }
    }
    Ok(canonical)
}

pub fn plan(document: &Value, operation: &str, requested_name: &str) -> Result<(Value, Value), String> {
    if !matches!(operation, "add" | "enable" | "disable") {
        return Err("INVALID_ASSISTANT_AUTHOR_OPERATION".into());
    }
    let requested_name = requested_name.trim();
    if requested_name.is_empty() {
        return Err("INVALID_ASSISTANT_AUTHOR_NAME".into());
    }
    let requested_key = canonical_author_key(requested_name);
    if requested_key.is_empty() {
        return Err("INVALID_ASSISTANT_AUTHOR_CANONICAL_KEY".into());
    }

    let index = validate_registry(document)?;
    let mut proposed = document.clone();
    let authors = proposed["authors"]
        .as_array_mut()
        .ok_or("INVALID_ASSISTANT_AUTHORS_ARRAY")?;

    if let Some(existing_index) = index.get(&requested_key).copied() {
        let author = &mut authors[existing_index];
        let id = author["author_id"].as_str().unwrap().to_owned();
        let stored_name = author["name"].as_str().unwrap().to_owned();
        let before = author["enabled"].as_bool().unwrap();
        let after = match operation {
            "add" | "enable" => true,
            "disable" => false,
            _ => unreachable!(),
        };
        author["enabled"] = json!(after);
        let outcome = match (operation, before, after) {
            ("add", false, true) => "REENABLED_EXISTING",
            ("add", true, true) => "NOOP_ALREADY_ENABLED",
            ("enable", false, true) => "ENABLED_EXISTING",
            ("enable", true, true) => "NOOP_ALREADY_ENABLED",
            ("disable", true, false) => "DISABLED_EXISTING",
            ("disable", false, false) => "NOOP_ALREADY_DISABLED",
            _ => "UPDATED_EXISTING",
        };
        let audit = json!({
            "schema_version": 1,
            "operation": operation,
            "requested_name": requested_name,
            "canonical_key": requested_key,
            "outcome": outcome,
            "author_id": id,
            "stored_name": stored_name,
            "enabled_before": before,
            "enabled_after": after,
        });
        return Ok((proposed, audit));
    }

    if operation != "add" {
        return Err("ASSISTANT_AUTHOR_NOT_FOUND".into());
    }

    let id = user_author_id(&requested_key);
    if authors.iter().any(|author| author["author_id"] == id) {
        return Err("ASSISTANT_AUTHOR_ID_COLLISION".into());
    }
    authors.push(json!({
        "author_id": id,
        "name": requested_name,
        "enabled": true,
    }));
    let audit = json!({
        "schema_version": 1,
        "operation": operation,
        "requested_name": requested_name,
        "canonical_key": requested_key,
        "outcome": "ADDED_NEW",
        "author_id": id,
        "stored_name": requested_name,
        "enabled_before": Value::Null,
        "enabled_after": true,
    });
    Ok((proposed, audit))
}
