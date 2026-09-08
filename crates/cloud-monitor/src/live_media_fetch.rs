//! A6.14 guarded live media-byte transport into the A6.12 staging kernel.
//!
//! This layer adds no new completion/promotion authority. It validates the
//! exact A6.11 descriptor set, applies only descriptor-bound transforms that
//! are reproduced from the pinned upstream, chooses a credential-isolated
//! source media client, and delegates filesystem mutation/completion proof to
//! A6.12.

use crate::{
    image_download_authorization::ImageDownloadAuthorization,
    isolated_staging_execution::{
        self, IsolatedStagingExecutionContext, IsolatedStagingExecutionResult, ProcessedMedia,
    },
    jm_media_transform,
    media_validation,
    live_media_transport::{JmTransport, PicaTransport},
    source_media_descriptors::{self, MediaDescriptor, SourceMediaDescriptorSet},
};

pub const LIVE_MEDIA_FETCH_SCHEMA_VERSION: u64 = 1;

#[derive(Clone)]
enum LiveFetcher {
    Jm(JmTransport),
    Pica(PicaTransport),
}

impl LiveFetcher {
    fn new(source: &str) -> Result<Self, String> {
        match source {
            "jm" => Ok(Self::Jm(JmTransport::new()?)),
            "pica" => Ok(Self::Pica(PicaTransport::new()?)),
            _ => Err("UNSUPPORTED_LIVE_MEDIA_FETCH_SOURCE".into()),
        }
    }

    async fn fetch_processed(&self, descriptor: MediaDescriptor) -> Result<ProcessedMedia, String> {
        let (source, bytes) = match self {
            Self::Jm(fetcher) => ("jm", fetcher.fetch_exact(&descriptor.request_url).await?),
            Self::Pica(fetcher) => ("pica", fetcher.fetch_exact(&descriptor.request_url).await?),
        };
        process_downloaded_bytes(source, &descriptor, bytes)
    }
}

/// A6.14B enables exactly the pinned JM block unscramble for WEBP descriptors.
/// The transform parameter is still supplied by the already-validated A6.13
/// descriptor and no transform can be inferred from response bytes.
fn validate_supported_fetch_scope(descriptors: &SourceMediaDescriptorSet) -> Result<(), String> {
    for chapter in &descriptors.chapters {
        for media in &chapter.media {
            match (
                descriptors.source.as_str(),
                media.source_format.as_str(),
                media.transform.as_str(),
                media.transform_parameter,
            ) {
                ("pica", _, "NONE", 0) => {}
                ("jm", "gif", "NONE", 0) => {}
                ("jm", "webp", "JM_SCRAMBLE_BLOCKS", _) => {}
                _ => return Err("LIVE_MEDIA_FETCH_TRANSFORM_NOT_SUPPORTED".into()),
            }
        }
    }
    Ok(())
}

fn process_downloaded_bytes(
    source: &str,
    descriptor: &MediaDescriptor,
    bytes: Vec<u8>,
) -> Result<ProcessedMedia, String> {
    if bytes.is_empty() {
        return Err("LIVE_MEDIA_SOURCE_FORMAT_MISMATCH".into());
    }
    media_validation::validate(&descriptor.source_format, &bytes)
        .map_err(|_| "LIVE_MEDIA_SOURCE_IMAGE_DECODE_FAILED")?;

    let bytes = match source {
        "jm" => jm_media_transform::apply(
            &descriptor.source_format,
            &descriptor.transform,
            descriptor.transform_parameter,
            bytes,
        )?,
        "pica" => match (
            descriptor.transform.as_str(),
            descriptor.transform_parameter,
        ) {
            ("NONE", 0) => bytes,
            _ => return Err("LIVE_MEDIA_FETCH_TRANSFORM_NOT_SUPPORTED".into()),
        },
        _ => return Err("UNSUPPORTED_LIVE_MEDIA_FETCH_SOURCE".into()),
    };

    media_validation::validate(&descriptor.source_format, &bytes)
        .map_err(|_| "LIVE_MEDIA_PROCESSED_IMAGE_DECODE_FAILED")?;

    Ok(ProcessedMedia {
        source_media_id: descriptor.source_media_id.clone(),
        request_url: descriptor.request_url.clone(),
        source_format: descriptor.source_format.clone(),
        applied_transform: descriptor.transform.clone(),
        applied_transform_parameter: descriptor.transform_parameter,
        bytes,
    })
}

