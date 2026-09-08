use cloud_monitor::{
    executor_handoff::ExecutorCommand,
    image_download_authorization::ImageDownloadAuthorization,
    isolated_staging_execution::{self, IsolatedStagingExecutionContext, ProcessedMedia},
    local_executor::{self, LocalExecutionPlan},
    monitor::{hash, Target},
    source_bridge_request,
    source_completion::PaginationProof,
    source_media_descriptors::{MediaChapterDescriptors, MediaDescriptor, SourceMediaDescriptorSet},
    source_preflight::{self, PreflightChapter, SourcePreflightEvidence, SourcePreflightProof},
};
use serde_json::Value;
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use state_model::Version;
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture {
    plan: LocalExecutionPlan,
    authorization: ImageDownloadAuthorization,
    evidence: SourcePreflightEvidence,
    proof: SourcePreflightProof,
    descriptors: SourceMediaDescriptorSet,
}

fn pages() -> PaginationProof {
    PaginationProof {
        total_pages: 1,
        successful_pages: vec![1],
        failed_pages: vec![],
    }
}

fn fixture(source: &str) -> Fixture {
    let (source_work_id, chapter_id, chapter_pagination, image_pagination) = match source {
        "jm" => ("123456", "123456", None, None),
        "pica" => (
            "0123456789abcdef01234567",
            "111111111111111111111111",
            Some(pages()),
            Some(pages()),
        ),
        _ => panic!("unsupported fixture source"),
    };

    let target = Target {
        source_key: format!("{source}:{source_work_id}"),
        author: "Writer".into(),
        title: "A6.12 fixture".into(),
        version: Version::default(),
        coverage: Value::Null,
    };
    let target_hash = hash(&target);
    let task_id = format!("TASK_A6_12_{}", source.to_ascii_uppercase());
    let work_id = format!("WORK_A6_12_{}", source.to_ascii_uppercase());
    let digest = hash(&(task_id.as_str(), 1u64, target_hash.as_str()));
    let command = ExecutorCommand {
        schema_version: 1,
        command_id: format!("EXEC_{}", &digest[..20]),
        task_id,
        work_id,
        task_revision: 1,
        target_hash,
        source: source.into(),
        source_work_id: source_work_id.into(),
        action: "download".into(),
        intent: "DOWNLOAD_TO_STAGING_ONLY".into(),
        target,
    };
    let plan = local_executor::plan(&command).unwrap();
    let request = source_bridge_request::build(&plan).unwrap();

    let media = if source == "jm" {
        vec![MediaDescriptor {
            image_index: 1,
            source_media_id: "001.gif".into(),
            request_url: "https://cdn-msp2.jmapiproxy2.cc/media/photos/123456/001.gif".into(),
            source_format: "gif".into(),
            transform: "NONE".into(),
            transform_parameter: 0,
            relative_path: "chapters/000001-123456/000001.gif".into(),
        }]
    } else {
        vec![
            MediaDescriptor {
                image_index: 1,
                source_media_id: "222222222222222222222222".into(),
                request_url: "https://storage.example.invalid/static/path/001.png".into(),
                source_format: "png".into(),
                transform: "NONE".into(),
                transform_parameter: 0,
                relative_path: "chapters/000001-111111111111111111111111/000001.png".into(),
            },
            MediaDescriptor {
                image_index: 2,
                source_media_id: "333333333333333333333333".into(),
                request_url: "https://storage.example.invalid/static/path/002.jpg".into(),
                source_format: "jpg".into(),
                transform: "NONE".into(),
                transform_parameter: 0,
                relative_path: "chapters/000001-111111111111111111111111/000002.jpg".into(),
            },
        ]
    };
    let expected_images = u64::try_from(media.len()).unwrap();

    let evidence = SourcePreflightEvidence {
        schema_version: 1,
        command_id: request.command_id.clone(),
        task_id: request.task_id.clone(),
        work_id: request.work_id.clone(),
        task_revision: request.task_revision,
        target_hash: request.target_hash.clone(),
        source: request.source.clone(),
        source_work_id: request.source_work_id.clone(),
        upstream_commit: request.upstream_commit.clone(),
        completion_contract_version: request.completion_contract_version,
        scope: request.scope.clone(),
        source_enumeration_complete: true,
        chapter_pagination,
        expected_chapter_count: 1,
        chapters: vec![PreflightChapter {
            chapter_id: chapter_id.into(),
            chapter_order: 1,
            expected_images,
            image_pagination,
        }],
        image_bytes_downloaded: false,
        staging_written: false,
    };
    let proof = source_preflight::validate(&plan, &request, &evidence).unwrap();
    let authorization = ImageDownloadAuthorization {
        schema_version: 1,
        command_id: request.command_id.clone(),
        task_id: request.task_id.clone(),
        work_id: request.work_id.clone(),
        task_revision: request.task_revision,
        target_hash: request.target_hash.clone(),
        source: request.source.clone(),
        source_work_id: request.source_work_id.clone(),
        preflight_hash: proof.preflight_hash.clone(),
        expected_chapter_count: proof.expected_chapter_count,
        expected_content_units: proof.expected_content_units,
        staging_subdir: plan.staging_subdir.clone(),
        write_scope: "COMMAND_OWNED_STAGING_ONLY".into(),
        current_state_binding_hash: "state-generation".into(),
        current_gate_ledger_hash: "gate-generation".into(),
        live_preflight_generation_verified: true,
        image_download_authorized: true,
        staging_write_authorized: true,
        reusable_permit: false,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    };
    let descriptors = SourceMediaDescriptorSet {
        schema_version: 1,
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
        chapters: vec![MediaChapterDescriptors {
            chapter_id: chapter_id.into(),
            chapter_order: 1,
            jm_scramble_id: (source == "jm").then_some(200_000),
            media,
        }],
        image_download_authorized: true,
        staging_write_authorized: true,
        inventory_mutation_authorized: false,
        task_completion_authorized: false,
        promotion_authorized: false,
        replacement_authorized: false,
        physical_delete_authorized: false,
    };

    Fixture {
        plan,
        authorization,
        evidence,
        proof,
        descriptors,
    }
}

