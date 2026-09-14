//! Advisory local-file presence, never completion proof or write authority.
use crate::{
    error,
    fs::{Directory, EntryKind},
    materialize, Result,
};
use serde::Serialize;
use workbench_storage::{library_relative_path_is_valid, DownloadRecord};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LocalFiles {
    Present,
    Missing,
    Incomplete,
    Unavailable,
}

/// Walk only this recorded relative path, without enumerating sibling works.
/// A missing intermediate name also means the recorded path is absent. An
/// existing non-directory parent or a redirected component is never absence.
pub(crate) fn probe_path(root: &Directory, relative: &str) -> Result<Option<EntryKind>> {
    if !library_relative_path_is_valid(relative) {
        return Err(error("DOWNLOAD_UNSAFE_PATH"));
    }
    let mut parts = relative.split('/').peekable();
    let mut parent = None;
    while let Some(part) = parts.next() {
        let directory = parent.as_ref().unwrap_or(root);
        let found = directory.probe(part)?;
        if parts.peek().is_none() || found.is_none() {
            return Ok(found);
        }
        if found != Some(EntryKind::Directory) {
            return Ok(Some(EntryKind::Other));
        }
        parent = Some(directory.child(part)?);
    }
    Err(error("DOWNLOAD_UNSAFE_PATH"))
}

pub(crate) fn check(record: &DownloadRecord) -> LocalFiles {
    check_inner(record).unwrap_or(LocalFiles::Unavailable)
}

/// Clearing queue history retains a receipt, not a manga identity relation.
/// The recorded path and current file identity are the only library evidence
/// used here. Titles, source metadata and old manual links are irrelevant.
pub(crate) fn check_history(
    receipt: &workbench_storage::DownloadHistoryEvidence,
    relocated: Option<&workbench_storage::LibraryRelocation>,
    record: Option<&workbench_storage::LibraryRecord>,
) -> LocalFiles {
    let result = (|| -> Result<LocalFiles> {
        let open_root = || -> Result<Directory> {
            let root = Directory::open(std::path::Path::new(&receipt.root.path))?;
            if root.key()? != receipt.root.file_key {
                return Err(error("DOWNLOAD_ROOT_CHANGED"));
            }
            Ok(root)
        };
        let root = open_root()?;
        let path = relocated.map_or(receipt.destination.as_str(), |v| &v.new_path);
        let Some(kind) = probe_path(&root, path)? else {
            open_root()?;
            return Ok(LocalFiles::Missing);
        };
        let Some(record) = record else {
            return Ok(LocalFiles::Unavailable);
        };
        let Some(expected) = relocated.map(|v| &v.identity).or(record.identity.as_ref()) else {
            return Ok(LocalFiles::Unavailable);
        };
        if record.item.relative_path != path
            || record.item.state != workbench_storage::LibraryItemState::Indexed
            || record.item.error_code.is_some()
            || record.item.page_count.is_none_or(|pages| pages == 0)
        {
            return Ok(LocalFiles::Incomplete);
        }
        let mut parts = path.split('/').peekable();
        let mut parent = None;
        let mut leaf = "";
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                leaf = part;
            } else {
                parent = Some(parent.as_ref().unwrap_or(&root).child(part)?);
            }
        }
        let directory = parent.as_ref().unwrap_or(&root);
        let file = match kind {
            EntryKind::File(bytes) if bytes == expected.bytes && bytes > 0 => {
                directory.read(leaf)?
            }
            EntryKind::Directory => directory.child(leaf)?.file,
            _ => return Ok(LocalFiles::Incomplete),
        };
        let metadata = file.metadata().map_err(|_| error("DOWNLOAD_READ_FAILED"))?;
        #[cfg(windows)]
        let modified = {
            use std::os::windows::fs::MetadataExt;
            metadata.last_write_time().to_string()
        };
        #[cfg(unix)]
        let modified = {
            use std::os::unix::fs::MetadataExt;
            format!("{}:{}", metadata.mtime(), metadata.mtime_nsec())
        };
        let state =
            if crate::fs::file_key(&file)? == expected.file_key && modified == expected.modified {
                LocalFiles::Present
            } else {
                LocalFiles::Incomplete
            };
        open_root()?;
        Ok(state)
    })();
    result.unwrap_or(LocalFiles::Unavailable)
}

