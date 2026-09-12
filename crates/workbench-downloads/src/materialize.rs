use crate::{
    adapter, error,
    fs::{self, Directory},
    hash, Result,
};
use cloud_monitor::{
    filesystem_verifier,
    isolated_staging_execution::{
        IsolatedStagingExecutionResult, ISOLATED_STAGING_EXECUTION_SCHEMA_VERSION,
    },
    local_execution_orchestrator::LocalExecutionReport,
    local_executor, verified_execution_receipt,
};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufReader, Cursor, Read, Write},
    path::Path,
};
use workbench_storage::{DownloadFile, DownloadRecord, Source, MAX_DOWNLOAD_FILES};

pub(crate) fn require_root(record: &DownloadRecord) -> Result<Directory> {
    let directory = Directory::open(Path::new(&record.root.path))?;
    if directory.key()? != record.root.file_key {
        return Err(error("DOWNLOAD_ROOT_CHANGED"));
    }
    Ok(directory)
}
pub(crate) fn validate_staging(
    record: &DownloadRecord,
    report: &LocalExecutionReport,
    staging: &Path,
) -> Result<()> {
    let (_, _, command) = adapter::current(record)?;
    let plan = local_executor::plan(&command).map_err(|_| error("DOWNLOAD_PROOF_INVALID"))?;
    if report.inventory_mutation_authorized
        || report.task_completion_authorized
        || report.promotion_authorized
        || report.replacement_authorized
        || report.physical_delete_authorized
        || report.production_enablement_authorized
        || report.command_id != command.command_id
        || report.task_revision != command.task_revision
        || report.target_hash != command.target_hash
        || report.source_work_id != record.metadata.work_id
        || report.source != crate::source_key(record.source)
        || !report.staging_execution_completed
    {
        return Err(error("DOWNLOAD_PROOF_INVALID"));
    }
    let result = IsolatedStagingExecutionResult {
        schema_version: ISOLATED_STAGING_EXECUTION_SCHEMA_VERSION,
        command_id: report.command_id.clone(),
        task_id: report.task_id.clone(),
        work_id: report.work_id.clone(),
        task_revision: report.task_revision,
        target_hash: report.target_hash.clone(),
        source: report.source.clone(),
        source_work_id: report.source_work_id.clone(),
        preflight_hash: report.preflight_hash.clone(),
        staging_execution_completed: report.staging_execution_completed,
        source_completion: report.source_completion.clone(),
        filesystem_verification: report.filesystem_verification.clone(),
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    };
    let receipt =
        verified_execution_receipt::build(&command, &plan, &result, &report.receipt.completed_at)
            .map_err(|_| error("DOWNLOAD_PROOF_INVALID"))?;
    if receipt != report.receipt
        || filesystem_verifier::verify(staging, &plan, &report.source_completion.manifest)
            .map_err(|_| error("DOWNLOAD_STAGING_CHANGED"))?
            != report.filesystem_verification
    {
        return Err(error("DOWNLOAD_STAGING_CHANGED"));
    }
    Ok(())
}

