//! A6.13 live source-media descriptor preparation.
//!
//! This layer is metadata-only. It re-reads the exact pinned source metadata
//! required to construct an A6.11 `SourceMediaDescriptorSet`, revalidates the
//! non-transferable A6.10 generation immediately before every source request,
//! and then runs the complete A6.11 validator before returning the work set.
//! It downloads no media bytes and writes no files.

use crate::{
    image_download_authorization::{
        ImageDownloadAuthorization, IMAGE_DOWNLOAD_AUTHORIZATION_SCHEMA_VERSION,
    },
    monitor::hash,
    source_media_descriptors::{
        self, MediaChapterDescriptors, MediaDescriptor, SourceMediaDescriptorSet,
        SOURCE_MEDIA_DESCRIPTOR_SCHEMA_VERSION,
    },
    source_preflight::{
        SourcePreflightEvidence, SourcePreflightProof, SOURCE_PREFLIGHT_SCHEMA_VERSION,
    },
};
use jm_adapter::media_descriptors::{JmChapterMediaEnumeration, IMAGE_DOMAIN};
use pica_adapter::media_descriptors::PicaChapterMediaEnumeration;

pub const LIVE_MEDIA_DESCRIPTOR_SCHEMA_VERSION: u64 = 1;
pub const PICA_MEDIA_DESCRIPTOR_MAX_PAGES: u64 = 1_000;
const WRITE_SCOPE: &str = "COMMAND_OWNED_STAGING_ONLY";

fn exact_authorization_generation(
    initial: &ImageDownloadAuthorization,
    current: ImageDownloadAuthorization,
) -> Result<(), String> {
    if &current != initial {
        return Err("LIVE_MEDIA_AUTHORIZATION_GENERATION_CHANGED".into());
    }
    Ok(())
}

