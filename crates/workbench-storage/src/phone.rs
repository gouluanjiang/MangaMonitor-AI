//! Phone presence is a user-maintained reference list, independent of PC files.
use crate::{
    library_hash_is_valid, model::ValidatedDocument, store::read_regular_bounded, Document,
    LibraryReference, Result, Source, StoreError, WorkbenchStore, MAX_SAFE_INTEGER,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::Path, time::SystemTime};
use unicode_normalization::UnicodeNormalization;

const MAX_NAMES: usize = 20_000;
const MAX_IMPORT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PhoneLibraryEntry {
    pub id: String,
    pub name: String,
    pub reference: Option<LibraryReference>,
    pub marked_at: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PhoneLibraryDocument {
    pub version: u32,
    pub imported_names: Vec<String>,
    pub imported_at: Option<u64>,
    pub import_file_name: Option<String>,
    pub manual_entries: Vec<PhoneLibraryEntry>,
}

impl Default for PhoneLibraryDocument {
    fn default() -> Self {
        Self {
            version: 1,
            imported_names: Vec::new(),
            imported_at: None,
            import_file_name: None,
            manual_entries: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneLibrarySnapshot {
    pub revision: u64,
    pub imported_names: Vec<String>,
    pub imported_at: Option<u64>,
    pub import_file_name: Option<String>,
    pub manual_entries: Vec<PhoneLibraryEntry>,
}

impl From<Document<PhoneLibraryDocument>> for PhoneLibrarySnapshot {
    fn from(document: Document<PhoneLibraryDocument>) -> Self {
        Self {
            revision: document.revision,
            imported_names: document.value.imported_names,
            imported_at: document.value.imported_at,
            import_file_name: document.value.import_file_name,
            manual_entries: document.value.manual_entries,
        }
    }
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value == value.trim()
        && value.chars().count() <= 1024
        && !value.chars().any(char::is_control)
}

fn valid_file_name(value: &str) -> bool {
    let drive_relative = value.as_bytes().get(1) == Some(&b':')
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic);
    valid_name(value)
        && !value.contains(['/', '\\'])
        && !drive_relative
        && value != "."
        && value != ".."
}

fn canonical_name(value: &str) -> String {
    let value = value.trim();
    let stem = value.rsplit_once('.').map_or(value, |(stem, extension)| {
        if ["zip", "cbz", "rar", "7z"]
            .iter()
            .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        {
            stem.trim()
        } else {
            value
        }
    });
    stem.nfc().collect()
}

fn entry_id(name: &str, reference: Option<&LibraryReference>) -> String {
    let mut digest = Sha256::new();
    digest.update(b"phone-library-v1\0");
    if let Some(reference) = reference {
        digest.update(match reference.source {
            Source::Jm => b"JM".as_slice(),
            Source::Pica => b"Pica".as_slice(),
        });
        digest.update(b"\0");
        digest.update(reference.work_id.as_bytes());
    } else {
        digest.update(b"name\0");
        digest.update(canonical_name(name).as_bytes());
    }
    format!("{:x}", digest.finalize())
}

impl ValidatedDocument for PhoneLibraryDocument {
    fn validate(&self) -> Result<()> {
        let invalid = || StoreError::new("VALIDATION_FAILED");
        if self.version != 1
            || self.imported_names.len() > MAX_NAMES
            || self.manual_entries.len() > MAX_NAMES
            || self
                .imported_at
                .is_some_and(|value| value > MAX_SAFE_INTEGER)
            || self.imported_at.is_some() != self.import_file_name.is_some()
            || self.imported_names.is_empty() == self.imported_at.is_some()
            || self
                .import_file_name
                .as_ref()
                .is_some_and(|name| !valid_file_name(name))
        {
            return Err(invalid());
        }
        let mut names = HashSet::new();
        for name in &self.imported_names {
            if !valid_file_name(name) || !names.insert(name) {
                return Err(invalid());
            }
        }
        let mut ids = HashSet::new();
        for entry in &self.manual_entries {
            if !library_hash_is_valid(&entry.id)
                || !ids.insert(&entry.id)
                || !valid_name(&entry.name)
                || entry.name != entry.name.nfc().collect::<String>()
                || entry.marked_at > MAX_SAFE_INTEGER
                || entry
                    .reference
                    .as_ref()
                    .is_some_and(|value| !value.is_valid())
                || entry.id != entry_id(&entry.name, entry.reference.as_ref())
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .filter(|value| *value <= MAX_SAFE_INTEGER)
        .ok_or(StoreError::new("PHONE_LIBRARY_UNAVAILABLE"))
}

fn current(store: &WorkbenchStore, revision: u64) -> Result<Document<PhoneLibraryDocument>> {
    let current = store.read_phone_library()?;
    if current.revision != revision {
        return Err(StoreError::new("REVISION_CONFLICT"));
    }
    Ok(current)
}

fn decode_names(bytes: &[u8]) -> Result<Vec<String>> {
    let invalid = || StoreError::new("PHONE_LIBRARY_INVALID_TXT");
    let text = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if bytes.len() % 2 != 0 {
            return Err(invalid());
        }
        let little_endian = bytes[0] == 0xff;
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| {
                if little_endian {
                    u16::from_le_bytes([chunk[0], chunk[1]])
                } else {
                    u16::from_be_bytes([chunk[0], chunk[1]])
                }
            })
            .collect();
        String::from_utf16(&units).map_err(|_| invalid())?
    } else {
        std::str::from_utf8(bytes)
            .map_err(|_| invalid())?
            .trim_start_matches('\u{feff}')
            .to_owned()
    };
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if !valid_file_name(line) {
            return Err(invalid());
        }
        let name: String = line.nfc().collect();
        if seen.insert(name.clone()) {
            names.push(name);
            if names.len() > MAX_NAMES {
                return Err(StoreError::new("PHONE_LIBRARY_LIMIT_EXCEEDED"));
            }
        }
    }
    if names.is_empty() {
        return Err(StoreError::new("PHONE_LIBRARY_EMPTY_TXT"));
    }
    Ok(names)
}

pub fn phone_library_read(store: &WorkbenchStore) -> Result<PhoneLibrarySnapshot> {
    Ok(store.read_phone_library()?.into())
}

/// Only the native TXT picker supplies this path. Original media and TXT stay untouched.
pub fn phone_library_from_path(
    store: &WorkbenchStore,
    path: &Path,
    revision: u64,
) -> Result<PhoneLibrarySnapshot> {
    let mut current = current(store, revision)?;
    if !path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("txt"))
    {
        return Err(StoreError::new("PHONE_LIBRARY_INVALID_TXT"));
    }
    let bytes = read_regular_bounded(path, MAX_IMPORT_BYTES).map_err(|error| match error.code {
        "DOCUMENT_TOO_LARGE" => StoreError::new("PHONE_LIBRARY_LIMIT_EXCEEDED"),
        "UNSAFE_PATH" => error,
        _ => StoreError::new("PHONE_LIBRARY_READ_FAILED"),
    })?;
    let names = decode_names(&bytes)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| valid_file_name(name))
        .ok_or(StoreError::new("PHONE_LIBRARY_INVALID_TXT"))?;
    current.value.imported_names = names;
    current.value.imported_at = Some(now()?);
    current.value.import_file_name = Some(file_name.to_owned());
    Ok(store.write_phone_library(revision, current.value)?.into())
}

