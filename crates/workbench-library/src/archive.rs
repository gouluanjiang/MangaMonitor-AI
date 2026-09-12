use crate::{error, metadata, Result};
use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Seek, SeekFrom},
};
use workbench_storage::{library_relative_path_is_valid, LibraryCoverFile, LibraryRecord};
use zip::ZipArchive;

pub(crate) const MAX_ARCHIVE_ENTRIES: usize = 10_000;
const MAX_CENTRAL_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_COMPRESSED_IMAGE_BYTES: u64 = 16 * 1024 * 1024;

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

/// Bound allocation before ZipArchive::new, which trusts the EOCD entry count
/// when reserving its metadata vector. ZIP64, multi-disk and SFX are unsupported.
pub(crate) fn preflight<R: Read + Seek>(reader: &mut R) -> Result<usize> {
    let length = reader
        .seek(SeekFrom::End(0))
        .map_err(|_| error("LIBRARY_ARCHIVE_INVALID"))?;
    if length > u32::MAX as u64 {
        return Err(error("LIBRARY_ARCHIVE_UNSUPPORTED"));
    }
    if length < 22 {
        return Err(error("LIBRARY_ARCHIVE_INVALID"));
    }
    let tail_len = length.min(65_557) as usize;
    reader
        .seek(SeekFrom::End(-(tail_len as i64)))
        .map_err(|_| error("LIBRARY_ARCHIVE_INVALID"))?;
    let mut tail = vec![0; tail_len];
    reader
        .read_exact(&mut tail)
        .map_err(|_| error("LIBRARY_ARCHIVE_INVALID"))?;
    let offset = (0..=tail_len - 22)
        .rev()
        .find(|offset| {
            &tail[*offset..*offset + 4] == b"PK\x05\x06"
                && *offset + 22 + usize::from(u16_at(&tail, *offset + 20)) == tail_len
        })
        .ok_or(error("LIBRARY_ARCHIVE_INVALID"))?;
    let end = &tail[offset..];
    let count = usize::from(u16_at(end, 10));
    let central_size = u32_at(end, 12) as usize;
    let central_start = u64::from(u32_at(end, 16));
    let end_position = length - tail_len as u64 + offset as u64;
    if count == u16::MAX as usize
        || central_size == u32::MAX as usize
        || central_start == u32::MAX as u64
        || u16_at(end, 4) != 0
        || u16_at(end, 6) != 0
        || usize::from(u16_at(end, 8)) != count
    {
        return Err(error("LIBRARY_ARCHIVE_UNSUPPORTED"));
    }
    if count > MAX_ARCHIVE_ENTRIES || central_size > MAX_CENTRAL_BYTES {
        return Err(error("LIBRARY_ARCHIVE_LIMIT"));
    }
    if central_start.checked_add(central_size as u64) != Some(end_position)
        || count.saturating_mul(46) > central_size
    {
        return Err(error("LIBRARY_ARCHIVE_INVALID"));
    }
    reader
        .seek(SeekFrom::Start(central_start))
        .map_err(|_| error("LIBRARY_ARCHIVE_INVALID"))?;
    let mut central = vec![0; central_size];
    reader
        .read_exact(&mut central)
        .map_err(|_| error("LIBRARY_ARCHIVE_INVALID"))?;
    let mut cursor = 0usize;
    let mut names = HashSet::new();
    for _ in 0..count {
        let block = central
            .get(cursor..cursor + 46)
            .ok_or(error("LIBRARY_ARCHIVE_INVALID"))?;
        if &block[..4] != b"PK\x01\x02" {
            return Err(error("LIBRARY_ARCHIVE_INVALID"));
        }
        let name_len = usize::from(u16_at(block, 28));
        let extra_len = usize::from(u16_at(block, 30));
        let comment_len = usize::from(u16_at(block, 32));
        if name_len == 0 || name_len > 4096 {
            return Err(error("LIBRARY_ARCHIVE_LIMIT"));
        }
        if u16_at(block, 8) & 1 != 0
            || ![0, 8].contains(&u16_at(block, 10))
            || u16_at(block, 34) != 0
            || u32_at(block, 20) == u32::MAX
            || u32_at(block, 24) == u32::MAX
            || u32_at(block, 42) == u32::MAX
        {
            return Err(error("LIBRARY_ARCHIVE_UNSUPPORTED"));
        }
        if u64::from(u32_at(block, 42)) >= central_start {
            return Err(error("LIBRARY_ARCHIVE_INVALID"));
        }
        let next = cursor + 46 + name_len + extra_len + comment_len;
        if next > central.len() {
            return Err(error("LIBRARY_ARCHIVE_INVALID"));
        }
        let name = &central[cursor + 46..cursor + 46 + name_len];
        if !names.insert(name.to_vec()) {
            return Err(error("LIBRARY_ARCHIVE_INVALID"));
        }
        let extra = &central[cursor + 46 + name_len..cursor + 46 + name_len + extra_len];
        let mut position = 0;
        while position < extra.len() {
            if position + 4 > extra.len() {
                return Err(error("LIBRARY_ARCHIVE_INVALID"));
            }
            if u16_at(extra, position) == 1 {
                return Err(error("LIBRARY_ARCHIVE_UNSUPPORTED"));
            }
            position += 4 + usize::from(u16_at(extra, position + 2));
            if position > extra.len() {
                return Err(error("LIBRARY_ARCHIVE_INVALID"));
            }
        }
        cursor = next;
    }
    if cursor != central.len() {
        return Err(error("LIBRARY_ARCHIVE_INVALID"));
    }
    reader
        .seek(SeekFrom::Start(0))
        .map_err(|_| error("LIBRARY_ARCHIVE_INVALID"))?;
    Ok(count)
}

