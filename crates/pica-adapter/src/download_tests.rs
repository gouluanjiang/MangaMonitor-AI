use super::*;

const COMIC: &str = "111111111111111111111111";

fn chapter(id: u64, order: u64) -> Value {
    json!({"_id":format!("{id:024x}"),"order":order})
}

fn image(id: u64) -> Value {
    json!({"_id":format!("{id:024x}"),"media":{
        "originalName":format!("{id}.jpg"),
        "path":format!("media/test/{id}.jpg"),
        "fileServer":"https://storage-b.picacomic.com"
    }})
}

fn page(key: &str, total: u64, limit: u64, page: u64, docs: Vec<Value>) -> Value {
    let mut data = json!({});
    data[key] = json!({"total":total,"limit":limit,"page":page,
        "pages":total.div_ceil(limit),"docs":docs});
    data
}

fn scripted(values: Vec<Value>) -> PicaClient {
    let mut client = PicaClient::new_for_download("synthetic-token".into()).unwrap();
    client.script = Some(values.into_iter().map(Ok).collect());
    client
}

#[test]
fn immediate_pacing_is_limited_to_download_metadata_operations() {
    let monitor = PicaClient::new(String::new()).unwrap();
    let download = PicaClient::new_for_download(String::new()).unwrap();
    for operation in [
        "preflight_chapters",
        "preflight_images",
        "live_media_descriptors",
    ] {
        assert!((1000..=3000).contains(&monitor.pacing.delay_ms(&reqwest::Method::GET, operation)));
        assert_eq!(
            download.pacing.delay_ms(&reqwest::Method::GET, operation),
            0
        );
        assert!(
            (1000..=3000).contains(&download.pacing.delay_ms(&reqwest::Method::POST, operation))
        );
    }
    for operation in ["login", "search", "detail", "chapters"] {
        assert!((1000..=3000).contains(&download.pacing.delay_ms(&reqwest::Method::GET, operation)));
    }
}

#[tokio::test]
async fn chapters_sort_only_after_every_page_and_still_reject_duplicate_identity_or_order() {
    let mut client = scripted(vec![
        page("eps", 3, 2, 1, vec![chapter(3, 3), chapter(1, 1)]),
        page("eps", 3, 2, 2, vec![chapter(2, 2)]),
    ]);
    let result = client.preflight_chapters(COMIC, 10).await.unwrap();
    assert_eq!(result.successful_pages, vec![1, 2]);
    assert_eq!(
        result
            .chapters
            .iter()
            .map(|c| c.chapter_order)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    for docs in [
        vec![chapter(1, 1), chapter(1, 2)],
        vec![chapter(1, 1), chapter(2, 1)],
    ] {
        let mut client = scripted(vec![page("eps", 2, 2, 1, docs)]);
        assert_eq!(
            client.preflight_chapters(COMIC, 10).await.unwrap_err(),
            "PICA_PREFLIGHT_CHAPTERS_NOT_CANONICAL"
        );
    }
}

#[tokio::test]
async fn image_enumerations_reject_short_or_changed_pages_and_duplicate_ids() {
    for pages in [
        vec![page("pages", 2, 2, 1, vec![image(1)])],
        vec![
            page("pages", 2, 1, 1, vec![image(1)]),
            page("pages", 3, 1, 2, vec![image(2)]),
        ],
        vec![
            page("pages", 2, 1, 1, vec![image(1)]),
            page("pages", 2, 1, 1, vec![image(2)]),
        ],
        vec![
            page("pages", 2, 1, 1, vec![image(1)]),
            page("pages", 2, 1, 2, vec![image(1)]),
        ],
    ] {
        let mut preflight = scripted(pages.clone());
        assert!(preflight
            .preflight_chapter_images(COMIC, 1, 10)
            .await
            .is_err());
        let mut descriptors = scripted(pages);
        assert!(descriptors.live_chapter_media(COMIC, 1, 10).await.is_err());
    }
}

#[tokio::test]
async fn image_metadata_preserves_page_and_in_page_order() {
    let pages = vec![
        page("pages", 3, 2, 1, vec![image(8), image(2)]),
        page("pages", 3, 2, 2, vec![image(5)]),
    ];
    let mut preflight = scripted(pages.clone());
    let proof = preflight
        .preflight_chapter_images(COMIC, 1, 10)
        .await
        .unwrap();
    assert_eq!(proof.expected_images, 3);
    assert_eq!(proof.successful_pages, vec![1, 2]);
    let mut descriptors = scripted(pages);
    let result = descriptors.live_chapter_media(COMIC, 1, 10).await.unwrap();
    assert_eq!(result.successful_pages, proof.successful_pages);
    assert_eq!(
        result
            .media
            .iter()
            .map(|v| v.media_id.clone())
            .collect::<Vec<_>>(),
        vec![
            format!("{:024x}", 8),
            format!("{:024x}", 2),
            format!("{:024x}", 5)
        ]
    );
}

#[tokio::test]
async fn revoked_scope_prevents_the_next_physical_metadata_request() {
    for kind in 0..3 {
        let key = if kind == 0 { "eps" } else { "pages" };
        let docs = if kind == 0 {
            vec![chapter(1, 1)]
        } else {
            vec![image(1)]
        };
        let mut client = scripted(vec![page(key, 2, 1, 1, docs)]);
        let mut checks = 0;
        let guard = || {
            checks += 1;
            if checks == 1 {
                Ok(())
            } else {
                Err("SESSION_EXPIRED".into())
            }
        };
        let error = match kind {
            0 => client
                .preflight_chapters_with_guard(COMIC, 10, guard)
                .await
                .unwrap_err(),
            1 => client
                .preflight_chapter_images_with_guard(COMIC, 1, 10, guard)
                .await
                .unwrap_err(),
            _ => client
                .live_chapter_media_with_guard(COMIC, 1, 10, guard)
                .await
                .unwrap_err(),
        };
        assert_eq!(error, "SESSION_EXPIRED");
        assert_eq!(checks, 2);
        assert_eq!(client.requested_paths.len(), 1);
        assert!(client.requested_paths[0].ends_with("page=1"));
    }
}

#[tokio::test]
async fn absent_test_script_never_falls_through_to_a_real_source() {
    let mut client = PicaClient::new_for_download(String::new()).unwrap();
    assert_eq!(
        client.preflight_chapters(COMIC, 1).await.unwrap_err(),
        "TEST_SOURCE_REQUEST_FORBIDDEN"
    );
}