fn execution_context<'a>(
    staging_root: &'a Path,
    fixture: &'a Fixture,
) -> IsolatedStagingExecutionContext<'a> {
    IsolatedStagingExecutionContext {
        staging_root,
        plan: &fixture.plan,
        authorization: &fixture.authorization,
        evidence: &fixture.evidence,
        preflight: &fixture.proof,
        descriptors: &fixture.descriptors,
    }
}

fn fake_bytes(format: &str) -> Vec<u8> {
    let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(2, 2, Rgb([12, 34, 56])));
    let image_format = match format {
        "gif" => ImageFormat::Gif,
        "png" => ImageFormat::Png,
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        "webp" => ImageFormat::WebP,
        _ => return vec![],
    };
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, image_format).unwrap();
    output.into_inner()
}

fn processed(descriptor: &MediaDescriptor) -> ProcessedMedia {
    ProcessedMedia {
        source_media_id: descriptor.source_media_id.clone(),
        request_url: descriptor.request_url.clone(),
        source_format: descriptor.source_format.clone(),
        applied_transform: descriptor.transform.clone(),
        applied_transform_parameter: descriptor.transform_parameter,
        bytes: fake_bytes(&descriptor.source_format),
    }
}

fn temp_staging(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "manga-monitor-a6-12-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("commands")).unwrap();
    root
}

