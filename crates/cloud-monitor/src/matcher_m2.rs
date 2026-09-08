//! Controlled offline identity path. Uses the production state and downstream
//! version rules; never instantiates an adapter or defaults unknown type to manga.
use crate::monitor::*;
use rules_core::{
    conservative_title as norm,
    title_m2::{self, Identity, Relation},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use state_model::Record;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScopeCertificate {
    pub author: String,
    pub inventory_hash: String,
    pub reference: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub source_key: String,
    pub disposition: String,
    pub reason: String,
    pub work_id: Option<String>,
    pub author_evidence: AuthorEvidence,
    pub source_identity: Identity,
    pub content_type_evidence: Value,
    pub candidate_evidence: Vec<Value>,
    pub matching_work_ids: Vec<String>,
    pub scope_work_ids: Vec<String>,
    pub uniqueness: String,
    pub downstream_result: String,
    #[serde(default)]
    pub binding_authority_hash: Option<String>,
}
fn type_evidence(r: &Record) -> (Option<String>, Value, bool) {
    let mut kinds = BTreeSet::new();
    let mut traces = vec![];
    let mut invalid = false;
    for field in ["content_type", "categories", "tags"] {
        if field == "content_type"
            && !r.metadata[field].is_null()
            && (!r.metadata[field].is_string()
                || r.metadata[field]
                    .as_str()
                    .is_some_and(|s| title_m2::type_token(&norm(s)).is_none()))
        {
            invalid = true;
            traces.push(json!({"rule":"UNRECOGNIZED_EXPLICIT_TYPE_FIELD","field":"record.metadata.content_type","raw":r.metadata[field]}));
        }
        let values: Vec<&Value> = if let Some(v) = r.metadata[field].as_array() {
            v.iter().collect()
        } else {
            vec![&r.metadata[field]]
        };
        for v in values {
            if let Some(s) = v.as_str() {
                if let Some(t) = title_m2::type_token(&norm(s)) {
                    kinds.insert(t.to_string());
                    traces.push(json!({"rule":"EXPLICIT_WHOLE_TYPE_TOKEN","field":format!("record.metadata.{field}"),"raw":s,"type":t}));
                }
            }
        }
    }
    let conflict = kinds.len() > 1 || invalid;
    (
        if kinds.len() == 1 {
            kinds.iter().next().cloned()
        } else {
            None
        },
        json!({"traces":traces,"conflict":conflict,"default_manga":false}),
        conflict,
    )
}
fn local_type(w: &Value) -> Option<String> {
    let vs = w["versions"].as_array()?;
    if vs.is_empty() {
        return None;
    }
    let mut types = BTreeSet::new();
    for v in vs {
        types.insert(title_m2::type_token(&norm(v["content"]["type"].as_str()?))?.to_owned());
    }
    if types.len() == 1 {
        types.into_iter().next()
    } else {
        None
    }
}
fn numbering_disjoint(a: &Identity, b: &Identity) -> bool {
    // A new work needs explicit disjoint single installments in the same field,
    // same complete family, no collection/extra/subtitle/fandom/type ambiguity.
    if title_m2::compare(a, b) != Relation::StructuralConflict || a.core != b.core {
        return false;
    }
    let mut aa = a.fields.clone();
    let mut bb = b.fields.clone();
    let pairs = [
        (&mut aa.series_number, &mut bb.series_number),
        (&mut aa.volume, &mut bb.volume),
        (&mut aa.episode, &mut bb.episode),
        (&mut aa.chapter, &mut bb.chapter),
        (&mut aa.part, &mut bb.part),
    ];
    let mut distinct = 0;
    for (x, y) in pairs {
        if x != y {
            let (Some(xv), Some(yv)) = (x.as_ref(), y.as_ref()) else {
                return false;
            };
            if xv.last.is_some() || yv.last.is_some() || xv.first == yv.first {
                return false;
            }
            // Leading zero spellings do not prove distinct numeric identities.
            if xv.first.parse::<u32>().ok() == yv.first.parse::<u32>().ok() {
                return false;
            }
            distinct += 1;
            *x = None;
            *y = None;
        }
    }
    distinct == 1 && aa == bb && aa.collection.is_none() && aa.extra.is_none()
}
pub fn decide(state: &State, r: &Record, certificates: &[ScopeCertificate]) -> Outcome {
    let k = key(r);
    let author = resolve_author_evidence(r, &state.author_names());
    let (ty, te, conflict) = type_evidence(r);
    let mut source = title_m2::parse(
        Some(&r.raw_title),
        author.canonical_author.as_deref(),
        None,
        ty.as_deref(),
    );
    if conflict {
        source.issues.insert("CONTENT_TYPE_CONFLICT".into());
    }
    let mut out = Outcome {
        source_key: k.clone(),
        disposition: "REVIEW_REQUIRED".into(),
        reason: String::new(),
        work_id: None,
        author_evidence: author.clone(),
        source_identity: source.clone(),
        content_type_evidence: te,
        candidate_evidence: vec![],
        matching_work_ids: vec![],
        scope_work_ids: vec![],
        uniqueness: "NO_IDENTITY_ASSERTION".into(),
        downstream_result: String::new(),
        binding_authority_hash: None,
    };
    let positive: BTreeSet<_> = state
        .decisions
        .positive_mappings
        .iter()
        .filter(|m| m.source_key == k)
        .map(|m| m.work_id.clone())
        .collect();
    let negative: BTreeSet<_> = state
        .decisions
        .negative_mappings
        .iter()
        .filter(|m| m.source_key == k)
        .map(|m| m.work_id.clone())
        .collect();
    let mappings: BTreeSet<_> = state.inventory["works"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|w| {
            w["source_mappings"][&r.source]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|v| v.as_str() == Some(&r.source_work_id)))
        })
        .filter_map(|w| w["work_id"].as_str().map(str::to_owned))
        .collect();
    if state.decisions.ignored_source_records.contains(&k) {
        out.disposition = "IGNORED".into();
        out.reason = "HUMAN_IGNORE_SOURCE".into();
        return out;
    }
    if positive.len() > 1 {
        out.reason = "CONFLICTING_SAME_DECISIONS".into();
        return out;
    }
    if mappings.len() > 1 {
        out.reason = "CONFLICTING_SOURCE_MAPPINGS".into();
        return out;
    }
    if positive.union(&mappings).any(|id| negative.contains(id))
        || (!positive.is_empty() && !mappings.is_empty() && positive != mappings)
    {
        out.reason = "CONTRADICTORY_AUTHORITY".into();
        return out;
    }
    if let Some(id) = positive.iter().next().or_else(|| mappings.iter().next()) {
        out.work_id = Some(id.clone());
        out.disposition = if state.decisions.ignored_works.contains(id) {
            "IGNORED"
        } else {
            "AUTHORITATIVE_EXISTING"
        }
        .into();
        out.reason = if !positive.is_empty() {
            "HUMAN_SAME"
        } else {
            "SAME_SITE_SOURCE_ID"
        }
        .into();
        out.uniqueness = "ONE_NONCONFLICTING_AUTHORITY".into();
        return out;
    }
    let Some(a) = author.canonical_author.as_deref() else {
        out.reason = "UNCONFIRMED_AUTHOR".into();
        return out;
    };
    let works: Vec<_> = state.inventory["works"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|w| {
            w["authors_confirmed"]
                .as_array()
                .is_some_and(|names| names.iter().any(|n| n.as_str() == Some(a)))
        })
        .collect();
    let mut exact = BTreeSet::new();
    let mut witnesses = BTreeSet::new();
    let mut missing = false;
    let mut all_disjoint = !works.is_empty();
    let mut local_issues = BTreeSet::new();
    let mut collision_unknown = false;
    for w in works {
        let id = w["work_id"].as_str().unwrap_or("").to_owned();
        out.scope_work_ids.push(id.clone());
        let titles: Vec<_> = w["title_candidates"]
            .as_array()
            .into_iter()
            .flatten()
            .collect();
        if titles.is_empty() {
            missing = true;
            all_disjoint = false;
        }
        let lt = local_type(w);
        let mut work_disjoint = !titles.is_empty();
        for t in titles {
            let local = title_m2::parse(
                t["primary"].as_str(),
                Some(a),
                t["fandom_or_source"].as_str(),
                lt.as_deref(),
            );
            if t["primary"].as_str().is_none_or(|s| s.trim().is_empty()) {
                missing = true;
            }
            let relation = title_m2::compare(&source, &local);
            if !negative.contains(&id)
                && (!local.issues.is_empty()
                    || (source.core == local.core && relation == Relation::Insufficient))
            {
                collision_unknown = true;
            }
            let mut sf = source.fields.clone();
            let mut lf = local.fields.clone();
            sf.content_type = None;
            lf.content_type = None;
            let title_witness = source.issues.is_empty()
                && local.issues.is_empty()
                && source.core == local.core
                && sf == lf;
            if title_witness && !negative.contains(&id) {
                witnesses.insert(id.clone());
            }
            if source.core == local.core {
                local_issues.extend(local.issues.iter().cloned());
            }
            if relation == Relation::Exact && !negative.contains(&id) {
                exact.insert(id.clone());
            }
            work_disjoint &= numbering_disjoint(&source, &local);
            out.candidate_evidence.push(json!({"work_id":id,"local_identity":local,"relation":relation,
                "title_witness_without_type_authorization":title_witness,"excluded_by_not_same":negative.contains(&id),
                "local_content_type_evidence":w["versions"].as_array().unwrap_or(&vec![]).iter().map(|v|json!({"local_item_id":v["local_item_id"],"content":v["content"]})).collect::<Vec<_>>() }));
        }
        all_disjoint &= work_disjoint;
    }
    out.scope_work_ids.sort();
    out.scope_work_ids.dedup();
    out.matching_work_ids = witnesses.into_iter().collect();
    // Unknown-title candidates can hide a collision even beside one exact match.
    if missing {
        out.reason = "LOCAL_PRIMARY_MISSING".into();
        return out;
    }
    if !source.issues.is_empty() {
        out.reason = source.issues.iter().next().unwrap().clone();
        return out;
    }
    if exact.len() > 1 {
        out.reason = "AMBIGUOUS_EXISTING_IDENTITY".into();
        return out;
    }
    if let Some(id) = exact.iter().next() {
        if collision_unknown || !local_issues.is_empty() {
            out.reason = "LOCAL_IDENTITY_COLLISION_UNRESOLVED".into();
            return out;
        }
        out.work_id = Some(id.clone());
        out.disposition = if state.decisions.ignored_works.contains(id) {
            "IGNORED"
        } else {
            "AUTO_EXISTING"
        }
        .into();
        out.reason = "UNIQUE_EXACT_STRUCTURED_IDENTITY".into();
        out.uniqueness = "ONE_EXACT_WORK_ALL_AUTHOR_SCOPE_CANDIDATES_AUDITED".into();
        return out;
    }
    if source.fields.content_type.is_none() {
        out.reason = if out.matching_work_ids.is_empty() {
            "UNKNOWN_SOURCE_CONTENT_TYPE_NO_IDENTITY_PROOF"
        } else {
            "TITLE_WITNESS_UNKNOWN_SOURCE_CONTENT_TYPE"
        }
        .into();
        return out;
    }
    let certificate = certificates.iter().find(|c| {
        c.author == a && c.inventory_hash == hash(&state.inventory) && !c.reference.is_empty()
    });
    let unindexed_catalog = state.catalog.iter().any(|(other, e)| {
        other != &k
            && e.author_evidence.canonical_author.as_deref() == Some(a)
            && e.work_id
                .as_ref()
                .is_some_and(|id| !out.scope_work_ids.contains(id))
    });
    if certificate.is_some() && all_disjoint && !unindexed_catalog {
        let id = format!("WORK_SRC_{}", &hash(&k)[..20]);
        if negative.contains(&id) {
            out.reason = "REJECTED_PREVIOUS_IDENTITY".into();
            return out;
        }
        out.work_id = Some(id);
        out.disposition = "PROVEN_NEW".into();
        out.reason = "COMPLETE_SCOPE_DISJOINT_EXPLICIT_INSTALLMENT".into();
        out.uniqueness = "PINNED_COMPLETE_SCOPE_ALL_WORKS_EXPLICITLY_DISJOINT".into();
        out.content_type_evidence["scope_certificates"] = json!(certificates);
        out.binding_authority_hash = certificate.map(|c| c.reference.clone());
        return out;
    }
    out.reason = if !local_issues.is_empty() {
        "LOCAL_IDENTITY_UNPARSED"
    } else if !out.matching_work_ids.is_empty() {
        "LOCAL_CONTENT_TYPE_UNKNOWN_OR_CONFLICT"
    } else if all_disjoint && certificate.is_none() {
        "DISJOINT_INSTALLMENT_INCOMPLETE_INVENTORY_SCOPE"
    } else {
        "NO_DETERMINISTIC_IDENTITY_OR_NEW_WORK_PROOF"
    }
    .into();
    out
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReplayAudit {
    pub context: String,
    pub outcomes: BTreeMap<String, Outcome>,
}
pub fn replay(
    state: &mut State,
    records: &[Record],
    audit: &mut ReplayAudit,
    certificates: &[ScopeCertificate],
) -> usize {
    let context = hash(&(
        title_m2::RULE_VERSION,
        &state.inventory,
        &state.authors,
        &state.decisions,
        certificates,
    ));
    let mut changed = 0;
    for r in records {
        let k = key(r);
        let fingerprint = hash(&(
            &r.source,
            &r.source_work_id,
            &r.author,
            &r.raw_title,
            &r.metadata,
        ));
        if audit.context == context
            && state
                .catalog
                .get(&k)
                .is_some_and(|e| e.detail_fingerprint == fingerprint)
            && audit.outcomes.contains_key(&k)
        {
            continue;
        }
        let result = decide(state, r, certificates);
        let old = state.catalog.get(&k);
        let trigger = if old.is_none() {
            AnalysisTrigger::NewRecord
        } else if old.is_some_and(|entry| entry.detail_fingerprint != fingerprint) {
            AnalysisTrigger::SourceEvidenceChanged
        } else {
            AnalysisTrigger::ContextMigration
        };
        let mut entry = old.cloned().unwrap_or_else(|| Entry {
            record: r.clone(),
            author_evidence: result.author_evidence.clone(),
            search_fingerprint: r.fingerprint.clone(),
            detail_fingerprint: fingerprint.clone(),
            analysis_context: context.clone(),
            work_id: None,
            analysis_count: 0,
            unavailable_streak: 0,
            active: true,
            last_unavailable_check: None,
            search_queries: BTreeSet::new(),
            matcher_version: String::new(),
            identity_evidence: Value::Null,
            identity_provenance: Value::Null,
        });
        entry.record = r.clone();
        entry.author_evidence = result.author_evidence.clone();
        entry.detail_fingerprint = fingerprint;
        entry.analysis_context = context.clone();
        entry.analysis_count += 1;
        // Previous automatic assignments are not treated as human/source authority.
        entry.work_id = None;
        state.catalog.insert(k.clone(), entry);
        let result = state.apply_identity_outcome(&k, r, result, trigger);
        audit.outcomes.insert(k, result);
        changed += 1;
    }
    audit.context = context;
    changed
}

