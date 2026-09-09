use crate::{
    model::ValidatedDocument, Booklists, Result, StoreError, WorkbenchPreferences, MAX_SAFE_INTEGER,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, Metadata, OpenOptions, TryLockError},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

pub const PRIVATE_DIRECTORY: &str = "workbench-preview-v1";
const PREFERENCES: &str = "preferences.json";
const BOOKLISTS: &str = "booklists.json";
const MAX_PREFERENCES_BYTES: usize = 12 * 1024 * 1024;
const MAX_BOOKLISTS_BYTES: usize = 5 * 1024 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Document<T> {
    pub revision: u64,
    pub value: T,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u32,
    revision: u64,
    value: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchemaProbe {
    schema_version: u64,
    value: ValueVersion,
}

#[derive(Deserialize)]
struct ValueVersion {
    version: u64,
}

/// The root is selected by the application, never by a renderer command argument.
pub struct WorkbenchStore {
    root: PathBuf,
    local_lock: Mutex<()>,
    // Windows handles keep ancestors from being renamed/replaced while the store is open.
    _directory_handles: Vec<File>,
}

impl WorkbenchStore {
    pub fn open(app_data_root: impl AsRef<Path>) -> Result<Self> {
        let root = app_data_root.as_ref().join(PRIVATE_DIRECTORY);
        let directory_handles = ensure_directory_tree(&root)?;
        Ok(Self {
            root,
            local_lock: Mutex::new(()),
            _directory_handles: directory_handles,
        })
    }

    pub fn read_preferences(&self) -> Result<Document<WorkbenchPreferences>> {
        self.read(PREFERENCES, MAX_PREFERENCES_BYTES)
    }

    pub fn write_preferences(
        &self,
        expected_revision: u64,
        value: WorkbenchPreferences,
    ) -> Result<Document<WorkbenchPreferences>> {
        self.write(PREFERENCES, MAX_PREFERENCES_BYTES, expected_revision, value)
    }

    pub fn read_booklists(&self) -> Result<Document<Booklists>> {
        self.read(BOOKLISTS, MAX_BOOKLISTS_BYTES)
    }

    pub fn write_booklists(
        &self,
        expected_revision: u64,
        value: Booklists,
    ) -> Result<Document<Booklists>> {
        self.write(BOOKLISTS, MAX_BOOKLISTS_BYTES, expected_revision, value)
    }

    fn read<T: ValidatedDocument>(&self, name: &str, maximum: usize) -> Result<Document<T>> {
        let _local = self
            .local_lock
            .lock()
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        let _file = self.acquire_lock()?;
        self.read_unlocked(name, maximum)
    }

    fn write<T: ValidatedDocument>(
        &self,
        name: &str,
        maximum: usize,
        expected_revision: u64,
        value: T,
    ) -> Result<Document<T>> {
        if expected_revision > MAX_SAFE_INTEGER {
            return Err(StoreError::new("VALIDATION_FAILED"));
        }
        value.validate()?;
        let _local = self
            .local_lock
            .lock()
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        let _file = self.acquire_lock()?;
        // A write never substitutes defaults for an unreadable or unsupported document.
        let current: Document<T> = self.read_unlocked(name, maximum)?;
        if current.revision != expected_revision {
            return Err(StoreError::new("REVISION_CONFLICT"));
        }
        let revision = current
            .revision
            .checked_add(1)
            .filter(|revision| *revision <= MAX_SAFE_INTEGER)
            .ok_or(StoreError::new("REVISION_EXHAUSTED"))?;
        let envelope = Envelope {
            schema_version: 1,
            revision,
            value,
        };
        let bytes =
            serde_json::to_vec(&envelope).map_err(|_| StoreError::new("VALIDATION_FAILED"))?;
        if bytes.len() > maximum {
            return Err(StoreError::new("DOCUMENT_TOO_LARGE"));
        }
        self.atomic_replace(name, &bytes)?;
        Ok(Document {
            revision,
            value: envelope.value,
        })
    }

    fn acquire_lock(&self) -> Result<File> {
        check_directory_tree(&self.root)?;
        let path = self.root.join(".workbench.lock");
        check_optional_regular(&path)?;
        let mut options = safe_options();
        options.read(true).write(true).create(true).truncate(false);
        let file = options
            .open(&path)
            .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
        check_open_regular(&file)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(StoreError::new("BUSY")),
            Err(TryLockError::Error(_)) => return Err(StoreError::new("STORE_UNAVAILABLE")),
        }
        // This file is deliberately retained. The OS releases its lock on process exit.
        check_directory_tree(&self.root)?;
        check_optional_regular(&path)?;
        Ok(file)
    }

    fn read_unlocked<T: ValidatedDocument>(
        &self,
        name: &str,
        maximum: usize,
    ) -> Result<Document<T>> {
        check_directory_tree(&self.root)?;
        let path = self.root.join(name);
        if check_optional_regular(&path)?.is_none() {
            return Ok(Document {
                revision: 0,
                value: T::default(),
            });
        }
        let bytes = read_regular_bounded(&path, maximum)?;
        let probe: SchemaProbe =
            serde_json::from_slice(&bytes).map_err(|_| StoreError::new("DOCUMENT_CORRUPT"))?;
        if probe.schema_version > 1 || probe.value.version > 1 {
            return Err(StoreError::new("UNSUPPORTED_SCHEMA"));
        }
        let envelope: Envelope<T> =
            serde_json::from_slice(&bytes).map_err(|_| StoreError::new("DOCUMENT_CORRUPT"))?;
        if envelope.schema_version != 1
            || envelope.revision == 0
            || envelope.revision > MAX_SAFE_INTEGER
            || envelope.value.validate().is_err()
        {
            return Err(StoreError::new("DOCUMENT_CORRUPT"));
        }
        Ok(Document {
            revision: envelope.revision,
            value: envelope.value,
        })
    }

    fn atomic_replace(&self, name: &str, bytes: &[u8]) -> Result<()> {
        check_directory_tree(&self.root)?;
        let destination = self.root.join(name);
        check_optional_regular(&destination)?;
        let (temporary_path, mut temporary) = self.create_temporary(name)?;
        let preparation = (|| {
            temporary
                .write_all(bytes)
                .and_then(|()| temporary.sync_all())
                .map_err(|_| StoreError::new("STORE_WRITE_FAILED"))?;
            check_directory_tree(&self.root)?;
            check_optional_regular(&destination)?;
            check_optional_regular(&temporary_path)?;
            Ok(())
        })();
        // Closing before rename also works on Windows. No old destination is removed first.
        drop(temporary);
        preparation?;
        fs::rename(&temporary_path, &destination)
            .map_err(|_| StoreError::new("STORE_WRITE_FAILED"))?;
        #[cfg(unix)]
        File::open(&self.root)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| StoreError::new("COMMIT_UNCERTAIN"))?;
        Ok(())
    }

    fn create_temporary(&self, name: &str) -> Result<(PathBuf, File)> {
        for _ in 0..16 {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = self
                .root
                .join(format!(".{name}.{}.{sequence}.tmp", std::process::id()));
            let mut options = safe_options();
            options.write(true).create_new(true);
            match options.open(&path) {
                Ok(file) => {
                    check_open_regular(&file)?;
                    return Ok((path, file));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err(StoreError::new("STORE_WRITE_FAILED")),
            }
        }
        Err(StoreError::new("STORE_WRITE_FAILED"))
    }
}

