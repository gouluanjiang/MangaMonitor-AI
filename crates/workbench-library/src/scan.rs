use crate::{
    archive, error, hash, metadata,
    paths::{self, Entries, Node, Root, SafeDirectory},
    Result,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::Read,
    time::{Duration, Instant},
};
use workbench_storage::{
    library_relative_path_is_valid, LibraryCoverFile, LibraryDocument, LibraryEvidence,
    LibraryFormat, LibraryItem, LibraryItemState, LibraryPhase, LibraryRecord, Source,
    MAX_LIBRARY_ITEMS, MAX_LIBRARY_VISITED, MAX_SAFE_INTEGER,
};

const BATCH_NODES: usize = 128;
const MAX_WORK_NODES: u64 = 20_000;
const MAX_WORK_DEPTH: usize = 8;
const MANAGED_LAYOUT: &str = "_mangamonitor-layout.json";
const MANAGED_MANIFEST: &str = "_mangamonitor.json";
const MAX_MANAGED_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedLayout {
    version: u64,
    source: String,
    work_id: String,
    expected_pages: u64,
    layout_version: u64,
    task_id: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedManifest {
    version: u64,
    origin: String,
    task_id: String,
    approval_revision: u64,
    target_hash: String,
    source: String,
    work_id: String,
    root_id: String,
    generation: u64,
    layout_version: u64,
    files: Vec<ManagedFile>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedFile {
    relative_path: String,
    size_bytes: u64,
    sha256: String,
}

fn managed_json<T: serde::de::DeserializeOwned>(file: &std::fs::File, limit: u64) -> Result<T> {
    let before = paths::identity(file)?;
    if before.bytes == 0 || before.bytes > limit {
        return Err(error("LIBRARY_DOWNLOAD_INCOMPLETE"));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error("LIBRARY_DOWNLOAD_INCOMPLETE"))?;
    if paths::identity(file)? != before || bytes.len() as u64 != before.bytes {
        return Err(error("LIBRARY_DOWNLOAD_INCOMPLETE"));
    }
    serde_json::from_slice(&bytes).map_err(|_| error("LIBRARY_DOWNLOAD_INCOMPLETE"))
}

fn opaque_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}

pub(crate) struct ScanJob {
    pub root: Root,
    pub generation: u64,
    pub revision: u64,
    entries: Entries,
    active: Option<WorkScan>,
    manual: HashMap<String, LibraryRecord>,
    incomplete: bool,
}

struct WorkScan {
    record: LibraryRecord,
    stack: Vec<(String, Entries)>,
    directories: Vec<(String, workbench_storage::LibraryFileIdentity)>,
    nodes: u64,
    top_pages: u64,
    chapter_pages: u64,
    unfinished: bool,
    registration_files: Option<Vec<(String, workbench_storage::LibraryFileIdentity)>>,
    managed: bool,
    managed_layout: Option<ManagedLayout>,
    managed_manifest: Option<ManagedManifest>,
    observed_files: BTreeMap<String, u64>,
}

/// Inspects only the already finalized directory. The caller owns download proof;
/// this path checks names, metadata, counts and stable file identities, not pixels.
pub(crate) fn completed_directory(
    root: &Root,
    relative: &str,
) -> Result<(LibraryRecord, u64, u64)> {
    root.verify()?;
    let mut work = WorkScan::new(&root.saved.id, relative, root.directory(relative)?)?;
    work.registration_files = Some(Vec::new());
    let mut counts = LibraryDocument {
        visited: 1,
        ..LibraryDocument::default()
    };
    // WorkScan limits directory depth, number of directories, pages and nodes.
    // Unlike ScanJob, this never obtains or advances the selected root iterator.
    while !work.step(root, &mut counts)? {}
    root.verify()?;
    if work.unfinished
        || work.record.item.state != LibraryItemState::Indexed
        || work.record.item.error_code.is_some()
    {
        return Err(error("LIBRARY_REGISTER_INCOMPLETE"));
    }
    Ok((work.record, counts.visited, counts.skipped))
}

pub(crate) fn base_record(root_id: &str, relative: &str, format: LibraryFormat) -> LibraryRecord {
    let name = relative.rsplit('/').next().unwrap_or(relative);
    let (reference, conflict) = metadata::filename_reference(name);
    let author = name
        .strip_prefix('[')
        .and_then(|v| v.split_once(']'))
        .map(|(v, _)| v)
        .filter(|v| !v.trim().is_empty() && v.chars().count() <= 200);
    LibraryRecord {
        item: LibraryItem {
            id: hash(format!("{root_id}\0{relative}").as_bytes()),
            relative_path: relative.into(),
            file_name: name.into(),
            format,
            title: name.into(),
            authors: author.map(|v| vec![v.to_owned()]).unwrap_or_default(),
            description: None,
            tags: Vec::new(),
            bytes: 0,
            modified_at: None,
            page_count: None,
            cover_available: false,
            state: LibraryItemState::Indexed,
            error_code: conflict.then(|| "LIBRARY_IDENTITY_CONFLICT".into()),
            identity_evidence: reference.as_ref().map(|_| LibraryEvidence::Filename),
            source_ref: reference,
        },
        identity: None,
        manual_override: false,
        cover: None,
    }
}

pub(crate) fn mark_error(record: &mut LibraryRecord, code: &'static str) {
    record.item.state = if matches!(
        code,
        "LIBRARY_ARCHIVE_UNSUPPORTED" | "LIBRARY_RAR_UNSUPPORTED"
    ) {
        LibraryItemState::Unsupported
    } else {
        LibraryItemState::Unreadable
    };
    record.item.error_code = Some(code.into());
    record.item.cover_available = false;
    record.cover = None;
}

impl ScanJob {
    pub fn new(root: Root, generation: u64, revision: u64, old: &[LibraryRecord]) -> Result<Self> {
        let entries = root.directory("")?.entries()?;
        let manual = old
            .iter()
            .filter(|r| r.manual_override)
            .map(|r| (r.item.id.clone(), r.clone()))
            .collect();
        Ok(Self {
            root,
            generation,
            revision,
            entries,
            active: None,
            manual,
            incomplete: false,
        })
    }

    pub fn batch(&mut self, document: &mut LibraryDocument) -> Result<()> {
        self.root.verify()?;
        let started = Instant::now();
        for _ in 0..BATCH_NODES {
            if document.visited >= MAX_LIBRARY_VISITED
                || document.records.len() >= MAX_LIBRARY_ITEMS
            {
                return Err(error("LIBRARY_LIMIT_REACHED"));
            }
            if let Some(mut active) = self.active.take() {
                match active.step(&self.root, document) {
                    Ok(true) => self.finish_work(document, active.record),
                    Ok(false) => self.active = Some(active),
                    Err(problem) => {
                        if active.managed {
                            // A task-owned directory is not a library work until
                            // its final manifest agrees with the observed tree.
                            document.skipped += 1;
                            self.incomplete = true;
                            continue;
                        }
                        mark_error(&mut active.record, problem.code);
                        self.incomplete = true;
                        self.finish_work(document, active.record);
                    }
                }
            } else {
                let Some(name) = self.entries.next_name()? else {
                    document.phase = if self.incomplete {
                        LibraryPhase::Error
                    } else {
                        LibraryPhase::Complete
                    };
                    document.error_code = self.incomplete.then(|| "LIBRARY_SCAN_INCOMPLETE".into());
                    self.root.verify()?;
                    return Ok(());
                };
                document.visited += 1;
                let Some(name) = name.to_str().filter(|v| library_relative_path_is_valid(v)) else {
                    document.skipped += 1;
                    self.incomplete = true;
                    continue;
                };
                let format = match name
                    .rsplit('.')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "zip" => Some(LibraryFormat::Zip),
                    "cbz" => Some(LibraryFormat::Cbz),
                    "rar" => Some(LibraryFormat::Rar),
                    _ => None,
                };
                match self.entries.directory.child(name) {
                    Ok(Node::Directory(directory)) => {
                        match WorkScan::new(&self.root.saved.id, name, directory) {
                            Ok(work) => self.active = Some(work),
                            Err(problem) => {
                                let mut record = base_record(
                                    &self.root.saved.id,
                                    name,
                                    LibraryFormat::Directory,
                                );
                                mark_error(&mut record, problem.code);
                                self.incomplete = true;
                                self.finish_work(document, record);
                            }
                        }
                    }
                    Ok(Node::File(mut file)) => {
                        if let Some(format) = format {
                            let mut record = base_record(&self.root.saved.id, name, format);
                            let result = (|| {
                                let identity = paths::identity(&file.file)?;
                                record.item.bytes = identity.bytes;
                                record.item.modified_at = file
                                    .file
                                    .metadata()
                                    .ok()
                                    .and_then(|v| paths::modified_at(&v));
                                record.identity = Some(identity.clone());
                                if format == LibraryFormat::Rar {
                                    return Err(error("LIBRARY_RAR_UNSUPPORTED"));
                                }
                                archive::inspect(&mut file.file, &mut record)?;
                                if paths::identity(&file.file)? != identity {
                                    return Err(error("LIBRARY_FILE_CHANGED"));
                                }
                                Ok(())
                            })();
                            if let Err(problem) = result {
                                mark_error(&mut record, problem.code);
                            }
                            self.finish_work(document, record);
                        } else {
                            document.skipped += 1;
                        }
                    }
                    Ok(Node::Skipped) => {
                        document.skipped += 1;
                    }
                    Err(problem) => {
                        if let Some(format) = format {
                            let mut record = base_record(&self.root.saved.id, name, format);
                            mark_error(&mut record, problem.code);
                            self.finish_work(document, record);
                        } else {
                            document.skipped += 1;
                            self.incomplete = true;
                        }
                    }
                }
            }
            if started.elapsed() >= Duration::from_millis(200) {
                break;
            }
        }
        self.root.verify()?;
        Ok(())
    }

    fn finish_work(&self, document: &mut LibraryDocument, mut record: LibraryRecord) {
        if let Some(old) = self
            .manual
            .get(&record.item.id)
            .filter(|old| old.identity.is_some() && old.identity == record.identity)
        {
            record.manual_override = true;
            record.item.source_ref = old.item.source_ref.clone();
            record.item.identity_evidence = old.item.identity_evidence;
            if record.item.error_code.as_deref() == Some("LIBRARY_IDENTITY_CONFLICT") {
                record.item.error_code = None;
            }
        }
        document.records.push(record);
    }
}

