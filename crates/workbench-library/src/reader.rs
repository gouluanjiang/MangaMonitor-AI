//! Indexed ZIP access for reading. Only the requested image is inflated; media
//! never leaves the archive on disk and reading never updates library evidence.
mod pages;
#[cfg(test)]
mod tests;

use crate::{
    archive, error, hash,
    paths::{self, Node, Root, SafeFile},
    service::require_scope,
    LibraryEvidence, LibraryFormat, LibraryReference, Result,
};
use image::{ImageDecoder, ImageFormat, ImageReader, Limits};
use serde::Serialize;
use std::{io::Cursor, sync::Mutex};
use workbench_storage::{library_hash_is_valid, LibraryFileIdentity, WorkbenchStore};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderChapter {
    pub id: String,
    pub title: String,
    pub page_count: u64,
}

#[derive(Debug)]
pub struct ReaderImage {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
}

pub fn source_reader_key(reference: &LibraryReference) -> Result<String> {
    if !reference.is_valid() {
        return Err(error("VALIDATION_FAILED"));
    }
    Ok(hash(
        format!("source:{:?}:{}", reference.source, reference.work_id).as_bytes(),
    ))
}

pub struct LocalReader {
    root: Root,
    file: Mutex<SafeFile>,
    identity: LibraryFileIdentity,
    generation: u64,
    entry_id: String,
    relative_path: String,
    pub title: String,
    pub source_ref: Option<LibraryReference>,
    pub progress_key: String,
    chapters: Vec<pages::Chapter>,
}

impl LocalReader {
    pub fn open(
        store: &WorkbenchStore,
        root_id: &str,
        generation: u64,
        entry_id: &str,
    ) -> Result<Self> {
        if !library_hash_is_valid(entry_id) {
            return Err(error("VALIDATION_FAILED"));
        }
        let document = store.read_library_shared()?;
        require_scope(&document.value, root_id, generation)?;
        let record = document
            .value
            .records
            .iter()
            .find(|r| r.item.id == entry_id)
            .ok_or(error("LIBRARY_ENTRY_UNKNOWN"))?;
        if !matches!(record.item.format, LibraryFormat::Zip | LibraryFormat::Cbz) {
            return Err(error("READER_FORMAT_UNSUPPORTED"));
        }
        let root = Root::restore(
            document
                .value
                .root
                .as_ref()
                .ok_or(error("LIBRARY_NOT_CONFIGURED"))?,
        )?;
        let Node::File(mut file) = root.node(&record.item.relative_path)? else {
            return Err(error("LIBRARY_FILE_CHANGED"));
        };
        let identity = paths::identity(&file.file)?;
        if record.identity.as_ref() != Some(&identity) {
            return Err(error("LIBRARY_FILE_CHANGED"));
        }
        let chapters = pages::index(&mut file.file)?;
        if paths::identity(&file.file)? != identity {
            return Err(error("LIBRARY_FILE_CHANGED"));
        }
        let progress_key = hash(
            format!(
                "local:{}:{}:{}:{}:{}",
                root.saved.id, entry_id, identity.file_key, identity.bytes, identity.modified
            )
            .as_bytes(),
        );
        let source_ref = if matches!(
            record.item.identity_evidence,
            Some(LibraryEvidence::Metadata | LibraryEvidence::Manual)
        ) {
            record.item.source_ref.clone()
        } else {
            None
        };
        let reader = Self {
            root,
            file: Mutex::new(file),
            identity,
            generation,
            entry_id: entry_id.into(),
            relative_path: record.item.relative_path.clone(),
            title: record.item.title.clone(),
            source_ref,
            progress_key,
            chapters,
        };
        reader.verify(store)?;
        Ok(reader)
    }

    /// Only previously confirmed source identities are eligible. A missing file
    /// permits an online fallback; permission, identity and ZIP failures do not.
    pub fn for_source(
        store: &WorkbenchStore,
        reference: &LibraryReference,
    ) -> Result<Option<Self>> {
        if !reference.is_valid() {
            return Err(error("VALIDATION_FAILED"));
        }
        let document = store.read_library_shared()?;
        let Some(root) = &document.value.root else {
            return Ok(None);
        };
        let mut candidates = Vec::new();
        for reviewed in &document.value.reviewed_works {
            if &reviewed.reference == reference
                && document.value.records.iter().any(|r| {
                    r.item.id == reviewed.library_entry_id
                        && r.identity.as_ref() == Some(&reviewed.identity)
                })
            {
                candidates.push(reviewed.library_entry_id.as_str());
            }
        }
        for record in &document.value.records {
            let confirmed = (matches!(
                record.item.identity_evidence,
                Some(LibraryEvidence::Metadata | LibraryEvidence::Manual)
            ) && record.item.source_ref.as_ref() == Some(reference))
                || record
                    .item
                    .links
                    .iter()
                    .any(|link| &link.reference == reference);
            if confirmed && !candidates.contains(&record.item.id.as_str()) {
                candidates.push(&record.item.id);
            }
        }
        for entry_id in candidates {
            match Self::open(store, &root.id, document.value.generation, entry_id) {
                Ok(mut reader) => {
                    reader.source_ref = Some(reference.clone());
                    return Ok(Some(reader));
                }
                Err(problem) if problem.code == "LIBRARY_ENTRY_MISSING" => continue,
                Err(problem) => return Err(problem),
            }
        }
        Ok(None)
    }