pub(crate) fn relocation<'a>(
    record: &DownloadRecord,
    library: &'a workbench_storage::LibraryDocument,
) -> Option<&'a workbench_storage::LibraryRelocation> {
    (record.phase == workbench_storage::DownloadPhase::Downloaded
        && library.root.as_ref() == Some(&record.root))
    .then(|| library.relocated_path(&record.destination))
    .flatten()
}

pub(crate) fn check_with_library(
    record: &DownloadRecord,
    library: &workbench_storage::LibraryDocument,
) -> LocalFiles {
    let Some(relocated) = relocation(record, library) else {
        return check(record);
    };
    let result = (|| -> Result<LocalFiles> {
        let root = materialize::require_root(record)?;
        let state = match root.probe(&relocated.new_path)? {
            None => LocalFiles::Missing,
            Some(EntryKind::File(size)) if size == relocated.identity.bytes => {
                let file = root.read(&relocated.new_path)?;
                let metadata = file.metadata().map_err(|_| error("DOWNLOAD_READ_FAILED"))?;
                #[cfg(windows)]
                let modified = {
                    use std::os::windows::fs::MetadataExt;
                    metadata.last_write_time().to_string()
                };
                #[cfg(unix)]
                let modified = {
                    use std::os::unix::fs::MetadataExt;
                    format!("{}:{}", metadata.mtime(), metadata.mtime_nsec())
                };
                if crate::fs::file_key(&file)? == relocated.identity.file_key
                    && modified == relocated.identity.modified
                {
                    LocalFiles::Present
                } else {
                    LocalFiles::Incomplete
                }
            }
            Some(_) => LocalFiles::Incomplete,
        };
        materialize::require_root(record)?;
        Ok(state)
    })();
    result.unwrap_or(LocalFiles::Unavailable)
}

fn check_inner(record: &DownloadRecord) -> Result<LocalFiles> {
    let root = materialize::require_root(record)?;
    let result = match root.probe(&record.destination)? {
        None => LocalFiles::Missing,
        Some(EntryKind::File(bytes)) if record.zip_output => {
            let file = root.read(&record.destination)?;
            if record
                .archive_file
                .as_ref()
                .is_some_and(|v| v.size_bytes == bytes)
                && record.output_manifest_hash.is_some()
                && record.output_identity.as_ref() == Some(&crate::fs::file_key(&file)?)
            {
                LocalFiles::Present
            } else {
                LocalFiles::Incomplete
            }
        }
        Some(EntryKind::Directory) => {
            if record.zip_output {
                return Ok(LocalFiles::Incomplete);
            }
            let output = root.child(&record.destination)?;
            if Some(output.key()?) != record.output_identity || record.output_files.is_empty() {
                LocalFiles::Incomplete
            } else {
                let mut result = LocalFiles::Present;
                for file in &record.output_files {
                    if probe_path(&output, &file.relative_path)?
                        != Some(EntryKind::File(file.size_bytes))
                    {
                        result = LocalFiles::Incomplete;
                        break;
                    }
                }
                // The final manifest is not part of output_files. Presence is
                // deliberately not a rehash or a repeated full proof check.
                if !matches!(output.probe("_mangamonitor.json")?, Some(EntryKind::File(n)) if n > 0)
                {
                    result = LocalFiles::Incomplete;
                }
                // Reopen the named output after the bounded stat pass so a
                // concurrently replaced directory cannot appear unchanged.
                if root.child(&record.destination)?.key()? != output.key()? {
                    result = LocalFiles::Incomplete;
                }
                result
            }
        }
        Some(_) => LocalFiles::Incomplete,
    };
    // An unlinked Unix root handle can still answer lookups. It does not prove
    // that the selected root remains available at its original location.
    materialize::require_root(record)?;
    Ok(result)
}
