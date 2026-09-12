//! Read-only, descriptor-anchored access. No user library write API exists here.
use crate::{error, hash, Result};
#[cfg(windows)]
use std::path::PathBuf;
use std::{
    ffi::OsString,
    fs::{self, File, Metadata},
    path::{Component, Path},
    time::UNIX_EPOCH,
};
use workbench_storage::{
    library_relative_path_is_valid, LibraryFileIdentity, LibraryRoot, MAX_SAFE_INTEGER,
};

pub(crate) struct Root {
    pub saved: LibraryRoot,
    directory: SafeDirectory,
}

pub(crate) struct SafeDirectory {
    pub file: File,
    #[cfg(windows)]
    path: PathBuf,
    #[cfg(windows)]
    _ancestors: Vec<File>,
}

pub(crate) struct SafeFile {
    pub file: File,
    #[cfg(windows)]
    _parent: SafeDirectory,
}

pub(crate) enum Node {
    Directory(SafeDirectory),
    File(SafeFile),
    Skipped,
}

pub(crate) struct Entries {
    pub directory: SafeDirectory,
    #[cfg(unix)]
    iterator: rustix::fs::Dir,
    #[cfg(windows)]
    iterator: fs::ReadDir,
}

impl Root {
    pub fn choose(path: &Path) -> Result<Self> {
        // Reject redirection before canonicalizing, so the picker cannot silently
        // turn a symlink/junction into permission for its destination.
        let initial = open_absolute_directory(path)?;
        let canonical = fs::canonicalize(path).map_err(|_| error("LIBRARY_ROOT_UNAVAILABLE"))?;
        let text = canonical
            .to_str()
            .filter(|s| s.len() <= 32768 && !s.chars().any(char::is_control))
            .ok_or(error("LIBRARY_UNSAFE_PATH"))?
            .to_owned();
        let directory = open_absolute_directory(&canonical)?;
        let key = file_key(&directory.file)?;
        if key != file_key(&initial.file)? {
            return Err(error("LIBRARY_FILE_CHANGED"));
        }
        Ok(Self {
            saved: LibraryRoot {
                id: hash(text.as_bytes()),
                path: text,
                file_key: key,
            },
            directory,
        })
    }

    pub fn restore(saved: &LibraryRoot) -> Result<Self> {
        let root = Self::choose(Path::new(&saved.path))?;
        if root.saved != *saved {
            return Err(error("LIBRARY_ROOT_CHANGED"));
        }
        Ok(root)
    }

    pub fn verify(&self) -> Result<()> {
        let current = open_absolute_directory(Path::new(&self.saved.path))?;
        if file_key(&current.file)? != self.saved.file_key {
            return Err(error("LIBRARY_ROOT_CHANGED"));
        }
        Ok(())
    }

    pub fn directory(&self, relative: &str) -> Result<SafeDirectory> {
        let mut directory = self.directory.duplicate()?;
        if relative.is_empty() {
            return Ok(directory);
        }
        if !library_relative_path_is_valid(relative) {
            return Err(error("LIBRARY_UNSAFE_PATH"));
        }
        for part in relative.split('/') {
            directory = match directory.child(part)? {
                Node::Directory(directory) => directory,
                _ => return Err(error("LIBRARY_UNSAFE_PATH")),
            };
        }
        Ok(directory)
    }

    pub fn node(&self, relative: &str) -> Result<Node> {
        if !library_relative_path_is_valid(relative) {
            return Err(error("LIBRARY_UNSAFE_PATH"));
        }
        let (parent, name) = relative.rsplit_once('/').unwrap_or(("", relative));
        self.directory(parent)?.child(name)
    }
}

impl SafeDirectory {
    pub fn duplicate(&self) -> Result<Self> {
        #[cfg(unix)]
        {
            Ok(Self {
                file: self
                    .file
                    .try_clone()
                    .map_err(|_| error("LIBRARY_READ_FAILED"))?,
            })
        }
        #[cfg(windows)]
        {
            open_absolute_directory(&self.path)
        }
    }

    pub fn entries(self) -> Result<Entries> {
        #[cfg(unix)]
        let iterator = rustix::fs::Dir::read_from(&self.file)
            .map_err(|_| error("LIBRARY_DIRECTORY_UNREADABLE"))?;
        #[cfg(windows)]
        let iterator =
            fs::read_dir(&self.path).map_err(|_| error("LIBRARY_DIRECTORY_UNREADABLE"))?;
        Ok(Entries {
            directory: self,
            iterator,
        })
    }

