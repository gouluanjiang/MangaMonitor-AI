//! Synthetic responses enter at the physical metadata-request boundary. The
//! real guard, failover loop, parsers and complete-book enumeration still run.
use super::*;
use serde_json::json;
use std::collections::VecDeque;

fn ok(body: Value) -> MetadataResponse {
    (Some(200), Ok(body))
}

fn album() -> MetadataResponse {
    ok(json!({"id":"99","series":[{"id":"101"},{"id":"102"}]}))
}

fn first_chapter() -> MetadataResponse {
    ok(json!({"id":"101","images":["001.webp","002.GIF","003.jpg"]}))
}

fn last_chapter() -> MetadataResponse {
    ok(json!({"id":"102","images":["001.webp"]}))
}

fn client(responses: Vec<MetadataResponse>) -> JmClient {
    let mut client = JmClient::new_for_download(DEFAULT_DOMAIN).unwrap();
    client.script = Some(VecDeque::from(responses));
    client
}

fn request(domain: &str, path: &str, id: &str) -> RecordedRequest {
    RecordedRequest {
        domain: domain.into(),
        path: path.into(),
        query: vec![("id".into(), id.into())],
    }
}

#[tokio::test]
async fn revoked_preflight_sends_no_album_or_chapter_request() {
    for code in ["DOWNLOAD_PAUSED", "SESSION_CHANGED", "DOWNLOAD_TASK_STALE"] {
        let mut client = client(vec![album(), first_chapter(), last_chapter()]);
        let result = client.preflight_with_guard("99", || Err(code.into())).await;
        assert_eq!(result.unwrap_err(), code);
        assert!(client.requests.is_empty());
        assert!(client.traces.is_empty());
        assert_eq!(client.script.as_ref().unwrap().len(), 3);
    }
}

