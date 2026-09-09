use crate::{AccountError, Result, Source, SourceFolder, SourceWork};
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{ImageDecoder, ImageFormat, ImageReader, Limits};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    io::Cursor,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use workbench_storage::{AccountCache, CacheEntry, StoreError, WorkbenchStore, MAX_SAFE_INTEGER};

pub(crate) const MAX_ITEMS: usize = 20_000;
pub(crate) const MAX_METADATA_BYTES: usize = 32 * 1024 * 1024;
const MAX_SCOPES: usize = 16;
const MAX_JPEG_BYTES: usize = 256 * 1024;
// Reserve every fixed slot up front: an interrupted index write must not hide
// newly written JPEG bytes from the global high-water budget.
const COVER_RESERVATION: u64 = 64 * 1024 * 1024;

struct WorkEntry {
    work: SourceWork,
    bytes: usize,
    stamp: u64,
}
#[derive(Default)]
pub(crate) struct WorkCache {
    entries: BTreeMap<String, WorkEntry>,
    order: BTreeMap<u64, String>,
    bytes: usize,
    sequence: u64,
}
impl WorkCache {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
    fn touch(&mut self, id: &str) {
        if self.sequence == u64::MAX {
            self.order.clear();
            for (index, (id, entry)) in self.entries.iter_mut().enumerate() {
                entry.stamp = index as u64;
                self.order.insert(entry.stamp, id.clone());
            }
            self.sequence = self.entries.len() as u64;
        }
        self.sequence += 1;
        if let Some(entry) = self.entries.get_mut(id) {
            self.order.remove(&entry.stamp);
            entry.stamp = self.sequence;
            self.order.insert(entry.stamp, id.into());
        }
    }
    pub(crate) fn contains_key(&mut self, id: &str) -> bool {
        self.touch(id);
        self.entries.contains_key(id)
    }
    pub(crate) fn get(&mut self, id: &str) -> Option<&SourceWork> {
        self.touch(id);
        self.entries.get(id).map(|entry| &entry.work)
    }
    pub(crate) fn insert(&mut self, work: SourceWork, bytes: usize) {
        if let Some(old) = self.entries.remove(&work.work_id) {
            self.bytes -= old.bytes;
            self.order.remove(&old.stamp);
        }
        self.touch(&work.work_id);
        self.order.insert(self.sequence, work.work_id.clone());
        self.bytes += bytes;
        self.entries.insert(
            work.work_id.clone(),
            WorkEntry {
                work,
                bytes,
                stamp: self.sequence,
            },
        );
        while self.entries.len() > MAX_ITEMS || self.bytes > MAX_METADATA_BYTES {
            let Some((_, id)) = self.order.pop_first() else {
                break;
            };
            if let Some(old) = self.entries.remove(&id) {
                self.bytes -= old.bytes;
            }
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CatalogAction {
    Read,
    Write,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogSnapshot {
    pub items: Vec<SourceWork>,
    pub page: u64,
    #[serde(deserialize_with = "required_option")]
    pub total: Option<u64>,
    #[serde(deserialize_with = "required_option")]
    pub pages: Option<u64>,
    #[serde(deserialize_with = "required_option")]
    pub has_more: Option<bool>,
    pub folders: Vec<SourceFolder>,
    pub complete: bool,
    pub updated_at: u64,
    pub first_page_ids: Vec<String>,
}
fn required_option<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogResult {
    pub source: Source,
    pub session_id: String,
    pub snapshot: Option<CatalogSnapshot>,
    pub complete_snapshot: Option<CatalogSnapshot>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogDocument {
    version: u32,
    entries: Vec<CatalogEntry>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogEntry {
    key: String,
    used_at: u64,
    snapshot: CatalogSnapshot,
    complete_snapshot: Option<CatalogSnapshot>,
}

pub(crate) fn now_ms() -> Result<u64> {
    let value = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| err("CACHE_UNAVAILABLE"))?
            .as_millis(),
    )
    .map_err(|_| err("CACHE_UNAVAILABLE"))?;
    if value > MAX_SAFE_INTEGER {
        return Err(err("CACHE_UNAVAILABLE"));
    }
    Ok(value)
}
fn err(code: &'static str) -> AccountError {
    AccountError::new(code)
}
fn store_error(error: AccountError) -> StoreError {
    StoreError { code: error.code }
}
fn account_error(error: StoreError) -> AccountError {
    err(error.code)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn digest(value: &str) -> String {
    hex(&Sha256::digest(value.as_bytes()))
}
fn serialized_size(
    value: &impl Serialize,
    maximum: usize,
) -> std::result::Result<usize, serde_json::Error> {
    struct Counter {
        bytes: usize,
        maximum: usize,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.bytes = self
                .bytes
                .checked_add(buffer.len())
                .filter(|size| *size <= self.maximum)
                .ok_or_else(|| std::io::Error::other("size limit"))?;
            Ok(buffer.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { bytes: 0, maximum };
    serde_json::to_writer(&mut counter, value)?;
    Ok(counter.bytes)
}
fn valid_key(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub(crate) fn validate_work(source: Source, work: &SourceWork) -> Result<usize> {
    if work.source != source
        || !valid_id(&work.work_id)
        || work.title.trim().is_empty()
        || work.chapter_count.is_some_and(|n| n > MAX_SAFE_INTEGER)
        || work.page_count.is_some_and(|n| n > MAX_SAFE_INTEGER)
    {
        return Err(err("SOURCE_RESPONSE_INVALID"));
    }
    serialized_size(work, 64 * 1024).map_err(|_| err("SOURCE_RESPONSE_INVALID"))
}

fn validate_snapshot(source: Source, snapshot: &CatalogSnapshot, now: u64) -> Result<()> {
    let invalid = || err("CATALOG_CACHE_INVALID");
    if snapshot.items.len() > MAX_ITEMS
        || !(1..=1000).contains(&snapshot.page)
        || snapshot.updated_at > MAX_SAFE_INTEGER
        || snapshot.updated_at > now.saturating_add(300_000)
        || snapshot
            .total
            .is_some_and(|n| n > MAX_SAFE_INTEGER || n < snapshot.items.len() as u64)
        || snapshot.pages.is_some_and(|n| {
            n > 1000
                || (n > 0 && snapshot.page > n)
                || (n == 0 && (!snapshot.items.is_empty() || snapshot.page != 1))
        })
        || snapshot.folders.len() > 1000
        || snapshot.first_page_ids.len() > 1000
        || snapshot.first_page_ids.len() > snapshot.items.len()
    {
        return Err(invalid());
    }
    let mut ids = HashSet::new();
    for work in &snapshot.items {
        validate_work(source, work).map_err(|_| invalid())?;
        if !ids.insert(&work.work_id) {
            return Err(invalid());
        }
    }
    if snapshot
        .first_page_ids
        .iter()
        .zip(&snapshot.items)
        .any(|(id, work)| id != &work.work_id)
    {
        return Err(invalid());
    }
    if !snapshot.items.is_empty() && snapshot.first_page_ids.is_empty() {
        return Err(invalid());
    }
    if snapshot.items.len() as u64 > snapshot.page * 1000
        || (snapshot.page == 1 && snapshot.first_page_ids.len() != snapshot.items.len())
        || (snapshot.page > 1 && snapshot.items.len() <= snapshot.first_page_ids.len())
    {
        return Err(invalid());
    }
    let mut folders = HashSet::new();
    for folder in &snapshot.folders {
        if !valid_id(&folder.id)
            || folder.id.len() > 80
            || folder.name.trim().is_empty()
            || folder.name.len() > 16_384
            || folder.count.is_some_and(|count| count > MAX_SAFE_INTEGER)
            || !folders.insert(&folder.id)
        {
            return Err(invalid());
        }
    }
    let terminal = snapshot.has_more == Some(false)
        || snapshot
            .pages
            .is_some_and(|pages| pages == 0 || pages == snapshot.page);
    if snapshot.has_more == Some(true)
        && snapshot
            .pages
            .is_some_and(|pages| pages == 0 || snapshot.page == pages)
    {
        return Err(invalid());
    }
    if snapshot.has_more == Some(false) && !snapshot.complete {
        return Err(invalid());
    }
    if snapshot.complete && snapshot.pages.is_some_and(|pages| pages > snapshot.page) {
        return Err(invalid());
    }
    if !snapshot.complete && snapshot.total == Some(snapshot.items.len() as u64) {
        return Err(invalid());
    }
    if snapshot.items.is_empty()
        && (snapshot.page != 1 || snapshot.total != Some(0) || !snapshot.complete || !terminal)
    {
        return Err(invalid());
    }
    if snapshot.complete
        && (!terminal
            || snapshot.has_more == Some(true)
            || snapshot
                .total
                .is_some_and(|total| total != snapshot.items.len() as u64))
    {
        return Err(invalid());
    }
    // Count without allocating another copy of a large IPC value. Both the
    // snapshot and the retained on-disk account document have independent bounds.
    serialized_size(snapshot, MAX_METADATA_BYTES).map_err(|_| err("CACHE_TOO_LARGE"))?;
    Ok(())
}

fn scope_key(folder_id: Option<&str>, reverse: bool) -> Result<String> {
    if folder_id.is_some_and(|folder| !valid_id(folder) || folder.len() > 80) {
        return Err(err("CATALOG_CACHE_INVALID"));
    }
    // JSON preserves the distinction between null and the literal folder strings.
    Ok(digest(
        &serde_json::to_string(&(folder_id, reverse)).map_err(|_| err("CATALOG_CACHE_INVALID"))?,
    ))
}

fn decode_catalog(source: Source, bytes: Option<Vec<u8>>, now: u64) -> Result<CatalogDocument> {
    let document = match bytes {
        None => CatalogDocument {
            version: 1,
            entries: vec![],
        },
        Some(bytes) => serde_json::from_slice::<CatalogDocument>(&bytes)
            .map_err(|_| err("CATALOG_CACHE_CORRUPT"))?,
    };
    let mut keys = HashSet::new();
    if document.version != 1 || document.entries.len() > MAX_SCOPES {
        return Err(err("CATALOG_CACHE_CORRUPT"));
    }
    for entry in &document.entries {
        if !valid_key(&entry.key) || !keys.insert(&entry.key) || entry.used_at > MAX_SAFE_INTEGER {
            return Err(err("CATALOG_CACHE_CORRUPT"));
        }
        validate_snapshot(source, &entry.snapshot, now)
            .map_err(|_| err("CATALOG_CACHE_CORRUPT"))?;
        if let Some(complete) = &entry.complete_snapshot {
            validate_snapshot(source, complete, now).map_err(|_| err("CATALOG_CACHE_CORRUPT"))?;
            if !complete.complete || complete.updated_at > entry.snapshot.updated_at {
                return Err(err("CATALOG_CACHE_CORRUPT"));
            }
        }
        if entry.snapshot.complete && entry.complete_snapshot.as_ref() != Some(&entry.snapshot) {
            return Err(err("CATALOG_CACHE_CORRUPT"));
        }
    }
    Ok(document)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn catalog(
    root: &Path,
    account: &str,
    source: Source,
    session_id: &str,
    folder_id: Option<&str>,
    reverse: bool,
    action: CatalogAction,
    snapshot: Option<CatalogSnapshot>,
) -> Result<CatalogResult> {
    let key = scope_key(folder_id, reverse)?;
    let now = now_ms()?;
    match (action, snapshot.as_ref()) {
        (CatalogAction::Read, Some(_)) | (CatalogAction::Write, None) => {
            return Err(err("CATALOG_CACHE_INVALID"))
        }
        (_, Some(value)) => validate_snapshot(source, value, now)?,
        _ => {}
    }
    let store = WorkbenchStore::open(root).map_err(account_error)?;
    store
        .with_account_cache(account, |cache| {
            let mut document = decode_catalog(source, cache.read(CacheEntry::Catalog)?, now)
                .map_err(store_error)?;
            let mut index = document.entries.iter().position(|entry| entry.key == key);
            if let Some(next) = snapshot {
                if let Some(index) = index {
                    let current = &mut document.entries[index];
                    if next.updated_at < current.snapshot.updated_at
                        || (next.updated_at == current.snapshot.updated_at
                            && next != current.snapshot)
                    {
                        return Err(StoreError {
                            code: "CATALOG_CACHE_STALE",
                        });
                    }
                    if next.complete {
                        current.complete_snapshot = Some(next.clone());
                    }
                    current.snapshot = next;
                    current.used_at = now;
                } else {
                    document.entries.push(CatalogEntry {
                        key: key.clone(),
                        used_at: now,
                        complete_snapshot: next.complete.then(|| next.clone()),
                        snapshot: next,
                    });
                    index = Some(document.entries.len() - 1);
                }
            } else if let Some(index) = index {
                document.entries[index].used_at = now;
            }
            let Some(_) = index else {
                return Ok(CatalogResult {
                    source,
                    session_id: session_id.into(),
                    snapshot: None,
                    complete_snapshot: None,
                });
            };
            let bytes = loop {
                let bytes = serde_json::to_vec(&document).map_err(|_| StoreError {
                    code: "CACHE_UNAVAILABLE",
                })?;
                if document.entries.len() <= MAX_SCOPES && bytes.len() <= MAX_METADATA_BYTES {
                    break bytes;
                }
                let oldest = document
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(_, entry)| entry.key != key)
                    .min_by_key(|(_, entry)| entry.used_at)
                    .map(|(index, _)| index)
                    .ok_or(StoreError {
                        code: "CACHE_TOO_LARGE",
                    })?;
                document.entries.remove(oldest);
            };
            cache.reserve(bytes.len() as u64, 0)?;
            cache.write(CacheEntry::Catalog, &bytes)?;
            let entry = document
                .entries
                .into_iter()
                .find(|entry| entry.key == key)
                .ok_or(StoreError {
                    code: "CACHE_UNAVAILABLE",
                })?;
            Ok(CatalogResult {
                source,
                session_id: session_id.into(),
                snapshot: Some(entry.snapshot),
                complete_snapshot: entry.complete_snapshot,
            })
        })
        .map_err(account_error)
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CoverIndex {
    version: u32,
    sequence: u64,
    entries: Vec<CoverRecord>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CoverRecord {
    slot: u8,
    key: String,
    used_at: u64,
    bytes: u64,
}

fn decode_cover_index(cache: &AccountCache<'_>) -> Result<CoverIndex> {
    let index = match cache.read(CacheEntry::CoverIndex).map_err(account_error)? {
        None => CoverIndex {
            version: 1,
            sequence: 0,
            entries: vec![],
        },
        Some(bytes) => {
            serde_json::from_slice::<CoverIndex>(&bytes).map_err(|_| err("CACHE_CORRUPT"))?
        }
    };
    let mut slots = HashSet::new();
    let mut keys = HashSet::new();
    if index.version != 1
        || index.sequence > MAX_SAFE_INTEGER
        || index.entries.len() > 255
        || index.entries.iter().any(|entry| {
            entry.slot == 255
                || !valid_key(&entry.key)
                || !slots.insert(entry.slot)
                || !keys.insert(&entry.key)
                || entry.bytes > MAX_JPEG_BYTES as u64 + 32
                || entry.bytes <= 32
                || entry.used_at > index.sequence
        })
    {
        return Err(err("CACHE_CORRUPT"));
    }
    Ok(index)
}
fn tick(index: &mut CoverIndex) -> u64 {
    if index.sequence == MAX_SAFE_INTEGER {
        index.entries.sort_by_key(|entry| entry.used_at);
        for (order, entry) in index.entries.iter_mut().enumerate() {
            entry.used_at = order as u64;
        }
        index.sequence = index.entries.len() as u64;
    }
    index.sequence += 1;
    index.sequence
}
fn jpeg_bytes(data_url: &str) -> Result<Vec<u8>> {
    let encoded = data_url
        .strip_prefix("data:image/jpeg;base64,")
        .ok_or(err("SOURCE_COVER_INVALID"))?;
    if encoded.len() > MAX_JPEG_BYTES.div_ceil(3) * 4 {
        return Err(err("SOURCE_COVER_INVALID"));
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| err("SOURCE_COVER_INVALID"))?;
    validate_jpeg(&bytes)?;
    Ok(bytes)
}
fn validate_jpeg(bytes: &[u8]) -> Result<()> {
    if bytes.is_empty()
        || bytes.len() > MAX_JPEG_BYTES
        || image::guess_format(bytes).ok() != Some(ImageFormat::Jpeg)
    {
        return Err(err("SOURCE_COVER_INVALID"));
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Jpeg);
    let mut limits = Limits::default();
    limits.max_image_width = Some(512);
    limits.max_image_height = Some(512);
    limits.max_alloc = Some(4 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|_| err("SOURCE_COVER_INVALID"))?;
    let (w, h) = decoder.dimensions();
    if w == 0 || h == 0 || w > 512 || h > 512 {
        return Err(err("SOURCE_COVER_INVALID"));
    }
    image::DynamicImage::from_decoder(decoder).map_err(|_| err("SOURCE_COVER_INVALID"))?;
    Ok(())
}

pub(crate) fn read_cover(root: &Path, account: &str, work_id: &str) -> Result<Option<String>> {
    let key = digest(work_id);
    let raw_key = Sha256::digest(work_id.as_bytes());
    WorkbenchStore::open(root)
        .map_err(account_error)?
        .with_account_cache(account, |cache| {
            let mut index = decode_cover_index(cache).map_err(store_error)?;
            let Some(position) = index.entries.iter().position(|entry| entry.key == key) else {
                return Ok(None);
            };
            let Some(bytes) = cache.read(CacheEntry::Cover(index.entries[position].slot))? else {
                return Ok(None);
            };
            if bytes.len() != index.entries[position].bytes as usize
                || bytes.get(..32) != Some(&raw_key[..])
            {
                return Ok(None);
            }
            validate_jpeg(&bytes[32..]).map_err(store_error)?;
            let used_at = tick(&mut index);
            index.entries[position].used_at = used_at;
            cache.reserve(0, COVER_RESERVATION)?;
            cache.write(
                CacheEntry::CoverIndex,
                &serde_json::to_vec(&index).map_err(|_| StoreError {
                    code: "CACHE_UNAVAILABLE",
                })?,
            )?;
            Ok(Some(format!(
                "data:image/jpeg;base64,{}",
                STANDARD.encode(&bytes[32..])
            )))
        })
        .map_err(account_error)
}

pub(crate) fn write_cover(root: &Path, account: &str, work_id: &str, data_url: &str) -> Result<()> {
    let jpeg = jpeg_bytes(data_url)?;
    let key = digest(work_id);
    let mut bytes = Sha256::digest(work_id.as_bytes()).to_vec();
    bytes.extend_from_slice(&jpeg);
    WorkbenchStore::open(root)
        .map_err(account_error)?
        .with_account_cache(account, |cache| {
            let mut index = decode_cover_index(cache).map_err(store_error)?;
            let position = index.entries.iter().position(|entry| entry.key == key);
            let slot = if let Some(position) = position {
                index.entries.remove(position).slot
            } else if index.entries.len() == 255 {
                let oldest = index
                    .entries
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, entry)| entry.used_at)
                    .map(|(i, _)| i)
                    .ok_or(StoreError {
                        code: "CACHE_UNAVAILABLE",
                    })?;
                index.entries.remove(oldest).slot
            } else {
                (0..255)
                    .find(|slot| !index.entries.iter().any(|entry| entry.slot == *slot))
                    .ok_or(StoreError {
                        code: "CACHE_UNAVAILABLE",
                    })?
            };
            let used_at = tick(&mut index);
            index.entries.push(CoverRecord {
                slot,
                key,
                used_at,
                bytes: bytes.len() as u64,
            });
            cache.reserve(0, COVER_RESERVATION)?;
            cache.write(CacheEntry::Cover(slot), &bytes)?;
            cache.write(
                CacheEntry::CoverIndex,
                &serde_json::to_vec(&index).map_err(|_| StoreError {
                    code: "CACHE_UNAVAILABLE",
                })?,
            )
        })
        .map_err(account_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn key(n: u8) -> String {
        format!("{n:064x}")
    }
    fn work(id: usize) -> SourceWork {
        SourceWork {
            source: Source::Jm,
            work_id: id.to_string(),
            title: format!("Synthetic {id}"),
            authors: vec![],
            description: None,
            tags: vec![],
            favorite: Some(true),
            chapter_count: None,
            page_count: None,
            cover_available: true,
        }
    }
    fn snapshot(count: usize, updated_at: u64) -> CatalogSnapshot {
        let items: Vec<_> = (0..count).map(work).collect();
        CatalogSnapshot {
            first_page_ids: items.iter().take(1000).map(|w| w.work_id.clone()).collect(),
            items,
            page: count.max(1).div_ceil(1000) as u64,
            total: Some(count as u64),
            pages: Some(count.max(1).div_ceil(1000) as u64),
            has_more: Some(false),
            folders: vec![],
            complete: true,
            updated_at,
        }
    }
    fn write(root: &Path, account: &str, snapshot: CatalogSnapshot) -> Result<CatalogResult> {
        catalog(
            root,
            account,
            Source::Jm,
            "generation",
            None,
            false,
            CatalogAction::Write,
            Some(snapshot),
        )
    }
    fn read(root: &Path, account: &str) -> Result<CatalogResult> {
        catalog(
            root,
            account,
            Source::Jm,
            "next-generation",
            None,
            false,
            CatalogAction::Read,
            None,
        )
    }
    fn code<T>(result: Result<T>) -> &'static str {
        match result {
            Ok(_) => panic!("expected cache error"),
            Err(error) => error.code,
        }
    }
    fn jpeg() -> String {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut bytes, ImageFormat::Jpeg)
            .unwrap();
        format!(
            "data:image/jpeg;base64,{}",
            STANDARD.encode(bytes.into_inner())
        )
    }

    #[test]
    fn two_thousand_items_survive_reopen_with_account_folder_and_direction_isolation() {
        let temp = TempDir::new().unwrap();
        let value = snapshot(2000, now_ms().unwrap());
        write(temp.path(), &key(1), value.clone()).unwrap();
        let restored = read(temp.path(), &key(1)).unwrap();
        assert_eq!(restored.session_id, "next-generation");
        assert_eq!(restored.snapshot, Some(value.clone()));
        assert_eq!(restored.complete_snapshot, Some(value));
        assert!(read(temp.path(), &key(2)).unwrap().snapshot.is_none());
        for (folder, reverse) in [(Some("7"), false), (None, true)] {
            assert!(catalog(
                temp.path(),
                &key(1),
                Source::Jm,
                "s",
                folder,
                reverse,
                CatalogAction::Read,
                None
            )
            .unwrap()
            .snapshot
            .is_none());
        }
    }

    #[test]
    fn partial_refresh_keeps_last_complete_and_another_instance_cannot_write_older_progress() {
        let temp = TempDir::new().unwrap();
        let time = now_ms().unwrap() - 100;
        let complete = snapshot(2, time);
        write(temp.path(), &key(1), complete.clone()).unwrap();
        let mut partial = snapshot(1, time + 1);
        partial.total = Some(2);
        partial.pages = Some(2);
        partial.has_more = Some(true);
        partial.complete = false;
        let next = write(temp.path(), &key(1), partial.clone()).unwrap();
        assert_eq!(next.complete_snapshot, Some(complete.clone()));
        assert_eq!(
            read(temp.path(), &key(1)).unwrap().snapshot,
            Some(partial.clone())
        );
        assert_eq!(
            code(write(temp.path(), &key(1), complete)),
            "CATALOG_CACHE_STALE"
        );
        let mut conflict = partial.clone();
        conflict.items[0].title = "Different write at same time".into();
        assert_eq!(
            code(write(temp.path(), &key(1), conflict)),
            "CATALOG_CACHE_STALE"
        );
        assert!(write(temp.path(), &key(1), partial).is_ok());
    }

    #[test]
    fn corrupt_catalog_is_preserved_and_never_replaced_by_a_default() {
        let temp = TempDir::new().unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        let corrupt = b"{broken-catalog";
        store
            .with_account_cache(&key(1), |cache| {
                cache.reserve(corrupt.len() as u64, 0)?;
                cache.write(CacheEntry::Catalog, corrupt)
            })
            .unwrap();
        assert_eq!(code(read(temp.path(), &key(1))), "CATALOG_CACHE_CORRUPT");
        assert_eq!(
            code(write(temp.path(), &key(1), snapshot(1, now_ms().unwrap()))),
            "CATALOG_CACHE_CORRUPT"
        );
        assert_eq!(
            store
                .with_account_cache(&key(1), |cache| cache.read(CacheEntry::Catalog))
                .unwrap(),
            Some(corrupt.to_vec())
        );
    }

    #[test]
    fn snapshot_schema_rejects_unknown_fields_duplicates_and_false_completion() {
        let now = now_ms().unwrap();
        let valid = snapshot(1, now);
        let mut json = serde_json::to_value(&valid).unwrap();
        json["token"] = serde_json::json!("untrusted");
        assert!(serde_json::from_value::<CatalogSnapshot>(json).is_err());
        let mut json = serde_json::to_value(&valid).unwrap();
        json.as_object_mut().unwrap().remove("hasMore");
        assert!(serde_json::from_value::<CatalogSnapshot>(json).is_err());
        let mut cases = vec![];
        let mut v = valid.clone();
        v.pages = Some(2);
        cases.push(v);
        let mut v = valid.clone();
        v.complete = false;
        v.has_more = Some(true);
        v.pages = None;
        cases.push(v);
        let mut v = valid.clone();
        v.items.push(v.items[0].clone());
        v.first_page_ids.push("0".into());
        v.total = Some(2);
        cases.push(v);
        let mut v = valid.clone();
        v.items[0].source = Source::Pica;
        cases.push(v);
        let mut v = valid.clone();
        v.updated_at = now + 300_001;
        cases.push(v);
        let mut v = valid.clone();
        v.first_page_ids.clear();
        cases.push(v);
        let mut v = snapshot(0, now);
        v.total = None;
        cases.push(v);
        let mut v = valid.clone();
        v.page = 2;
        v.pages = Some(2);
        cases.push(v);
        let mut v = valid;
        v.folders.push(SourceFolder {
            id: "../outside".into(),
            name: "Folder".into(),
            count: Some(1),
        });
        cases.push(v);
        for value in cases {
            assert_eq!(
                code(validate_snapshot(Source::Jm, &value, now)),
                "CATALOG_CACHE_INVALID"
            );
        }
        assert!(validate_snapshot(Source::Jm, &snapshot(0, now), now).is_ok());
    }

    #[test]
    fn scope_lru_evicts_whole_old_entries_and_keeps_the_requested_scope() {
        let temp = TempDir::new().unwrap();
        let value = snapshot(1, now_ms().unwrap());
        for id in 0..17 {
            catalog(
                temp.path(),
                &key(1),
                Source::Jm,
                "s",
                Some(&id.to_string()),
                false,
                CatalogAction::Write,
                Some(value.clone()),
            )
            .unwrap();
        }
        let latest = catalog(
            temp.path(),
            &key(1),
            Source::Jm,
            "s",
            Some("16"),
            false,
            CatalogAction::Read,
            None,
        )
        .unwrap();
        assert!(latest.snapshot.is_some());
        assert!(catalog(
            temp.path(),
            &key(1),
            Source::Jm,
            "s",
            Some("0"),
            false,
            CatalogAction::Read,
            None
        )
        .unwrap()
        .snapshot
        .is_none());
    }

    #[test]
    fn work_cache_keeps_two_thousand_and_evicts_individual_entries_by_bytes_and_recency() {
        let mut cache = WorkCache::default();
        for id in 0..2000 {
            let work = work(id);
            let size = validate_work(Source::Jm, &work).unwrap();
            cache.insert(work, size);
        }
        assert!(cache.contains_key("0"));
        assert!(cache.contains_key("1999"));
        cache.clear();
        for id in 0..600 {
            let mut work = work(id);
            work.description = Some("d".repeat(60 * 1024));
            let size = validate_work(Source::Jm, &work).unwrap();
            cache.insert(work, size);
        }
        assert!(cache.bytes <= MAX_METADATA_BYTES);
        assert!(!cache.contains_key("0"));
        assert!(cache.entries.len() > 500);
        assert!(cache.contains_key("599"));
        let oldest = cache.order.first_key_value().unwrap().1.clone();
        cache.contains_key(&oldest);
        let mut next = work(601);
        next.description = Some("d".repeat(60 * 1024));
        let size = validate_work(Source::Jm, &next).unwrap();
        cache.insert(next, size);
        assert!(cache.contains_key(&oldest));
    }

    #[test]
    fn jpeg_cover_survives_reopen_but_wrong_account_invalid_type_and_reused_slot_do_not() {
        let temp = TempDir::new().unwrap();
        let image = jpeg();
        write_cover(temp.path(), &key(1), "old-work", &image).unwrap();
        assert_eq!(
            read_cover(temp.path(), &key(1), "old-work").unwrap(),
            Some(image.clone())
        );
        assert!(read_cover(temp.path(), &key(2), "old-work")
            .unwrap()
            .is_none());
        assert_eq!(
            code(write_cover(
                temp.path(),
                &key(1),
                "bad",
                "data:image/png;base64,AA=="
            )),
            "SOURCE_COVER_INVALID"
        );
        assert_eq!(
            code(write_cover(
                temp.path(),
                &key(1),
                "bad",
                "data:image/jpeg;base64,AA=="
            )),
            "SOURCE_COVER_INVALID"
        );
        let store = WorkbenchStore::open(temp.path()).unwrap();
        store
            .with_account_cache(&key(1), |cache| {
                cache.reserve(0, 1)?;
                cache.write(CacheEntry::Cover(0), &vec![0; 256])
            })
            .unwrap();
        assert!(read_cover(temp.path(), &key(1), "old-work")
            .unwrap()
            .is_none());
    }

    #[test]
    fn cover_lru_reuses_one_fixed_slot_and_read_touch_keeps_recent_image() {
        let temp = TempDir::new().unwrap();
        let image = jpeg();
        write_cover(temp.path(), &key(1), "0", &image).unwrap();
        let store = WorkbenchStore::open(temp.path()).unwrap();
        store
            .with_account_cache(&key(1), |cache| {
                let old = decode_cover_index(cache).map_err(store_error)?;
                let bytes = old.entries[0].bytes;
                let index = CoverIndex {
                    version: 1,
                    sequence: 255,
                    entries: (0..255u8)
                        .map(|slot| CoverRecord {
                            slot,
                            key: digest(&slot.to_string()),
                            used_at: slot as u64 + 1,
                            bytes,
                        })
                        .collect(),
                };
                cache.reserve(0, 1)?;
                cache.write(CacheEntry::CoverIndex, &serde_json::to_vec(&index).unwrap())
            })
            .unwrap();
        assert_eq!(
            read_cover(temp.path(), &key(1), "0").unwrap(),
            Some(image.clone())
        );
        write_cover(temp.path(), &key(1), "new", &image).unwrap();
        store
            .with_account_cache(&key(1), |cache| {
                let index = decode_cover_index(cache).map_err(store_error)?;
                assert_eq!(index.entries.len(), 255);
                assert!(index.entries.iter().any(|entry| entry.key == digest("0")));
                assert!(!index.entries.iter().any(|entry| entry.key == digest("1")));
                assert_eq!(
                    index
                        .entries
                        .iter()
                        .find(|entry| entry.key == digest("new"))
                        .unwrap()
                        .slot,
                    1
                );
                Ok(())
            })
            .unwrap();
    }
}