    pub fn child(&self, name: &str) -> Result<Node> {
        if name.contains('/') || !library_relative_path_is_valid(name) {
            return Err(error("LIBRARY_UNSAFE_PATH"));
        }
        #[cfg(unix)]
        {
            use rustix::fs::{openat, statat, AtFlags, FileType, Mode, OFlags};
            let stat = statat(&self.file, name, AtFlags::SYMLINK_NOFOLLOW)
                .map_err(|_| error("LIBRARY_READ_FAILED"))?;
            let kind = FileType::from_raw_mode(stat.st_mode);
            if kind != FileType::Directory && kind != FileType::RegularFile {
                return Ok(Node::Skipped);
            }
            let mut flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK;
            if kind == FileType::Directory {
                flags |= OFlags::DIRECTORY;
            }
            let file = File::from(
                openat(&self.file, name, flags, Mode::empty())
                    .map_err(|_| error("LIBRARY_UNSAFE_PATH"))?,
            );
            let current = file.metadata().map_err(|_| error("LIBRARY_READ_FAILED"))?;
            use std::os::unix::fs::MetadataExt;
            if current.dev() != stat.st_dev || current.ino() != stat.st_ino {
                return Err(error("LIBRARY_FILE_CHANGED"));
            }
            if current.is_dir() {
                Ok(Node::Directory(Self { file }))
            } else if current.is_file() {
                Ok(Node::File(SafeFile { file }))
            } else {
                Ok(Node::Skipped)
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let path = self.path.join(name);
            let metadata = fs::symlink_metadata(&path).map_err(|_| error("LIBRARY_READ_FAILED"))?;
            if redirected(&metadata) {
                return Ok(Node::Skipped);
            }
            if metadata.is_dir() {
                return Ok(Node::Directory(open_absolute_directory(&path)?));
            }
            if !metadata.is_file() {
                return Ok(Node::Skipped);
            }
            let parent = self.duplicate()?;
            let file = fs::OpenOptions::new()
                .read(true)
                .share_mode(0x1)
                .custom_flags(0x0020_0000)
                .open(&path)
                .map_err(|_| error("LIBRARY_READ_FAILED"))?;
            let metadata = file.metadata().map_err(|_| error("LIBRARY_READ_FAILED"))?;
            if redirected(&metadata) || !metadata.is_file() {
                return Err(error("LIBRARY_UNSAFE_PATH"));
            }
            Ok(Node::File(SafeFile {
                file,
                _parent: parent,
            }))
        }
    }
}

impl Entries {
    pub fn next_name(&mut self) -> Result<Option<OsString>> {
        loop {
            #[cfg(unix)]
            let next = self.iterator.next().map(|entry| {
                use std::os::unix::ffi::OsStringExt;
                entry
                    .map(|entry| OsString::from_vec(entry.file_name().to_bytes().to_vec()))
                    .map_err(|_| error("LIBRARY_DIRECTORY_UNREADABLE"))
            });
            #[cfg(windows)]
            let next = self.iterator.next().map(|entry| {
                entry
                    .map(|entry| entry.file_name())
                    .map_err(|_| error("LIBRARY_DIRECTORY_UNREADABLE"))
            });
            match next {
                Some(Ok(name)) if name == "." || name == ".." => continue,
                Some(result) => return result.map(Some),
                None => return Ok(None),
            }
        }
    }
}

fn open_absolute_directory(path: &Path) -> Result<SafeDirectory> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(error("LIBRARY_UNSAFE_PATH"));
    }
    #[cfg(unix)]
    {
        use rustix::fs::{open, openat, Mode, OFlags};
        let flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::DIRECTORY;
        let mut file = File::from(
            open("/", flags, Mode::empty()).map_err(|_| error("LIBRARY_ROOT_UNAVAILABLE"))?,
        );
        for part in path.components() {
            if let Component::Normal(name) = part {
                file = File::from(
                    openat(&file, name, flags, Mode::empty())
                        .map_err(|_| error("LIBRARY_UNSAFE_PATH"))?,
                );
            }
        }
        Ok(SafeDirectory { file })
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let mut current = PathBuf::new();
        let mut handles = Vec::new();
        for component in path.components() {
            current.push(component);
            if matches!(component, Component::Prefix(_)) {
                continue;
            }
            let file = fs::OpenOptions::new()
                .access_mode(0x1 | 0x80)
                .custom_flags(0x0220_0000)
                .share_mode(0x1 | 0x2)
                .open(&current)
                .map_err(|_| error("LIBRARY_ROOT_UNAVAILABLE"))?;
            let metadata = file.metadata().map_err(|_| error("LIBRARY_READ_FAILED"))?;
            if !metadata.is_dir() || redirected(&metadata) {
                return Err(error("LIBRARY_UNSAFE_PATH"));
            }
            handles.push(file);
        }
        let file = handles.pop().ok_or(error("LIBRARY_UNSAFE_PATH"))?;
        Ok(SafeDirectory {
            file,
            path: path.to_path_buf(),
            _ancestors: handles,
        })
    }
}

#[cfg(windows)]
fn redirected(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
}

pub(crate) fn file_key(file: &File) -> Result<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata().map_err(|_| error("LIBRARY_READ_FAILED"))?;
        Ok(hash(
            format!("{}:{}", metadata.dev(), metadata.ino()).as_bytes(),
        ))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        // The live File owns this valid handle for the entire call; the output
        // buffer is correctly aligned and initialized before it is inspected.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(error("LIBRARY_READ_FAILED"));
        }
        Ok(hash(
            format!(
                "{}:{}:{}",
                info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
            )
            .as_bytes(),
        ))
    }
}

pub(crate) fn identity(file: &File) -> Result<LibraryFileIdentity> {
    let metadata = file.metadata().map_err(|_| error("LIBRARY_READ_FAILED"))?;
    if metadata.len() > MAX_SAFE_INTEGER {
        return Err(error("LIBRARY_LIMIT_REACHED"));
    }
    #[cfg(unix)]
    let modified = {
        use std::os::unix::fs::MetadataExt;
        format!("{}:{}", metadata.mtime(), metadata.mtime_nsec())
    };
    #[cfg(windows)]
    let modified = {
        use std::os::windows::fs::MetadataExt;
        metadata.last_write_time().to_string()
    };
    Ok(LibraryFileIdentity {
        file_key: file_key(file)?,
        bytes: metadata.len(),
        modified,
    })
}

pub(crate) fn modified_at(metadata: &Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis()
        .try_into()
        .ok()
        .filter(|v| *v <= MAX_SAFE_INTEGER)
}