#[tokio::test]
async fn pause_or_session_change_between_chapters_stops_the_next_request() {
    for code in ["DOWNLOAD_PAUSED", "SESSION_CHANGED"] {
        let mut client = client(vec![album(), first_chapter(), last_chapter()]);
        let mut checks = 0;
        let result = client
            .preflight_with_guard("99", || {
                checks += 1;
                if checks == 3 {
                    // The album and first chapter have completed. The task or
                    // account is no longer current when the next chapter begins.
                    Err(code.into())
                } else {
                    Ok(())
                }
            })
            .await;
        assert_eq!(result.unwrap_err(), code);
        assert_eq!(checks, 3);
        assert_eq!(
            client.requests,
            vec![
                request(DEFAULT_DOMAIN, "/album", "99"),
                request(DEFAULT_DOMAIN, "/chapter", "101"),
            ]
        );
        assert_eq!(client.traces.len(), 2);
        assert_eq!(client.script.as_ref().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn revocation_stops_album_and_chapter_failover_before_another_domain() {
    for during_chapter in [false, true] {
        let responses = if during_chapter {
            vec![
                album(),
                (Some(503), Err("HTTP_503".into())),
                first_chapter(),
                last_chapter(),
            ]
        } else {
            vec![
                (None, Err("TIMEOUT".into())),
                album(),
                first_chapter(),
                last_chapter(),
            ]
        };
        let mut client = client(responses);
        let permitted = if during_chapter { 2 } else { 1 };
        let mut checks = 0;
        let result = client
            .preflight_with_guard("99", || {
                checks += 1;
                if checks > permitted {
                    Err("DOWNLOAD_PAUSED".into())
                } else {
                    Ok(())
                }
            })
            .await;
        assert_eq!(result.unwrap_err(), "DOWNLOAD_PAUSED");
        assert_eq!(checks, permitted + 1);
        assert_eq!(client.requests.len(), permitted);
        assert_eq!(client.traces.len(), permitted);
        assert!(client.requests.iter().all(|r| r.domain == DEFAULT_DOMAIN));
        assert_eq!(client.script.as_ref().unwrap().len(), 4 - permitted);
    }
}

#[tokio::test]
async fn guarded_preflight_retains_complete_chapter_order_and_exact_image_filter() {
    let mut client = client(vec![album(), first_chapter(), last_chapter()]);
    let mut checks = 0;
    let result = client
        .preflight_with_guard("99", || {
            checks += 1;
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(
        result,
        vec![
            JmChapterPreflight {
                chapter_id: "101".into(),
                chapter_order: 1,
                expected_images: 2,
            },
            JmChapterPreflight {
                chapter_id: "102".into(),
                chapter_order: 2,
                expected_images: 1,
            },
        ]
    );
    assert_eq!(
        client.requests,
        vec![
            request(DEFAULT_DOMAIN, "/album", "99"),
            request(DEFAULT_DOMAIN, "/chapter", "101"),
            request(DEFAULT_DOMAIN, "/chapter", "102"),
        ]
    );
    assert_eq!(checks, 3);
    assert_eq!(client.traces.len(), 3);
    assert!(client.traces.iter().all(|trace| trace.outcome == "OK"));
}

#[tokio::test]
async fn current_preflight_counts_every_existing_pinned_failover_attempt() {
    let mut client = client(vec![
        (None, Err("TIMEOUT".into())),
        album(),
        (Some(503), Err("HTTP_503".into())),
        first_chapter(),
        last_chapter(),
    ]);
    let mut checks = 0;
    let result = client
        .preflight_with_guard("99", || {
            checks += 1;
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(
        result
            .iter()
            .map(|chapter| chapter.expected_images)
            .sum::<u64>(),
        3
    );
    assert_eq!(
        client.requests,
        vec![
            request(DEFAULT_DOMAIN, "/album", "99"),
            request(BASELINE_DOMAINS[1], "/album", "99"),
            request(DEFAULT_DOMAIN, "/chapter", "101"),
            request(BASELINE_DOMAINS[1], "/chapter", "101"),
            request(DEFAULT_DOMAIN, "/chapter", "102"),
        ]
    );
    assert_eq!(checks, 5);
    assert_eq!(client.traces.len(), 5);
    assert_eq!(client.traces[0].outcome, "TIMEOUT");
    assert_eq!(client.traces[2].outcome, "HTTP_503");
}

#[tokio::test]
async fn incomplete_last_chapter_cannot_publish_the_successful_prefix() {
    for (response, code) in [
        (
            ok(json!({"id":"103","images":["001.webp"]})),
            "PREFLIGHT_CHAPTER_ID_MISMATCH",
        ),
        (
            ok(json!({"id":"102","images":[]})),
            "JM_PREFLIGHT_IMAGES_EMPTY",
        ),
        (
            (Some(200), Err("DECRYPTED_JSON_ERROR".into())),
            "DECRYPTED_JSON_ERROR",
        ),
    ] {
        let mut client = client(vec![album(), first_chapter(), response, last_chapter()]);
        let result = client.preflight_with_guard("99", || Ok(())).await;
        assert_eq!(result.unwrap_err(), code);
        assert_eq!(client.requests.len(), 3);
        assert_eq!(client.traces.len(), 3);
        assert_eq!(client.script.as_ref().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn absent_or_exhausted_test_responses_never_reach_a_live_source() {
    let mut empty = JmClient::new_for_download(DEFAULT_DOMAIN).unwrap();
    assert_eq!(
        empty
            .preflight_with_guard("99", || Ok(()))
            .await
            .unwrap_err(),
        "TEST_SOURCE_REQUEST_FORBIDDEN"
    );
    let mut short = client(vec![album(), first_chapter()]);
    assert_eq!(
        short
            .preflight_with_guard("99", || Ok(()))
            .await
            .unwrap_err(),
        "TEST_SOURCE_REQUEST_FORBIDDEN"
    );
    assert_eq!(short.requests.len(), 3);
    assert_eq!(short.traces.len(), 3);
}
