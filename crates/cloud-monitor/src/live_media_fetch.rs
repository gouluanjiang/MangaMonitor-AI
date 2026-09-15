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
        VerifiedMedia,
    },
    jm_media_transform,
    live_media_transport::{JmTransport, PicaTransport},
    media_request_guard::{self, RequestGuard},
    media_validation,
    parallel_media_processing::MediaProcessor,
    source_media_descriptors::{self, MediaDescriptor, SourceMediaDescriptorSet},
};
use std::sync::Arc;

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

    async fn fetch_parallel(
        self,
        descriptor: MediaDescriptor,
        processor: Arc<MediaProcessor>,
        guard: RequestGuard,
    ) -> Result<VerifiedMedia, String> {
        let (source, bytes) = match &self {
            Self::Jm(fetcher) => ("jm", fetcher.fetch_exact(&descriptor.request_url).await?),
            Self::Pica(fetcher) => (
                "pica",
                fetcher
                    .fetch_with_redirects(&descriptor.request_url, || guard.require_current())
                    .await?,
            ),
        };
        // Same upstream division of work: async GET, independent CPU job,
        // await result. The job never receives a path or filesystem authority.
        processor
            .process(move || {
                let media = process_downloaded_bytes(source, &descriptor, bytes)?;
                VerifiedMedia::validate(descriptor, media)
            })
            .await
    }
}