struct Output {
    file: DownloadFile,
    staged: Option<String>,
    bytes: Option<Vec<u8>>,
}
fn generated(path: &str, bytes: Vec<u8>) -> Output {
    Output {
        file: DownloadFile {
            relative_path: path.into(),
            size_bytes: bytes.len() as u64,
            sha256: hash(&bytes),
        },
        staged: None,
        bytes: Some(bytes),
    }
}
fn json_bytes(value: &serde_json::Value) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(value).map_err(|_| error("DOWNLOAD_METADATA_INVALID"))
}
fn source_file(root: &Directory, relative: &str) -> Result<std::fs::File> {
    let parts: Vec<_> = relative.split('/').collect();
    if parts.len() != 3 || parts[0] != "chapters" {
        return Err(error("DOWNLOAD_PROOF_INVALID"));
    }
    root.child(parts[0])?.child(parts[1])?.read(parts[2])
}
fn fallback_cover(root: &Directory, relative: &str) -> Result<Vec<u8>> {
    let file = source_file(root, relative)?;
    if file
        .metadata()
        .map_err(|_| error("DOWNLOAD_COVER_FAILED"))?
        .len()
        > 128 * 1024 * 1024
    {
        return Err(error("DOWNLOAD_COVER_FAILED"));
    }
    let reader = image::ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .map_err(|_| error("DOWNLOAD_COVER_FAILED"))?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| error("DOWNLOAD_COVER_FAILED"))?;
    if width == 0 || height == 0 || width > 20_000 || height > 20_000 {
        return Err(error("DOWNLOAD_COVER_FAILED"));
    }
    let mut reader = image::ImageReader::new(BufReader::new(source_file(root, relative)?))
        .with_guessed_format()
        .map_err(|_| error("DOWNLOAD_COVER_FAILED"))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(20_000);
    limits.max_image_height = Some(20_000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| error("DOWNLOAD_COVER_FAILED"))?
        .thumbnail(640, 960)
        .to_rgb8();
    let mut result = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut result, 82)
        .encode_image(&image)
        .map_err(|_| error("DOWNLOAD_COVER_FAILED"))?;
    Ok(result)
}
fn layout(
    record: &DownloadRecord,
    report: &LocalExecutionReport,
    stage: &Directory,
) -> Result<Vec<Output>> {
    let artifacts = &report.source_completion.manifest.artifacts;
    if artifacts.is_empty() || artifacts.len() > MAX_DOWNLOAD_FILES {
        return Err(error("DOWNLOAD_LIMIT_REACHED"));
    }
    let mut chapters: BTreeMap<String, (u64, String, usize)> = BTreeMap::new();
    let mut result = Vec::new();
    for artifact in artifacts {
        let parts: Vec<_> = artifact.relative_path.split('/').collect();
        if parts.len() != 3 || parts[0] != "chapters" {
            return Err(error("DOWNLOAD_PROOF_INVALID"));
        }
        let (order, id) = parts[1]
            .split_once('-')
            .ok_or(error("DOWNLOAD_PROOF_INVALID"))?;
        let order = order
            .parse::<u64>()
            .map_err(|_| error("DOWNLOAD_PROOF_INVALID"))?;
        let chapter_reference = workbench_storage::LibraryReference {
            source: record.source,
            work_id: id.into(),
        };
        if order == 0
            || !chapter_reference.is_valid()
            || (record.source == Source::Jm && !id.parse::<i64>().is_ok_and(|id| id > 0))
        {
            return Err(error("DOWNLOAD_PROOF_INVALID"));
        }
        let (number, extension) = parts[2]
            .rsplit_once('.')
            .ok_or(error("DOWNLOAD_PROOF_INVALID"))?;
        let number = number
            .parse::<u64>()
            .map_err(|_| error("DOWNLOAD_PROOF_INVALID"))?;
        let expected_static_format = if record.jpeg_output { "jpg" } else { "webp" };
        let format_valid = match record.source {
            Source::Jm => extension == expected_static_format || extension == "gif",
            Source::Pica => {
                !record.jpeg_output && matches!(extension, "jpg" | "jpeg" | "png" | "webp" | "gif")
            }
        };
        if !format_valid || number == 0 {
            return Err(error("DOWNLOAD_PROOF_INVALID"));
        }
        let (directory, filename) = match record.source {
            Source::Jm => (
                format!("{order:04}-{id}"),
                format!("{number:04}.{extension}"),
            ),
            Source::Pica => (
                format!("{order:03}-{id}"),
                format!("{number:03}.{extension}"),
            ),
        };
        let chapter = chapters
            .entry(directory.clone())
            .or_insert((order, id.into(), 0));
        chapter.2 += 1;
        result.push(Output {
            file: DownloadFile {
                relative_path: format!("{directory}/{filename}"),
                size_bytes: artifact.size_bytes,
                sha256: artifact.sha256.clone(),
            },
            staged: Some(artifact.relative_path.clone()),
            bytes: None,
        });
    }
    if chapters.len() > 200 {
        return Err(error("DOWNLOAD_LIMIT_REACHED"));
    }
    let infos: Vec<_> = chapters
        .values()
        .map(|(order, id, count)| chapter_metadata(record.source, *order, id, *count))
        .collect();
    let metadata = &record.metadata;
    let comic_metadata = match record.source {
        Source::Jm => {
            json!({"id":metadata.work_id.parse::<i64>().map_err(|_| error("DOWNLOAD_METADATA_INVALID"))?, "name":metadata.title, "author":metadata.authors, "tags":metadata.tags, "description":metadata.description.as_deref().unwrap_or(""), "chapterInfos":infos,
        "addtime":"","total_views":"","likes":"","series_id":"","comment_total":"","works":[],"actors":[],"related_list":[],"liked":false,"is_favorite":false,"is_aids":false,
        "mangaMonitor":{"layoutVersion":1,"origin":"manual","coverOrigin":"first-verified-page","compatibilityPlaceholders":["addtime","total_views","likes","series_id","comment_total","works","actors","related_list","liked","is_favorite","is_aids"]}})
        }
        Source::Pica => pica_metadata(record, artifacts.len(), &infos),
    };
    result.push(generated("元数据.json", json_bytes(&comic_metadata)?));
    result.push(generated(
        "cover.jpg",
        fallback_cover(stage, &artifacts[0].relative_path)?,
    ));
    for (directory, (order, id, count)) in chapters {
        result.push(generated(
            &format!("{directory}/章节元数据.json"),
            json_bytes(&chapter_metadata(record.source, order, &id, count))?,
        ));
    }
    result.push(generated("_mangamonitor-layout.json", json_bytes(&json!({"version":1,"source":record.source,"workId":metadata.work_id,"expectedPages":artifacts.len(),"layoutVersion":1,"taskId":record.id}))?));
    result.sort_by(|a, b| a.file.relative_path.cmp(&b.file.relative_path));
    Ok(result)
}