pub fn phone_library_mark(
    store: &WorkbenchStore,
    revision: u64,
    name: String,
    mut reference: Option<LibraryReference>,
) -> Result<PhoneLibrarySnapshot> {
    let mut current = current(store, revision)?;
    let name: String = name.trim().nfc().collect();
    if let Some(reference) = &mut reference {
        if reference.source == Source::Pica {
            reference.work_id.make_ascii_lowercase();
        }
    }
    if !valid_name(&name) || reference.as_ref().is_some_and(|value| !value.is_valid()) {
        return Err(StoreError::new("VALIDATION_FAILED"));
    }
    let id = entry_id(&name, reference.as_ref());
    if current
        .value
        .manual_entries
        .iter()
        .any(|entry| entry.id == id)
    {
        return Ok(current.into());
    }
    if current.value.manual_entries.len() >= MAX_NAMES {
        return Err(StoreError::new("PHONE_LIBRARY_LIMIT_EXCEEDED"));
    }
    current.value.manual_entries.push(PhoneLibraryEntry {
        id,
        name,
        reference,
        marked_at: now()?,
    });
    Ok(store.write_phone_library(revision, current.value)?.into())
}

pub fn phone_library_unmark(
    store: &WorkbenchStore,
    revision: u64,
    entry_id: &str,
) -> Result<PhoneLibrarySnapshot> {
    if !library_hash_is_valid(entry_id) {
        return Err(StoreError::new("VALIDATION_FAILED"));
    }
    let mut current = current(store, revision)?;
    let count = current.value.manual_entries.len();
    current
        .value
        .manual_entries
        .retain(|entry| entry.id != entry_id);
    if current.value.manual_entries.len() == count {
        return Err(StoreError::new("PHONE_LIBRARY_ENTRY_NOT_FOUND"));
    }
    Ok(store.write_phone_library(revision, current.value)?.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn raw_multiple_suffixes_survive_restart_and_import_limits_keep_previous_data() {
        let fixture = tempfile::tempdir().unwrap();
        let store = WorkbenchStore::open(fixture.path().join("app")).unwrap();
        let marked = phone_library_mark(&store, 0, "合成.zip.rar".into(), None).unwrap();
        assert_eq!(marked.manual_entries[0].name, "合成.zip.rar");
        assert_eq!(canonical_name(&marked.manual_entries[0].name), "合成.zip");
        let reopened = WorkbenchStore::open(fixture.path().join("app")).unwrap();
        assert_eq!(phone_library_read(&reopened).unwrap(), marked);
        let txt = fixture.path().join("phone.txt");
        let excessive = (0..=MAX_NAMES)
            .map(|n| format!("Synthetic {n}.zip\n"))
            .collect::<String>();
        fs::write(&txt, excessive).unwrap();
        assert_eq!(
            phone_library_from_path(&store, &txt, marked.revision)
                .unwrap_err()
                .code,
            "PHONE_LIBRARY_LIMIT_EXCEEDED"
        );
        fs::write(&txt, vec![b'x'; MAX_IMPORT_BYTES + 1]).unwrap();
        assert_eq!(
            phone_library_from_path(&store, &txt, marked.revision)
                .unwrap_err()
                .code,
            "PHONE_LIBRARY_LIMIT_EXCEEDED"
        );
        fs::write(&txt, "C:comic.zip").unwrap();
        assert_eq!(
            phone_library_from_path(&store, &txt, marked.revision)
                .unwrap_err()
                .code,
            "PHONE_LIBRARY_INVALID_TXT"
        );
        assert_eq!(phone_library_read(&store).unwrap(), marked);
    }

    #[test]
    fn accepts_bom_encodings_and_normalizes_without_losing_version_names() {
        let source =
            "[作者] カ\u{3099}イド [修正版].zip\r\n[作者] ガイド [修正版].zip\r\n作品.rar\r\n";
        let names = decode_names(source.as_bytes()).unwrap();
        assert_eq!(names, ["[作者] ガイド [修正版].zip", "作品.rar"]);
        for little_endian in [true, false] {
            let mut bytes = if little_endian {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for unit in source.encode_utf16() {
                bytes.extend(if little_endian {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            assert_eq!(decode_names(&bytes).unwrap(), names);
        }
        assert_ne!(
            canonical_name("作品 [修正版].zip"),
            canonical_name("作品.zip")
        );
    }

    #[test]
    fn marks_reimports_and_restart_preserve_pc_files_and_separate_evidence() {
        let fixture = tempfile::tempdir().unwrap();
        let app = fixture.path().join("app");
        let media = fixture.path().join("comic.zip");
        let txt = fixture.path().join("phone.txt");
        fs::write(&media, b"existing comic bytes").unwrap();
        fs::write(&txt, "原有作品.zip\n").unwrap();
        let store = WorkbenchStore::open(&app).unwrap();
        let imported = phone_library_from_path(&store, &txt, 0).unwrap();
        let marked =
            phone_library_mark(&store, imported.revision, "新作.zip".into(), None).unwrap();
        let duplicate = phone_library_mark(&store, marked.revision, "新作".into(), None).unwrap();
        assert_eq!(duplicate.revision, marked.revision);
        fs::write(&txt, "最新作品.rar\n").unwrap();
        let refreshed = phone_library_from_path(&store, &txt, marked.revision).unwrap();
        assert_eq!(refreshed.imported_names, ["最新作品.rar"]);
        assert_eq!(refreshed.manual_entries, marked.manual_entries);
        drop(store);
        let store = WorkbenchStore::open(&app).unwrap();
        assert_eq!(phone_library_read(&store).unwrap(), refreshed);
        let removed =
            phone_library_unmark(&store, refreshed.revision, &marked.manual_entries[0].id).unwrap();
        assert!(removed.manual_entries.is_empty());
        assert_eq!(removed.imported_names, ["最新作品.rar"]);
        assert_eq!(fs::read(&media).unwrap(), b"existing comic bytes");
        assert_eq!(fs::read_to_string(&txt).unwrap(), "最新作品.rar\n");
    }

    #[test]
    fn invalid_import_and_stale_revision_leave_previous_inventory_unchanged() {
        let fixture = tempfile::tempdir().unwrap();
        let txt = fixture.path().join("phone.txt");
        let store = WorkbenchStore::open(fixture.path().join("app")).unwrap();
        fs::write(&txt, "原作品.zip\n").unwrap();
        let initial = phone_library_from_path(&store, &txt, 0).unwrap();
        for invalid in [
            b"".as_slice(),
            b"C:\\private\\comic.zip",
            b"../comic.zip",
            &[0xff, 0xfe, 0x40],
            &[0x81, 0x40],
        ] {
            fs::write(&txt, invalid).unwrap();
            assert!(phone_library_from_path(&store, &txt, initial.revision).is_err());
            assert_eq!(phone_library_read(&store).unwrap(), initial);
        }
        assert_eq!(
            phone_library_mark(&store, 0, "new".into(), None)
                .unwrap_err()
                .code,
            "REVISION_CONFLICT"
        );
        assert_eq!(phone_library_read(&store).unwrap(), initial);
    }

    #[test]
    fn manual_source_ids_are_scoped_and_name_only_evidence_stays_separate() {
        let fixture = tempfile::tempdir().unwrap();
        let store = WorkbenchStore::open(fixture.path()).unwrap();
        let name_only = phone_library_mark(&store, 0, "同名".into(), None).unwrap();
        let jm = phone_library_mark(
            &store,
            name_only.revision,
            "同名".into(),
            Some(LibraryReference {
                source: Source::Jm,
                work_id: "123".into(),
            }),
        )
        .unwrap();
        assert_eq!(jm.manual_entries.len(), 2);
        let pica = phone_library_mark(
            &store,
            jm.revision,
            "同名".into(),
            Some(LibraryReference {
                source: Source::Pica,
                work_id: "ABCDEFABCDEFABCDEFABCDEF".into(),
            }),
        )
        .unwrap();
        assert_eq!(pica.manual_entries.len(), 3);
        assert_eq!(
            pica.manual_entries[2].reference.as_ref().unwrap().work_id,
            "abcdefabcdefabcdefabcdef"
        );
        assert_eq!(
            phone_library_mark(
                &store,
                pica.revision,
                "invalid".into(),
                Some(LibraryReference {
                    source: Source::Jm,
                    work_id: "../123".into()
                })
            )
            .unwrap_err()
            .code,
            "VALIDATION_FAILED"
        );
    }
}
