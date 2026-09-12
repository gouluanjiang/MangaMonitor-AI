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

fn check_inner(record: &DownloadRecord) -> Result<LocalFiles> {
    let root = materialize::require_root(record)?;
    let result = match root.probe(&record.destination)? {
        None => LocalFiles::Missing,
        Some(EntryKind::Directory) => {
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