struct MediaTask<T>(tokio::task::JoinHandle<Result<T, String>>);
impl<T> Drop for MediaTask<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn tracked_media_task<T, F>(processor: Arc<MediaProcessor>, future: F) -> Result<T, String>
where
    T: Send + 'static,
    F: std::future::Future<Output = Result<T, String>> + Send + 'static,
{
    // Called on the fetch future's first poll, immediately after the staging
    // coordinator rechecks authorization. Dropping it aborts the GET task.
    let lifetime = processor.track();
    let mut task = MediaTask(tokio::spawn(async move {
        let _lifetime = lifetime;
        future.await
    }));
    (&mut task.0)
        .await
        .map_err(|_| "LIVE_MEDIA_TASK_FAILED".to_owned())?
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
                ("jm", "webp", "JM_SCRAMBLE_BLOCKS" | "JM_SCRAMBLE_BLOCKS_JPEG", _) => {}
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
    let direct_jpeg = source == "jm"
        && descriptor.source_format == "webp"
        && descriptor.transform == "JM_SCRAMBLE_BLOCKS_JPEG";
    // The direct JPEG transform performs the same bounded source decode itself.
    if !direct_jpeg {
        media_validation::validate(&descriptor.source_format, &bytes)
            .map_err(|_| "LIVE_MEDIA_SOURCE_IMAGE_DECODE_FAILED")?;
    }

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

    if !direct_jpeg {
        media_validation::validate(descriptor.stored_format(), &bytes)
            .map_err(|_| "LIVE_MEDIA_PROCESSED_IMAGE_DECODE_FAILED")?;
    }
    // The staging boundary independently decodes output before accepting bytes.

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
pub async fn execute_live_resumable<Reauthorize, Progress>(
    context: IsolatedStagingExecutionContext<'_>,
    resume: Option<&isolated_staging_execution::StagingCheckpoint>,
    reauthorize: Reauthorize,
    progress: Progress,
) -> Result<IsolatedStagingExecutionResult, String>
where
    Reauthorize: FnMut() -> Result<ImageDownloadAuthorization, String>,
    Progress: FnMut(&isolated_staging_execution::StagingCheckpoint) -> Result<(), String>,
{
    source_media_descriptors::validate(
        context.authorization,
        context.evidence,
        context.preflight,
        context.descriptors,
    )?;
    validate_supported_fetch_scope(context.descriptors)?;
    let fetcher = LiveFetcher::new(&context.authorization.source)?;
    let processor = Arc::new(MediaProcessor::new());
    let processing = Arc::clone(&processor);
    let (guard, requests) =
        media_request_guard::channel(isolated_staging_execution::MAX_CONCURRENT_MEDIA);
    let result = isolated_staging_execution::execute_resumable_with_verified_fetcher(
        context,
        resume,
        move |descriptor| {
            let fetcher = fetcher.clone();
            let processor = Arc::clone(&processing);
            let guard = guard.clone();
            tracked_media_task(
                Arc::clone(&processor),
                fetcher.fetch_parallel(descriptor, processor, guard),
            )
        },
        reauthorize,
        progress,
        requests,
    )
    .await;
    // AbortOnDrop cancels residual async requests before reaching here. A CPU
    // closure may already be running, so retain the owning download worker
    // until both request tasks and real processing closures have exited.
    processor.drain().await;
    result
}

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
    use std::{
        future::Future,
        io::Cursor,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            mpsc,
        },
        time::Duration,
    };
    use tokio::{sync::oneshot, time::timeout};

    const TASK_TEST_TIMEOUT: Duration = Duration::from_secs(5);

    struct TaskDropSignal(Option<oneshot::Sender<()>>);

    impl Drop for TaskDropSignal {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }

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
    fn direct_jpeg_is_reencoded_once_with_zero_or_positive_scramble() {
        let mut source = RgbImage::new(48, 64);
        for (x, y, pixel) in source.enumerate_pixels_mut() {
            *pixel = Rgb([((x * 3 + y) % 200) as u8, 40, if y < 32 { 20 } else { 180 }]);
        }
        let mut original = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(source.clone())
            .write_to(&mut original, ImageFormat::WebP)
            .unwrap();
        for blocks in [0, 2] {
            let media = descriptor("webp", "JM_SCRAMBLE_BLOCKS_JPEG", blocks);
            validate_supported_fetch_scope(&descriptor_set("jm", media.clone())).unwrap();
            let result =
                process_downloaded_bytes("jm", &media, original.get_ref().clone()).unwrap();
            assert_eq!(
                image::guess_format(&result.bytes).unwrap(),
                ImageFormat::Jpeg
            );
            assert_eq!(result.source_format, "webp");
            let image = media_validation::decode("jpg", &result.bytes)
                .unwrap()
                .into_rgb8();
            assert_eq!(image.dimensions(), source.dimensions());
            let expected_top = if blocks == 0 { 20_i16 } else { 180 };
            assert!((i16::from(image.get_pixel(20, 10)[2]) - expected_top).abs() < 15);
        }
        let media = descriptor("webp", "JM_SCRAMBLE_BLOCKS_JPEG", 0);
        assert!(process_downloaded_bytes("jm", &media, b"RIFF1234WEBPbroken".to_vec()).is_err());
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
        let err = process_downloaded_bytes("jm", &media, b"RIFF1234WEBPnot-a-real-webp".to_vec())
            .unwrap_err();
        assert_eq!(err, "LIVE_MEDIA_SOURCE_IMAGE_DECODE_FAILED");
    }

    #[test]
    fn content_magic_mismatch_fails_before_transform_or_a6_12_write() {
        let media = descriptor("png", "NONE", 0);
        assert_eq!(
            process_downloaded_bytes("pica", &media, b"<html>error</html>".to_vec()).unwrap_err(),
            "LIVE_MEDIA_SOURCE_IMAGE_DECODE_FAILED"
        );
    }

    #[tokio::test]
    async fn dropping_tracked_task_cancels_pending_fetch_and_drains_its_guard() {
        let processor = Arc::new(MediaProcessor::new());
        let (entered, started) = oneshot::channel();
        let (dropped, was_dropped) = oneshot::channel();
        let receiver = tokio::spawn(tracked_media_task(processor.clone(), async move {
            let _drop = TaskDropSignal(Some(dropped));
            let _ = entered.send(());
            std::future::pending::<Result<(), String>>().await
        }));
        timeout(TASK_TEST_TIMEOUT, started).await.unwrap().unwrap();

        receiver.abort();
        assert!(receiver.await.unwrap_err().is_cancelled());
        timeout(TASK_TEST_TIMEOUT, processor.drain()).await.unwrap();
        timeout(TASK_TEST_TIMEOUT, was_dropped)
            .await
            .unwrap()
            .expect("drain must include the aborted inner fetch task");
    }

    #[tokio::test]
    async fn pausing_before_a_redirect_get_cancels_and_drains_the_tracked_request() {
        let processor = Arc::new(MediaProcessor::new());
        let (guard, mut requests) =
            media_request_guard::channel(isolated_staging_execution::MAX_CONCURRENT_MEDIA);
        let performed = Arc::new(AtomicUsize::new(0));
        let worker_performed = performed.clone();
        let (dropped, was_dropped) = oneshot::channel();
        let mut task = Box::pin(tracked_media_task(processor.clone(), async move {
            let _drop = TaskDropSignal(Some(dropped));
            guard.require_current().await?;
            worker_performed.fetch_add(1, Ordering::AcqRel);
            // The synthetic first response is a redirect; no second GET may
            // start after the coordinator observes a paused download task.
            guard.require_current().await?;
            worker_performed.fetch_add(1, Ordering::AcqRel);
            Ok(())
        }));
        let error = timeout(
            TASK_TEST_TIMEOUT,
            std::future::poll_fn(|cx| {
                requests.poll_current(cx, || {
                    if performed.load(Ordering::Acquire) == 0 {
                        Ok(())
                    } else {
                        Err("DOWNLOAD_PAUSED".into())
                    }
                })?;
                task.as_mut().poll(cx)
            }),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert_eq!(error, "DOWNLOAD_PAUSED");
        drop(task);
        drop(requests);
        timeout(TASK_TEST_TIMEOUT, processor.drain()).await.unwrap();
        timeout(TASK_TEST_TIMEOUT, was_dropped)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(performed.load(Ordering::Acquire), 1);
    }

    #[tokio::test]
    async fn cancelling_a_tracked_request_waiting_for_its_first_grant_does_not_hang_drain() {
        let processor = Arc::new(MediaProcessor::new());
        let (guard, _requests) =
            media_request_guard::channel(isolated_staging_execution::MAX_CONCURRENT_MEDIA);
        let (entered, started) = oneshot::channel();
        let (dropped, was_dropped) = oneshot::channel();
        let performed = Arc::new(AtomicBool::new(false));
        let worker_performed = performed.clone();
        let task = tokio::spawn(tracked_media_task(processor.clone(), async move {
            let _drop = TaskDropSignal(Some(dropped));
            let _ = entered.send(());
            guard.require_current().await?;
            worker_performed.store(true, Ordering::Release);
            Ok(())
        }));
        timeout(TASK_TEST_TIMEOUT, started).await.unwrap().unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        // The receiver remains alive. Cancellation itself must drop the guard
        // wait and the tracked task, without needing a grant or channel close.
        timeout(TASK_TEST_TIMEOUT, processor.drain()).await.unwrap();
        timeout(TASK_TEST_TIMEOUT, was_dropped)
            .await
            .unwrap()
            .unwrap();
        assert!(!performed.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn dropping_tracked_task_still_drains_its_dispatched_cpu_closure() {
        let processor = Arc::new(MediaProcessor::new());
        let processing = processor.clone();
        let (entered, started) = oneshot::channel();
        let (release, held) = mpsc::channel();
        let completed = Arc::new(AtomicBool::new(false));
        let worker_completed = completed.clone();
        let receiver = tokio::spawn(tracked_media_task(processor.clone(), async move {
            processing
                .process(move || {
                    let _ = entered.send(());
                    held.recv_timeout(TASK_TEST_TIMEOUT)
                        .map_err(|_| "synthetic CPU release missing".to_owned())?;
                    worker_completed.store(true, Ordering::Release);
                    Ok(7)
                })
                .await
        }));
        timeout(TASK_TEST_TIMEOUT, started).await.unwrap().unwrap();

        receiver.abort();
        assert!(receiver.await.unwrap_err().is_cancelled());
        assert!(timeout(Duration::from_millis(25), processor.drain())
            .await
            .is_err());
        assert!(!completed.load(Ordering::Acquire));
        release.send(()).unwrap();
        timeout(TASK_TEST_TIMEOUT, processor.drain()).await.unwrap();
        assert!(completed.load(Ordering::Acquire));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn tracked_task_progresses_while_coordinator_stops_polling_its_wrapper() {
        let processor = Arc::new(MediaProcessor::new());
        let (release, held) = oneshot::channel();
        let (completed, completion) = mpsc::channel();
        let mut request = Box::pin(tracked_media_task(processor.clone(), async move {
            held.await.map_err(|_| "synthetic fetch release missing")?;
            completed
                .send(())
                .map_err(|_| "synthetic completion receiver missing")?;
            Ok(9)
        }));
        std::future::poll_fn(|context| {
            assert!(request.as_mut().poll(context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        release.send(()).unwrap();
        // Model a coordinator performing synchronous work: no wrapper polling
        // happens here, but the independently spawned async task must progress.
        completion
            .recv_timeout(TASK_TEST_TIMEOUT)
            .expect("fetch task must progress independently of wrapper polling");
        assert_eq!(request.await.unwrap(), 9);
        timeout(TASK_TEST_TIMEOUT, processor.drain()).await.unwrap();
    }
}