impl WorkScan {
    fn new(root_id: &str, relative: &str, directory: SafeDirectory) -> Result<Self> {
        let mut record = base_record(root_id, relative, LibraryFormat::Directory);
        record.identity = Some(paths::identity(&directory.file)?);
        record.item.modified_at = directory
            .file
            .metadata()
            .ok()
            .and_then(|v| paths::modified_at(&v));
        let directories = vec![(
            relative.to_owned(),
            record
                .identity
                .clone()
                .ok_or(error("LIBRARY_READ_FAILED"))?,
        )];
        Ok(Self {
            record,
            stack: vec![(relative.into(), directory.entries()?)],
            directories,
            nodes: 0,
            top_pages: 0,
            chapter_pages: 0,
            unfinished: false,
            registration_files: None,
            managed: false,
            managed_layout: None,
            managed_manifest: None,
            observed_files: BTreeMap::new(),
        })
    }

    fn step(&mut self, root: &Root, document: &mut LibraryDocument) -> Result<bool> {
        let Some((parent, entries)) = self.stack.last_mut() else {
            // Revalidate directory identities before publishing a completed work.
            // This is metadata-only; image bytes were never read by the scan.
            for (relative, expected) in &self.directories {
                if paths::identity(&root.directory(relative)?.file)? != *expected {
                    return Err(error("LIBRARY_FILE_CHANGED"));
                }
            }
            if let Some(files) = &self.registration_files {
                for (relative, expected) in files {
                    let Node::File(file) = root.node(relative)? else {
                        return Err(error("LIBRARY_FILE_CHANGED"));
                    };
                    if paths::identity(&file.file)? != *expected {
                        return Err(error("LIBRARY_FILE_CHANGED"));
                    }
                }
            }
            let cover_only = self.chapter_pages == 0
                && self.top_pages == 1
                && self.record.cover.as_ref().is_some_and(|cover| {
                    cover
                        .relative_path
                        .rsplit('/')
                        .next()
                        .is_some_and(|name| name.to_ascii_lowercase().starts_with("cover."))
                });
            self.record.item.page_count = Some(if self.chapter_pages > 0 {
                self.chapter_pages
            } else if cover_only {
                0
            } else {
                self.top_pages
            });
            self.record.item.cover_available = self.record.cover.is_some();
            if self.managed {
                self.validate_managed()?;
            }
            if self.record.item.error_code.is_none() {
                self.record.item.error_code = if self.unfinished {
                    Some("LIBRARY_DOWNLOAD_INCOMPLETE".into())
                } else if cover_only {
                    Some("LIBRARY_COVER_ONLY".into())
                } else if self.record.cover.is_none() {
                    Some("LIBRARY_NO_IMAGES".into())
                } else {
                    None
                };
            }
            return Ok(true);
        };
        let Some(name) = entries.next_name()? else {
            self.stack.pop();
            return Ok(false);
        };
        self.nodes += 1;
        document.visited += 1;
        if self.nodes > MAX_WORK_NODES {
            return Err(error("LIBRARY_LIMIT_REACHED"));
        }
        let name = name
            .to_str()
            .filter(|v| library_relative_path_is_valid(v))
            .ok_or(error("LIBRARY_UNSAFE_PATH"))?;
        let relative = format!("{parent}/{name}");
        if parent.as_str() == self.record.item.relative_path && name == MANAGED_LAYOUT {
            self.managed = true;
        }
        if !library_relative_path_is_valid(&relative) {
            return Err(error("LIBRARY_UNSAFE_PATH"));
        }
        match entries.directory.child(name)? {
            Node::Directory(directory) => {
                if name.starts_with(".下载中-") {
                    self.unfinished = true;
                    document.skipped += 1;
                    return Ok(false);
                }
                if self.stack.len() >= MAX_WORK_DEPTH || self.directories.len() >= 1000 {
                    return Err(error("LIBRARY_LIMIT_REACHED"));
                }
                self.directories
                    .push((relative.clone(), paths::identity(&directory.file)?));
                self.stack.push((relative, directory.entries()?));
            }
            Node::File(file) => {
                let identity = paths::identity(&file.file)?;
                let work_relative = relative
                    .strip_prefix(&format!("{}/", self.record.item.relative_path))
                    .ok_or(error("LIBRARY_UNSAFE_PATH"))?;
                self.observed_files
                    .insert(work_relative.into(), identity.bytes);
                if let Some(files) = &mut self.registration_files {
                    files.push((relative.clone(), identity.clone()));
                    if identity.bytes == 0 && archive::is_image(name) {
                        return Err(error("LIBRARY_REGISTER_INCOMPLETE"));
                    }
                    if name.ends_with(".part") || name.ends_with(".tmp") {
                        self.unfinished = true;
                    }
                }
                self.record.item.bytes = self
                    .record
                    .item
                    .bytes
                    .checked_add(identity.bytes)
                    .filter(|v| *v <= MAX_SAFE_INTEGER)
                    .ok_or(error("LIBRARY_LIMIT_REACHED"))?;
                if parent.as_str() == self.record.item.relative_path && name == MANAGED_LAYOUT {
                    self.managed_layout = Some(managed_json(&file.file, 4096)?);
                } else if parent.as_str() == self.record.item.relative_path
                    && name == MANAGED_MANIFEST
                {
                    // An unrelated external directory with this filename keeps
                    // its existing behavior unless the managed marker is found.
                    self.managed_manifest =
                        managed_json(&file.file, MAX_MANAGED_MANIFEST_BYTES).ok();
                } else if archive::is_image(name) {
                    if parent.as_str() == self.record.item.relative_path {
                        self.top_pages += 1;
                    } else {
                        self.chapter_pages += 1;
                    }
                    if self.top_pages + self.chapter_pages > 10_000 {
                        return Err(error("LIBRARY_LIMIT_REACHED"));
                    }
                    if self
                        .record
                        .cover
                        .as_ref()
                        .is_none_or(|old| archive::cover_precedes(&relative, &old.relative_path))
                    {
                        self.record.cover = Some(LibraryCoverFile {
                            relative_path: relative,
                            identity: Some(identity),
                        });
                    }
                } else if parent.as_str() == self.record.item.relative_path
                    && (name == "元数据.json" || name.eq_ignore_ascii_case("ComicInfo.xml"))
                {
                    let metadata_result = (|| {
                        if identity.bytes > metadata::MAX_METADATA_BYTES as u64 {
                            return Err(error("LIBRARY_METADATA_LIMIT"));
                        }
                        let mut bytes = Vec::new();
                        (&file.file)
                            .take(metadata::MAX_METADATA_BYTES as u64 + 1)
                            .read_to_end(&mut bytes)
                            .map_err(|_| error("LIBRARY_METADATA_INVALID"))?;
                        if paths::identity(&file.file)? != identity {
                            return Err(error("LIBRARY_FILE_CHANGED"));
                        }
                        if name == "元数据.json" {
                            metadata::downloader_json(&bytes)
                        } else {
                            metadata::comic_info(&bytes)
                        }
                    })();
                    match metadata_result {
                        Ok(value) => metadata::apply(&mut self.record.item, value),
                        Err(problem) => {
                            if self.record.item.error_code.as_deref()
                                != Some("LIBRARY_IDENTITY_CONFLICT")
                            {
                                self.record.item.error_code = Some(problem.code.into());
                            }
                        }
                    }
                } else {
                    document.skipped += 1;
                }
            }
            Node::Skipped => {
                if self.registration_files.is_some() {
                    return Err(error("LIBRARY_REGISTER_INCOMPLETE"));
                }
                document.skipped += 1;
            }
        }
        Ok(false)
    }