#[tokio::test]
async fn exact_pica_bytes_reach_verified_command_staging_only() {
    let f = fixture("pica");
    let root = temp_staging("pica-success");
    let auth = f.authorization.clone();
    let result = isolated_staging_execution::execute_with_fetcher(
        execution_context(&root, &f),
        |descriptor| async move { Ok(processed(&descriptor)) },
        || Ok(auth.clone()),
    )
    .await
    .unwrap();

    assert!(result.staging_execution_completed);
    assert!(result.source_completion.source_contract_verified);
    assert!(result.filesystem_verification.filesystem_verified);
    assert_eq!(result.source_completion.manifest.artifacts.len(), 2);
    assert!(!result.inventory_mutation_authorized);
    assert!(!result.task_completion_authorized);
    assert!(!result.promotion_authorized);
    assert!(!result.replacement_authorized);
    assert!(!result.physical_delete_authorized);
    assert!(root
        .join(&f.plan.staging_subdir)
        .join("chapters/000001-111111111111111111111111/000001.png")
        .is_file());
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn exact_jm_gif_bytes_are_compatible_with_a6_5_and_a6_4() {
    let f = fixture("jm");
    let root = temp_staging("jm-success");
    let auth = f.authorization.clone();
    let result = isolated_staging_execution::execute_with_fetcher(
        execution_context(&root, &f),
        |descriptor| async move { Ok(processed(&descriptor)) },
        || Ok(auth.clone()),
    )
    .await
    .unwrap();

    assert_eq!(result.source, "jm");
    assert_eq!(result.source_completion.manifest.completed_content_units, 1);
    assert!(result.filesystem_verification.filesystem_verified);
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn stale_generation_fails_before_command_directory_creation() {
    let f = fixture("pica");
    let root = temp_staging("stale-before");
    let mut stale = f.authorization.clone();
    stale.current_gate_ledger_hash = "revoked-generation".into();
    let err = isolated_staging_execution::execute_with_fetcher(
        execution_context(&root, &f),
        |descriptor| async move { Ok(processed(&descriptor)) },
        || Ok(stale.clone()),
    )
    .await
    .unwrap_err();

    assert_eq!(err, "MEDIA_TRANSFER_AUTHORIZATION_GENERATION_CHANGED");
    assert!(!root.join(&f.plan.staging_subdir).exists());
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn processed_binding_or_magic_mismatch_never_writes_an_artifact() {
    let f = fixture("pica");
    let root = temp_staging("bad-processed");
    let auth = f.authorization.clone();
    let err = isolated_staging_execution::execute_with_fetcher(
        execution_context(&root, &f),
        |descriptor| async move {
            let mut item = processed(&descriptor);
            item.request_url = "https://evil.invalid/static/forged.png".into();
            Ok(item)
        },
        || Ok(auth.clone()),
    )
    .await
    .unwrap_err();

    assert_eq!(err, "PROCESSED_MEDIA_DESCRIPTOR_BINDING_MISMATCH");
    let first = root
        .join(&f.plan.staging_subdir)
        .join("chapters/000001-111111111111111111111111/000001.png");
    assert!(!first.exists());
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn fetch_failure_leaves_partial_staging_but_emits_no_false_completion() {
    let f = fixture("pica");
    let root = temp_staging("partial");
    let auth = f.authorization.clone();
    let err = isolated_staging_execution::execute_with_fetcher(
        execution_context(&root, &f),
        |descriptor| async move {
            if descriptor.image_index == 2 {
                Err("SIMULATED_FETCH_FAILURE".into())
            } else {
                Ok(processed(&descriptor))
            }
        },
        || Ok(auth.clone()),
    )
    .await
    .unwrap_err();

    assert_eq!(err, "SIMULATED_FETCH_FAILURE");
    let command_root = root.join(&f.plan.staging_subdir);
    assert!(command_root
        .join("chapters/000001-111111111111111111111111/000001.png")
        .is_file());
    assert!(!command_root
        .join("chapters/000001-111111111111111111111111/000002.jpg")
        .exists());
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn authorization_change_after_all_writes_still_blocks_success() {
    let f = fixture("pica");
    let root = temp_staging("stale-after-write");
    let current = f.authorization.clone();
    let mut stale = f.authorization.clone();
    stale.current_state_binding_hash = "new-task-generation".into();
    let mut checks = 0usize;
    let err = isolated_staging_execution::execute_with_fetcher(
        execution_context(&root, &f),
        |descriptor| async move { Ok(processed(&descriptor)) },
        move || {
            checks += 1;
            if checks >= 6 {
                Ok(stale.clone())
            } else {
                Ok(current.clone())
            }
        },
    )
    .await
    .unwrap_err();

    assert_eq!(err, "MEDIA_TRANSFER_AUTHORIZATION_GENERATION_CHANGED");
    let command_root = root.join(&f.plan.staging_subdir);
    assert!(command_root
        .join("chapters/000001-111111111111111111111111/000001.png")
        .is_file());
    assert!(command_root
        .join("chapters/000001-111111111111111111111111/000002.jpg")
        .is_file());
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn command_generation_is_create_new_and_never_overwritten_on_replay() {
    let f = fixture("jm");
    let root = temp_staging("replay");
    let auth = f.authorization.clone();
    isolated_staging_execution::execute_with_fetcher(
        execution_context(&root, &f),
        |descriptor| async move { Ok(processed(&descriptor)) },
        || Ok(auth.clone()),
    )
    .await
    .unwrap();
    let path = root
        .join(&f.plan.staging_subdir)
        .join("chapters/000001-123456/000001.gif");
    let before = fs::read(&path).unwrap();

    let auth = f.authorization.clone();
    let err = isolated_staging_execution::execute_with_fetcher(
        execution_context(&root, &f),
        |descriptor| async move { Ok(processed(&descriptor)) },
        || Ok(auth.clone()),
    )
    .await
    .unwrap_err();
    assert_eq!(err, "STAGING_COMMAND_DIRECTORY_ALREADY_EXISTS");
    assert_eq!(fs::read(path).unwrap(), before);
    fs::remove_dir_all(root).unwrap();
}
