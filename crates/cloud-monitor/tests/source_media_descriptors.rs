use cloud_monitor::{
    image_download_authorization::ImageDownloadAuthorization,
    monitor::hash,
    source_completion::PaginationProof,
    source_media_descriptors::{self, MediaChapterDescriptors, MediaDescriptor, SourceMediaDescriptorSet},
    source_preflight::{PreflightChapter, SourcePreflightEvidence, SourcePreflightProof},
};

fn pages() -> PaginationProof {
    PaginationProof {
        total_pages: 1,
        successful_pages: vec![1],
        failed_pages: vec![],
    }
}

fn evidence(source: &str) -> SourcePreflightEvidence {
    let (source_work_id, chapter_id, upstream_commit, pagination) = if source == "jm" {
        (
            "123456",
            "123456",
            "f0cdd724af6892002f2fb7be883b88832cebe7e9",
            None,
        )
    } else {
        (
            "0123456789abcdef01234567",
            "111111111111111111111111",
            "77c8b62ede42b3afc074506d092313816af8092d",
            Some(pages()),
        )
    };
    SourcePreflightEvidence {
        schema_version: 1,
        command_id: "EXEC_1234567890abcdef1234".into(),
        task_id: "TASK_A6_11".into(),
        work_id: "WORK_A6_11".into(),
        task_revision: 1,
        target_hash: "a".repeat(64),
        source: source.into(),
        source_work_id: source_work_id.into(),
        upstream_commit: upstream_commit.into(),
        completion_contract_version: 1,
        scope: "FULL_SOURCE_WORK".into(),
        source_enumeration_complete: true,
        chapter_pagination: pagination.clone(),
        expected_chapter_count: 1,
        chapters: vec![PreflightChapter {
            chapter_id: chapter_id.into(),
            chapter_order: 1,
            expected_images: 2,
            image_pagination: pagination,
        }],
        image_bytes_downloaded: false,
        staging_written: false,
    }
}