fn redirected(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // FILE_ATTRIBUTE_REPARSE_POINT includes junctions as well as symbolic links.
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

fn safe_options() -> OpenOptions {
    #[allow(unused_mut)]
    let mut options = OpenOptions::new();
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Open the reparse point itself and deny deletion/replacement while held.
        options.custom_flags(0x0020_0000).share_mode(0x1 | 0x2);
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20_000).mode(0o600); // O_NOFOLLOW
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x100).mode(0o600); // O_NOFOLLOW
    }
    options
}

fn check_path_form(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(StoreError::new("UNSAFE_PATH"));
    }
    Ok(())
}

fn ensure_directory_tree(path: &Path) -> Result<Vec<File>> {
    check_path_form(path)?;
    let mut current = PathBuf::new();
    #[allow(unused_mut)]
    let mut handles = Vec::new();
    for component in path.components() {
        current.push(component);
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !redirected(&metadata) => {}
            Ok(_) => return Err(StoreError::new("UNSAFE_PATH")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                #[allow(unused_mut)]
                let mut builder = fs::DirBuilder::new();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                match builder.create(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(_) => return Err(StoreError::new("STORE_UNAVAILABLE")),
                }
                let metadata = fs::symlink_metadata(&current)
                    .map_err(|_| StoreError::new("STORE_UNAVAILABLE"))?;
                if !metadata.is_dir() || redirected(&metadata) {
                    return Err(StoreError::new("UNSAFE_PATH"));
                }
            }
            Err(_) => return Err(StoreError::new("STORE_UNAVAILABLE")),
        }
        #[cfg(windows)]
        handles.push(open_directory_guard(&current)?);
    }
    Ok(handles)
}