/// Isolated repair of one official inventory row, with a pinned before/after
/// contract. Caller operates on a staged copy; the five-author seed is untouched.
pub fn repair_primary(inventory: &mut Value, repair: &Value) -> Result<Value, String> {
    let before_inventory_hash = hash(inventory);
    let expected_before_hash = repair["analysis_inventory_hash_before"]
        .as_str()
        .ok_or("REPAIR_INVENTORY_HASH_BEFORE")?;
    let expected_after_hash = repair["analysis_inventory_hash_after"]
        .as_str()
        .ok_or("REPAIR_INVENTORY_HASH_AFTER")?;
    let item = &repair["source_item"];
    let id = item["work_id"].as_str().ok_or("REPAIR_WORK_ID")?;
    if item["title"]["primary"] != Value::Null
        || item["title"]["parse_evidence"] != "NO_SAFE_CREATOR_ANCHOR"
    {
        return Err("REPAIR_SOURCE_CONTRACT".into());
    }
    let raw = item["filename_raw"].as_str().ok_or("REPAIR_FILENAME")?;
    let name = raw.strip_suffix(".zip").ok_or("REPAIR_EXTENSION")?;
    let authors = item["authors_confirmed"]
        .as_array()
        .ok_or("REPAIR_AUTHOR")?;
    if authors.len() != 1 {
        return Err("REPAIR_AUTHOR".into());
    }
    if item["content"]["type"] != "manga" {
        return Err("REPAIR_CONTENT_TYPE".into());
    }
    let p = title_m2::parse(
        Some(name),
        authors[0].as_str(),
        None,
        item["content"]["type"].as_str(),
    );
    if !p.issues.is_empty()
        || p.core != repair["expected_primary"]
        || p.fields.fandom.as_deref() != repair["expected_fandom"].as_str()
    {
        return Err("REPAIR_PARSE_MISMATCH".into());
    }
    let works = inventory["works"]
        .as_array_mut()
        .ok_or("REPAIR_INVENTORY")?;
    let w = works
        .iter_mut()
        .find(|w| w["work_id"] == id)
        .ok_or("REPAIR_MISSING_WORK")?;
    if w["authors_confirmed"] != item["authors_confirmed"]
        || w["local_item_ids"] != json!([item["local_item_id"]])
    {
        return Err("REPAIR_WRONG_LOCAL_PROVENANCE".into());
    }
    let before = w["title_candidates"].clone();
    let after =
        json!([{"primary":p.core,"normalized_key":Value::Null,"fandom_or_source":p.fields.fandom}]);
    if before != repair["expected_before"] && before != after {
        return Err("REPAIR_OVERWRITE_REFUSED".into());
    }
    if before_inventory_hash != expected_before_hash && before_inventory_hash != expected_after_hash
    {
        return Err("REPAIR_INVENTORY_HASH_MISMATCH".into());
    }
    w["title_candidates"] = after.clone();
    let after_inventory_hash = hash(inventory);
    if after_inventory_hash != expected_after_hash {
        return Err("REPAIR_INVENTORY_AFTER_HASH_MISMATCH".into());
    }
    Ok(
        json!({"work_id":id,"local_item_id":item["local_item_id"],"before":repair["expected_before"],"after":after,
        "rule":"OFFICIAL_AUTHOR_ANCHORED_BASENAME_ZIP_SUFFIX_AND_CLOSED_FANDOM","parse":p,"provenance":repair["provenance"],
        "analysis_inventory_hash_before":expected_before_hash,"analysis_inventory_hash_after":after_inventory_hash}),
    )
}