    fn validate_managed(&self) -> Result<()> {
        let invalid = || error("LIBRARY_DOWNLOAD_INCOMPLETE");
        let layout = self.managed_layout.as_ref().ok_or_else(invalid)?;
        let manifest = self.managed_manifest.as_ref().ok_or_else(invalid)?;
        if layout.version != 1
            || layout.layout_version != 1
            || layout.source != "JM"
            || !(1..=10_000).contains(&layout.expected_pages)
            || !opaque_hash(&layout.task_id)
            || manifest.version != 1
            || manifest.layout_version != 1
            || manifest.origin != "manual"
            || manifest.source != layout.source
            || manifest.work_id != layout.work_id
            || manifest.task_id != layout.task_id
            || !opaque_hash(&manifest.target_hash)
            || !opaque_hash(&manifest.root_id)
            || manifest.approval_revision > MAX_SAFE_INTEGER
            || manifest.generation > MAX_SAFE_INTEGER
            || manifest.files.len() > 10_402
            || self.unfinished
            || self.record.item.error_code.is_some()
            || self
                .record
                .item
                .source_ref
                .as_ref()
                .is_none_or(|reference| {
                    reference.source != Source::Jm || reference.work_id != layout.work_id
                })
            || self.record.item.page_count != Some(layout.expected_pages)
        {
            return Err(invalid());
        }
        let mut expected = BTreeMap::new();
        let mut chapters = BTreeSet::new();
        let mut pages = 0_u64;
        for file in &manifest.files {
            if !library_relative_path_is_valid(&file.relative_path)
                || file.size_bytes == 0
                || file.size_bytes > MAX_SAFE_INTEGER
                || !opaque_hash(&file.sha256)
                || expected
                    .insert(file.relative_path.clone(), file.size_bytes)
                    .is_some()
            {
                return Err(invalid());
            }
            if let Some((chapter, name)) = file.relative_path.split_once('/') {
                let (order, id) = chapter.split_once('-').ok_or_else(invalid)?;
                if order.is_empty()
                    || !order.bytes().all(|v| v.is_ascii_digit())
                    || id.is_empty()
                    || !id.bytes().all(|v| v.is_ascii_digit())
                    || id.parse::<u64>().ok().is_none_or(|v| v == 0)
                    || name.contains('/')
                {
                    return Err(invalid());
                }
                chapters.insert(chapter.to_owned());
                if name != "章节元数据.json" {
                    let (number, extension) = name.rsplit_once('.').ok_or_else(invalid)?;
                    if !matches!(extension, "jpg" | "webp" | "gif")
                        || number.is_empty()
                        || !number.bytes().all(|v| v.is_ascii_digit())
                        || number.parse::<u64>().ok().is_none_or(|v| v == 0)
                    {
                        return Err(invalid());
                    }
                    pages += 1;
                }
            } else if !matches!(
                file.relative_path.as_str(),
                MANAGED_LAYOUT | "元数据.json" | "cover.jpg"
            ) {
                return Err(invalid());
            }
        }
        if pages != layout.expected_pages
            || chapters.is_empty()
            || chapters.len() > 200
            || ![MANAGED_LAYOUT, "元数据.json", "cover.jpg"]
                .iter()
                .all(|v| expected.contains_key(*v))
            || !chapters
                .iter()
                .all(|chapter| expected.contains_key(&format!("{chapter}/章节元数据.json")))
        {
            return Err(invalid());
        }
        let mut observed = self.observed_files.clone();
        if observed.remove(MANAGED_MANIFEST).is_none() || observed != expected {
            return Err(invalid());
        }
        let work_prefix = format!("{}/", self.record.item.relative_path);
        let directories: BTreeSet<_> = self
            .directories
            .iter()
            .skip(1)
            .map(|(path, _)| path.strip_prefix(&work_prefix).unwrap_or("").to_owned())
            .collect();
        if directories != chapters {
            return Err(invalid());
        }
        Ok(())
    }
}