fn chapter_metadata(source: Source, order: u64, id: &str, count: usize) -> serde_json::Value {
    match source {
        Source::Jm => {
            json!({"chapterId":id.parse::<i64>().unwrap_or_default(),"chapterTitle":id,"order":order,"imageCount":count,"isPdfExported":false,"isCbzExported":false})
        }
        Source::Pica => {
            json!({"chapterId":id,"chapterTitle":id,"order":order,"imageCount":count,"isDownloaded":true})
        }
    }
}

fn pica_metadata(
    record: &DownloadRecord,
    pages: usize,
    chapters: &[serde_json::Value],
) -> serde_json::Value {
    let metadata = &record.metadata;
    // These neutral compatibility fields are not source observations. The
    // pinned upstream Comic has required fields without serde defaults.
    json!({
        "id":metadata.work_id,"title":metadata.title,"author":metadata.authors.join(", "),
        "pagesCount":pages,"chapterCount":chapters.len(),"chapterInfos":chapters,
        "description":metadata.description.as_deref().unwrap_or(""),"tags":metadata.tags,
        "downloaded":true,"isDownloaded":true,
        "thumb":{"originalName":"cover.jpg","path":"cover.jpg","fileServer":""},
        "finished":false,"categories":[],"likesCount":0,"chineseTeam":"",
        "updatedAt":"1970-01-01T00:00:00Z","createdAt":"","allowDownload":false,
        "viewsCount":0,"isLiked":false,"commentsCount":0,
        "creator":{"id":"","gender":"","name":"","title":"","verified":null,
            "exp":0,"level":0,"characters":[],"avatar":{"originalName":"","path":"","fileServer":""},
            "slogan":"","role":"","character":""},
        "mangaMonitor":{"layoutVersion":1,"origin":"manual","coverOrigin":"first-verified-page",
            "compatibilityPlaceholders":["finished","categories","likesCount","chineseTeam","updatedAt",
                "createdAt","allowDownload","viewsCount","isLiked","commentsCount","creator","thumb.fileServer","chapterInfos.chapterTitle"]}
    })
}

