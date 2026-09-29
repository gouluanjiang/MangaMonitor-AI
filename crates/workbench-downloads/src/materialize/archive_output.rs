//! ZIP is a final container for the already verified layout, not new media
//! acquisition. Temporary archive bytes are disposable; resumable media stays
//! in the task's existing isolated staging until durable PC registration.
use super::*;
use sha2::Digest;
use std::io::{Seek, SeekFrom};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

fn archive_proof(record: &DownloadRecord) -> Result<&DownloadFile> {
    record
        .archive_file
        .as_ref()
        .filter(|v| v.relative_path == record.destination && v.size_bytes > 0)
        .ok_or(error("DOWNLOAD_PROOF_INVALID"))
}

pub(super) fn verify(record: &DownloadRecord) -> Result<()> {
    let root = require_root(record)?;
    let mut file = root.read(&record.destination)?;
    if record.output_identity.as_ref() != Some(&fs::file_key(&file)?) {
        return Err(error("DOWNLOAD_OUTPUT_CHANGED"));
    }
    let proof = archive_proof(record)?;
    if fs::digest(&mut file)? != (proof.size_bytes, proof.sha256.clone()) {
        return Err(error("DOWNLOAD_OUTPUT_CHANGED"));
    }
    let manifest_bytes = manifest(record)?;
    if record.output_manifest_hash.as_ref() != Some(&hash(&manifest_bytes)) {
        return Err(error("DOWNLOAD_PROOF_INVALID"));
    }
    // The whole ZIP hash was checked before opening its central directory.
    // Its bytes originate in this task's fixed layout, not an arbitrary archive.
    file.rewind().map_err(|_| error("DOWNLOAD_READ_FAILED"))?;
    let mut archive = ZipArchive::new(file).map_err(|_| error("DOWNLOAD_OUTPUT_CHANGED"))?;
    if archive.len() != record.output_files.len() + 1 {
        return Err(error("DOWNLOAD_OUTPUT_CHANGED"));
    }
    let manifest_file = DownloadFile {
        relative_path: "_mangamonitor.json".into(),
        size_bytes: manifest_bytes.len() as u64,
        sha256: hash(&manifest_bytes),
    };
    for expected in record
        .output_files
        .iter()
        .chain(std::iter::once(&manifest_file))
    {
        let mut entry = archive
            .by_name(&expected.relative_path)
            .map_err(|_| error("DOWNLOAD_OUTPUT_CHANGED"))?;
        if entry.is_dir() || entry.is_symlink() || entry.size() != expected.size_bytes {
            return Err(error("DOWNLOAD_OUTPUT_CHANGED"));
        }
        let mut digest = sha2::Sha256::new();
        let mut count = 0_u64;
        let mut buffer = [0_u8; 65536];
        loop {
            let n = entry
                .read(&mut buffer)
                .map_err(|_| error("DOWNLOAD_OUTPUT_CHANGED"))?;
            if n == 0 {
                break;
            }
            count += n as u64;
            if count > expected.size_bytes {
                return Err(error("DOWNLOAD_OUTPUT_CHANGED"));
            }
            digest.update(&buffer[..n]);
        }
        if count != expected.size_bytes || format!("{:x}", digest.finalize()) != expected.sha256 {
            return Err(error("DOWNLOAD_OUTPUT_CHANGED"));
        }
    }
    require_root(record)?;
    Ok(())
}

pub(super) fn save(
    record: &mut DownloadRecord,
    report: &LocalExecutionReport,
    staging: &Path,
    require: &impl Fn() -> Result<()>,
    persist: &mut impl FnMut(&DownloadRecord) -> Result<()>,
) -> Result<()> {
    require()?;
    validate_staging(record, report, staging)?;
    if record.output_manifest_hash.is_some() {
        verify(record)?;
        return require();
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
    let manifest_bytes = manifest(record)?;
    // Stored entries avoid recompressing already compressed JPG/WebP/PNG/GIF.
    // Bound the resulting classic ZIP to the library's supported container size.
    let maximum = outputs
        .iter()
        .try_fold(manifest_bytes.len() as u64 + 22, |n, output| {
            n.checked_add(output.file.size_bytes + 256 + output.file.relative_path.len() as u64 * 2)
        })
        .ok_or(error("DOWNLOAD_ARCHIVE_LIMIT"))?;
    if maximum >= u32::MAX as u64 {
        return Err(error("DOWNLOAD_ARCHIVE_LIMIT"));
    }
    let mut scratch = tempfile::tempfile_in(staging).map_err(|_| error("DOWNLOAD_WRITE_FAILED"))?;
    {
        let mut writer = ZipWriter::new(&mut scratch);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .last_modified_time(zip::DateTime::default())
            .unix_permissions(0o644);
        for output in &outputs {
            require()?;
            writer
                .start_file(&output.file.relative_path, options)
                .map_err(|_| error("DOWNLOAD_WRITE_FAILED"))?;
            if let Some(bytes) = &output.bytes {
                writer
                    .write_all(bytes)
                    .map_err(|_| error("DOWNLOAD_WRITE_FAILED"))?;
            } else {
                let mut source = source_file(
                    &stage,
                    output
                        .staged
                        .as_deref()
                        .ok_or(error("DOWNLOAD_PROOF_INVALID"))?,
                )?;
                let copied = std::io::copy(&mut source, &mut writer)
                    .map_err(|_| error("DOWNLOAD_WRITE_FAILED"))?;
                if copied != output.file.size_bytes {
                    return Err(error("DOWNLOAD_STAGING_CHANGED"));
                }
            }
        }
        writer
            .start_file("_mangamonitor.json", options)
            .map_err(|_| error("DOWNLOAD_WRITE_FAILED"))?;
        writer
            .write_all(&manifest_bytes)
            .map_err(|_| error("DOWNLOAD_WRITE_FAILED"))?;
        writer
            .finish()
            .map_err(|_| error("DOWNLOAD_WRITE_FAILED"))?;
    }
    scratch
        .rewind()
        .map_err(|_| error("DOWNLOAD_READ_FAILED"))?;
    let (size_bytes, sha256) = fs::digest(&mut scratch)?;
    let archive_file = DownloadFile {
        relative_path: record.destination.clone(),
        size_bytes,
        sha256,
    };
    if record
        .archive_file
        .as_ref()
        .is_some_and(|old| old != &archive_file)
    {
        return Err(error("DOWNLOAD_LAYOUT_CHANGED"));
    }
    record.archive_file = Some(archive_file);
    persist(record)?;
    require()?;
    let root = require_root(record)?;
    let mut target = if let Some(expected) = &record.output_identity {
        let target = root.resume_file(&record.destination)?;
        if &fs::file_key(&target)? != expected {
            return Err(error("DOWNLOAD_OUTPUT_CHANGED"));
        }
        target
    } else {
        let target = root.create_file(&record.destination)?;
        record.output_identity = Some(fs::file_key(&target)?);
        persist(record)?;
        root.sync()?;
        target
    };
    require()?;
    scratch
        .seek(SeekFrom::Start(0))
        .map_err(|_| error("DOWNLOAD_READ_FAILED"))?;
    complete_file(&mut target, &mut scratch, size_bytes)?;
    drop(target);
    record.output_manifest_hash = Some(hash(&manifest_bytes));
    root.sync()?;
    verify(record)?;
    require()?;
    persist(record)
}
