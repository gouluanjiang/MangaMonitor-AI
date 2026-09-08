//! Deterministic state transitions. No network or filesystem effects.
use chrono::{DateTime, Months, Utc};
use rules_core::{conservative_title, select, upgrade, Candidate, Upgrade};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use state_model::{Record, SearchPage, Version};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnalysisTrigger {
    NewRecord,
    SourceEvidenceChanged,
    ContextMigration,
}

pub fn hash(v: &impl Serialize) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(v).expect("serializable state"))
    )
}
pub fn key(r: &Record) -> String {
    format!("{}:{}", r.source, r.source_work_id)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub record: Record,
    #[serde(default)]
    pub author_evidence: AuthorEvidence,
    pub search_fingerprint: String,
    pub detail_fingerprint: String,
    pub analysis_context: String,
    pub work_id: Option<String>,
    pub analysis_count: u64,
    pub unavailable_streak: u32,
    pub active: bool,
    pub last_unavailable_check: Option<String>,
    #[serde(default)]
    pub search_queries: BTreeSet<String>,
    #[serde(default)]
    pub matcher_version: String,
    #[serde(default)]
    pub identity_evidence: Value,
    #[serde(default)]
    pub identity_provenance: Value,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorEvidence {
    pub canonical_author: Option<String>,
    pub rule: String,
    #[serde(default)]
    pub matched_tokens: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Target {
    pub source_key: String,
    pub author: String,
    pub title: String,
    pub version: Version,
    pub coverage: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub task_id: String,
    pub work_id: String,
    pub first_seen: String,
    pub task_revision: u64,
    pub target: Target,
    pub action: String,
    pub status: String,
    pub old_local_item_ids: Vec<String>,
    #[serde(default)]
    pub binding_authority_hash: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Decisions {
    #[serde(default)]
    pub positive_mappings: Vec<Mapping>,
    #[serde(default)]
    pub negative_mappings: Vec<Mapping>,
    #[serde(default)]
    pub ignored_source_records: Vec<String>,
    #[serde(default)]
    pub ignored_works: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mapping {
    pub source_key: String,
    pub work_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Review {
    pub review_id: String,
    pub source_key: String,
    pub reason: String,
    pub author: Vec<String>,
    pub title: String,
    pub candidates: Vec<String>,
    pub status: String,
    #[serde(default)]
    pub matcher_version: String,
    #[serde(default)]
    pub identity_evidence: Value,
    #[serde(default)]
    pub provenance: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaginationContract {
    pub reported_total: Option<u64>,
    pub reported_pages: Option<u64>,
    pub reported_limit: Option<u64>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Cursor {
    pub next_page: u64,
    pub historical_streak: usize,
    pub observed_ids: BTreeSet<String>,
    pub boundary: String,
    pub mode: String,
    #[serde(default)]
    pub pagination_contract: Option<PaginationContract>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReviewMigrationAudit {
    pub matcher_version: String,
    pub total: usize,
    pub reclassified: usize,
    pub classification_unchanged: usize,
    pub suppressed_new_review_notifications: usize,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Scan {
    pub scan_id: String,
    pub started_at: String,
    pub selected_authors: Vec<String>,
    pub requested_mode: String,
    pub threshold: usize,
    pub historical_ids: BTreeSet<String>,
    pub progress: BTreeMap<String, Cursor>,
    pub last_full: BTreeMap<String, String>,
    pub event_keys: BTreeSet<String>,
    pub events: Vec<Value>,
    pub complete: bool,
    #[serde(default)]
    pub direct_failures: BTreeMap<String, String>,
    #[serde(default)]
    pub inventory_repairs: Vec<Value>,
    #[serde(default)]
    pub review_migration: ReviewMigrationAudit,
    #[serde(default)]
    pub identity_authority_hash: String,
    #[serde(skip)]
    pub scope_certificates: Vec<crate::matcher_m2::ScopeCertificate>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    pub authors: Value,
    pub inventory: Value,
    pub catalog: BTreeMap<String, Entry>,
    pub pending: BTreeMap<String, Task>,
    pub review: BTreeMap<String, Review>,
    pub cleanup_review: Value,
    pub decisions: Decisions,
    pub scan: Scan,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", content = "record")]
pub enum Detail {
    Available(Box<Record>),
    ExplicitUnavailable,
    SourceError(String),
}

impl State {
    pub fn context(&self) -> String {
        // Schema metadata describes the representation, not the authority
        // content. Excluding it keeps staged assistant replays stable when a
        // legacy in-memory value is normalized on persistence.
        let semantic_document = |value: &Value| {
            let mut value = value.clone();
            if let Some(object) = value.as_object_mut() {
                object.remove("schema_version");
            }
            value
        };
        // Author registry edits can change deterministic author evidence and must
        // therefore trigger the same offline reanalysis as decisions/inventory.
        // The frozen matcher version is an input too: changing the rule set must
        // cause exactly one deterministic production reanalysis.
        let base = (
            rules_core::title_m2::RULE_VERSION,
            &self.decisions,
            semantic_document(&self.inventory),
            semantic_document(&self.authors),
        );
        if self.scan.identity_authority_hash.is_empty() {
            hash(&base)
        } else {
            hash(&(base, &self.scan.identity_authority_hash))
        }
    }
    pub fn event(&mut self, id: String, kind: &str, data: Value) {
        if self.scan.event_keys.insert(id.clone()) {
            self.scan
                .events
                .push(json!({"event_id":id,"kind":kind,"data":data}));
        }
    }
    pub fn begin(
        &mut self,
        id: &str,
        now: &str,
        authors: Vec<String>,
        mode: &str,
        threshold: usize,
    ) -> Result<(), String> {
        self.begin_with_event_history(id, now, authors, mode, threshold, false)
    }

    pub fn begin_with_event_history(
        &mut self,
        id: &str,
        now: &str,
        authors: Vec<String>,
        mode: &str,
        threshold: usize,
        preserve_latest_events: bool,
    ) -> Result<(), String> {
        if !["full", "incremental"].contains(&mode) || threshold == 0 {
            return Err("INVALID_SCAN_OPTIONS".into());
        }
        self.scan.scan_id = id.into();
        self.scan.started_at = now.into();
        self.scan.selected_authors = authors;
        self.scan.requested_mode = mode.into();
        self.scan.threshold = threshold;
        self.scan.historical_ids = self.catalog.keys().cloned().collect();
        self.scan.progress.clear();
        self.scan.review_migration = ReviewMigrationAudit::default();
        if !preserve_latest_events {
            self.scan.events.clear();
        }
        self.scan.complete = false;
        self.scan.direct_failures.clear();
        // Human decisions and inventory edits take effect even if the source is offline.
        let context = self.context();
        let changed: Vec<_> = self
            .catalog
            .iter()
            .filter(|(_, e)| e.analysis_context != context)
            .map(|(k, _)| k.clone())
            .collect();
        for k in changed {
            let e = self.catalog.get_mut(&k).unwrap();
            e.analysis_context = context.clone();
            e.analysis_count += 1;
            self.analyze(&k, AnalysisTrigger::ContextMigration);
        }
        Ok(())
    }
    pub fn cursor_key(source: &str, author: &str) -> String {
        format!("{source}|{author}")
    }
    pub fn effective_mode(&self, source: &str, author: &str) -> String {
        let last = self.scan.last_full.get(&Self::cursor_key(source, author));
        let due = last
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .and_then(|d| d.checked_add_months(Months::new(6)))
            .zip(DateTime::parse_from_rfc3339(&self.scan.started_at).ok())
            .map(|(due, now)| now >= due)
            .unwrap_or(true);
        if self.scan.requested_mode == "full" || due {
            "full"
        } else {
            "incremental"
        }
        .into()
    }
    pub fn needs_detail(&self, r: &Record) -> bool {
        self.catalog
            .get(&key(r))
            .map(|e| e.search_fingerprint != r.fingerprint || e.analysis_context != self.context())
            .unwrap_or(true)
    }
    pub fn accept(&mut self, search: &Record, detail: &Record) -> Result<(), String> {
        if key(search) != key(detail) {
            return Err("DETAIL_ID_MISMATCH".into());
        }
        let k = key(search);
        let ctx = self.context();
        let old = self.catalog.get(&k).cloned();
        let changed = old
            .as_ref()
            .map(|e| {
                e.search_fingerprint != search.fingerprint
                    || e.detail_fingerprint != detail.fingerprint
                    || e.analysis_context != ctx
            })
            .unwrap_or(true);
        let mut r = detail.clone();
        if let Some(previous) = &old {
            r.processing_result = previous.record.processing_result.clone();
        }
        r.first_seen = old
            .as_ref()
            .map(|e| e.record.first_seen.clone())
            .unwrap_or(self.scan.started_at.clone());
        r.last_seen = self.scan.started_at.clone();
        r.last_checked = self.scan.started_at.clone();
        let entry = Entry {
            record: r,
            author_evidence: resolve_author_evidence(detail, &self.author_names()),
            search_fingerprint: search.fingerprint.clone(),
            detail_fingerprint: detail.fingerprint.clone(),
            analysis_context: ctx,
            work_id: old.as_ref().and_then(|e| e.work_id.clone()),
            analysis_count: old.as_ref().map(|e| e.analysis_count).unwrap_or(0)
                + u64::from(changed),
            unavailable_streak: 0,
            active: true,
            last_unavailable_check: None,
            search_queries: old
                .as_ref()
                .map(|e| e.search_queries.clone())
                .unwrap_or_default(),
            matcher_version: old
                .as_ref()
                .map(|e| e.matcher_version.clone())
                .unwrap_or_default(),
            identity_evidence: old
                .as_ref()
                .map(|e| e.identity_evidence.clone())
                .unwrap_or(Value::Null),
            identity_provenance: old
                .as_ref()
                .map(|e| e.identity_provenance.clone())
                .unwrap_or(Value::Null),
        };
        self.catalog.insert(k.clone(), entry);
        if old.as_ref().is_some_and(|e| !e.active) {
            for t in self
                .pending
                .values_mut()
                .filter(|t| t.target.source_key == k && t.status == "inactive")
            {
                t.status = "pending".into();
            }
            self.event(
                format!(
                    "reactivated:{k}:{}",
                    old.as_ref()
                        .unwrap()
                        .last_unavailable_check
                        .as_deref()
                        .unwrap_or("unknown")
                ),
                "SOURCE_REACTIVATED",
                json!({"source_key":k}),
            );
        }
        if changed {
            let trigger = if old.is_none() {
                AnalysisTrigger::NewRecord
            } else if old.as_ref().is_some_and(|e| {
                e.search_fingerprint != search.fingerprint
                    || e.detail_fingerprint != detail.fingerprint
            }) {
                AnalysisTrigger::SourceEvidenceChanged
            } else {
                AnalysisTrigger::ContextMigration
            };
            self.analyze(&k, trigger);
        }
        Ok(())
    }
    pub fn observe_unchanged(&mut self, r: &Record) {
        if let Some(e) = self.catalog.get_mut(&key(r)) {
            e.record.last_seen = self.scan.started_at.clone();
        }
    }
    pub fn note_search_query(&mut self, r: &Record, query: &str) {
        if let Some(entry) = self.catalog.get_mut(&key(r)) {
            entry.search_queries.insert(query.to_owned());
        }
    }
    pub(crate) fn review_item(&mut self, k: &str, reason: &str, candidates: Vec<String>) {
        let r = &self.catalog[k].record;
        let id = format!("REVIEW_{}", &hash(&(k, reason, &candidates))[..20]);
        for old in self
            .review
            .values_mut()
            .filter(|old| old.source_key == k && old.review_id != id)
        {
            old.status = "RESOLVED".into();
        }
        let notify = match self.review.entry(id.clone()) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(Review {
                    review_id: id.clone(),
                    source_key: k.into(),
                    reason: reason.into(),
                    author: r.author.clone(),
                    title: r.raw_title.clone(),
                    candidates,
                    status: "REVIEW_REQUIRED".into(),
                    matcher_version: String::new(),
                    identity_evidence: Value::Null,
                    provenance: Value::Null,
                });
                true
            }
            std::collections::btree_map::Entry::Occupied(mut slot) => {
                let changed = slot.get().status != "REVIEW_REQUIRED";
                slot.get_mut().status = "REVIEW_REQUIRED".into();
                changed
            }
        };
        self.catalog.get_mut(k).unwrap().record.processing_result = "REVIEW_REQUIRED".into();
        if notify {
            self.event(id, "NEW_REVIEW", json!({"source_key":k,"reason":reason}));
        }
    }
    fn analyze(&mut self, k: &str, trigger: AnalysisTrigger) {
        let r = self.catalog[k].record.clone();
        let canonical_author = resolve_author_evidence(&r, &self.author_names()).canonical_author;
        let certificates: Vec<_> = canonical_author
            .as_deref()
            .and_then(|author| {
                self.scan
                    .scope_certificates
                    .iter()
                    .find(|certificate| certificate.author == author)
                    .cloned()
            })
            .into_iter()
            .collect();
        let outcome = crate::matcher_m2::decide(self, &r, &certificates);
        self.apply_identity_outcome(k, &r, outcome, trigger);
    }

    pub(crate) fn apply_identity_outcome(
        &mut self,
        k: &str,
        r: &Record,
        mut outcome: crate::matcher_m2::Outcome,
        trigger: AnalysisTrigger,
    ) -> crate::matcher_m2::Outcome {
        let provenance = json!({
            "matcher_version": rules_core::title_m2::RULE_VERSION,
            "analysis_context": self.catalog[k].analysis_context,
            "trigger": match trigger {
                AnalysisTrigger::NewRecord => "NEW_RECORD",
                AnalysisTrigger::SourceEvidenceChanged => "SOURCE_EVIDENCE_CHANGED",
                AnalysisTrigger::ContextMigration => "ANALYSIS_CONTEXT_MIGRATION",
            },
            "source_key": k,
            "detail_fingerprint": self.catalog[k].detail_fingerprint,
        });
        {
            let entry = self.catalog.get_mut(k).unwrap();
            entry.matcher_version = rules_core::title_m2::RULE_VERSION.into();
            entry.author_evidence = outcome.author_evidence.clone();
            entry.identity_provenance = provenance.clone();
            // Legacy automatic bindings are not an authority under M2.
            entry.work_id = None;
        }
        if outcome.disposition == "IGNORED" {
            let source_ignore_changed = outcome.reason == "HUMAN_IGNORE_SOURCE"
                && self.catalog[k].record.processing_result != "IGNORED";
            self.catalog.get_mut(k).unwrap().record.processing_result = "IGNORED".into();
            self.catalog.get_mut(k).unwrap().work_id = outcome.work_id.clone();
            for review in self
                .review
                .values_mut()
                .filter(|review| review.source_key == k)
            {
                review.status = "RESOLVED".into();
            }
            for task in self.pending.values_mut().filter(|task| {
                task.target.source_key == k || outcome.work_id.as_ref() == Some(&task.work_id)
            }) {
                task.status = "ignored".into();
            }
            if source_ignore_changed {
                self.event(
                    format!("ignored:{k}"),
                    "SOURCE_IGNORED",
                    json!({"source_key":k}),
                );
            }
        } else if let Some(work_id) = outcome.work_id.clone() {
            self.bind_work(
                k,
                r,
                work_id,
                outcome.author_evidence.canonical_author.clone(),
                outcome.binding_authority_hash.clone(),
            );
        } else {
            // A record that no longer has a trusted work binding must stop its
            // old pending task. A stable binding is handled by bind_work and
            // keeps an executable pending task across unrelated migrations.
            for task in self
                .pending
                .values_mut()
                .filter(|task| task.target.source_key == k)
            {
                task.status = "superseded_by_identity_reanalysis".into();
            }
            self.review_identity_item(k, &outcome, trigger, &provenance);
        }
        outcome.downstream_result = self.catalog[k].record.processing_result.clone();
        let evidence = serde_json::to_value(&outcome).expect("identity outcome serializes");
        self.catalog.get_mut(k).unwrap().identity_evidence = evidence.clone();
        for review in self
            .review
            .values_mut()
            .filter(|review| review.source_key == k && review.status == "REVIEW_REQUIRED")
        {
            review.matcher_version = rules_core::title_m2::RULE_VERSION.into();
            review.identity_evidence = evidence.clone();
            if review.provenance.is_null() {
                review.provenance = json!({"analysis":provenance});
            }
        }
        outcome
    }

    fn review_identity_item(
        &mut self,
        k: &str,
        outcome: &crate::matcher_m2::Outcome,
        trigger: AnalysisTrigger,
        provenance: &Value,
    ) {
        let candidates = outcome.matching_work_ids.clone();
        let id = format!("REVIEW_{}", &hash(&(k, &outcome.reason, &candidates))[..20]);
        let prior_active = self
            .review
            .values()
            .find(|review| review.source_key == k && review.status == "REVIEW_REQUIRED")
            .cloned();
        for old in self
            .review
            .values_mut()
            .filter(|old| old.source_key == k && old.review_id != id)
        {
            old.status = "RESOLVED".into();
        }
        let is_migration = trigger == AnalysisTrigger::ContextMigration && prior_active.is_some();
        let is_reclassification = prior_active
            .as_ref()
            .is_some_and(|old| old.reason != outcome.reason || old.review_id != id);
        let r = &self.catalog[k].record;
        let migration = prior_active.as_ref().map(|old| {
            json!({
                "kind": if is_reclassification {"REVIEW_RECLASSIFIED"} else {"REVIEW_MIGRATED_UNCHANGED_CLASSIFICATION"},
                "from_review_id": old.review_id,
                "from_reason": old.reason,
                "to_reason": outcome.reason,
                "notification": "SUPPRESSED_NO_NEW_ACTIONABLE_CONDITION",
            })
        });
        let notify = match self.review.entry(id.clone()) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(Review {
                    review_id: id.clone(),
                    source_key: k.into(),
                    reason: outcome.reason.clone(),
                    author: r.author.clone(),
                    title: r.raw_title.clone(),
                    candidates,
                    status: "REVIEW_REQUIRED".into(),
                    matcher_version: rules_core::title_m2::RULE_VERSION.into(),
                    identity_evidence: serde_json::to_value(outcome)
                        .expect("identity outcome serializes"),
                    provenance: json!({"analysis":provenance,"migration":migration}),
                });
                !is_migration
            }
            std::collections::btree_map::Entry::Occupied(mut slot) => {
                let changed = slot.get().status != "REVIEW_REQUIRED";
                let review = slot.get_mut();
                review.status = "REVIEW_REQUIRED".into();
                review.matcher_version = rules_core::title_m2::RULE_VERSION.into();
                review.identity_evidence =
                    serde_json::to_value(outcome).expect("identity outcome serializes");
                review.provenance = json!({"analysis":provenance,"migration":migration});
                changed && !is_migration
            }
        };
        self.catalog.get_mut(k).unwrap().record.processing_result = "REVIEW_REQUIRED".into();
        if is_migration {
            let audit = &mut self.scan.review_migration;
            audit.matcher_version = rules_core::title_m2::RULE_VERSION.into();
            audit.total += 1;
            audit.suppressed_new_review_notifications += 1;
            if is_reclassification {
                audit.reclassified += 1;
            } else {
                audit.classification_unchanged += 1;
            }
        } else if notify {
            self.event(
                id,
                "NEW_REVIEW",
                json!({"source_key":k,"reason":outcome.reason,"matcher_version":rules_core::title_m2::RULE_VERSION}),
            );
        }
    }
    /// Shared downstream version/coverage path for legacy and controlled M2 identity.
    pub(crate) fn bind_work(
        &mut self,
        k: &str,
        r: &Record,
        work: String,
        author: Option<String>,
        binding_authority_hash: Option<String>,
    ) {
        for t in self
            .pending
            .values_mut()
            .filter(|t| t.target.source_key == k && t.work_id != work)
        {
            t.status = "superseded_by_decision".into();
        }
        self.catalog.get_mut(k).unwrap().work_id = Some(work.clone());
        if self.decisions.ignored_works.contains(&work) {
            self.catalog.get_mut(k).unwrap().record.processing_result = "IGNORED".into();
            if let Some(t) = self.pending.get_mut(&work) {
                t.status = "ignored".into();
            }
            return;
        }
        for v in self.review.values_mut().filter(|v| v.source_key == k) {
            v.status = "RESOLVED".into();
        }
        let target = Target {
            source_key: k.into(),
            author: author.unwrap_or_else(|| r.author.join(" / ")),
            title: r.raw_title.clone(),
            version: version(r),
            coverage: Value::Null,
        };
        let new_authority = binding_authority_hash.unwrap_or_default();
        let owned = self.inventory["works"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|w| w["work_id"] == work && w["owned"] == true)
            .cloned();
        let mut action = "download";
        let mut old_ids = vec![];
        if let Some(w) = owned {
            let versions = w["versions"].as_array().cloned().unwrap_or_default();
            let comparisons: Vec<_> = versions
                .iter()
                .map(|v| upgrade(&local_version(v), &target.version))
                .collect();
            if comparisons.is_empty() || comparisons.contains(&Upgrade::ReviewUnknown) {
                self.review_item(k, "UNKNOWN_LOCAL_VERSION", vec![work]);
                return;
            }
            if comparisons.iter().all(|u| *u == Upgrade::Upgrade) {
                action = "upgrade";
                old_ids = w["local_item_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect();
            } else {
                self.catalog.get_mut(k).unwrap().record.processing_result = "OWNED".into();
                return;
            }
        }
        if let Some(old) = self.pending.get(&work).cloned() {
            if old.target.source_key == target.source_key && hash(&old.target) == hash(&target) {
                let authority_changed = old.binding_authority_hash != new_authority;
                if old.status == "pending" && !authority_changed {
                    return;
                }
                let t = self.pending.get_mut(&work).unwrap();
                t.task_revision += 1;
                t.binding_authority_hash = new_authority;
                t.status = "pending".into();
                let rev = t.task_revision;
                let task_id = t.task_id.clone();
                self.event(
                    format!("{task_id}:{rev}"),
                    if authority_changed {
                        "PENDING_AUTHORITY_EPOCH"
                    } else {
                        "PENDING_AUTHORITY_RESTORED"
                    },
                    json!({"task_id":task_id,"task_revision":rev}),
                );
                self.catalog.get_mut(k).unwrap().record.processing_result = "PENDING".into();
                return;
            }
            let mut old_candidate = candidate(&old.target);
            old_candidate.id = "0_old".into();
            let mut new_candidate = candidate(&target);
            new_candidate.id = "1_new".into();
            let candidates = [old_candidate, new_candidate];
            match select(&candidates) {
                Ok(id) if id == "1_new" && hash(&old.target) != hash(&target) => {
                    let t = self.pending.get_mut(&work).unwrap();
                    t.task_revision += 1;
                    t.target = target;
                    t.binding_authority_hash = new_authority.clone();
                    t.status = "pending".into();
                    let rev = t.task_revision;
                    let task_id = t.task_id.clone();
                    self.event(
                        format!("{task_id}:{rev}"),
                        "PENDING_UPGRADE",
                        json!({"task_id":task_id,"task_revision":rev}),
                    );
                }
                Err(_) => {
                    self.review_item(k, "UNKNOWN_CANDIDATE_COMPARISON", vec![work]);
                    return;
                }
                _ => {}
            }
        } else {
            let id = format!("TASK_{}", &hash(&work)[..20]);
            self.pending.insert(
                work.clone(),
                Task {
                    task_id: id.clone(),
                    work_id: work,
                    first_seen: self.scan.started_at.clone(),
                    task_revision: 1,
                    target,
                    action: action.into(),
                    status: "pending".into(),
                    old_local_item_ids: old_ids,
                    binding_authority_hash: new_authority,
                },
            );
            self.event(
                id.clone(),
                "NEW_PENDING",
                json!({"task_id":id,"task_revision":1}),
            );
        }
        self.catalog.get_mut(k).unwrap().record.processing_result = "PENDING".into();
    }
    pub fn author_names(&self) -> BTreeSet<String> {
        self.authors["authors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|a| a["enabled"] != false)
            .filter_map(|a| a["name"].as_str().map(str::to_owned))
            .collect()
    }
    pub fn source_error(&mut self, source: &str, author: &str, code: &str) {
        let ck = Self::cursor_key(source, author);
        self.scan.progress.entry(ck.clone()).or_default().boundary = "SOURCE_ERROR".into();
        self.event(
            format!("warning:{}:{ck}:{code}", self.scan.scan_id),
            "SCAN_PARTIAL",
            json!({"source":source,"author":author,"reason":"SOURCE_ERROR","code":code}),
        );
    }
    /// Page boundary is a resume cursor, never evidence for availability.
    pub fn page_boundary(&mut self, source: &str, author: &str, page: &SearchPage) -> bool {
        let ck = Self::cursor_key(source, author);
        let mode = self.effective_mode(source, author);
        let committed = self.scan.progress.get(&ck).cloned().unwrap_or_default();
        let previous_count = committed.observed_ids.len();
        let expected_page = if committed.next_page == 0 {
            1
        } else {
            committed.next_page
        };
        let page_contract = PaginationContract {
            reported_total: page.reported_total,
            reported_pages: page.reported_pages,
            reported_limit: page.reported_limit,
        };

        // Build the next cursor off to the side. A malformed page must not
        // consume its cursor, observations, historical streak, or the
        // first-page pagination contract: resume must retry the same page.
        let mut candidate = committed.clone();
        candidate.mode = mode.clone();
        candidate.next_page = page.page.saturating_add(1);
        let page_sequence_valid = page.page > 0 && page.page == expected_page;
        let contract_consistent = if expected_page == 1 {
            page_sequence_valid && committed.pagination_contract.is_none()
        } else {
            committed.pagination_contract.as_ref() == Some(&page_contract)
        };
        if page.page == 1 {
            candidate.pagination_contract = Some(page_contract.clone());
        }

        let mut page_ids = BTreeSet::new();
        let mut duplicate_page = false;
        let mut early = false;
        for r in &page.records {
            let k = key(r);
            if !page_ids.insert(k.clone()) || committed.observed_ids.contains(&k) {
                duplicate_page = true;
            }
            if candidate.observed_ids.insert(k.clone()) {
                if self.scan.historical_ids.contains(&k) {
                    candidate.historical_streak += 1;
                } else {
                    candidate.historical_streak = 0;
                }
                if mode == "incremental" && candidate.historical_streak >= self.scan.threshold {
                    early = true;
                }
            }
        }
        let observed_count = candidate.observed_ids.len() as u64;
        let limit_valid = page
            .reported_limit
            .is_none_or(|limit| limit > 0 && page.records.len() as u64 <= limit);
        let total_valid = page
            .reported_total
            .is_none_or(|total| observed_count <= total);
        let pages_valid = page
            .reported_pages
            .is_none_or(|pages| pages > 0 && page.page <= pages);
        let totals_agree = match (
            page.reported_total,
            page.reported_pages,
            page.reported_limit,
        ) {
            (Some(total), Some(pages), Some(limit)) if total > 0 && limit > 0 => {
                pages == total.div_ceil(limit)
            }
            (Some(0), Some(pages), _) => pages == 0 || pages == 1,
            _ => true,
        };
        let terminal_page = page
            .reported_pages
            .is_some_and(|pages| pages > 0 && page.page == pages);
        let terminal_count_valid = !(terminal_page
            && page
                .reported_total
                .is_some_and(|total| observed_count != total));
        let valid_empty = page.records.is_empty()
            && page.page == 1
            && page.reported_total == Some(0)
            && page
                .reported_pages
                .is_none_or(|pages| pages == 0 || pages == 1)
            && page_sequence_valid
            && contract_consistent;
        let metadata_consistent = page_sequence_valid
            && contract_consistent
            && limit_valid
            && total_valid
            && pages_valid
            && totals_agree
            && terminal_count_valid
            && !duplicate_page;
        // Only the canonical zero-result first page may be empty. Every
        // ordinary empty page fails before candidate commit so resume cannot
        // skip an unproven page.
        let page_valid = valid_empty || (!page.records.is_empty() && metadata_consistent);
        let redirect_complete = page.redirect_to_detail
            && page.records.len() == 1
            && metadata_consistent
            && page.page == 1
            && page.reported_total.is_none_or(|total| total == 1)
            && page.reported_pages.is_none_or(|pages| pages == 1);
        let exhausted = if valid_empty {
            true
        } else if metadata_consistent && !page.records.is_empty() {
            match (page.reported_total, page.reported_pages) {
                (Some(total), Some(pages)) => page.page == pages && observed_count == total,
                (None, Some(pages)) => page.page == pages,
                (Some(total), None) => observed_count == total,
                (None, None) => false,
            }
        } else {
            false
        };

        if !page_valid {
            let cursor = self.scan.progress.entry(ck.clone()).or_default();
            cursor.boundary = "INCOMPLETE_PAGINATION".into();
            self.event(
                format!("pagination:{}:{ck}", self.scan.scan_id),
                "SCAN_PARTIAL",
                json!({
                    "source": source,
                    "author": author,
                    "reason": "INCOMPLETE_PAGINATION",
                    "expected_page": expected_page,
                    "page": page.page,
                    "observed_count": previous_count,
                }),
            );
            return true;
        }

        // Commit the candidate only after every page invariant, including the
        // durable first-page pagination contract, has passed.
        self.scan.progress.insert(ck.clone(), candidate);
        if redirect_complete || exhausted {
            self.scan
                .progress
                .get_mut(&ck)
                .expect("candidate cursor was just inserted")
                .boundary = "COMPLETE".into();
            if mode == "full" {
                self.scan.last_full.insert(ck, self.scan.started_at.clone());
            }
            true
        } else if early {
            self.scan
                .progress
                .get_mut(&ck)
                .expect("candidate cursor was just inserted")
                .boundary = "EARLY_STOP_HEURISTIC".into();
            self.event(format!("early:{}:{ck}", self.scan.scan_id),"SCAN_PARTIAL",json!({"source":source,"author":author,"reason":"EARLY_STOP_HEURISTIC","boundary":"AFTER_FETCHED_PAGE"}));
            true
        } else {
            self.scan
                .progress
                .get_mut(&ck)
                .expect("candidate cursor was just inserted")
                .boundary = "CHECKPOINT".into();
            false
        }
    }
    pub fn direct(&mut self, k: &str, outcome: Detail, check_id: &str) -> Result<(), String> {
        let Some(e) = self.catalog.get(k).cloned() else {
            return Err("UNKNOWN_SOURCE_ID".into());
        };
        match outcome {
            Detail::Available(r) => {
                self.scan.direct_failures.remove(k);
                let mut search = e.record;
                search.fingerprint = e.search_fingerprint;
                self.accept(&search, &r)?;
            }
            Detail::SourceError(code) => {
                self.scan.direct_failures.insert(k.into(), code.clone());
                self.event(
                    format!("direct-error:{}:{k}:{code}", self.scan.scan_id),
                    "SCAN_WARNING",
                    json!({"source_key":k,"reason":"SOURCE_ERROR","code":code}),
                );
            }
            Detail::ExplicitUnavailable => {
                if e.record.source != "pica" {
                    self.scan
                        .direct_failures
                        .insert(k.into(), "UNAVAILABLE_NOT_CERTIFIED_FOR_SOURCE".into());
                    self.event(
                        format!("uncertified-unavailable:{k}:{check_id}"),
                        "SCAN_WARNING",
                        json!({"source_key":k,"reason":"UNAVAILABLE_NOT_CERTIFIED_FOR_SOURCE"}),
                    );
                    return Ok(());
                }
                let e = self.catalog.get_mut(k).unwrap();
                if e.last_unavailable_check.as_deref() == Some(check_id) {
                    return Ok(());
                }
                e.last_unavailable_check = Some(check_id.into());
                e.unavailable_streak += 1;
                if e.unavailable_streak >= 3 && e.active {
                    e.active = false;
                    for t in self
                        .pending
                        .values_mut()
                        .filter(|t| t.target.source_key == k)
                    {
                        t.status = "inactive".into();
                    }
                    self.event(
                        format!("inactive:{k}:{check_id}"),
                        "SOURCE_INACTIVE",
                        json!({"source_key":k}),
                    );
                }
            }
        }
        Ok(())
    }
    pub fn complete_task(&mut self, work: &str, revision: u64, local: &Value) -> bool {
        let Some(t) = self.pending.get_mut(work) else {
            return false;
        };
        if t.task_revision != revision || local["work_id"] != work {
            return false;
        }
        let mut target = serde_json::to_value(&t.target.version).unwrap();
        target["revision"] = json!(revision);
        if !t.target.coverage.is_null() {
            target["coverage"] = t.target.coverage.clone();
        }
        if !rules_core::task_done(&target, local, revision) {
            return false;
        }
        // Completion is only a state proposal in this phase; no filesystem action.
        t.status = "done".into();
        true
    }
    pub fn finish(&mut self) {
        self.scan.complete = self.scan.direct_failures.is_empty()
            && self.scan.selected_authors.iter().all(|a| {
                ["jm", "pica"].iter().all(|s| {
                    self.scan
                        .progress
                        .get(&Self::cursor_key(s, a))
                        .is_some_and(|c| c.boundary == "COMPLETE")
                })
            });
    }
}

fn canonical_author_key(value: &str) -> String {
    let normalized = conservative_title(value.trim());
    // Explicitly confirmed character-form equivalence. This is deliberately a
    // closed table; it is not a general CJK conversion or fuzzy matcher.
    match normalized.as_str() {
        "10駅" | "10驛" => "10驛".into(),
        _ => normalized,
    }
}

fn parenthesized_tokens(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut inside = false;
    for ch in value.chars() {
        match ch {
            '(' | '（' if !inside => {
                inside = true;
                current.clear();
            }
            ')' | '）' if inside => {
                tokens.extend(
                    current
                        .split([',', '，', '、', '/', '&'])
                        .map(str::trim)
                        .filter(|part| !part.is_empty())
                        .map(str::to_owned),
                );
                inside = false;
                current.clear();
            }
            _ if inside => current.push(ch),
            _ => {}
        }
    }
    tokens
}

/// Resolve source author evidence against the already-confirmed author registry.
/// Raw source fields remain untouched. Only whole normalized fields and whole
/// tokens inside parentheses are accepted; substrings and spelling guesses are
/// intentionally rejected.
pub fn resolve_author_evidence(record: &Record, official: &BTreeSet<String>) -> AuthorEvidence {
    if record.author.len() != 1 {
        return AuthorEvidence {
            canonical_author: None,
            rule: if record.author.is_empty() {
                "NO_CONFIRMED_AUTHOR_EVIDENCE"
            } else {
                "MULTIPLE_RAW_AUTHOR_FIELDS"
            }
            .into(),
            matched_tokens: Vec::new(),
        };
    }
    let registry: BTreeMap<String, Vec<String>> =
        official.iter().fold(BTreeMap::new(), |mut map, author| {
            map.entry(canonical_author_key(author))
                .or_default()
                .push(author.clone());
            map
        });
    let mut direct = BTreeMap::<String, Vec<String>>::new();
    let mut parenthesized = BTreeMap::<String, Vec<String>>::new();
    for raw in &record.author {
        let key = canonical_author_key(raw);
        if let Some(authors) = registry.get(&key) {
            for author in authors {
                direct.entry(author.clone()).or_default().push(raw.clone());
            }
        }
        for token in parenthesized_tokens(raw) {
            if let Some(authors) = registry.get(&canonical_author_key(&token)) {
                for author in authors {
                    parenthesized
                        .entry(author.clone())
                        .or_default()
                        .push(token.clone());
                }
            }
        }
    }
    let matches: BTreeSet<String> = direct.keys().chain(parenthesized.keys()).cloned().collect();
    if matches.len() != 1 {
        return AuthorEvidence {
            canonical_author: None,
            rule: if matches.is_empty() {
                "NO_CONFIRMED_AUTHOR_EVIDENCE"
            } else {
                "AMBIGUOUS_CONFIRMED_AUTHOR_EVIDENCE"
            }
            .into(),
            matched_tokens: direct
                .values()
                .chain(parenthesized.values())
                .flatten()
                .cloned()
                .collect(),
        };
    }
    let author = matches.into_iter().next().unwrap();
    let tokens = direct
        .get(&author)
        .or_else(|| parenthesized.get(&author))
        .cloned()
        .unwrap_or_default();
    let normalized_only = tokens.iter().any(|token| token.trim() != author)
        && tokens
            .iter()
            .all(|token| canonical_author_key(token) == canonical_author_key(&author));
    AuthorEvidence {
        canonical_author: Some(author.clone()),
        rule: if direct.contains_key(&author) {
            if normalized_only {
                "DIRECT_NORMALIZED"
            } else {
                "DIRECT_EXACT"
            }
        } else {
            "PARENTHESIZED_OFFICIAL_TOKEN"
        }
        .into(),
        matched_tokens: tokens,
    }
}
pub fn low_information(title: &str) -> bool {
    title.chars().count() < 4
        || title
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_whitespace())
        || [
            "part ",
            "第",
            "前篇",
            "後篇",
            "后篇",
            "番外",
            "おまけ",
            "特典",
            "総集編",
            "总集篇",
            "合集",
            "collection",
            "cg集",
            "画集",
            "小說",
            "小说",
        ]
        .iter()
        .any(|s| title.starts_with(s) || title.contains("合集") || title.contains("総集編"))
}
/// Remove only whole, recognized metadata annotations. Unknown brackets and all
/// ranges/parts/extras stay in the identity; there is no generic bracket stripping.
pub fn core_title(r: &Record) -> String {
    const FLAGS: &[&str] = &[
        "chinese",
        "中文",
        "中国翻译",
        "中國翻譯",
        "汉化",
        "漢化",
        "無碼",
        "无码",
        "有码",
        "有碼",
        "無修正",
        "无修正",
        "全彩",
        "黑白",
        "dl版",
        "digital",
        "dl",
        "扫描版",
        "掃描版",
    ];
    let raw: Vec<char> = r.raw_title.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    while i < raw.len() {
        let end = match raw[i] {
            '[' => Some(']'),
            '【' => Some('】'),
            _ => None,
        };
        if let Some(end) = end {
            if let Some(offset) = raw[i + 1..].iter().position(|c| *c == end) {
                let last = i + 1 + offset;
                let group: String = raw[i + 1..last].iter().collect();
                let normalized = conservative_title(&group);
                let author = r.author.len() == 1 && normalized == conservative_title(&r.author[0]);
                let flags = FLAGS.contains(&normalized.as_str());
                if author || flags {
                    i = last + 1;
                    continue;
                }
            }
        }
        result.push(raw[i]);
        i += 1;
    }
    conservative_title(&result)
}
pub fn content_type(r: &Record) -> &'static str {
    let mut evidence = r.raw_title.clone();
    for field in ["categories", "tags"] {
        for value in r.metadata[field]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            evidence.push(' ');
            evidence.push_str(value);
        }
    }
    let evidence = evidence.to_lowercase();
    if ["cg集", "cg collection", "cg set"]
        .iter()
        .any(|token| evidence.contains(token))
    {
        "cg_set"
    } else if ["画集", "畫集", "イラスト集", "artbook"]
        .iter()
        .any(|token| evidence.contains(token))
    {
        "artbook"
    } else if ["小说", "小說", "小説", "novel"]
        .iter()
        .any(|token| evidence.contains(token))
    {
        "novel"
    } else if ["設定集", "设定集", "設定資料", "setting book"]
        .iter()
        .any(|token| evidence.contains(token))
    {
        "setting_book"
    } else {
        "manga"
    }
}
pub fn version(r: &Record) -> Version {
    // Only explicit tokens. Han characters alone cannot deterministically prove translation.
    let mut evidence = r.raw_title.clone();
    for tag in r.metadata["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        evidence.push(' ');
        evidence.push_str(tag);
    }
    let s = evidence.to_lowercase();
    let has = |terms: &[&str]| terms.iter().any(|t| s.contains(t));
    let known = |yes: bool, no: bool| match (yes, no) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    };
    Version {
        chinese: {
            let negative = has(&["未汉化", "未漢化", "untranslated", "not translated"]);
            let positive_text = s
                .replace("未汉化", "")
                .replace("未漢化", "")
                .replace("untranslated", "")
                .replace("not translated", "");
            let positive = [
                "汉化",
                "漢化",
                "中文",
                "chinese",
                "中国翻译",
                "中國翻譯",
                "机翻",
                "機翻",
            ]
            .iter()
            .any(|token| positive_text.contains(token));
            known(positive, negative)
        },
        uncensored: known(
            has(&["无码", "無碼", "無修正", "无修正", "uncensored"]),
            has(&["有码", "有碼"]),
        ),
        color: known(
            has(&["全彩", "full color", "full colour", "フルカラー"]),
            has(&["黑白", "黑 白", "モノクロ"]),
        ),
        translation: if has(&["机翻", "機翻", "ai翻译", "ai翻譯"]) {
            "ai"
        } else if has(&["人工汉化", "人工漢化"]) {
            "human"
        } else {
            "unknown"
        }
        .into(),
        sample: known(
            has(&["sample", "preview", "試閱", "试阅", "体験版", "サンプル"]),
            has(&["完整版", "完全版"]),
        ),
    }
}
fn candidate(t: &Target) -> Candidate {
    Candidate {
        id: t.source_key.clone(),
        source: t.source_key.split(':').next().unwrap_or("").into(),
        version: t.version.clone(),
        size: None,
    }
}
fn local_version(v: &Value) -> Version {
    Version {
        chinese: match v["language"]["chinese"].as_str() {
            Some("confirmed") => Some(true),
            Some("not_chinese") => Some(false),
            _ => None,
        },
        uncensored: match v["version"]["censorship"].as_str() {
            Some("uncensored") => Some(true),
            Some("censored") => Some(false),
            _ => None,
        },
        color: match v["version"]["color"].as_str() {
            Some("color") => Some(true),
            Some("monochrome" | "black_white") => Some(false),
            _ => None,
        },
        translation: v["version"]["translation_type"]
            .as_str()
            .unwrap_or("unknown")
            .into(),
        sample: v["version"]["sample_or_preview"].as_bool(),
    }
}
pub fn now() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod a03_tests {
    use super::*;

    fn fixture() -> (State, Record) {
        let record = Record::new(
            "jm",
            "SOURCE_1".into(),
            vec!["santa".into()],
            "作品 01".to_string(),
            json!({"content_type":"manga"}),
        );
        let k = key(&record);
        let mut state = State {
            authors: json!({"authors":[{"name":"santa","enabled":true}]}),
            inventory: json!({"works":[]}),
            catalog: BTreeMap::new(),
            pending: BTreeMap::new(),
            review: BTreeMap::new(),
            cleanup_review: json!([]),
            decisions: Decisions::default(),
            scan: Scan { started_at: "fixed".into(), ..Scan::default() },
        };
        state.catalog.insert(
            k,
            Entry {
                record: record.clone(),
                author_evidence: AuthorEvidence::default(),
                search_fingerprint: record.fingerprint.clone(),
                detail_fingerprint: record.fingerprint.clone(),
                analysis_context: String::new(),
                work_id: None,
                analysis_count: 0,
                unavailable_streak: 0,
                active: true,
                last_unavailable_check: None,
                search_queries: BTreeSet::new(),
                matcher_version: String::new(),
                identity_evidence: Value::Null,
                identity_provenance: Value::Null,
            },
        );
        (state, record)
    }

    #[test]
    fn authority_epoch_changes_pending_revision_and_reactivation() {
        let (mut state, record) = fixture();
        let k = key(&record);
        state.bind_work(&k, &record, "WORK_1".into(), Some("santa".into()), Some("cert-a".into()));
        assert_eq!(state.pending["WORK_1"].task_revision, 1);
        state.bind_work(&k, &record, "WORK_1".into(), Some("santa".into()), Some("cert-a".into()));
        assert_eq!(state.pending["WORK_1"].task_revision, 1);
        state.bind_work(&k, &record, "WORK_1".into(), Some("santa".into()), Some("".into()));
        assert_eq!(state.pending["WORK_1"].task_revision, 2);
        assert!(state.pending["WORK_1"].binding_authority_hash.is_empty());
        state.bind_work(&k, &record, "WORK_1".into(), Some("santa".into()), Some("cert-a".into()));
        assert_eq!(state.pending["WORK_1"].task_revision, 3);
        assert_eq!(state.pending["WORK_1"].binding_authority_hash, "cert-a");
        state.bind_work(&k, &record, "WORK_1".into(), Some("santa".into()), Some("cert-b".into()));
        assert_eq!(state.pending["WORK_1"].task_revision, 4);
        assert_eq!(state.pending["WORK_1"].binding_authority_hash, "cert-b");
        state.pending.get_mut("WORK_1").unwrap().status = "superseded_by_identity_reanalysis".into();
        state.bind_work(&k, &record, "WORK_1".into(), Some("santa".into()), Some("cert-b".into()));
        assert_eq!(state.pending["WORK_1"].task_revision, 5);
        assert_eq!(state.pending["WORK_1"].status, "pending");
    }
}