fn verify_file(directory: &Directory, name: &str, expected: &DownloadFile) -> Result<()> {
    let (size, hash) = fs::digest(&mut directory.read(name)?)?;
    if size != expected.size_bytes || hash != expected.sha256 {
        return Err(error("DOWNLOAD_OUTPUT_CHANGED"));
    }
    Ok(())
}
pub(crate) fn verify_output(record: &DownloadRecord) -> Result<()> {
    let root = require_root(record)?;
    let output = root.child(&record.destination)?;
    if Some(output.key()?) != record.output_identity {
        return Err(error("DOWNLOAD_OUTPUT_CHANGED"));
    }
    let mut wanted = BTreeMap::<String, BTreeSet<String>>::new();
    for file in &record.output_files {
        let (directory, name) = file
            .relative_path
            .rsplit_once('/')
            .unwrap_or(("", &file.relative_path));
        wanted
            .entry(directory.into())
            .or_default()
            .insert(name.into());
        if directory.is_empty() {
            verify_file(&output, name, file)?;
        } else {
            verify_file(&output.child(directory)?, name, file)?;
        }
    }
    let root_files = wanted.entry(String::new()).or_default();
    root_files.insert("_mangamonitor.json".into());
    let mut expected: BTreeSet<_> = root_files.iter().cloned().collect();
    expected.extend(wanted.keys().filter(|k| !k.is_empty()).cloned());
    if output.names()?.into_iter().collect::<BTreeSet<_>>() != expected {
        return Err(error("DOWNLOAD_OUTPUT_CHANGED"));
    }
    for (directory, files) in &wanted {
        if !directory.is_empty()
            && output
                .child(directory)?
                .names()?
                .into_iter()
                .collect::<BTreeSet<_>>()
                != *files
        {
            return Err(error("DOWNLOAD_OUTPUT_CHANGED"));
        }
    }
    let bytes = manifest(record)?;
    let proof = DownloadFile {
        relative_path: "_mangamonitor.json".into(),
        size_bytes: bytes.len() as u64,
        sha256: hash(&bytes),
    };
    if record.output_manifest_hash.as_ref() != Some(&proof.sha256) {
        return Err(error("DOWNLOAD_PROOF_INVALID"));
    }
    verify_file(&output, "_mangamonitor.json", &proof)
}
fn manifest(record: &DownloadRecord) -> Result<Vec<u8>> {
    json_bytes(
        &json!({"version":1,"origin":"manual","taskId":record.id,"approvalRevision":record.approval_revision,"targetHash":record.target_hash,"source":record.source,"workId":record.metadata.work_id,"rootId":record.root.id,"generation":record.generation,"layoutVersion":1,"files":record.output_files}),
    )
}

/// Continue only an exact verified prefix, through the same exclusively opened
/// file handle. Different content and oversized files are retained and refused.
fn complete_file(target: &mut std::fs::File, source: &mut impl Read, size: u64) -> Result<()> {
    let prefix = target
        .metadata()
        .map_err(|_| error("DOWNLOAD_READ_FAILED"))?
        .len();
    if prefix > size {
        return Err(error("DOWNLOAD_OUTPUT_CHANGED"));
    }
    let mut consumed = 0_u64;
    let mut buffer = [0_u8; 65536];
    let mut previous = [0_u8; 65536];
    loop {
        let n = source
            .read(&mut buffer)
            .map_err(|_| error("DOWNLOAD_READ_FAILED"))?;
        if n == 0 {
            break;
        }
        let existing = usize::try_from(prefix.saturating_sub(consumed).min(n as u64))
            .map_err(|_| error("DOWNLOAD_OUTPUT_CHANGED"))?;
        if existing > 0 {
            target
                .read_exact(&mut previous[..existing])
                .map_err(|_| error("DOWNLOAD_READ_FAILED"))?;
            if previous[..existing] != buffer[..existing] {
                return Err(error("DOWNLOAD_OUTPUT_CHANGED"));
            }
        }
        consumed += n as u64;
        if consumed > size {
            return Err(error("DOWNLOAD_STAGING_CHANGED"));
        }
        target
            .write_all(&buffer[existing..n])
            .map_err(|_| error("DOWNLOAD_WRITE_FAILED"))?;
    }
    if consumed != size {
        return Err(error("DOWNLOAD_STAGING_CHANGED"));
    }
    target
        .sync_all()
        .map_err(|_| error("DOWNLOAD_WRITE_FAILED"))
}

