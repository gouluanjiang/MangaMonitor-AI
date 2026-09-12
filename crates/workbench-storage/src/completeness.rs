//! Read-only language/version completeness, separate from physical library ownership.
//! Only explicit identities join versions. Titles can raise review candidates, never
//! hide a work or establish a translation upgrade. This module performs no media IO.
use crate::{
    library_hash_is_valid, model::ValidatedDocument, DiscoveryRecord, Document, LibraryItemState,
    LibraryPhase, LibraryReference, Result, Source, StoreError, WorkbenchStore,
    MAX_DISCOVERY_RECORDS, MAX_SAFE_INTEGER,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use unicode_normalization::UnicodeNormalization;

const FILE: &str = "completeness.json";
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_FAMILIES: usize = 10_000;
const MAX_LANGUAGES: usize = 20_000;
const MAX_MEMBERS: usize = 50;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CompletenessLanguage {
    Chinese,
    Japanese,
    Other,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum CompletenessMember {
    Source {
        reference: LibraryReference,
    },
    Phone {
        name: String,
    },
    Computer {
        #[serde(rename = "itemId")]
        item_id: String,
    },
}

fn name_key(value: &str) -> String {
    let value = value.trim();
    let stem = value.rsplit_once('.').map_or(value, |(stem, extension)| {
        if ["zip", "cbz", "rar", "7z"]
            .iter()
            .any(|suffix| extension.eq_ignore_ascii_case(suffix))
        {
            stem.trim()
        } else {
            value
        }
    });
    stem.nfc().collect()
}

fn text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= maximum
        && !value.chars().any(char::is_control)
}

fn source_key(reference: &LibraryReference) -> String {
    format!(
        "source:{}:{}",
        match reference.source {
            Source::Jm => "JM",
            Source::Pica => "Pica",
        },
        reference.work_id
    )
}

impl CompletenessMember {
    fn normalized(self) -> Self {
        match self {
            Self::Phone { name } => Self::Phone {
                name: name.trim().nfc().collect(),
            },
            other => other,
        }
    }

    fn valid(&self) -> bool {
        match self {
            Self::Source { reference } => reference.is_valid(),
            Self::Phone { name } => {
                text(name, 1024)
                    && name.trim() == name
                    && name.nfc().collect::<String>() == *name
                    && !name.contains(['/', '\\'])
                    && name != "."
                    && name != ".."
                    && !(name.as_bytes().get(1) == Some(&b':')
                        && name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic))
            }
            Self::Computer { item_id } => library_hash_is_valid(item_id),
        }
    }

    fn key(&self) -> String {
        match self {
            Self::Source { reference } => source_key(reference),
            Self::Phone { name } => format!("phone:{}", name_key(name)),
            Self::Computer { item_id } => format!("computer:{item_id}"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompletenessFamily {
    pub id: String,
    pub members: Vec<CompletenessMember>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompletenessLanguageCorrection {
    pub member: CompletenessMember,
    pub language: CompletenessLanguage,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompletenessDocument {
    pub version: u32,
    pub families: Vec<CompletenessFamily>,
    pub languages: Vec<CompletenessLanguageCorrection>,
}

impl Default for CompletenessDocument {
    fn default() -> Self {
        Self {
            version: 1,
            families: vec![],
            languages: vec![],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletenessSettings {
    pub revision: u64,
    pub families: Vec<CompletenessFamily>,
    pub languages: Vec<CompletenessLanguageCorrection>,
}

impl From<Document<CompletenessDocument>> for CompletenessSettings {
    fn from(document: Document<CompletenessDocument>) -> Self {
        Self {
            revision: document.revision,
            families: document.value.families,
            languages: document.value.languages,
        }
    }
}

fn hash(value: &impl Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| StoreError::new("COMPLETENESS_INVALID"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn family_id(members: &[CompletenessMember]) -> Result<String> {
    hash(&("completeness-family-v1", members))
}

impl ValidatedDocument for CompletenessDocument {
    fn validate(&self) -> Result<()> {
        let invalid = || StoreError::new("VALIDATION_FAILED");
        if self.version != 1
            || self.families.len() > MAX_FAMILIES
            || self.languages.len() > MAX_LANGUAGES
        {
            return Err(invalid());
        }
        let mut all_members = HashSet::new();
        for family in &self.families {
            if family.members.len() < 2
                || family.members.len() > MAX_MEMBERS
                || family.id != family_id(&family.members)?
                || !family
                    .members
                    .iter()
                    .any(|member| matches!(member, CompletenessMember::Source { .. }))
                || family
                    .members
                    .iter()
                    .any(|member| !member.valid() || !all_members.insert(member.key()))
                || family
                    .members
                    .windows(2)
                    .any(|pair| pair[0].key() >= pair[1].key())
            {
                return Err(invalid());
            }
        }
        let mut corrected = HashSet::new();
        if self
            .languages
            .iter()
            .any(|entry| !entry.member.valid() || !corrected.insert(entry.member.key()))
        {
            return Err(invalid());
        }
        Ok(())
    }
}

impl WorkbenchStore {
    pub fn read_completeness(&self) -> Result<Document<CompletenessDocument>> {
        self.read(FILE, MAX_BYTES)
    }
}

pub fn completeness_settings_read(store: &WorkbenchStore) -> Result<CompletenessSettings> {
    store.read_completeness().map(Into::into)
}

/// An explicit relation may extend/merge prior families, but never invents presence.
pub fn completeness_family_confirm(
    store: &WorkbenchStore,
    revision: u64,
    members: Vec<CompletenessMember>,
) -> Result<CompletenessSettings> {
    let members: Vec<_> = members
        .into_iter()
        .map(CompletenessMember::normalized)
        .collect();
    if members.len() < 2
        || members.len() > MAX_MEMBERS
        || members.iter().any(|member| !member.valid())
    {
        return Err(StoreError::new("COMPLETENESS_INVALID"));
    }
    let mut merged: BTreeMap<_, _> = members
        .into_iter()
        .map(|member| (member.key(), member))
        .collect();
    if merged.len() < 2 {
        return Err(StoreError::new("COMPLETENESS_INVALID"));
    }
    let mut current = store.read_completeness()?;
    if current.revision != revision {
        return Err(StoreError::new("REVISION_CONFLICT"));
    }
    let previous = current.value.clone();
    current.value.families.retain(|family| {
        if family
            .members
            .iter()
            .any(|member| merged.contains_key(&member.key()))
        {
            for member in &family.members {
                merged.insert(member.key(), member.clone());
            }
            false
        } else {
            true
        }
    });
    let members: Vec<_> = merged.into_values().collect();
    if members.len() > MAX_MEMBERS || current.value.families.len() >= MAX_FAMILIES {
        return Err(StoreError::new("COMPLETENESS_LIMIT"));
    }
    if !members
        .iter()
        .any(|member| matches!(member, CompletenessMember::Source { .. }))
    {
        return Err(StoreError::new("COMPLETENESS_INVALID"));
    }
    current.value.families.push(CompletenessFamily {
        id: family_id(&members)?,
        members,
    });
    current
        .value
        .families
        .sort_by(|left, right| left.id.cmp(&right.id));
    if current.value == previous {
        return Ok(current.into());
    }
    store
        .write(FILE, MAX_BYTES, revision, current.value)
        .map(Into::into)
}

pub fn completeness_family_unlink(
    store: &WorkbenchStore,
    revision: u64,
    family_id: &str,
) -> Result<CompletenessSettings> {
    if !library_hash_is_valid(family_id) {
        return Err(StoreError::new("COMPLETENESS_INVALID"));
    }
    let mut current = store.read_completeness()?;
    if current.revision != revision {
        return Err(StoreError::new("REVISION_CONFLICT"));
    }
    let before = current.value.families.len();
    current
        .value
        .families
        .retain(|family| family.id != family_id);
    if before == current.value.families.len() {
        return Err(StoreError::new("COMPLETENESS_NOT_FOUND"));
    }
    store
        .write(FILE, MAX_BYTES, revision, current.value)
        .map(Into::into)
}

pub fn completeness_language_set(
    store: &WorkbenchStore,
    revision: u64,
    member: CompletenessMember,
    language: Option<CompletenessLanguage>,
) -> Result<CompletenessSettings> {
    let member = member.normalized();
    if !member.valid() {
        return Err(StoreError::new("COMPLETENESS_INVALID"));
    }
    let mut current = store.read_completeness()?;
    if current.revision != revision {
        return Err(StoreError::new("REVISION_CONFLICT"));
    }
    let previous = current.value.clone();
    current
        .value
        .languages
        .retain(|entry| entry.member.key() != member.key());
    if let Some(language) = language {
        current
            .value
            .languages
            .push(CompletenessLanguageCorrection { member, language });
    }
    current
        .value
        .languages
        .sort_by_key(|entry| entry.member.key());
    if current.value == previous {
        return Ok(current.into());
    }
    if current.value.languages.len() > MAX_LANGUAGES {
        return Err(StoreError::new("COMPLETENESS_LIMIT"));
    }
    store
        .write(FILE, MAX_BYTES, revision, current.value)
        .map(Into::into)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletenessStatus {
    Missing,
    Downloaded,
    OwnedChinese,
    WaitingTranslation,
    TranslationAvailable,
    TranslationDownloaded,
    ReviewRequired,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CompletenessCandidateKind {
    Missing,
    Translation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompletenessCandidate {
    pub group_id: String,
    pub reference: LibraryReference,
    pub kind: CompletenessCandidateKind,
    pub evidence_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletenessSource {
    pub reference: LibraryReference,
    pub title: String,
    pub language: CompletenessLanguage,
    pub author_verified: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletenessCopy {
    pub member: CompletenessMember,
    pub name: String,
    pub language: CompletenessLanguage,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletenessGroup {
    pub group_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub status: CompletenessStatus,
    pub reasons: Vec<String>,
    pub sources: Vec<CompletenessSource>,
    pub phone: Vec<CompletenessCopy>,
    pub computer: Vec<CompletenessCopy>,
    pub eligible: Option<CompletenessCandidate>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletenessSnapshot {
    pub revision: u64,
    pub phone_revision: u64,
    pub library_revision: u64,
    pub matches_revision: u64,
    pub discovery_revision: u64,
    pub evidence_hash: String,
    pub groups: Vec<CompletenessGroup>,
}

#[derive(Clone, Copy, Default)]
struct LanguageEvidence {
    chinese: bool,
    japanese: bool,
    other: bool,
    unresolved: bool,
}

impl LanguageEvidence {
    fn merge(&mut self, other: Self) {
        self.chinese |= other.chinese;
        self.japanese |= other.japanese;
        self.other |= other.other;
        self.unresolved |= other.unresolved;
    }
    fn language(self) -> CompletenessLanguage {
        match (self.chinese, self.japanese, self.other, self.unresolved) {
            (true, false, false, false) => CompletenessLanguage::Chinese,
            (false, true, false, false) => CompletenessLanguage::Japanese,
            (false, false, true, false) => CompletenessLanguage::Other,
            _ => CompletenessLanguage::Unknown,
        }
    }
    fn from_language(language: CompletenessLanguage) -> Self {
        Self {
            chinese: language == CompletenessLanguage::Chinese,
            japanese: language == CompletenessLanguage::Japanese,
            other: language == CompletenessLanguage::Other,
            unresolved: language == CompletenessLanguage::Unknown,
        }
    }
}

/// Closed explicit language labels. Han/kana characters and `untranslated` alone
/// do not prove a language. Conflicting labels remain unresolved.
fn language_tag(value: &str) -> LanguageEvidence {
    let value = value.trim().to_lowercase();
    let chinese = [
        "chinese",
        "中文",
        "汉化",
        "漢化",
        "中国翻译",
        "中國翻譯",
        "中国翻訳",
        "中國翻訳",
        "中文翻译",
        "中文翻譯",
        "机翻",
        "機翻",
        "简体中文",
        "繁体中文",
        "繁體中文",
        "人工汉化",
        "人工漢化",
    ];
    let japanese = [
        "japanese",
        "日本語",
        "日文",
        "日语",
        "日語",
        "日文原版",
        "日语原版",
        "日語原版",
    ];
    let other = [
        "english", "korean", "英语", "英語", "英文", "韩语", "韓語", "韩文", "韓文",
    ];
    LanguageEvidence {
        chinese: chinese.contains(&value.as_str()),
        japanese: japanese.contains(&value.as_str()),
        other: other.contains(&value.as_str()),
        unresolved: ["未汉化", "未漢化", "untranslated", "not translated"]
            .contains(&value.as_str()),
    }
}

/// Returns top-level brackets only; malformed/nested groups never become proof.
fn bracket_groups(value: &str) -> Vec<(usize, usize, &str)> {
    let mut groups = Vec::new();
    let mut open: Option<(usize, usize, char)> = None;
    let mut nested = false;
    for (index, ch) in value.char_indices() {
        if let Some(close) = match ch {
            '[' => Some(']'),
            '【' => Some('】'),
            '(' => Some(')'),
            _ => None,
        } {
            if open.is_none() {
                open = Some((index, index + ch.len_utf8(), close));
                nested = false;
            } else {
                nested = true;
            }
        } else if let Some((start, content, close)) = open {
            if ch == close {
                if !nested {
                    groups.push((start, index + ch.len_utf8(), &value[content..index]));
                }
                open = None;
            }
        }
    }
    groups
}

fn language_evidence(title: &str, tags: &[String]) -> LanguageEvidence {
    let mut result = LanguageEvidence::default();
    for tag in tags {
        result.merge(language_tag(tag));
    }
    for (_, _, tag) in bracket_groups(title) {
        result.merge(language_tag(tag));
    }
    result
}

// Explicit source IDs use the already accepted desktop destination or existing
// scanner token grammar. Multiple different IDs are never a title-identity guess.
fn filename_reference(name: &str) -> Option<LibraryReference> {
    let mut found = None;
    for (_, _, token) in bracket_groups(name) {
        let (source, id) = if let Some(id) = token.strip_prefix("JM") {
            (Source::Jm, id)
        } else if let Some(id) = token.strip_prefix("Pica") {
            (Source::Pica, id)
        } else {
            continue;
        };
        let id = id
            .strip_prefix('-')
            .or_else(|| id.strip_prefix(':'))
            .unwrap_or(id);
        let reference = LibraryReference {
            source,
            work_id: id.to_ascii_lowercase(),
        };
        if !reference.is_valid() {
            continue;
        }
        if found
            .as_ref()
            .is_some_and(|previous| previous != &reference)
        {
            return None;
        }
        found = Some(reference);
    }
    found
}

/// A candidate key only, never identity evidence. Every edition/number/creator
/// token stays intact; the only removed tokens are explicit language labels.
fn possible_name(value: &str) -> String {
    let value = name_key(value);
    let mut result = String::new();
    let mut previous = 0;
    for (start, end, content) in bracket_groups(&value) {
        let evidence = language_tag(content);
        if evidence.chinese || evidence.japanese || evidence.other || evidence.unresolved {
            result.push_str(&value[previous..start]);
            previous = end;
        }
    }
    result.push_str(&value[previous..]);
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

struct Node {
    member: CompletenessMember,
    name: String,
    language: LanguageEvidence,
    corrected: bool,
    record: Option<usize>,
    copy: Option<bool>, // true phone; false indexed PC
}

struct Graph {
    nodes: Vec<Node>,
    indices: BTreeMap<String, usize>,
    parents: Vec<usize>,
}

impl Graph {
    fn new() -> Self {
        Self {
            nodes: vec![],
            indices: BTreeMap::new(),
            parents: vec![],
        }
    }
    fn add(&mut self, node: Node) -> usize {
        let key = node.member.key();
        if let Some(index) = self.indices.get(&key).copied() {
            self.nodes[index].language.merge(node.language);
            return index;
        }
        let index = self.nodes.len();
        self.nodes.push(node);
        self.parents.push(index);
        self.indices.insert(key, index);
        index
    }
    fn source(&mut self, reference: &LibraryReference) -> usize {
        if let Some(index) = self.indices.get(&source_key(reference)) {
            return *index;
        }
        self.add(Node {
            member: CompletenessMember::Source {
                reference: reference.clone(),
            },
            name: String::new(),
            language: LanguageEvidence::default(),
            corrected: false,
            record: None,
            copy: None,
        })
    }
    fn root(&mut self, index: usize) -> usize {
        let mut root = index;
        while self.parents[root] != root {
            root = self.parents[root];
        }
        let mut current = index;
        while self.parents[current] != current {
            let next = self.parents[current];
            self.parents[current] = root;
            current = next;
        }
        root
    }
    fn join(&mut self, left: usize, right: usize) {
        let left = self.root(left);
        let right = self.root(right);
        if left != right {
            self.parents[right] = left;
        }
    }
    fn copy_language(&mut self, target: usize, exact: usize) {
        if !self.nodes[target].corrected {
            let evidence = self.nodes[exact].language;
            self.nodes[target].language.merge(evidence);
        }
    }
}

/// Input records must come from the current native account discovery service.
/// The caller must reconcile stale PC entries before using `eligible`; this pure
/// metadata projection deliberately does not open manga paths or launch work.
pub fn completeness_project(
    store: &WorkbenchStore,
    discovery_revision: u64,
    records: &[DiscoveryRecord],
) -> Result<CompletenessSnapshot> {
    if discovery_revision > MAX_SAFE_INTEGER || records.len() > MAX_DISCOVERY_RECORDS {
        return Err(StoreError::new("COMPLETENESS_INVALID"));
    }
    let mut seen = HashSet::new();
    for record in records {
        if !record.work.is_valid()
            || !seen.insert((record.work.source, &record.work.work_id))
            || record.matched_authors.is_empty()
            || record.observed_at > MAX_SAFE_INTEGER
            || !library_hash_is_valid(&record.scan_id)
            || (record.author_verified
                && record.matched_authors.iter().any(|author| {
                    !record
                        .work
                        .authors
                        .iter()
                        .any(|name| name.trim() == author.trim())
                }))
        {
            return Err(StoreError::new("COMPLETENESS_INVALID"));
        }
    }
    let settings = store.read_completeness()?;
    let phone = store.read_phone_library()?;
    let library = store.read_library()?;
    let matches = store.read_source_matches()?;
    let evidence_hash = hash(&(
        "completeness-v1",
        discovery_revision,
        records,
        &settings,
        &phone,
        &library,
        &matches,
    ))?;
    let mut graph = Graph::new();
    for (index, record) in records.iter().enumerate() {
        graph.add(Node {
            member: CompletenessMember::Source {
                reference: LibraryReference {
                    source: record.work.source,
                    work_id: record.work.work_id.clone(),
                },
            },
            name: record.work.title.clone(),
            language: language_evidence(&record.work.title, &record.work.tags),
            corrected: false,
            record: Some(index),
            copy: None,
        });
    }
    let mut exact_edges = Vec::new();
    for name in &phone.value.imported_names {
        graph.add(Node {
            member: CompletenessMember::Phone { name: name.clone() },
            name: name.clone(),
            language: language_evidence(name, &[]),
            corrected: false,
            record: None,
            copy: Some(true),
        });
    }
    for entry in &phone.value.manual_entries {
        let index = graph.add(Node {
            member: CompletenessMember::Phone {
                name: entry.name.clone(),
            },
            name: entry.name.clone(),
            language: language_evidence(&entry.name, &[]),
            corrected: false,
            record: None,
            copy: Some(true),
        });
        if let Some(reference) = &entry.reference {
            let source = graph.source(reference);
            graph.join(index, source);
            exact_edges.push((index, source));
        }
    }
    let manually_referenced_names: HashSet<_> = phone
        .value
        .manual_entries
        .iter()
        .filter(|entry| entry.reference.is_some())
        .map(|entry| name_key(&entry.name))
        .collect();
    let phone_filename_refs: Vec<_> = graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            node.copy == Some(true) && !manually_referenced_names.contains(&name_key(&node.name))
        })
        .filter_map(|(index, node)| {
            filename_reference(&node.name).map(|reference| (index, reference))
        })
        .collect();
    for (index, reference) in phone_filename_refs {
        let source = graph.source(&reference);
        graph.join(index, source);
        exact_edges.push((index, source));
    }
    for record in &library.value.records {
        if record.item.state != LibraryItemState::Indexed {
            continue;
        }
        let mut language = language_evidence(&record.item.file_name, &record.item.tags);
        language.merge(language_evidence(&record.item.title, &[]));
        let index = graph.add(Node {
            member: CompletenessMember::Computer {
                item_id: record.item.id.clone(),
            },
            name: record.item.file_name.clone(),
            language,
            corrected: false,
            record: None,
            copy: Some(false),
        });
        if let Some(reference) = &record.item.source_ref {
            let source = graph.source(reference);
            graph.join(index, source);
            exact_edges.push((index, source));
        }
        // Existing accepted phone/file semantics: identical full filename (only
        // archive extension and NFC normalized), not a stripped source title.
        let phone_key = CompletenessMember::Phone {
            name: record.item.file_name.clone(),
        }
        .key();
        if let Some(phone_index) = graph.indices.get(&phone_key).copied() {
            graph.join(index, phone_index);
            exact_edges.push((phone_index, index));
        }
    }
    for pair in &matches.value.pairs {
        let jm = graph.source(&LibraryReference {
            source: Source::Jm,
            work_id: pair.jm.work_id.clone(),
        });
        let pica = graph.source(&LibraryReference {
            source: Source::Pica,
            work_id: pair.pica.work_id.clone(),
        });
        graph.join(jm, pica);
    }
    for family in &settings.value.families {
        let mut previous = None;
        for member in &family.members {
            let index = match member {
                CompletenessMember::Source { reference } => Some(graph.source(reference)),
                other => graph.indices.get(&other.key()).copied(),
            };
            if let Some(index) = index {
                if let Some(previous) = previous {
                    graph.join(previous, index);
                }
                previous = Some(index);
            }
        }
    }
    for correction in &settings.value.languages {
        if let Some(index) = graph.indices.get(&correction.member.key()).copied() {
            graph.nodes[index].language = LanguageEvidence::from_language(correction.language);
            graph.nodes[index].corrected = true;
        }
    }
    // Source -> exact PC/phone -> identical phone filename, never across a family.
    for &(target, exact) in &exact_edges {
        graph.copy_language(target, exact);
    }
    for &(target, exact) in &exact_edges {
        graph.copy_language(target, exact);
    }
    let mut grouped = BTreeMap::<usize, Vec<usize>>::new();
    for index in 0..graph.nodes.len() {
        let root = graph.root(index);
        grouped.entry(root).or_default().push(index);
    }
    let mut candidate_names = BTreeMap::<String, BTreeSet<usize>>::new();
    for (root, indices) in &grouped {
        for &index in indices {
            let node = &graph.nodes[index];
            if node.copy.is_some() || node.record.is_some() {
                let key = possible_name(&node.name);
                if !key.is_empty() {
                    candidate_names.entry(key).or_default().insert(*root);
                }
            }
        }
    }
    let phone_ready = phone.value.imported_at.is_some() || !phone.value.manual_entries.is_empty();
    let computer_ready =
        library.value.root.is_some() && library.value.phase == LibraryPhase::Complete;
    let mut groups = Vec::new();
    for (root, indices) in grouped {
        let mut sources = Vec::new();
        let mut phone_copies = Vec::new();
        let mut computer = Vec::new();
        let mut authors = BTreeSet::new();
        let mut member_keys = Vec::new();
        let mut ambiguous = false;
        for index in indices {
            let node = &graph.nodes[index];
            member_keys.push(node.member.key());
            if let Some(record_index) = node.record {
                let record = &records[record_index];
                authors.extend(record.matched_authors.iter().cloned());
                let reference = LibraryReference {
                    source: record.work.source,
                    work_id: record.work.work_id.clone(),
                };
                sources.push(CompletenessSource {
                    reference,
                    title: node.name.clone(),
                    language: node.language.language(),
                    author_verified: record.author_verified,
                });
                ambiguous |= candidate_names
                    .get(&possible_name(&node.name))
                    .is_some_and(|roots| roots.iter().any(|other| *other != root));
            }
            if let Some(is_phone) = node.copy {
                let copy = CompletenessCopy {
                    member: node.member.clone(),
                    name: node.name.clone(),
                    language: node.language.language(),
                };
                if is_phone {
                    phone_copies.push(copy);
                } else {
                    computer.push(copy);
                }
            }
        }
        if sources.is_empty() {
            continue;
        }
        sources.sort_by_key(|source| source_key(&source.reference));
        phone_copies.sort_by_key(|copy| copy.member.key());
        computer.sort_by_key(|copy| copy.member.key());
        member_keys.sort();
        let group_id = hash(&("completeness-group-v1", member_keys))?;
        let has_phone = !phone_copies.is_empty();
        let phone_chinese = phone_copies
            .iter()
            .any(|copy| copy.language == CompletenessLanguage::Chinese);
        let phone_japanese = phone_copies
            .iter()
            .any(|copy| copy.language == CompletenessLanguage::Japanese);
        let phone_uncertain = phone_copies.iter().any(|copy| {
            matches!(
                copy.language,
                CompletenessLanguage::Unknown | CompletenessLanguage::Other
            )
        });
        let pc_chinese = computer
            .iter()
            .any(|copy| copy.language == CompletenessLanguage::Chinese);
        let pc_uncertain = computer
            .iter()
            .any(|copy| copy.language == CompletenessLanguage::Unknown);
        let chinese = sources.iter().find(|source| {
            source.language == CompletenessLanguage::Chinese && source.author_verified
        });
        let mut reasons = Vec::new();
        let mut kind = None;
        let status = if phone_chinese {
            CompletenessStatus::OwnedChinese
        } else if phone_japanese && pc_chinese {
            CompletenessStatus::TranslationDownloaded
        } else if has_phone && phone_uncertain {
            reasons.push("PHONE_LANGUAGE_UNCONFIRMED");
            CompletenessStatus::ReviewRequired
        } else if ambiguous {
            reasons.push("VERSION_IDENTITY_UNCONFIRMED");
            CompletenessStatus::ReviewRequired
        } else if phone_japanese {
            if pc_uncertain {
                reasons.push("COMPUTER_LANGUAGE_UNCONFIRMED");
                CompletenessStatus::ReviewRequired
            } else if chinese.is_some() {
                if phone_ready && computer_ready {
                    kind = Some(CompletenessCandidateKind::Translation);
                } else {
                    reasons.push("COMPUTER_CATALOG_INCOMPLETE");
                }
                CompletenessStatus::TranslationAvailable
            } else {
                reasons.push("CHINESE_VERSION_NOT_CONFIRMED");
                CompletenessStatus::WaitingTranslation
            }
        } else if !computer.is_empty() {
            CompletenessStatus::Downloaded
        } else if !phone_ready || !computer_ready {
            reasons.push("LIBRARY_EVIDENCE_INCOMPLETE");
            CompletenessStatus::Unknown
        } else if chinese.is_some() {
            kind = Some(CompletenessCandidateKind::Missing);
            CompletenessStatus::Missing
        } else {
            reasons.push("SOURCE_LANGUAGE_OR_AUTHOR_UNCONFIRMED");
            CompletenessStatus::ReviewRequired
        };
        let eligible = kind
            .zip(chinese)
            .map(|(kind, source)| CompletenessCandidate {
                group_id: group_id.clone(),
                reference: source.reference.clone(),
                kind,
                evidence_hash: evidence_hash.clone(),
            });
        groups.push(CompletenessGroup {
            group_id,
            title: sources[0].title.clone(),
            authors: authors.into_iter().collect(),
            status,
            reasons: reasons.into_iter().map(str::to_owned).collect(),
            sources,
            phone: phone_copies,
            computer,
            eligible,
        });
    }
    groups.sort_by(|left, right| {
        left.title
            .cmp(&right.title)
            .then_with(|| left.group_id.cmp(&right.group_id))
    });
    // Avoid publishing a mixed generation if another explicit UI edit raced the projection.
    if store.read_completeness()?.revision != settings.revision
        || store.read_phone_library()?.revision != phone.revision
        || store.read_library()?.revision != library.revision
        || store.read_source_matches()?.revision != matches.revision
    {
        return Err(StoreError::new("REVISION_CONFLICT"));
    }
    Ok(CompletenessSnapshot {
        revision: settings.revision,
        phone_revision: phone.revision,
        library_revision: library.revision,
        matches_revision: matches.revision,
        discovery_revision,
        evidence_hash,
        groups,
    })
}