fn open(file: &mut File) -> Result<ZipArchive<&mut File>> {
    let count = preflight(file)?;
    let archive = ZipArchive::new(file).map_err(|_| error("LIBRARY_ARCHIVE_INVALID"))?;
    if archive.len() != count || archive.offset() != 0 {
        return Err(error("LIBRARY_ARCHIVE_INVALID"));
    }
    Ok(archive)
}

pub(crate) fn is_image(name: &str) -> bool {
    matches!(
        name.rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "jpg" | "jpeg" | "png" | "webp" | "gif"
    )
}

pub(crate) fn cover_precedes(candidate: &str, current: &str) -> bool {
    // A downloader's top-level cover wins over chapter pages. Padded page names
    // then have stable lexical order; no entry supplied path is ever extracted.
    (candidate.matches('/').count(), candidate) < (current.matches('/').count(), current)
}

pub(crate) fn inspect(file: &mut File, record: &mut LibraryRecord) -> Result<()> {
    let mut archive = open(file)?;
    let mut images = 0u64;
    let mut cover: Option<String> = None;
    let mut metadata_entries = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index_raw(index)
            .map_err(|_| error("LIBRARY_ARCHIVE_INVALID"))?;
        let name = entry.name();
        if !library_relative_path_is_valid(name.trim_end_matches('/')) || entry.is_symlink() {
            return Err(error("LIBRARY_ARCHIVE_UNSAFE"));
        }
        if entry.is_dir() {
            continue;
        }
        if is_image(name) {
            images += 1;
            if cover.as_ref().is_none_or(|old| cover_precedes(name, old)) {
                cover = Some(name.to_owned());
            }
        }
        if !name.contains('/')
            && (name.eq_ignore_ascii_case("ComicInfo.xml") || name == "元数据.json")
        {
            metadata_entries.push((index, name.to_owned()));
        }
    }
    record.item.page_count = Some(images);
    record.item.cover_available = cover.is_some();
    record.cover = cover.map(|relative_path| LibraryCoverFile {
        relative_path,
        identity: None,
    });
    for (index, name) in metadata_entries {
        let result =
            read_entry(&mut archive, index, metadata::MAX_METADATA_BYTES).and_then(|bytes| {
                if name == "元数据.json" {
                    metadata::downloader_json(&bytes)
                } else {
                    metadata::comic_info(&bytes)
                }
            });
        match result {
            Ok(value) => metadata::apply(&mut record.item, value),
            Err(problem) => {
                if record.item.error_code.as_deref() != Some("LIBRARY_IDENTITY_CONFLICT") {
                    record.item.error_code = Some(problem.code.into());
                }
            }
        }
    }
    Ok(())
}

fn read_entry<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    index: usize,
    maximum: usize,
) -> Result<Vec<u8>> {
    let entry = archive
        .by_index(index)
        .map_err(|_| error("LIBRARY_ARCHIVE_INVALID"))?;
    if entry.size() > maximum as u64
        || entry.compressed_size() > MAX_COMPRESSED_IMAGE_BYTES
        || entry.size()
            > entry
                .compressed_size()
                .saturating_mul(1000)
                .max(1024 * 1024)
    {
        return Err(error("LIBRARY_ENTRY_LIMIT"));
    }
    let mut bytes = Vec::new();
    entry
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error("LIBRARY_ARCHIVE_INVALID"))?;
    if bytes.len() > maximum {
        return Err(error("LIBRARY_ENTRY_LIMIT"));
    }
    Ok(bytes)
}

pub(crate) fn cover(file: &mut File, name: &str) -> Result<Vec<u8>> {
    let mut archive = open(file)?;
    let index = archive
        .index_for_name(name)
        .ok_or(error("LIBRARY_FILE_CHANGED"))?;
    read_entry(&mut archive, index, MAX_IMAGE_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn forged_large_entry_count_is_rejected_before_zip_allocation() {
        let mut bytes = vec![0u8; 22];
        bytes[..4].copy_from_slice(b"PK\x05\x06");
        bytes[8..10].copy_from_slice(&60_000u16.to_le_bytes());
        bytes[10..12].copy_from_slice(&60_000u16.to_le_bytes());
        assert_eq!(
            preflight(&mut Cursor::new(bytes)).unwrap_err().code,
            "LIBRARY_ARCHIVE_LIMIT"
        );
    }

    #[test]
    fn zip64_and_invalid_directory_offsets_fail_before_archive_creation() {
        let mut bytes = vec![0u8; 22];
        bytes[..4].copy_from_slice(b"PK\x05\x06");
        bytes[10..12].copy_from_slice(&u16::MAX.to_le_bytes());
        assert_eq!(
            preflight(&mut Cursor::new(&bytes)).unwrap_err().code,
            "LIBRARY_ARCHIVE_UNSUPPORTED"
        );
        bytes[10..12].copy_from_slice(&0u16.to_le_bytes());
        bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            preflight(&mut Cursor::new(bytes)).unwrap_err().code,
            "LIBRARY_ARCHIVE_INVALID"
        );
    }
}