/// A complete byte manifest is persisted before the first destination write.
/// Resumption only reuses exact planned bytes in the directory whose OS identity
/// was reserved by this task. No existing work directory is adopted.
pub(crate) fn save(
    record: &mut DownloadRecord,
    report: &LocalExecutionReport,
    staging: &Path,
    require: &impl Fn() -> Result<()>,
    persist: &mut impl FnMut(&DownloadRecord) -> Result<()>,
) -> Result<()> {
    require()?;
    validate_staging(record, report, staging)?;
    if record.output_manifest_hash.is_some() {
        verify_output(record)?;
        require()?;
        return Ok(());
    }
    let stage = Directory::open(staging)?
        .child("commands")?
        .child(&report.command_id)?;
    let outputs = layout(record, report, &stage)?;
    let files: Vec<_> = outputs.iter().map(|v| v.file.clone()).collect();
    if !record.output_files.is_empty() && record.output_files != files {
        return Err(error("DOWNLOAD_LAYOUT_CHANGED"));
    }
    record.output_files = files;
    persist(record)?;
    require()?;
    let root = require_root(record)?;
    let output = if let Some(identity) = &record.output_identity {
        let output = root.child(&record.destination)?;
        if &output.key()? != identity {
            return Err(error("DOWNLOAD_OUTPUT_CHANGED"));
        }
        output
    } else {
        let output = root.create(&record.destination)?;
        record.output_identity = Some(output.key()?);
        persist(record)?;
        root.sync()?;
        output
    };
    let mut directories = BTreeMap::new();
    let mut names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut present: BTreeSet<_> = output.names()?.into_iter().collect();
    let mut allowed = BTreeSet::from(["_mangamonitor.json".to_string()]);
    for entry in &outputs {
        allowed.insert(
            entry
                .file
                .relative_path
                .split('/')
                .next()
                .ok_or(error("DOWNLOAD_PROOF_INVALID"))?
                .to_string(),
        );
    }
    if !present.is_subset(&allowed) {
        return Err(error("DOWNLOAD_OUTPUT_CHANGED"));
    }
    let marker = outputs
        .iter()
        .find(|v| v.file.relative_path == "_mangamonitor-layout.json")
        .ok_or(error("DOWNLOAD_PROOF_INVALID"))?;
    let marker_bytes = marker
        .bytes
        .as_ref()
        .ok_or(error("DOWNLOAD_PROOF_INVALID"))?;
    require()?;
    let mut marker_file = if present.contains("_mangamonitor-layout.json") {
        output.resume_file("_mangamonitor-layout.json")?
    } else {
        output.create_file("_mangamonitor-layout.json")?
    };
    complete_file(
        &mut marker_file,
        &mut Cursor::new(marker_bytes),
        marker.file.size_bytes,
    )?;
    drop(marker_file);
    verify_file(&output, "_mangamonitor-layout.json", &marker.file)?;
    output.sync()?;
    present.insert("_mangamonitor-layout.json".into());
    names.insert(String::new(), present.clone());
    for entry in &outputs {
        require()?;
        if entry.file.relative_path == "_mangamonitor-layout.json" {
            continue;
        }
        let (parent, name) = entry
            .file
            .relative_path
            .rsplit_once('/')
            .unwrap_or(("", &entry.file.relative_path));
        if !parent.is_empty() && !directories.contains_key(parent) {
            let directory = if present.contains(parent) {
                output.child(parent)?
            } else {
                let v = output.create(parent)?;
                present.insert(parent.into());
                v
            };
            let existing: BTreeSet<String> = directory.names()?.into_iter().collect();
            let expected: BTreeSet<String> = outputs
                .iter()
                .filter_map(|v| {
                    v.file
                        .relative_path
                        .split_once('/')
                        .filter(|(p, _)| *p == parent)
                        .map(|(_, n)| n.to_string())
                })
                .collect();
            if !existing.is_subset(&expected) {
                return Err(error("DOWNLOAD_OUTPUT_CHANGED"));
            }
            names.insert(parent.to_string(), existing);
            directories.insert(parent.to_owned(), directory);
        }
        let directory = if parent.is_empty() {
            &output
        } else {
            &directories[parent]
        };
        let existing = names
            .get_mut(parent)
            .ok_or(error("DOWNLOAD_PROOF_INVALID"))?;
        let mut target = if existing.contains(name) {
            directory.resume_file(name)?
        } else {
            directory.create_file(name)?
        };
        if let Some(bytes) = &entry.bytes {
            complete_file(&mut target, &mut Cursor::new(bytes), entry.file.size_bytes)?;
        } else {
            let mut source = source_file(
                &stage,
                entry
                    .staged
                    .as_deref()
                    .ok_or(error("DOWNLOAD_PROOF_INVALID"))?,
            )?;
            // One bounded file finishes before honoring a new control epoch.
            // Crash prefixes are resumed only after exact source-byte comparison.
            complete_file(&mut target, &mut source, entry.file.size_bytes)?;
        }
        drop(target);
        verify_file(directory, name, &entry.file)?;
        directory.sync()?;
        existing.insert(name.into());
        require()?;
    }
    require()?;
    let bytes = manifest(record)?;
    let mut final_file = if present.contains("_mangamonitor.json") {
        output.resume_file("_mangamonitor.json")?
    } else {
        output.create_file("_mangamonitor.json")?
    };
    complete_file(
        &mut final_file,
        &mut Cursor::new(&bytes),
        bytes.len() as u64,
    )?;
    drop(final_file);
    record.output_manifest_hash = Some(hash(&bytes));
    output.sync()?;
    verify_output(record)?;
    require()?;
    persist(record)
}