fn validate_before_source_reads(
    authorization: &ImageDownloadAuthorization,
    evidence: &SourcePreflightEvidence,
    preflight: &SourcePreflightProof,
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
        || authorization.write_scope != WRITE_SCOPE
    {
        return Err("INVALID_LIVE_MEDIA_AUTHORIZATION".into());
    }
    if evidence.schema_version != SOURCE_PREFLIGHT_SCHEMA_VERSION
        || !evidence.source_enumeration_complete
        || evidence.image_bytes_downloaded
        || evidence.staging_written
        || hash(evidence) != authorization.preflight_hash
        || evidence.command_id != authorization.command_id
        || evidence.task_id != authorization.task_id
        || evidence.work_id != authorization.work_id
        || evidence.task_revision != authorization.task_revision
        || evidence.target_hash != authorization.target_hash
        || evidence.source != authorization.source
        || evidence.source_work_id != authorization.source_work_id
        || evidence.expected_chapter_count != authorization.expected_chapter_count
    {
        return Err("INVALID_LIVE_MEDIA_PREFLIGHT_EVIDENCE".into());
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
        || preflight.chapter_pagination != evidence.chapter_pagination
        || preflight.expected_chapter_count != evidence.expected_chapter_count
        || preflight.chapters != evidence.chapters
    {
        return Err("INVALID_LIVE_MEDIA_PREFLIGHT_PROOF".into());
    }
    let expected_content_units = evidence.chapters.iter().try_fold(0u64, |sum, chapter| {
        sum.checked_add(chapter.expected_images)
            .ok_or("LIVE_MEDIA_CONTENT_UNIT_OVERFLOW")
    })?;
    if expected_content_units == 0
        || expected_content_units != authorization.expected_content_units
        || expected_content_units != preflight.expected_content_units
        || evidence.chapters.is_empty()
        || u64::try_from(evidence.chapters.len())
            .map_err(|_| "LIVE_MEDIA_CHAPTER_COUNT_OVERFLOW")?
            != authorization.expected_chapter_count
    {
        return Err("LIVE_MEDIA_PREFLIGHT_SCOPE_MISMATCH".into());
    }
    match authorization.source.as_str() {
        "jm" if evidence.upstream_commit == jm_adapter::UPSTREAM_COMMIT
            && preflight.upstream_commit == jm_adapter::UPSTREAM_COMMIT => {}
        "pica"
            if evidence.upstream_commit == pica_adapter::UPSTREAM_COMMIT
                && preflight.upstream_commit == pica_adapter::UPSTREAM_COMMIT => {}
        "jm" | "pica" => return Err("LIVE_MEDIA_UPSTREAM_PIN_MISMATCH".into()),
        _ => return Err("UNSUPPORTED_LIVE_MEDIA_DESCRIPTOR_SOURCE".into()),
    }
    Ok(())
}

fn relative_path(chapter_order: u64, chapter_id: &str, image_index: u64, format: &str) -> String {
    format!("chapters/{chapter_order:06}-{chapter_id}/{image_index:06}.{format}")
}

fn base_set(authorization: &ImageDownloadAuthorization) -> SourceMediaDescriptorSet {
    SourceMediaDescriptorSet {
        schema_version: SOURCE_MEDIA_DESCRIPTOR_SCHEMA_VERSION,
        command_id: authorization.command_id.clone(),
        task_id: authorization.task_id.clone(),
        work_id: authorization.work_id.clone(),
        task_revision: authorization.task_revision,
        target_hash: authorization.target_hash.clone(),
        source: authorization.source.clone(),
        source_work_id: authorization.source_work_id.clone(),
        preflight_hash: authorization.preflight_hash.clone(),
        expected_chapter_count: authorization.expected_chapter_count,
        expected_content_units: authorization.expected_content_units,
        staging_subdir: authorization.staging_subdir.clone(),
        write_scope: authorization.write_scope.clone(),
        chapters: Vec::new(),
        image_download_authorized: true,
        staging_write_authorized: true,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    }
}

fn build_jm_set(
    authorization: &ImageDownloadAuthorization,
    preflight: &SourcePreflightProof,
    enumerations: &[JmChapterMediaEnumeration],
) -> Result<SourceMediaDescriptorSet, String> {
    if enumerations.len() != preflight.chapters.len() {
        return Err("JM_LIVE_MEDIA_CHAPTER_COUNT_MISMATCH".into());
    }
    let mut set = base_set(authorization);
    for (expected, live) in preflight.chapters.iter().zip(enumerations) {
        if expected.chapter_id != live.chapter_id
            || expected.image_pagination.is_some()
            || u64::try_from(live.media.len()).map_err(|_| "LIVE_MEDIA_CONTENT_UNIT_OVERFLOW")?
                != expected.expected_images
        {
            return Err("JM_LIVE_MEDIA_PREFLIGHT_SCOPE_MISMATCH".into());
        }
        let mut media = Vec::with_capacity(live.media.len());
        for (offset, item) in live.media.iter().enumerate() {
            let image_index = u64::try_from(offset + 1).map_err(|_| "LIVE_MEDIA_INDEX_OVERFLOW")?;
            let (transform, transform_parameter) = match item.source_format.as_str() {
                "gif" if item.block_num == 0 => ("NONE", 0),
                "webp" => ("JM_SCRAMBLE_BLOCKS", item.block_num),
                _ => return Err("INVALID_JM_LIVE_MEDIA_FORMAT".into()),
            };
            media.push(MediaDescriptor {
                image_index,
                source_media_id: item.filename.clone(),
                request_url: format!(
                    "https://{IMAGE_DOMAIN}/media/photos/{}/{}",
                    live.chapter_id, item.filename
                ),
                source_format: item.source_format.clone(),
                transform: transform.into(),
                transform_parameter,
                relative_path: relative_path(
                    expected.chapter_order,
                    &expected.chapter_id,
                    image_index,
                    &item.source_format,
                ),
            });
        }
        set.chapters.push(MediaChapterDescriptors {
            chapter_id: expected.chapter_id.clone(),
            chapter_order: expected.chapter_order,
            jm_scramble_id: Some(live.scramble_id),
            media,
        });
    }
    Ok(set)
}

fn build_pica_set(
    authorization: &ImageDownloadAuthorization,
    preflight: &SourcePreflightProof,
    enumerations: &[PicaChapterMediaEnumeration],
) -> Result<SourceMediaDescriptorSet, String> {
    if enumerations.len() != preflight.chapters.len() {
        return Err("PICA_LIVE_MEDIA_CHAPTER_COUNT_MISMATCH".into());
    }
    let mut set = base_set(authorization);
    for (expected, live) in preflight.chapters.iter().zip(enumerations) {
        let expected_pagination = expected
            .image_pagination
            .as_ref()
            .ok_or("PICA_LIVE_MEDIA_PAGINATION_PROOF_REQUIRED")?;
        if live.chapter_order != expected.chapter_order
            || live.total_pages != expected_pagination.total_pages
            || live.successful_pages != expected_pagination.successful_pages
            || !expected_pagination.failed_pages.is_empty()
            || u64::try_from(live.media.len()).map_err(|_| "LIVE_MEDIA_CONTENT_UNIT_OVERFLOW")?
                != expected.expected_images
        {
            return Err("PICA_LIVE_MEDIA_PREFLIGHT_SCOPE_MISMATCH".into());
        }
        let mut media = Vec::with_capacity(live.media.len());
        for (offset, item) in live.media.iter().enumerate() {
            let image_index = u64::try_from(offset + 1).map_err(|_| "LIVE_MEDIA_INDEX_OVERFLOW")?;
            media.push(MediaDescriptor {
                image_index,
                source_media_id: item.media_id.clone(),
                request_url: format!("{}/static/{}", item.file_server, item.path),
                source_format: item.source_format.clone(),
                transform: "NONE".into(),
                transform_parameter: 0,
                relative_path: relative_path(
                    expected.chapter_order,
                    &expected.chapter_id,
                    image_index,
                    &item.source_format,
                ),
            });
        }
        set.chapters.push(MediaChapterDescriptors {
            chapter_id: expected.chapter_id.clone(),
            chapter_order: expected.chapter_order,
            jm_scramble_id: None,
            media,
        });
    }
    Ok(set)
}

/// Re-read pinned source metadata and materialize the exact A6.11 media work
/// set. The supplied reauthorization callback must produce the same A6.10
/// authorization generation; it is invoked by the adapters immediately before
/// every source request and once again after final validation.
pub async fn run_live<Reauthorize>(
    authorization: &ImageDownloadAuthorization,
    evidence: &SourcePreflightEvidence,
    preflight: &SourcePreflightProof,
    pica_token: Option<&str>,
    reauthorize: Reauthorize,
) -> Result<SourceMediaDescriptorSet, String>
where
    Reauthorize: FnMut() -> Result<ImageDownloadAuthorization, String>,
{
    run_with_pacing(
        authorization,
        evidence,
        preflight,
        pica_token,
        false,
        reauthorize,
    )
    .await
}

pub(crate) async fn run_for_download<Reauthorize>(
    authorization: &ImageDownloadAuthorization,
    evidence: &SourcePreflightEvidence,
    preflight: &SourcePreflightProof,
    pica_token: Option<&str>,
    reauthorize: Reauthorize,
) -> Result<SourceMediaDescriptorSet, String>
where
    Reauthorize: FnMut() -> Result<ImageDownloadAuthorization, String>,
{
    run_with_pacing(
        authorization,
        evidence,
        preflight,
        pica_token,
        true,
        reauthorize,
    )
    .await
}

async fn run_with_pacing<Reauthorize>(
    authorization: &ImageDownloadAuthorization,
    evidence: &SourcePreflightEvidence,
    preflight: &SourcePreflightProof,
    pica_token: Option<&str>,
    download: bool,
    mut reauthorize: Reauthorize,
) -> Result<SourceMediaDescriptorSet, String>
where
    Reauthorize: FnMut() -> Result<ImageDownloadAuthorization, String>,
{
    validate_before_source_reads(authorization, evidence, preflight)?;

    let descriptors = match authorization.source.as_str() {
        "jm" => {
            let mut client = if download {
                jm_adapter::JmClient::new_for_download(jm_adapter::DEFAULT_DOMAIN)?
            } else {
                jm_adapter::JmClient::new(jm_adapter::DEFAULT_DOMAIN)?
            };
            let mut enumerations = Vec::with_capacity(preflight.chapters.len());
            for chapter in &preflight.chapters {
                let live = client
                    .live_chapter_media_with_guard(&chapter.chapter_id, || {
                        exact_authorization_generation(authorization, reauthorize()?)
                    })
                    .await?;
                enumerations.push(live);
            }
            build_jm_set(authorization, preflight, &enumerations)?
        }
        "pica" => {
            let token = pica_token
                .filter(|value| !value.trim().is_empty())
                .ok_or("PICA_LIVE_MEDIA_TOKEN_REQUIRED")?;
            let mut client = if download {
                pica_adapter::PicaClient::new_for_download(token.to_owned())?
            } else {
                pica_adapter::PicaClient::new(token.to_owned())?
            };
            let mut enumerations = Vec::with_capacity(preflight.chapters.len());
            for chapter in &preflight.chapters {
                let expected_pagination = chapter
                    .image_pagination
                    .as_ref()
                    .ok_or("PICA_LIVE_MEDIA_PAGINATION_PROOF_REQUIRED")?;
                if expected_pagination.total_pages == 0
                    || expected_pagination.total_pages > PICA_MEDIA_DESCRIPTOR_MAX_PAGES
                {
                    return Err("PICA_LIVE_MEDIA_PAGE_BUDGET_INVALID".into());
                }
                let live = client
                    .live_chapter_media_with_guard(
                        &authorization.source_work_id,
                        chapter.chapter_order,
                        expected_pagination.total_pages,
                        || exact_authorization_generation(authorization, reauthorize()?),
                    )
                    .await?;
                enumerations.push(live);
            }
            build_pica_set(authorization, preflight, &enumerations)?
        }
        _ => return Err("UNSUPPORTED_LIVE_MEDIA_DESCRIPTOR_SOURCE".into()),
    };

    source_media_descriptors::validate(authorization, evidence, preflight, &descriptors)?;
    exact_authorization_generation(authorization, reauthorize()?)?;
    Ok(descriptors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{source_completion::PaginationProof, source_preflight::PreflightChapter};
    use jm_adapter::media_descriptors::JmMediaItem;
    use pica_adapter::media_descriptors::PicaMediaItem;

    fn authorization(source: &str) -> ImageDownloadAuthorization {
        ImageDownloadAuthorization {
            schema_version: 1,
            command_id: "EXEC_1234567890abcdef1234".into(),
            task_id: "TASK_A6_13".into(),
            work_id: "WORK_A6_13".into(),
            task_revision: 7,
            target_hash: "a".repeat(64),
            source: source.into(),
            source_work_id: if source == "jm" {
                "123456".into()
            } else {
                "0123456789abcdef01234567".into()
            },
            preflight_hash: "b".repeat(64),
            expected_chapter_count: 1,
            expected_content_units: 2,
            staging_subdir: "commands/EXEC_1234567890abcdef1234".into(),
            write_scope: WRITE_SCOPE.into(),
            current_state_binding_hash: "c".repeat(64),
            current_gate_ledger_hash: "d".repeat(64),
            live_preflight_generation_verified: true,
            image_download_authorized: true,
            staging_write_authorized: true,
            reusable_permit: false,
            inventory_mutation_authorized: false,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
        }
    }

    fn proof(source: &str) -> SourcePreflightProof {
        let pagination = (source == "pica").then(|| PaginationProof {
            total_pages: 2,
            successful_pages: vec![1, 2],
            failed_pages: vec![],
        });
        SourcePreflightProof {
            schema_version: 1,
            command_id: "EXEC_1234567890abcdef1234".into(),
            task_id: "TASK_A6_13".into(),
            work_id: "WORK_A6_13".into(),
            task_revision: 7,
            target_hash: "a".repeat(64),
            source: source.into(),
            source_work_id: if source == "jm" {
                "123456".into()
            } else {
                "0123456789abcdef01234567".into()
            },
            upstream_commit: if source == "jm" {
                jm_adapter::UPSTREAM_COMMIT.into()
            } else {
                pica_adapter::UPSTREAM_COMMIT.into()
            },
            completion_contract_version: 1,
            scope: "FULL_SOURCE_WORK".into(),
            preflight_hash: "b".repeat(64),
            chapter_pagination: pagination.clone(),
            expected_chapter_count: 1,
            chapters: vec![PreflightChapter {
                chapter_id: if source == "jm" {
                    "123456".into()
                } else {
                    "111111111111111111111111".into()
                },
                chapter_order: 1,
                expected_images: 2,
                image_pagination: pagination,
            }],
            expected_content_units: 2,
            source_scope_verified: true,
            image_download_authorized: false,
            staging_write_authorized: false,
            inventory_mutation_authorized: false,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
        }
    }

    #[test]
    fn jm_live_metadata_materializes_exact_descriptor_inputs() {
        let auth = authorization("jm");
        let proof = proof("jm");
        let set = build_jm_set(
            &auth,
            &proof,
            &[JmChapterMediaEnumeration {
                chapter_id: "123456".into(),
                scramble_id: 100_000,
                media: vec![
                    JmMediaItem {
                        filename: "001.webp".into(),
                        source_format: "webp".into(),
                        block_num: 10,
                    },
                    JmMediaItem {
                        filename: "002.GIF".into(),
                        source_format: "gif".into(),
                        block_num: 0,
                    },
                ],
            }],
        )
        .unwrap();
        assert_eq!(set.chapters[0].jm_scramble_id, Some(100_000));
        assert_eq!(
            set.chapters[0].media[0].request_url,
            "https://cdn-msp2.jmapiproxy2.cc/media/photos/123456/001.webp"
        );
        assert_eq!(set.chapters[0].media[0].transform, "JM_SCRAMBLE_BLOCKS");
        assert_eq!(set.chapters[0].media[0].transform_parameter, 10);
        assert_eq!(set.chapters[0].media[1].transform, "NONE");
    }

    #[test]
    fn jm_count_or_chapter_drift_fails_closed() {
        let auth = authorization("jm");
        let proof = proof("jm");
        let live = JmChapterMediaEnumeration {
            chapter_id: "654321".into(),
            scramble_id: 100_000,
            media: vec![],
        };
        assert!(build_jm_set(&auth, &proof, &[live]).is_err());
    }

    #[test]
    fn pica_live_metadata_binds_pagination_id_url_extension_and_order() {
        let auth = authorization("pica");
        let proof = proof("pica");
        let set = build_pica_set(
            &auth,
            &proof,
            &[PicaChapterMediaEnumeration {
                chapter_order: 1,
                total_pages: 2,
                successful_pages: vec![1, 2],
                media: vec![
                    PicaMediaItem {
                        media_id: "222222222222222222222222".into(),
                        original_name: "page1.jpg".into(),
                        file_server: "https://storage.example.invalid".into(),
                        path: "media/path/page1.jpg".into(),
                        source_format: "jpg".into(),
                    },
                    PicaMediaItem {
                        media_id: "333333333333333333333333".into(),
                        original_name: "page2.WEBP".into(),
                        file_server: "https://storage.example.invalid".into(),
                        path: "media/path/page2.WEBP".into(),
                        source_format: "webp".into(),
                    },
                ],
            }],
        )
        .unwrap();
        assert_eq!(set.chapters[0].chapter_id, "111111111111111111111111");
        assert_eq!(
            set.chapters[0].media[0].request_url,
            "https://storage.example.invalid/static/media/path/page1.jpg"
        );
        assert_eq!(set.chapters[0].media[1].source_format, "webp");
        assert_eq!(set.chapters[0].media[1].transform, "NONE");
        assert!(!set.inventory_mutation_authorized);
        assert!(!set.task_completion_authorized);
        assert!(!set.promotion_authorized);
        assert!(!set.replacement_authorized);
        assert!(!set.physical_delete_authorized);
    }

    #[test]
    fn pica_pagination_or_expected_count_drift_fails_closed() {
        let auth = authorization("pica");
        let proof = proof("pica");
        let live = PicaChapterMediaEnumeration {
            chapter_order: 1,
            total_pages: 1,
            successful_pages: vec![1],
            media: vec![],
        };
        assert!(build_pica_set(&auth, &proof, &[live]).is_err());
    }

    #[test]
    fn changed_authorization_generation_fails_closed() {
        let auth = authorization("jm");
        let mut changed = auth.clone();
        changed.current_gate_ledger_hash = "e".repeat(64);
        assert_eq!(
            exact_authorization_generation(&auth, changed).unwrap_err(),
            "LIVE_MEDIA_AUTHORIZATION_GENERATION_CHANGED"
        );
    }
}
