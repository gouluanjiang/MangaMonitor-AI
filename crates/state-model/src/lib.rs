use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocalItemId(pub String);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkId(pub String);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceKey {
    pub source: String,
    pub source_work_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Version {
    pub chinese: Option<bool>,
    pub uncensored: Option<bool>,
    pub color: Option<bool>,
    #[serde(default)]
    pub translation: String,
    #[serde(default)]
    pub sample: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Coverage {
    #[serde(default)]
    pub episodes: Vec<i64>,
    #[serde(default)]
    pub ranges: Vec<[i64; 2]>,
    #[serde(default)]
    pub parts: Vec<String>,
    #[serde(default)]
    pub extras: Vec<String>,
    #[serde(default)]
    pub collection_membership_unknown: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub source: String,
    pub source_work_id: String,
    pub author: Vec<String>,
    pub raw_title: String,
    // Whitelisted metadata only: never raw responses, image URLs, tokens or user profiles.
    pub metadata: Value,
    pub fingerprint: String,
    pub first_seen: String,
    pub last_seen: String,
    pub last_checked: String,
    pub processing_result: String,
}

impl Record {
    pub fn new(
        source: &str,
        id: String,
        author: Vec<String>,
        title: String,
        metadata: Value,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        let data = json!({"author": author, "title": title, "metadata": metadata});
        let fingerprint = format!("{:x}", Sha256::digest(data.to_string().as_bytes()));
        Self {
            source: source.into(),
            source_work_id: id,
            author,
            raw_title: title,
            metadata,
            fingerprint,
            first_seen: now.clone(),
            last_seen: now.clone(),
            last_checked: now,
            processing_result: "METADATA_OBSERVED_NOT_MATCHED".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestTrace {
    pub operation: String,
    pub page: Option<u64>,
    pub delay_ms: u64,
    pub elapsed_ms: u64,
    pub http_status: Option<u16>,
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchPage {
    pub page: u64,
    pub reported_total: Option<u64>,
    pub reported_pages: Option<u64>,
    pub reported_limit: Option<u64>,
    pub response_fields: Vec<String>,
    pub record_fields: Vec<String>,
    pub records: Vec<Record>,
    pub redirect_to_detail: bool,
}

pub fn fields(value: &Value) -> Vec<String> {
    value
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}
pub fn number(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.parse().ok())
}
pub fn string(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::to_owned)
        .or_else(|| v.as_u64().map(|n| n.to_string()))
}
pub fn authors(v: &Value) -> Vec<String> {
    match v {
        Value::String(s) if !s.trim().is_empty() => vec![s.clone()],
        Value::Array(a) => a
            .iter()
            .filter_map(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
            .collect(),
        _ => vec![],
    }
}
pub fn whitelist(v: &Value, keys: &[&str]) -> Value {
    let mut map = serde_json::Map::new();
    for key in keys {
        if let Some(value) = v.get(*key) {
            map.insert((*key).into(), value.clone());
        }
    }
    Value::Object(map)
}
