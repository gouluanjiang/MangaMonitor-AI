//! Explicit abandoned-task housekeeping. Never opens the final library root.
use super::*;
use cloud_monitor::isolated_staging_execution::StagingCheckpoint;

pub(crate) fn cleanup_abandoned(record: &DownloadRecord, staging: &Path) -> Result<()> {
    if record.phase != workbench_storage::DownloadPhase::Abandoned || record.library_entry_id.is_some() {
        return Err(error("DOWNLOAD_CONTROL_INVALID"));
    }
    let (_, _, command) = adapter::current(record)?;
    let commands = Directory::open(staging)?.child("commands")?;
    if commands.probe(&command.command_id)?.is_none() { return Ok(()); }
    let task_root = commands.child(&command.command_id)?;
    let names = task_root.names()?;
    if names.is_empty() {
        drop(task_root);
        return commands.remove_empty_child(&command.command_id);
    }
    if names != vec!["chapters".to_owned()] {
        return Err(error("DOWNLOAD_CLEANUP_REVIEW_REQUIRED"));
    }
    let mut wanted = BTreeMap::new();
    let mut pending_path = None;
    let artifacts = if let Some(json) = &record.staging_report_json {
        let report: LocalExecutionReport = serde_json::from_str(json).map_err(|_| error("DOWNLOAD_PROOF_INVALID"))?;
        let plan = local_executor::plan(&command).map_err(|_| error("DOWNLOAD_PROOF_INVALID"))?;
        cloud_monitor::staging_manifest::validate(&plan, &report.source_completion.manifest)
            .map_err(|_| error("DOWNLOAD_PROOF_INVALID"))?;
        report.source_completion.manifest.artifacts
    } else if let Some(json) = &record.checkpoint_json {
        let checkpoint: StagingCheckpoint = serde_json::from_str(json).map_err(|_| error("DOWNLOAD_PROOF_INVALID"))?;
        if checkpoint.expected_files == 0 || checkpoint.expected_files > MAX_DOWNLOAD_FILES as u64
            || checkpoint.artifacts.len() > MAX_DOWNLOAD_FILES {
            return Err(error("DOWNLOAD_PROOF_INVALID"));
        }
        let mut artifacts = checkpoint.artifacts;
        if let Some(pending) = checkpoint.pending {
            pending_path = Some(pending.relative_path.clone());
            artifacts.push(pending);
        }
        artifacts
    } else { Vec::new() };
    for artifact in artifacts {
        let parts: Vec<_> = artifact.relative_path.split('/').collect();
        if parts.len() != 3 || parts[0] != "chapters"
            || !workbench_storage::library_relative_path_is_valid(&artifact.relative_path)
            || !workbench_storage::library_hash_is_valid(&artifact.sha256)
            || artifact.size_bytes == 0 || wanted.len() >= MAX_DOWNLOAD_FILES
            || wanted.insert(artifact.relative_path.clone(), artifact).is_some() {
            return Err(error("DOWNLOAD_PROOF_INVALID"));
        }
    }
    let allowed_chapters: BTreeSet<_> = wanted.keys().filter_map(|path| path.split('/').nth(1).map(str::to_owned)).collect();
    let chapters = task_root.child("chapters")?;
    let chapter_names = chapters.names()?;
    if chapter_names.len() > MAX_DOWNLOAD_FILES { return Err(error("DOWNLOAD_CLEANUP_REVIEW_REQUIRED")); }
    let mut removals = Vec::new();
    // Validate the entire tree before the first deletion. Unknown files, links,
    // directories and changed completed artifacts preserve the managed record.
    for chapter_name in &chapter_names {
        if !allowed_chapters.contains(chapter_name) { return Err(error("DOWNLOAD_CLEANUP_REVIEW_REQUIRED")); }
        let chapter = chapters.child(chapter_name)?;
        for filename in chapter.names()? {
            let relative = format!("chapters/{chapter_name}/{filename}");
            let expected = wanted.get(&relative).ok_or(error("DOWNLOAD_CLEANUP_REVIEW_REQUIRED"))?;
            let (size, digest) = fs::digest(&mut chapter.read(&filename)?)?;
            let is_pending = pending_path.as_ref() == Some(&relative);
            if (!is_pending && (size != expected.size_bytes || digest != expected.sha256))
                || (is_pending && (size > expected.size_bytes || (size == expected.size_bytes && digest != expected.sha256))) {
                return Err(error("DOWNLOAD_CLEANUP_REVIEW_REQUIRED"));
            }
            // A pending partial prefix has no complete-file proof. Explicit
            // cleanup authorizes only this recorded staging path; bind its
            // observed bytes again at deletion, never infer a completed file.
            removals.push((chapter_name.clone(), filename, size, digest));
        }
    }
    for (chapter, name, size, digest) in removals {
        chapters.child(&chapter)?.remove_exact_file(&name, size, &digest)?;
    }
    for name in chapter_names { chapters.remove_empty_child(&name)?; }
    drop(chapters);
    task_root.remove_empty_child("chapters")?;
    drop(task_root);
    commands.remove_empty_child(&command.command_id)?;
    commands.sync()
}