#[cfg(windows)]
fn open_directory_guard(path: &Path) -> Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    let file = OpenOptions::new()
        .access_mode(0x1 | 0x80) // FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES
        .custom_flags(0x0220_0000) // BACKUP_SEMANTICS | OPEN_REPARSE_POINT
        .share_mode(0x1 | 0x2)
        .open(path)
        .map_err(|_| StoreError::new("STORE_READ_FAILED"))?;
    let metadata = file
        .metadata()
        .map_err(|_| StoreError::new("STORE_READ_FAILED"))?;
    if !metadata.is_dir() || redirected(&metadata) {
        return Err(StoreError::new("UNSAFE_PATH"));
    }
    Ok(file)
}

fn hold_existing_directories(path: &Path) -> Result<Vec<File>> {
    check_path_form(path)?;
    let mut current = PathBuf::new();
    #[allow(unused_mut)]
    let mut handles = Vec::new();
    for component in path.components() {
        current.push(component);
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        let metadata =
            fs::symlink_metadata(&current).map_err(|_| StoreError::new("STORE_READ_FAILED"))?;
        if !metadata.is_dir() || redirected(&metadata) {
            return Err(StoreError::new("UNSAFE_PATH"));
        }
        #[cfg(windows)]
        handles.push(open_directory_guard(&current)?);
    }
    Ok(handles)
}

fn check_directory_tree(path: &Path) -> Result<()> {
    check_path_form(path)?;
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        let metadata =
            fs::symlink_metadata(&current).map_err(|_| StoreError::new("STORE_READ_FAILED"))?;
        if !metadata.is_dir() || redirected(&metadata) {
            return Err(StoreError::new("UNSAFE_PATH"));
        }
    }
    Ok(())
}

fn check_optional_regular(path: &Path) -> Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !redirected(&metadata) => Ok(Some(metadata)),
        Ok(_) => Err(StoreError::new("UNSAFE_PATH")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(StoreError::new("STORE_READ_FAILED")),
    }
}

fn check_open_regular(file: &File) -> Result<Metadata> {
    let metadata = file
        .metadata()
        .map_err(|_| StoreError::new("STORE_READ_FAILED"))?;
    if !metadata.is_file() || redirected(&metadata) {
        return Err(StoreError::new("UNSAFE_PATH"));
    }
    Ok(metadata)
}

pub(crate) fn read_regular_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let parent = path.parent().ok_or(StoreError::new("UNSAFE_PATH"))?;
    let _directories = hold_existing_directories(parent)?;
    check_optional_regular(path)?.ok_or(StoreError::new("STORE_READ_FAILED"))?;
    let file = safe_options()
        .read(true)
        .open(path)
        .map_err(|_| StoreError::new("STORE_READ_FAILED"))?;
    if check_open_regular(&file)?.len() > maximum as u64 {
        return Err(StoreError::new("DOCUMENT_TOO_LARGE"));
    }
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| StoreError::new("STORE_READ_FAILED"))?;
    if bytes.len() > maximum {
        return Err(StoreError::new("DOCUMENT_TOO_LARGE"));
    }
    check_directory_tree(parent)?;
    check_optional_regular(path)?.ok_or(StoreError::new("STORE_READ_FAILED"))?;
    Ok(bytes)
}