/// Execute an exact A6.13 descriptor set with real source media GETs and the
/// already-audited A6.12 command-owned staging/completion chain.
///
/// There is deliberately no Pica token parameter. A6.12 invokes
/// `reauthorize` immediately before every call into the fetcher and before every
/// file write, and verifies the same generation again after filesystem proof.
pub async fn execute_live<Reauthorize>(
    context: IsolatedStagingExecutionContext<'_>,
    reauthorize: Reauthorize,
) -> Result<IsolatedStagingExecutionResult, String>
where
    Reauthorize: FnMut() -> Result<ImageDownloadAuthorization, String>,
{
    source_media_descriptors::validate(
        context.authorization,
        context.evidence,
        context.preflight,
        context.descriptors,
    )?;
    validate_supported_fetch_scope(context.descriptors)?;

    let fetcher = LiveFetcher::new(&context.authorization.source)?;
    isolated_staging_execution::execute_with_fetcher(
        context,
        move |descriptor| {
            let fetcher = fetcher.clone();
            async move { fetcher.fetch_processed(descriptor).await }
        },
        reauthorize,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    use std::io::Cursor;

    fn valid_image(format: ImageFormat) -> Vec<u8> {
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(2, 2, Rgb([12, 34, 56])));
        let mut output = Cursor::new(Vec::new());
        image.write_to(&mut output, format).unwrap();
        output.into_inner()
    }
    use crate::source_media_descriptors::MediaChapterDescriptors;

    fn descriptor(format: &str, transform: &str, parameter: u64) -> MediaDescriptor {
        MediaDescriptor {
            image_index: 1,
            source_media_id: if format == "gif" || format == "webp" {
                format!("001.{format}")
            } else {
                "222222222222222222222222".into()
            },
            request_url: match format {
                "gif" | "webp" => {
                    format!("https://cdn-msp2.jmapiproxy2.cc/media/photos/123456/001.{format}")
                }
                _ => format!("https://storage-b.picacomic.com/static/path/001.{format}"),
            },
            source_format: format.into(),
            transform: transform.into(),
            transform_parameter: parameter,
            relative_path: format!("chapters/000001-chapter/000001.{format}"),
        }
    }

    fn descriptor_set(source: &str, media: MediaDescriptor) -> SourceMediaDescriptorSet {
        SourceMediaDescriptorSet {
            schema_version: 1,
            command_id: "EXEC_test".into(),
            task_id: "TASK_test".into(),
            work_id: "WORK_test".into(),
            task_revision: 1,
            target_hash: "a".repeat(64),
            source: source.into(),
            source_work_id: "work".into(),
            preflight_hash: "b".repeat(64),
            expected_chapter_count: 1,
            expected_content_units: 1,
            staging_subdir: "commands/EXEC_test".into(),
            write_scope: "COMMAND_OWNED_STAGING_ONLY".into(),
            chapters: vec![MediaChapterDescriptors {
                chapter_id: "chapter".into(),
                chapter_order: 1,
                jm_scramble_id: (source == "jm").then_some(220_980),
                media: vec![media],
            }],
            image_download_authorized: true,
            staging_write_authorized: true,
            inventory_mutation_authorized: false,
            task_completion_authorized: false,
            promotion_authorized: false,
            replacement_authorized: false,
            physical_delete_authorized: false,
        }
    }

    #[test]
    fn pica_transform_free_media_is_supported() {
        let set = descriptor_set("pica", descriptor("png", "NONE", 0));
        validate_supported_fetch_scope(&set).unwrap();
        let processed = process_downloaded_bytes(
            "pica",
            &set.chapters[0].media[0],
            valid_image(ImageFormat::Png),
        )
        .unwrap();
        assert_eq!(processed.applied_transform, "NONE");
        assert_eq!(processed.applied_transform_parameter, 0);
    }

    #[test]
    fn valid_gif_and_zero_block_webp_are_exact_noop_transforms() {
        let gif = descriptor("gif", "NONE", 0);
        let gif_set = descriptor_set("jm", gif.clone());
        validate_supported_fetch_scope(&gif_set).unwrap();
        assert_eq!(
            process_downloaded_bytes("jm", &gif, valid_image(ImageFormat::Gif))
                .unwrap()
                .bytes,
            valid_image(ImageFormat::Gif)
        );

        let webp = descriptor("webp", "JM_SCRAMBLE_BLOCKS", 0);
        let webp_set = descriptor_set("jm", webp.clone());
        validate_supported_fetch_scope(&webp_set).unwrap();
        let bytes = valid_image(ImageFormat::WebP);
        assert_eq!(
            process_downloaded_bytes("jm", &webp, bytes.clone())
                .unwrap()
                .bytes,
            bytes
        );
    }

    #[test]
    fn truncated_supported_images_never_reach_processed_media() {
        for (format, transform) in [
            ("gif", "NONE"),
            ("webp", "NONE"),
            ("png", "NONE"),
            ("jpg", "NONE"),
        ] {
            let media = descriptor(format, transform, 0);
            let valid = valid_image(match format {
                "gif" => ImageFormat::Gif,
                "webp" => ImageFormat::WebP,
                "png" => ImageFormat::Png,
                "jpg" => ImageFormat::Jpeg,
                _ => unreachable!(),
            });
            let truncated = valid[..valid.len() / 2].to_vec();
            assert_eq!(
                process_downloaded_bytes("pica", &media, truncated).unwrap_err(),
                "LIVE_MEDIA_SOURCE_IMAGE_DECODE_FAILED"
            );
        }
    }

    #[test]
    fn positive_jm_scramble_is_allowed_only_for_bound_webp_transform() {
        let set = descriptor_set("jm", descriptor("webp", "JM_SCRAMBLE_BLOCKS", 10));
        validate_supported_fetch_scope(&set).unwrap();

        let unsupported = descriptor_set("jm", descriptor("gif", "JM_SCRAMBLE_BLOCKS", 10));
        assert_eq!(
            validate_supported_fetch_scope(&unsupported).unwrap_err(),
            "LIVE_MEDIA_FETCH_TRANSFORM_NOT_SUPPORTED"
        );
    }

    #[test]
    fn positive_jm_scramble_requires_a_decodable_webp() {
        let media = descriptor("webp", "JM_SCRAMBLE_BLOCKS", 10);
        let err = process_downloaded_bytes(
            "jm",
            &media,
            b"RIFF1234WEBPnot-a-real-webp".to_vec(),
        )
        .unwrap_err();
        assert_eq!(err, "LIVE_MEDIA_SOURCE_IMAGE_DECODE_FAILED");
    }

    #[test]
    fn content_magic_mismatch_fails_before_transform_or_a6_12_write() {
        let media = descriptor("png", "NONE", 0);
        assert_eq!(
            process_downloaded_bytes("pica", &media, b"<html>error</html>".to_vec())
                .unwrap_err(),
            "LIVE_MEDIA_SOURCE_IMAGE_DECODE_FAILED"
        );
    }
}