fn authorization(source: &str) -> ImageDownloadAuthorization {
    let evidence = evidence(source);
    ImageDownloadAuthorization {
        schema_version: 1,
        command_id: evidence.command_id.clone(),
        task_id: evidence.task_id.clone(),
        work_id: evidence.work_id.clone(),
        task_revision: evidence.task_revision,
        target_hash: evidence.target_hash.clone(),
        source: evidence.source.clone(),
        source_work_id: evidence.source_work_id.clone(),
        preflight_hash: hash(&evidence),
        expected_chapter_count: 1,
        expected_content_units: 2,
        staging_subdir: "commands/EXEC_1234567890abcdef1234".into(),
        write_scope: "COMMAND_OWNED_STAGING_ONLY".into(),
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

fn preflight(source: &str) -> SourcePreflightProof {
    let evidence = evidence(source);
    SourcePreflightProof {
        schema_version: 1,
        command_id: evidence.command_id.clone(),
        task_id: evidence.task_id.clone(),
        work_id: evidence.work_id.clone(),
        task_revision: evidence.task_revision,
        target_hash: evidence.target_hash.clone(),
        source: evidence.source.clone(),
        source_work_id: evidence.source_work_id.clone(),
        upstream_commit: evidence.upstream_commit.clone(),
        completion_contract_version: evidence.completion_contract_version,
        scope: evidence.scope.clone(),
        preflight_hash: hash(&evidence),
        chapter_pagination: evidence.chapter_pagination.clone(),
        expected_chapter_count: evidence.expected_chapter_count,
        chapters: evidence.chapters.clone(),
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

fn jm_set() -> SourceMediaDescriptorSet {
    let auth = authorization("jm");
    SourceMediaDescriptorSet {
        schema_version: 1,
        command_id: auth.command_id,
        task_id: auth.task_id,
        work_id: auth.work_id,
        task_revision: auth.task_revision,
        target_hash: auth.target_hash,
        source: auth.source,
        source_work_id: auth.source_work_id,
        preflight_hash: auth.preflight_hash,
        expected_chapter_count: 1,
        expected_content_units: 2,
        staging_subdir: auth.staging_subdir,
        write_scope: auth.write_scope,
        chapters: vec![MediaChapterDescriptors {
            chapter_id: "123456".into(),
            chapter_order: 1,
            jm_scramble_id: Some(100_000),
            media: vec![
                MediaDescriptor {
                    image_index: 1,
                    source_media_id: "001.webp".into(),
                    request_url: "https://cdn-msp2.jmapiproxy2.cc/media/photos/123456/001.webp".into(),
                    source_format: "webp".into(),
                    transform: "JM_SCRAMBLE_BLOCKS".into(),
                    transform_parameter: 10,
                    relative_path: "chapters/000001-123456/000001.webp".into(),
                },
                MediaDescriptor {
                    image_index: 2,
                    source_media_id: "002.gif".into(),
                    request_url: "https://cdn-msp2.jmapiproxy2.cc/media/photos/123456/002.gif".into(),
                    source_format: "gif".into(),
                    transform: "NONE".into(),
                    transform_parameter: 0,
                    relative_path: "chapters/000001-123456/000002.gif".into(),
                },
            ],
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

fn pica_set() -> SourceMediaDescriptorSet {
    let auth = authorization("pica");
    SourceMediaDescriptorSet {
        schema_version: 1,
        command_id: auth.command_id,
        task_id: auth.task_id,
        work_id: auth.work_id,
        task_revision: auth.task_revision,
        target_hash: auth.target_hash,
        source: auth.source,
        source_work_id: auth.source_work_id,
        preflight_hash: auth.preflight_hash,
        expected_chapter_count: 1,
        expected_content_units: 2,
        staging_subdir: auth.staging_subdir,
        write_scope: auth.write_scope,
        chapters: vec![MediaChapterDescriptors {
            chapter_id: "111111111111111111111111".into(),
            chapter_order: 1,
            jm_scramble_id: None,
            media: vec![
                MediaDescriptor {
                    image_index: 1,
                    source_media_id: "222222222222222222222222".into(),
                    request_url: "https://storage.example.invalid/static/media/path/001.jpg".into(),
                    source_format: "jpg".into(),
                    transform: "NONE".into(),
                    transform_parameter: 0,
                    relative_path: "chapters/000001-111111111111111111111111/000001.jpg".into(),
                },
                MediaDescriptor {
                    image_index: 2,
                    source_media_id: "333333333333333333333333".into(),
                    request_url: "https://storage.example.invalid/static/media/path/002.webp".into(),
                    source_format: "webp".into(),
                    transform: "NONE".into(),
                    transform_parameter: 0,
                    relative_path: "chapters/000001-111111111111111111111111/000002.webp".into(),
                },
            ],
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
fn exact_jm_descriptors_are_bound_to_authorization_paths_and_preflight_scope() {
    source_media_descriptors::validate(
        &authorization("jm"),
        &evidence("jm"),
        &preflight("jm"),
        &jm_set(),
    )
    .unwrap();
}

#[test]
fn exact_pica_descriptors_are_bound_to_authorization_paths_and_preflight_scope() {
    source_media_descriptors::validate(
        &authorization("pica"),
        &evidence("pica"),
        &preflight("pica"),
        &pica_set(),
    )
    .unwrap();
}

#[test]
fn preflight_or_staging_binding_tampering_fails_closed() {
    let auth = authorization("jm");
    let evidence = evidence("jm");
    let proof = preflight("jm");
    let mut set = jm_set();
    set.preflight_hash = "0".repeat(64);
    assert_eq!(
        source_media_descriptors::validate(&auth, &evidence, &proof, &set).unwrap_err(),
        "SOURCE_MEDIA_AUTHORIZATION_BINDING_MISMATCH"
    );

    let mut set = jm_set();
    set.staging_subdir = "commands/forged".into();
    assert_eq!(
        source_media_descriptors::validate(&auth, &evidence, &proof, &set).unwrap_err(),
        "SOURCE_MEDIA_AUTHORIZATION_BINDING_MISMATCH"
    );
}

#[test]
fn forged_proof_and_descriptors_cannot_reuse_old_preflight_hash() {
    let auth = authorization("jm");
    let original_evidence = evidence("jm");
    let mut forged_proof = preflight("jm");
    forged_proof.chapters[0].chapter_id = "654321".into();
    let mut forged_set = jm_set();
    forged_set.chapters[0].chapter_id = "654321".into();
    for media in &mut forged_set.chapters[0].media {
        media.request_url = media.request_url.replace("/123456/", "/654321/");
        media.relative_path = media.relative_path.replace("-123456/", "-654321/");
    }
    assert_eq!(
        source_media_descriptors::validate(
            &auth,
            &original_evidence,
            &forged_proof,
            &forged_set,
        )
        .unwrap_err(),
        "SOURCE_MEDIA_PREFLIGHT_PROOF_EVIDENCE_MISMATCH"
    );
}

#[test]
fn exact_preflight_chapter_identity_and_per_chapter_count_are_mandatory() {
    let auth = authorization("jm");
    let original_evidence = evidence("jm");
    let mut proof = preflight("jm");
    proof.chapters[0].chapter_id = "654321".into();
    assert_eq!(
        source_media_descriptors::validate(&auth, &original_evidence, &proof, &jm_set())
            .unwrap_err(),
        "SOURCE_MEDIA_PREFLIGHT_PROOF_EVIDENCE_MISMATCH"
    );

    let mut set = jm_set();
    set.chapters[0].media.pop();
    assert_eq!(
        source_media_descriptors::validate(
            &auth,
            &original_evidence,
            &preflight("jm"),
            &set,
        )
        .unwrap_err(),
        "SOURCE_MEDIA_PREFLIGHT_SCOPE_MISMATCH"
    );
}

#[test]
fn path_traversal_duplicate_or_index_reordering_fails_closed() {
    let auth = authorization("jm");
    let evidence = evidence("jm");
    let proof = preflight("jm");
    let mut set = jm_set();
    set.chapters[0].media[0].relative_path = "../escape.webp".into();
    assert_eq!(
        source_media_descriptors::validate(&auth, &evidence, &proof, &set).unwrap_err(),
        "SOURCE_MEDIA_ITEMS_NOT_CANONICAL"
    );

    let mut set = jm_set();
    set.chapters[0].media[1].image_index = 1;
    assert_eq!(
        source_media_descriptors::validate(&auth, &evidence, &proof, &set).unwrap_err(),
        "SOURCE_MEDIA_ITEMS_NOT_CANONICAL"
    );

    let mut set = jm_set();
    set.chapters[0].media[1].source_media_id = set.chapters[0].media[0].source_media_id.clone();
    assert_eq!(
        source_media_descriptors::validate(&auth, &evidence, &proof, &set).unwrap_err(),
        "SOURCE_MEDIA_ITEMS_NOT_CANONICAL"
    );
}

#[test]
fn wrong_jm_host_filename_format_or_transform_fails_closed() {
    let auth = authorization("jm");
    let evidence = evidence("jm");
    let proof = preflight("jm");
    let mut set = jm_set();
    set.chapters[0].media[0].request_url = "https://evil.invalid/media/photos/123456/001.webp".into();
    assert_eq!(
        source_media_descriptors::validate(&auth, &evidence, &proof, &set).unwrap_err(),
        "INVALID_JM_MEDIA_DESCRIPTOR"
    );

    let mut set = jm_set();
    set.chapters[0].media[0].request_url.push_str("?token=secret");
    assert_eq!(
        source_media_descriptors::validate(&auth, &evidence, &proof, &set).unwrap_err(),
        "INVALID_JM_MEDIA_DESCRIPTOR"
    );

    let mut set = jm_set();
    set.chapters[0].media[0].source_format = "png".into();
    set.chapters[0].media[0].relative_path = "chapters/000001-123456/000001.png".into();
    assert_eq!(
        source_media_descriptors::validate(&auth, &evidence, &proof, &set).unwrap_err(),
        "INVALID_JM_MEDIA_DESCRIPTOR"
    );

    let mut set = jm_set();
    set.chapters[0].media[0].transform = "NONE".into();
    assert_eq!(
        source_media_descriptors::validate(&auth, &evidence, &proof, &set).unwrap_err(),
        "INVALID_JM_MEDIA_TRANSFORM"
    );

    let mut set = jm_set();
    set.chapters[0].media[0].transform_parameter = 12;
    assert_eq!(
        source_media_descriptors::validate(&auth, &evidence, &proof, &set).unwrap_err(),
        "INVALID_JM_MEDIA_TRANSFORM"
    );
}

#[test]
fn pica_rejects_credentials_query_format_mismatch_and_downstream_authority() {
    let auth = authorization("pica");
    let evidence = evidence("pica");
    let proof = preflight("pica");
    let mut set = pica_set();
    set.chapters[0].media[0].request_url = "https://user:pass@storage.example.invalid/static/1.jpg".into();
    assert_eq!(
        source_media_descriptors::validate(&auth, &evidence, &proof, &set).unwrap_err(),
        "INVALID_PICA_MEDIA_DESCRIPTOR"
    );

    let mut set = pica_set();
    set.chapters[0].media[0].request_url.push_str("?token=secret");
    assert_eq!(
        source_media_descriptors::validate(&auth, &evidence, &proof, &set).unwrap_err(),
        "INVALID_PICA_MEDIA_DESCRIPTOR"
    );

    let mut set = pica_set();
    set.chapters[0].media[0].source_format = "png".into();
    set.chapters[0].media[0].relative_path =
        "chapters/000001-111111111111111111111111/000001.png".into();
    assert_eq!(
        source_media_descriptors::validate(&auth, &evidence, &proof, &set).unwrap_err(),
        "INVALID_PICA_MEDIA_DESCRIPTOR"
    );

    let mut set = pica_set();
    set.inventory_mutation_authorized = true;
    assert_eq!(
        source_media_descriptors::validate(&auth, &evidence, &proof, &set).unwrap_err(),
        "UNSAFE_SOURCE_MEDIA_DESCRIPTOR_CAPABILITIES"
    );
}

#[test]
fn descriptor_count_must_equal_a6_10_expected_scope() {
    let auth = authorization("jm");
    let evidence = evidence("jm");
    let proof = preflight("jm");
    let mut set = jm_set();
    set.chapters[0].media.pop();
    assert_eq!(
        source_media_descriptors::validate(&auth, &evidence, &proof, &set).unwrap_err(),
        "SOURCE_MEDIA_PREFLIGHT_SCOPE_MISMATCH"
    );
}