/// Desktop-only temporary-data lifecycle after durable PC registration. This
/// never receives a user-selected root as its removal root, never recurses, and
/// never removes paused/error staging or an unrecognized/changed file.
pub(crate) fn cleanup_completed(record: &DownloadRecord, staging: &Path) -> Result<()> {
    if record.phase != workbench_storage::DownloadPhase::Downloaded
        || record.library_entry_id.is_none()
    {
        return Err(error("DOWNLOAD_INDEX_REQUIRED"));
    }
    verify_output(record)?;
    let report: LocalExecutionReport = serde_json::from_str(
        record
            .staging_report_json
            .as_deref()
            .ok_or(error("DOWNLOAD_PROOF_INVALID"))?,
    )
    .map_err(|_| error("DOWNLOAD_PROOF_INVALID"))?;
    let (_, _, command) = adapter::current(record)?;
    let plan = local_executor::plan(&command).map_err(|_| error("DOWNLOAD_PROOF_INVALID"))?;
    cloud_monitor::staging_manifest::validate(&plan, &report.source_completion.manifest)
        .map_err(|_| error("DOWNLOAD_PROOF_INVALID"))?;
    let commands = Directory::open(staging)?.child("commands")?;
    if !commands.names()?.iter().any(|v| v == &command.command_id) {
        return Ok(());
    }
    let command_root = commands.child(&command.command_id)?;
    if command_root.names()? != vec!["chapters".to_string()] {
        return Err(error("DOWNLOAD_CLEANUP_INCOMPLETE"));
    }
    let chapters = command_root.child("chapters")?;
    let mut expected =
        BTreeMap::<String, Vec<&cloud_monitor::staging_manifest::StagedArtifact>>::new();
    for artifact in &report.source_completion.manifest.artifacts {
        let parts: Vec<_> = artifact.relative_path.split('/').collect();
        if parts.len() != 3 || parts[0] != "chapters" {
            return Err(error("DOWNLOAD_PROOF_INVALID"));
        }
        expected.entry(parts[1].into()).or_default().push(artifact);
    }
    let present = chapters.names()?;
    if present.iter().any(|name| !expected.contains_key(name)) {
        return Err(error("DOWNLOAD_CLEANUP_INCOMPLETE"));
    }
    for name in present {
        let chapter = chapters.child(&name)?;
        let artifacts = &expected[&name];
        let allowed: BTreeSet<_> = artifacts
            .iter()
            .filter_map(|a| a.relative_path.rsplit('/').next())
            .collect();
        let existing = chapter.names()?;
        if existing.iter().any(|n| !allowed.contains(n.as_str())) {
            return Err(error("DOWNLOAD_CLEANUP_INCOMPLETE"));
        }
        for artifact in artifacts {
            let filename = artifact
                .relative_path
                .rsplit('/')
                .next()
                .ok_or(error("DOWNLOAD_PROOF_INVALID"))?;
            if existing.iter().any(|n| n == filename) {
                chapter.remove_exact_file(filename, artifact.size_bytes, &artifact.sha256)?;
            }
        }
        drop(chapter);
        chapters.remove_empty_child(&name)?;
    }
    drop(chapters);
    command_root.remove_empty_child("chapters")?;
    drop(command_root);
    commands.remove_empty_child(&command.command_id)?;
    commands.sync()
}
