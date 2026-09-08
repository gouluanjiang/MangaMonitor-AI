//! A6.11 exact source-media descriptor and deterministic staging-path contract.
//!
//! This module validates metadata only. It downloads no bytes and writes no files.
//! A descriptor set is bound to the exact non-transferable A6.10 authorization
//! generation and the exact A6.7 preflight evidence/proof/content scope.

use crate::{
    image_download_authorization::{
        ImageDownloadAuthorization, IMAGE_DOWNLOAD_AUTHORIZATION_SCHEMA_VERSION,
    },
    monitor::hash,
    source_preflight::{
        SourcePreflightEvidence, SourcePreflightProof, SOURCE_PREFLIGHT_SCHEMA_VERSION,
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

pub const SOURCE_MEDIA_DESCRIPTOR_SCHEMA_VERSION: u64 = 1;
const WRITE_SCOPE: &str = "COMMAND_OWNED_STAGING_ONLY";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaDescriptor {
    pub image_index: u64,
    pub source_media_id: String,
    pub request_url: String,
    pub source_format: String,
    /// `NONE` for Pica/GIF; `JM_SCRAMBLE_BLOCKS` for JM WEBP.
    pub transform: String,
    pub transform_parameter: u64,
    /// Portable path relative to the command-owned staging root.
    pub relative_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaChapterDescriptors {
    pub chapter_id: String,
    pub chapter_order: u64,
    /// Exact `/chapter_view_template` scramble ID for JM; must be absent for Pica.
    pub jm_scramble_id: Option<u64>,
    pub media: Vec<MediaDescriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceMediaDescriptorSet {
    pub schema_version: u64,
    pub command_id: String,
    pub task_id: String,
    pub work_id: String,
    pub task_revision: u64,
    pub target_hash: String,
    pub source: String,
    pub source_work_id: String,
    pub preflight_hash: String,
    pub expected_chapter_count: u64,
    pub expected_content_units: u64,
    pub staging_subdir: String,
    pub write_scope: String,
    pub chapters: Vec<MediaChapterDescriptors>,
    pub image_download_authorized: bool,
    pub staging_write_authorized: bool,
    pub inventory_mutation_authorized: bool,
    pub task_completion_authorized: bool,
    pub promotion_authorized: bool,
    pub replacement_authorized: bool,
    pub physical_delete_authorized: bool,
}

fn safe_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.ends_with(' ')
        && !value.ends_with('.')
        && !value.chars().any(|c| {
            c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\')
        })
}

fn portable_relative_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.contains('\\')
        && value.split('/').all(safe_segment)
}

fn format_allowed(value: &str) -> bool {
    matches!(value, "gif" | "webp" | "jpg" | "jpeg" | "png")
}

fn https_url_without_credentials_query_or_fragment(value: &str) -> bool {
    if !value.starts_with("https://")
        || value.contains('?')
        || value.contains('#')
        || value.contains('\n')
        || value.contains('\r')
    {
        return false;
    }
    let rest = &value["https://".len()..];
    let authority = rest.split('/').next().unwrap_or_default();
    !authority.is_empty() && !authority.contains('@')
}

fn url_extension(value: &str) -> Option<String> {
    let tail = value.rsplit('/').next()?;
    let (_, extension) = tail.rsplit_once('.')?;
    let extension = extension.to_ascii_lowercase();
    format_allowed(&extension).then_some(extension)
}

fn expected_path(chapter_order: u64, chapter_id: &str, image_index: u64, ext: &str) -> String {
    format!("chapters/{chapter_order:06}-{chapter_id}/{image_index:06}.{ext}")
}

fn md5_digest(input: &[u8]) -> [u8; 16] {
    const SHIFT: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9,
        14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
        4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a,
        0xa8304613, 0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be,
        0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340,
        0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
        0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8,
        0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c,
        0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92,
        0xffeff47d, 0x85845dd1, 0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1,
        0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];

    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_le_bytes());

    let mut a0 = 0x67452301u32;
    let mut b0 = 0xefcdab89u32;
    let mut c0 = 0x98badcfeu32;
    let mut d0 = 0x10325476u32;

    for chunk in message.as_chunks::<64>().0 {
        let mut words = [0u32; 16];
        for (index, word) in words.iter_mut().enumerate() {
            let start = index * 4;
            *word = u32::from_le_bytes([
                chunk[start],
                chunk[start + 1],
                chunk[start + 2],
                chunk[start + 3],
            ]);
        }

        let mut a = a0;
        let mut b = b0;
        let mut c = c0;
        let mut d = d0;

        for index in 0..64 {
            let (f, g) = match index {
                0..=15 => ((b & c) | ((!b) & d), index),
                16..=31 => ((d & b) | ((!d) & c), (5 * index + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * index + 5) % 16),
                _ => (c ^ (b | (!d)), (7 * index) % 16),
            };
            let next_b = b.wrapping_add(
                a.wrapping_add(f)
                    .wrapping_add(K[index])
                    .wrapping_add(words[g])
                    .rotate_left(SHIFT[index]),
            );
            a = d;
            d = c;
            c = b;
            b = next_b;
        }

        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut digest = [0u8; 16];
    digest[0..4].copy_from_slice(&a0.to_le_bytes());
    digest[4..8].copy_from_slice(&b0.to_le_bytes());
    digest[8..12].copy_from_slice(&c0.to_le_bytes());
    digest[12..16].copy_from_slice(&d0.to_le_bytes());
    digest
}

fn md5_hex(input: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = md5_digest(input);
    let mut output = String::with_capacity(32);
    for byte in digest {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

/// Exact pinned `jmcomic-downloader` scramble block algorithm.
fn jm_block_num(scramble_id: u64, chapter_id: u64, filename_stem: &str) -> u64 {
    if chapter_id < scramble_id {
        0
    } else if chapter_id < 268_850 {
        10
    } else {
        let modulus = if chapter_id < 421_926 { 10 } else { 8 };
        let digest = md5_hex(format!("{chapter_id}{filename_stem}").as_bytes());
        let last_ascii = u64::from(*digest.as_bytes().last().expect("md5 hex is non-empty"));
        (last_ascii % modulus) * 2 + 2
    }
}

fn validate_jm(chapter: &MediaChapterDescriptors, media: &MediaDescriptor) -> Result<(), String> {
    let chapter_id = chapter
        .chapter_id
        .parse::<u64>()
        .map_err(|_| "INVALID_JM_MEDIA_DESCRIPTOR")?;
    let scramble_id = chapter
        .jm_scramble_id
        .ok_or("MISSING_JM_MEDIA_SCRAMBLE_ID")?;
    if media.source_media_id.trim().is_empty() {
        return Err("INVALID_JM_MEDIA_DESCRIPTOR".into());
    }
    let expected_prefix = format!(
        "https://cdn-msp2.jmapiproxy2.cc/media/photos/{}/",
        chapter.chapter_id
    );
    if !media.request_url.starts_with(&expected_prefix)
        || media.request_url.len() <= expected_prefix.len()
        || !https_url_without_credentials_query_or_fragment(&media.request_url)
        || !matches!(media.source_format.as_str(), "gif" | "webp")
    {
        return Err("INVALID_JM_MEDIA_DESCRIPTOR".into());
    }
    let filename = &media.request_url[expected_prefix.len()..];
    if filename != media.source_media_id
        || filename.contains('/')
        || filename.contains("..")
        || url_extension(&media.request_url).as_deref() != Some(media.source_format.as_str())
    {
        return Err("INVALID_JM_MEDIA_DESCRIPTOR".into());
    }
    match media.source_format.as_str() {
        "gif" if media.transform == "NONE" && media.transform_parameter == 0 => Ok(()),
        "webp" if media.transform == "JM_SCRAMBLE_BLOCKS" => {
            let filename_stem = Path::new(filename)
                .file_stem()
                .and_then(|value| value.to_str())
                .filter(|value| !value.is_empty())
                .ok_or("INVALID_JM_MEDIA_DESCRIPTOR")?;
            let expected = jm_block_num(scramble_id, chapter_id, filename_stem);
            if media.transform_parameter != expected {
                return Err("INVALID_JM_MEDIA_TRANSFORM".into());
            }
            Ok(())
        }
        _ => Err("INVALID_JM_MEDIA_TRANSFORM".into()),
    }
}

fn validate_pica(chapter: &MediaChapterDescriptors, media: &MediaDescriptor) -> Result<(), String> {
    if chapter.jm_scramble_id.is_some()
        || chapter.chapter_id.len() != 24
        || !chapter.chapter_id.bytes().all(|b| b.is_ascii_hexdigit())
        || media.source_media_id.len() != 24
        || !media.source_media_id.bytes().all(|b| b.is_ascii_hexdigit())
        || !https_url_without_credentials_query_or_fragment(&media.request_url)
        || !media.request_url.contains("/static/")
        || !format_allowed(&media.source_format)
        || url_extension(&media.request_url).as_deref() != Some(media.source_format.as_str())
        || media.transform != "NONE"
        || media.transform_parameter != 0
    {
        return Err("INVALID_PICA_MEDIA_DESCRIPTOR".into());
    }
    Ok(())
}

fn validate_preflight_binding(
    authorization: &ImageDownloadAuthorization,
    evidence: &SourcePreflightEvidence,
    preflight: &SourcePreflightProof,
) -> Result<(), String> {
    if evidence.schema_version != SOURCE_PREFLIGHT_SCHEMA_VERSION
        || !evidence.source_enumeration_complete
        || evidence.image_bytes_downloaded
        || evidence.staging_written
        || hash(evidence) != authorization.preflight_hash
    {
        return Err("INVALID_SOURCE_MEDIA_PREFLIGHT_EVIDENCE".into());
    }
    if preflight.schema_version != SOURCE_PREFLIGHT_SCHEMA_VERSION
        || !preflight.source_scope_verified
        || preflight.image_download_authorized
        || preflight.staging_write_authorized
        || preflight.inventory_mutation_authorized
        || preflight.task_completion_authorized
        || preflight.promotion_authorized
        || preflight.replacement_authorized
        || preflight.physical_delete_authorized
    {
        return Err("INVALID_SOURCE_MEDIA_PREFLIGHT_PROOF".into());
    }
    if evidence.command_id != authorization.command_id
        || evidence.task_id != authorization.task_id
        || evidence.work_id != authorization.work_id
        || evidence.task_revision != authorization.task_revision
        || evidence.target_hash != authorization.target_hash
        || evidence.source != authorization.source
        || evidence.source_work_id != authorization.source_work_id
        || evidence.expected_chapter_count != authorization.expected_chapter_count
        || preflight.command_id != authorization.command_id
        || preflight.task_id != authorization.task_id
        || preflight.work_id != authorization.work_id
        || preflight.task_revision != authorization.task_revision
        || preflight.target_hash != authorization.target_hash
        || preflight.source != authorization.source
        || preflight.source_work_id != authorization.source_work_id
        || preflight.preflight_hash != authorization.preflight_hash
        || preflight.expected_chapter_count != authorization.expected_chapter_count
        || preflight.expected_content_units != authorization.expected_content_units
    {
        return Err("SOURCE_MEDIA_PREFLIGHT_AUTHORIZATION_MISMATCH".into());
    }

    let evidence_content_units = evidence.chapters.iter().try_fold(0u64, |sum, chapter| {
        sum.checked_add(chapter.expected_images)
            .ok_or("SOURCE_MEDIA_CONTENT_UNIT_OVERFLOW")
    })?;
    if evidence.expected_chapter_count
        != u64::try_from(evidence.chapters.len())
            .map_err(|_| "SOURCE_MEDIA_CHAPTER_COUNT_OVERFLOW")?
        || evidence_content_units != authorization.expected_content_units
        || preflight.upstream_commit != evidence.upstream_commit
        || preflight.completion_contract_version != evidence.completion_contract_version
        || preflight.scope != evidence.scope
        || preflight.chapter_pagination != evidence.chapter_pagination
        || preflight.expected_chapter_count != evidence.expected_chapter_count
        || preflight.chapters != evidence.chapters
        || preflight.expected_content_units != evidence_content_units
    {
        return Err("SOURCE_MEDIA_PREFLIGHT_PROOF_EVIDENCE_MISMATCH".into());
    }
    Ok(())
}

/// Validate an exact source-media set against the live A6.10 image/staging gate
/// and the exact A6.7 source evidence/proof that A6.10 authorized.
pub fn validate(
    authorization: &ImageDownloadAuthorization,
    evidence: &SourcePreflightEvidence,
    preflight: &SourcePreflightProof,
    descriptors: &SourceMediaDescriptorSet,
) -> Result<(), String> {
    if authorization.schema_version != IMAGE_DOWNLOAD_AUTHORIZATION_SCHEMA_VERSION
        || !authorization.live_preflight_generation_verified
        || !authorization.image_download_authorized
        || !authorization.staging_write_authorized
        || authorization.reusable_permit
        || authorization.inventory_mutation_authorized
        || authorization.task_completion_authorized
        || authorization.promotion_authorized
        || authorization.replacement_authorized
        || authorization.physical_delete_authorized
    {
        return Err("INVALID_IMAGE_DOWNLOAD_AUTHORIZATION".into());
    }
    validate_preflight_binding(authorization, evidence, preflight)?;
    if descriptors.schema_version != SOURCE_MEDIA_DESCRIPTOR_SCHEMA_VERSION {
        return Err("INVALID_SOURCE_MEDIA_DESCRIPTOR_SCHEMA".into());
    }
    if descriptors.command_id != authorization.command_id
        || descriptors.task_id != authorization.task_id
        || descriptors.work_id != authorization.work_id
        || descriptors.task_revision != authorization.task_revision
        || descriptors.target_hash != authorization.target_hash
        || descriptors.source != authorization.source
        || descriptors.source_work_id != authorization.source_work_id
        || descriptors.preflight_hash != authorization.preflight_hash
        || descriptors.expected_chapter_count != authorization.expected_chapter_count
        || descriptors.expected_content_units != authorization.expected_content_units
        || descriptors.staging_subdir != authorization.staging_subdir
        || descriptors.write_scope != authorization.write_scope
        || descriptors.write_scope != WRITE_SCOPE
    {
        return Err("SOURCE_MEDIA_AUTHORIZATION_BINDING_MISMATCH".into());
    }
    if !descriptors.image_download_authorized
        || !descriptors.staging_write_authorized
        || descriptors.inventory_mutation_authorized
        || descriptors.task_completion_authorized
        || descriptors.promotion_authorized
        || descriptors.replacement_authorized
        || descriptors.physical_delete_authorized
    {
        return Err("UNSAFE_SOURCE_MEDIA_DESCRIPTOR_CAPABILITIES".into());
    }

    let chapter_count = u64::try_from(descriptors.chapters.len())
        .map_err(|_| "SOURCE_MEDIA_CHAPTER_COUNT_OVERFLOW")?;
    if chapter_count != descriptors.expected_chapter_count
        || chapter_count
            != u64::try_from(preflight.chapters.len())
                .map_err(|_| "SOURCE_MEDIA_CHAPTER_COUNT_OVERFLOW")?
        || chapter_count == 0
    {
        return Err("SOURCE_MEDIA_CHAPTER_COUNT_MISMATCH".into());
    }

    let mut chapter_ids = BTreeSet::new();
    let mut chapter_orders = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut previous_order = 0u64;
    let mut total = 0u64;

    for (chapter, expected) in descriptors.chapters.iter().zip(&preflight.chapters) {
        if chapter.chapter_id != expected.chapter_id
            || chapter.chapter_order != expected.chapter_order
            || u64::try_from(chapter.media.len())
                .map_err(|_| "SOURCE_MEDIA_CONTENT_UNIT_OVERFLOW")?
                != expected.expected_images
        {
            return Err("SOURCE_MEDIA_PREFLIGHT_SCOPE_MISMATCH".into());
        }
        if chapter.chapter_order == 0
            || chapter.chapter_order <= previous_order
            || !chapter_ids.insert(chapter.chapter_id.clone())
            || !chapter_orders.insert(chapter.chapter_order)
            || chapter.media.is_empty()
        {
            return Err("SOURCE_MEDIA_CHAPTERS_NOT_CANONICAL".into());
        }
        previous_order = chapter.chapter_order;

        let mut media_ids = BTreeSet::new();
        for (offset, media) in chapter.media.iter().enumerate() {
            let expected_index =
                u64::try_from(offset + 1).map_err(|_| "SOURCE_MEDIA_INDEX_OVERFLOW")?;
            if media.image_index != expected_index
                || !media_ids.insert(media.source_media_id.clone())
                || !format_allowed(&media.source_format)
                || !portable_relative_path(&media.relative_path)
                || media.relative_path
                    != expected_path(
                        chapter.chapter_order,
                        &chapter.chapter_id,
                        media.image_index,
                        &media.source_format,
                    )
                || !paths.insert(media.relative_path.to_ascii_lowercase())
            {
                return Err("SOURCE_MEDIA_ITEMS_NOT_CANONICAL".into());
            }
            match descriptors.source.as_str() {
                "jm" => validate_jm(chapter, media)?,
                "pica" => validate_pica(chapter, media)?,
                _ => return Err("UNSUPPORTED_SOURCE_MEDIA_DESCRIPTOR_SOURCE".into()),
            }
            total = total
                .checked_add(1)
                .ok_or("SOURCE_MEDIA_CONTENT_UNIT_OVERFLOW")?;
        }
    }

    if total != descriptors.expected_content_units
        || total != preflight.expected_content_units
        || total == 0
    {
        return Err("SOURCE_MEDIA_CONTENT_UNIT_MISMATCH".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_matches_standard_vectors() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn jm_scramble_thresholds_match_pinned_worker() {
        assert_eq!(jm_block_num(200_000, 123_456, "001"), 0);
        assert_eq!(jm_block_num(100_000, 123_456, "001"), 10);
    }
}