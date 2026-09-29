//! Directory-handle-anchored access. Only safe single components are accepted.
use crate::{error, hash, Result};
#[cfg(windows)]
use std::path::PathBuf;
use std::{
    fs::{File, Metadata},
    io::Read,
    path::{Component, Path},
};

pub(crate) struct Directory {
    pub file: File,
    #[cfg(windows)]
    path: PathBuf,
    #[cfg(windows)]
    _parents: Vec<File>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EntryKind {
    Directory,
    File(u64),
    Other,
}
fn name(value: &str) -> Result<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains(['/', '\\', ':'])
        || value.chars().any(char::is_control)
    {
        return Err(error("DOWNLOAD_UNSAFE_PATH"));
    }
    Ok(())
}
fn regular(metadata: &Metadata) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(error("DOWNLOAD_UNSAFE_PATH"));
        }
    }
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(error("DOWNLOAD_UNSAFE_PATH"));
    }
    Ok(())
}
impl Directory {
    /// Metadata-only lookup anchored to this verified parent. Only a precise
    /// missing-name error becomes None; redirection and I/O failures stay errors.
    pub fn probe(&self, child: &str) -> Result<Option<EntryKind>> {
        name(child)?;
        #[cfg(unix)]
        {
            use rustix::fs::{statat, AtFlags, FileType};
            let stat = match statat(&self.file, child, AtFlags::SYMLINK_NOFOLLOW) {
                Ok(stat) => stat,
                Err(rustix::io::Errno::NOENT) => return Ok(None),
                Err(_) => return Err(error("DOWNLOAD_READ_FAILED")),
            };
            let kind = match FileType::from_raw_mode(stat.st_mode) {
                FileType::Directory => EntryKind::Directory,
                FileType::RegularFile => EntryKind::File(
                    u64::try_from(stat.st_size).map_err(|_| error("DOWNLOAD_READ_FAILED"))?,
                ),
                FileType::Symlink => return Err(error("DOWNLOAD_UNSAFE_PATH")),
                _ => EntryKind::Other,
            };
            Ok(Some(kind))
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            let metadata = match std::fs::symlink_metadata(self.path.join(child)) {
                Ok(metadata) => metadata,
                Err(problem) if problem.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err(error("DOWNLOAD_READ_FAILED")),
            };
            if metadata.file_attributes() & 0x400 != 0 || metadata.file_type().is_symlink() {
                return Err(error("DOWNLOAD_UNSAFE_PATH"));
            }
            Ok(Some(if metadata.is_dir() {
                EntryKind::Directory
            } else if metadata.is_file() {
                EntryKind::File(metadata.len())
            } else {
                EntryKind::Other
            }))
        }
    }
    pub fn open(path: &Path) -> Result<Self> {
        if !path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        {
            return Err(error("DOWNLOAD_UNSAFE_PATH"));
        }
        #[cfg(unix)]
        {
            use rustix::fs::{open, openat, Mode, OFlags};
            let flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::DIRECTORY;
            let mut file = File::from(
                open("/", flags, Mode::empty()).map_err(|_| error("DOWNLOAD_ROOT_UNAVAILABLE"))?,
            );
            for part in path.components() {
                if let Component::Normal(part) = part {
                    file = File::from(
                        openat(&file, part, flags, Mode::empty())
                            .map_err(|_| error("DOWNLOAD_UNSAFE_PATH"))?,
                    );
                }
            }
            Ok(Self { file })
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
            let mut current = PathBuf::new();
            let mut files = Vec::new();
            for part in path.components() {
                current.push(part);
                if matches!(part, Component::Prefix(_)) {
                    continue;
                }
                let file = std::fs::OpenOptions::new()
                    .access_mode(0x81)
                    .share_mode(0x3)
                    .custom_flags(0x0220_0000)
                    .open(&current)
                    .map_err(|_| error("DOWNLOAD_ROOT_UNAVAILABLE"))?;
                let metadata = file
                    .metadata()
                    .map_err(|_| error("DOWNLOAD_ROOT_UNAVAILABLE"))?;
                if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
                    return Err(error("DOWNLOAD_UNSAFE_PATH"));
                }
                files.push(file);
            }
            let file = files.pop().ok_or(error("DOWNLOAD_UNSAFE_PATH"))?;
            Ok(Self {
                file,
                path: path.to_path_buf(),
                _parents: files,
            })
        }
    }
    pub fn child(&self, child: &str) -> Result<Self> {
        name(child)?;
        #[cfg(unix)]
        {
            use rustix::fs::{openat, Mode, OFlags};
            let file = File::from(
                openat(
                    &self.file,
                    child,
                    OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::DIRECTORY,
                    Mode::empty(),
                )
                .map_err(|_| error("DOWNLOAD_UNSAFE_PATH"))?,
            );
            Ok(Self { file })
        }
        #[cfg(windows)]
        {
            Self::open(&self.path.join(child))
        }
    }
    pub fn create(&self, child: &str) -> Result<Self> {
        name(child)?;
        #[cfg(unix)]
        {
            rustix::fs::mkdirat(&self.file, child, rustix::fs::Mode::from_raw_mode(0o700))
                .map_err(|_| error("DOWNLOAD_DESTINATION_EXISTS"))?;
        }
        #[cfg(windows)]
        {
            std::fs::create_dir(self.path.join(child))
                .map_err(|_| error("DOWNLOAD_DESTINATION_EXISTS"))?;
        }
        self.child(child)
    }
    pub fn read(&self, child: &str) -> Result<File> {
        name(child)?;
        #[cfg(unix)]
        let file = {
            use rustix::fs::{openat, Mode, OFlags};
            File::from(
                openat(
                    &self.file,
                    child,
                    OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
                    Mode::empty(),
                )
                .map_err(|_| error("DOWNLOAD_FILE_UNAVAILABLE"))?,
            )
        };
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0x1)
                .custom_flags(0x0020_0000)
                .open(self.path.join(child))
                .map_err(|_| error("DOWNLOAD_FILE_UNAVAILABLE"))?
        };
        regular(
            &file
                .metadata()
                .map_err(|_| error("DOWNLOAD_FILE_UNAVAILABLE"))?,
        )?;
        Ok(file)
    }
    pub fn create_file(&self, child: &str) -> Result<File> {
        name(child)?;
        #[cfg(unix)]
        let file = {
            use rustix::fs::{openat, Mode, OFlags};
            File::from(
                openat(
                    &self.file,
                    child,
                    OFlags::WRONLY
                        | OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::NOFOLLOW
                        | OFlags::CLOEXEC,
                    Mode::from_raw_mode(0o600),
                )
                .map_err(|_| error("DOWNLOAD_FILE_EXISTS"))?,
            )
        };
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .share_mode(0)
                .custom_flags(0x0020_0000)
                .open(self.path.join(child))
                .map_err(|_| error("DOWNLOAD_FILE_EXISTS"))?
        };
        regular(
            &file
                .metadata()
                .map_err(|_| error("DOWNLOAD_FILE_UNAVAILABLE"))?,
        )?;
        Ok(file)
    }
    pub fn resume_file(&self, child: &str) -> Result<File> {
        name(child)?;
        #[cfg(unix)]
        let file = {
            use rustix::fs::{openat, Mode, OFlags};
            File::from(
                openat(
                    &self.file,
                    child,
                    OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
                    Mode::empty(),
                )
                .map_err(|_| error("DOWNLOAD_FILE_UNAVAILABLE"))?,
            )
        };
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0)
                .custom_flags(0x0020_0000)
                .open(self.path.join(child))
                .map_err(|_| error("DOWNLOAD_FILE_UNAVAILABLE"))?
        };
        regular(
            &file
                .metadata()
                .map_err(|_| error("DOWNLOAD_FILE_UNAVAILABLE"))?,
        )?;
        Ok(file)
    }
    pub fn names(&self) -> Result<Vec<String>> {
        let mut result = Vec::new();
        #[cfg(unix)]
        {
            let entries = rustix::fs::Dir::read_from(&self.file)
                .map_err(|_| error("DOWNLOAD_READ_FAILED"))?;
            for entry in entries {
                let entry = entry.map_err(|_| error("DOWNLOAD_READ_FAILED"))?;
                let name = entry
                    .file_name()
                    .to_str()
                    .map_err(|_| error("DOWNLOAD_UNSAFE_PATH"))?;
                if matches!(name, "." | "..") {
                    continue;
                }
                result.push(name.to_owned());
                if result.len() > 20_203 {
                    return Err(error("DOWNLOAD_LIMIT_REACHED"));
                }
            }
        }
        #[cfg(windows)]
        {
            for entry in std::fs::read_dir(&self.path).map_err(|_| error("DOWNLOAD_READ_FAILED"))? {
                let name = entry
                    .map_err(|_| error("DOWNLOAD_READ_FAILED"))?
                    .file_name()
                    .into_string()
                    .map_err(|_| error("DOWNLOAD_UNSAFE_PATH"))?;
                result.push(name);
                if result.len() > 20_203 {
                    return Err(error("DOWNLOAD_LIMIT_REACHED"));
                }
            }
        }
        Ok(result)
    }
    pub fn key(&self) -> Result<String> {
        file_key(&self.file)
    }
    pub fn remove_exact_file(&self, child: &str, size: u64, hash: &str) -> Result<()> {
        name(child)?;
        let mut file = self.read(child)?;
        if digest(&mut file)? != (size, hash.to_owned()) {
            return Err(error("DOWNLOAD_STAGING_CHANGED"));
        }
        drop(file);
        #[cfg(unix)]
        rustix::fs::unlinkat(&self.file, child, rustix::fs::AtFlags::empty())
            .map_err(|_| error("DOWNLOAD_CLEANUP_INCOMPLETE"))?;
        #[cfg(windows)]
        std::fs::remove_file(self.path.join(child))
            .map_err(|_| error("DOWNLOAD_CLEANUP_INCOMPLETE"))?;
        Ok(())
    }
    pub fn remove_empty_child(&self, child: &str) -> Result<()> {
        name(child)?;
        let directory = self.child(child)?;
        if !directory.names()?.is_empty() {
            return Err(error("DOWNLOAD_CLEANUP_INCOMPLETE"));
        }
        drop(directory);
        #[cfg(unix)]
        rustix::fs::unlinkat(&self.file, child, rustix::fs::AtFlags::REMOVEDIR)
            .map_err(|_| error("DOWNLOAD_CLEANUP_INCOMPLETE"))?;
        #[cfg(windows)]
        std::fs::remove_dir(self.path.join(child))
            .map_err(|_| error("DOWNLOAD_CLEANUP_INCOMPLETE"))?;
        Ok(())
    }
    pub fn sync(&self) -> Result<()> {
        #[cfg(unix)]
        self.file
            .sync_all()
            .map_err(|_| error("DOWNLOAD_COMMIT_UNCERTAIN"))?;
        Ok(())
    }
}
pub(crate) fn file_key(file: &File) -> Result<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = file.metadata().map_err(|_| error("DOWNLOAD_READ_FAILED"))?;
        Ok(hash(format!("{}:{}", m.dev(), m.ino()).as_bytes()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        // The File owns the valid handle and the initialized output stays live.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(error("DOWNLOAD_READ_FAILED"));
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
pub(crate) fn digest(file: &mut File) -> Result<(u64, String)> {
    use sha2::{Digest, Sha256};
    let mut sha = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 65536];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|_| error("DOWNLOAD_READ_FAILED"))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        sha.update(&buffer[..n]);
    }
    Ok((total, format!("{:x}", sha.finalize())))
}