    pub fn chapters(&self) -> Vec<ReaderChapter> {
        self.chapters
            .iter()
            .map(|c| ReaderChapter {
                id: c.id.clone(),
                title: c.title.clone(),
                page_count: c.pages.len() as u64,
            })
            .collect()
    }

    pub fn page_count(&self, chapter_id: &str) -> Result<u64> {
        self.chapters
            .iter()
            .find(|c| c.id == chapter_id)
            .map(|c| c.pages.len() as u64)
            .ok_or(error("READER_CHAPTER_UNKNOWN"))
    }

    pub fn verify(&self, store: &WorkbenchStore) -> Result<()> {
        self.root.verify()?;
        let document = store.read_library_shared()?;
        require_scope(&document.value, &self.root.saved.id, self.generation)?;
        let record = document
            .value
            .records
            .iter()
            .find(|r| r.item.id == self.entry_id)
            .ok_or(error("LIBRARY_ENTRY_UNKNOWN"))?;
        if record.identity.as_ref() != Some(&self.identity)
            || record.item.relative_path != self.relative_path
        {
            return Err(error("LIBRARY_FILE_CHANGED"));
        }
        let Node::File(file) = self.root.node(&self.relative_path)? else {
            return Err(error("LIBRARY_FILE_CHANGED"));
        };
        if paths::identity(&file.file)? != self.identity {
            return Err(error("LIBRARY_FILE_CHANGED"));
        }
        Ok(())
    }

    pub fn page(
        &self,
        store: &WorkbenchStore,
        chapter_id: &str,
        page_index: u64,
    ) -> Result<ReaderImage> {
        self.verify(store)?;
        let chapter = self
            .chapters
            .iter()
            .find(|c| c.id == chapter_id)
            .ok_or(error("READER_CHAPTER_UNKNOWN"))?;
        let page = chapter
            .pages
            .get(usize::try_from(page_index).map_err(|_| error("READER_PAGE_UNKNOWN"))?)
            .ok_or(error("READER_PAGE_UNKNOWN"))?;
        let mut file = self.file.lock().map_err(|_| error("READER_UNAVAILABLE"))?;
        if paths::identity(&file.file)? != self.identity {
            return Err(error("LIBRARY_FILE_CHANGED"));
        }
        let bytes = archive::cover(&mut file.file, page)?;
        if paths::identity(&file.file)? != self.identity {
            return Err(error("LIBRARY_FILE_CHANGED"));
        }
        drop(file);
        self.verify(store)?;
        original_image(bytes)
    }
}

/// Decode headers with allocation/dimension bounds. Preserve original encoded
/// pixels, including GIF animation; never route reader pages through thumbnails.
fn original_image(bytes: Vec<u8>) -> Result<ReaderImage> {
    if bytes.len() > archive::MAX_IMAGE_BYTES {
        return Err(error("READER_IMAGE_LIMIT"));
    }
    let format = image::guess_format(&bytes).map_err(|_| error("READER_IMAGE_INVALID"))?;
    let mime = match format {
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Png => "image/png",
        ImageFormat::WebP => "image/webp",
        ImageFormat::Gif => "image/gif",
        _ => return Err(error("READER_IMAGE_INVALID")),
    };
    let mut limits = Limits::default();
    limits.max_image_width = Some(20_000);
    limits.max_image_height = Some(20_000);
    limits.max_alloc = Some(128 * 1024 * 1024);
    let mut reader = ImageReader::with_format(Cursor::new(&bytes), format);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|_| error("READER_IMAGE_INVALID"))?;
    let (width, height) = decoder.dimensions();
    if width == 0
        || height == 0
        || width > 20_000
        || height > 20_000
        || u64::from(width) * u64::from(height) > 32_000_000
    {
        return Err(error("READER_IMAGE_LIMIT"));
    }
    drop(decoder);
    Ok(ReaderImage {
        bytes,
        mime,
        width,
        height,
    })
}
