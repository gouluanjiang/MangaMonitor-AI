use super::*;
use aes::{
    cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit},
    Aes256,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use image::ImageEncoder;

const PICA_ID: &str = "0123456789abcdef01234567";

fn session(source: Source) -> SourceSession {
    new_session(
        source,
        StoredCredential::new("fixture", expected_kind(source), "fixture-session").unwrap(),
    )
}

fn scripted(responses: Vec<SourceResult<Value>>) -> WorkbenchSources {
    let mut sources = WorkbenchSources::new().unwrap();
    sources.script = Some(Mutex::new(responses.into()));
    sources
}

fn detail_value(source: Source, favorite: Option<bool>) -> Value {
    match source {
        Source::Jm => {
            json!({"id":"123","name":"Fixture","author":["Author"],"is_favorite":favorite})
        }
        Source::Pica => {
            json!({"comic":{"_id":PICA_ID,"title":"Fixture","author":"Author","isFavourite":favorite}})
        }
    }
}

#[test]
fn ids_and_links_do_not_accept_arbitrary_destinations() {
    assert_eq!(
        parse_work_id(Source::Jm, "https://18comic.vip/album/123").unwrap(),
        "123"
    );
    assert_eq!(
        parse_work_id(Source::Jm, "https://www.cdnhth.cc/album?id=123").unwrap(),
        "123"
    );
    assert_eq!(
        parse_work_id(
            Source::Pica,
            &format!("https://picaapi.picacomic.com/comics/{PICA_ID}")
        )
        .unwrap(),
        PICA_ID
    );
    for input in [
        "https://evil.test/album/123",
        "https://127.0.0.1/album/123",
        "https://user:pass@18comic.vip/album/123",
        "https://18comic.vip/album/123?redirect=evil",
        "https://18comic.vip/album/../123",
        "123/../../login",
        "0",
        "-1",
    ] {
        assert!(parse_work_id(Source::Jm, input).is_err(), "{input}");
    }
    assert!(parse_work_id(Source::Pica, "123").is_err());
    assert_eq!(
        parse_work_id(Source::Pica, "0123456789ABCDEF01234567").unwrap(),
        PICA_ID
    );
}

#[test]
fn unknown_metadata_is_not_invented_as_false_or_zero() {
    let (item, _) =
        protocol::work(Source::Pica, &json!({"_id":PICA_ID,"title":"Title"}), false).unwrap();
    assert_eq!(item.favorite, None);
    assert_eq!(item.chapter_count, None);
    assert_eq!(item.page_count, None);
    assert!(item.authors.is_empty());
    assert!(!item.cover_available);
    assert!(protocol::work(Source::Pica, &json!({"_id":PICA_ID}), false).is_err());
    assert!(protocol::work(
        Source::Pica,
        &json!({"_id":PICA_ID,"title":"T","pagesCount":-1}),
        false
    )
    .is_err());
}

#[test]
fn counts_remain_unknown_or_exact_javascript_safe_integers() {
    let maximum = 9_007_199_254_740_991_u64;
    assert_eq!(protocol::count(&Value::Null).unwrap(), None);
    assert_eq!(protocol::count(&json!(maximum)).unwrap(), Some(maximum));
    assert_eq!(
        protocol::count(&json!(maximum.to_string())).unwrap(),
        Some(maximum)
    );
    for value in [
        json!(maximum + 1),
        json!((maximum + 1).to_string()),
        json!(u64::MAX),
    ] {
        assert_eq!(
            protocol::count(&value).unwrap_err().code,
            "SOURCE_RESPONSE_INVALID"
        );
    }
}

#[test]
fn work_field_limits_match_renderer_utf16_limits() {
    let accepted = json!({"_id":PICA_ID,"title":"界".repeat(2000),"description":"界".repeat(10_000),"author":["𠮷".repeat(1000)],"tags":["tag"]});
    assert!(protocol::work(Source::Pica, &accepted, false).is_ok());
    let base = json!({"_id":PICA_ID,"title":"Title","author":["Author"],"tags":["Tag"]});
    for (field, value) in [
        ("title", json!("t".repeat(2001))),
        ("description", json!("d".repeat(10_001))),
        ("description", json!("𠮷".repeat(5001))),
        ("author", json!("a".repeat(2001))),
        ("author", json!(["𠮷".repeat(1001)])),
        ("author", json!(vec!["author"; 65])),
        ("tags", json!(["t".repeat(2001)])),
        ("tags", json!(vec!["tag"; 65])),
    ] {
        let mut rejected = base.clone();
        rejected[field] = value;
        assert_eq!(
            protocol::work(Source::Pica, &rejected, false)
                .unwrap_err()
                .code,
            "SOURCE_RESPONSE_INVALID",
            "{field}"
        );
    }
    let mut accepted_arrays = base;
    accepted_arrays["author"] = json!(vec!["author"; 64]);
    accepted_arrays["tags"] = json!(vec!["tag"; 64]);
    assert!(protocol::work(Source::Pica, &accepted_arrays, false).is_ok());
}

#[test]
fn whole_work_json_budget_includes_combined_fields_and_escaping() {
    let mut value = json!({"_id":PICA_ID,"title":"Title","author":vec!["a".repeat(1000); 64]});
    let (work, _) = protocol::work(Source::Pica, &value, false).unwrap();
    assert!(serde_json::to_vec(&work).unwrap().len() <= protocol::MAX_WORK_JSON_BYTES);
    value["tags"] = json!(["t".repeat(2000)]);
    assert_eq!(
        protocol::work(Source::Pica, &value, false)
            .unwrap_err()
            .code,
        "SOURCE_RESPONSE_INVALID"
    );
    let escaped = json!({"_id":PICA_ID,"title":"Title","author":vec!["\u{0000}".repeat(200); 64]});
    assert_eq!(
        protocol::work(Source::Pica, &escaped, false)
            .unwrap_err()
            .code,
        "SOURCE_RESPONSE_INVALID"
    );
}

#[test]
fn folder_names_are_bounded_without_inventing_counts() {
    let mut data =
        json!({"total":"0","folder_list":[{"FID":"2","name":"界".repeat(2000)}],"list":[]});
    let (page, _) = protocol::page(Source::Jm, &data, 1, true).unwrap();
    assert_eq!(page.folders[0].count, None);
    data["folder_list"][0]["name"] = json!("界".repeat(2001));
    assert_eq!(
        protocol::page(Source::Jm, &data, 1, true).unwrap_err().code,
        "SOURCE_RESPONSE_INVALID"
    );
}

#[test]
fn favorite_page_retains_folders_and_only_claims_page_scope() {
    let data = json!({"total":"42","count":20,"folder_list":[{"FID":"0","UID":"7","name":"All"},{"FID":"2","UID":"7","name":"Reading"}],"list":[{"id":"123","name":"T","author":"A"}]});
    let (page, _) = protocol::page(Source::Jm, &data, 1, true).unwrap();
    assert_eq!(page.total, Some(42));
    assert_eq!(page.pages, None);
    assert_eq!(page.has_more, None);
    assert_eq!(page.items[0].favorite, Some(true));
    assert_eq!(page.folders[1].id, "2");
    assert_eq!(page.folders[1].count, None);
}

#[test]
fn folder_serialization_matches_frontend_contract_with_unknown_count() {
    let data = json!({"total":"0","count":20,"folder_list":[{"FID":"2","UID":"7","name":"Reading"}],"list":[]});
    let (page, _) = protocol::page(Source::Jm, &data, 1, true).unwrap();
    let value = serde_json::to_value(page).unwrap();
    assert_eq!(
        value["folders"],
        json!([{"id":"2","name":"Reading","count":null}])
    );
    assert!(value["folders"][0].get("folderId").is_none());
    assert_eq!(value["page"], 1);
    assert_eq!(value["total"], 0);
    assert_eq!(value["pages"], Value::Null);
    assert_eq!(value["hasMore"], false);
}

#[test]
fn pica_pagination_rejects_partial_or_contradictory_pages() {
    let good = json!({"comics":{"page":1,"pages":2,"limit":1,"total":2,"docs":[{"_id":PICA_ID,"title":"T"}]}});
    let (page, _) = protocol::page(Source::Pica, &good, 1, false).unwrap();
    assert_eq!(page.has_more, Some(true));
    for field in ["page", "pages", "limit", "total"] {
        let mut bad = good.clone();
        bad["comics"][field] = Value::Null;
        assert!(
            protocol::page(Source::Pica, &bad, 1, false).is_err(),
            "{field}"
        );
    }
    let mut bad = good.clone();
    bad["comics"]["docs"] = json!([]);
    assert!(protocol::page(Source::Pica, &bad, 1, false).is_err());
    assert!(protocol::page(Source::Pica, &good, 2, false).is_err());
    let mut duplicate = good;
    duplicate["comics"]["limit"] = json!(2);
    duplicate["comics"]["pages"] = json!(1);
    duplicate["comics"]["docs"] = json!([{"_id":PICA_ID,"title":"T"},{"_id":PICA_ID,"title":"T"}]);
    assert!(protocol::page(Source::Pica, &duplicate, 1, false).is_err());
}

#[test]
fn cover_descriptors_reject_untrusted_hosts_and_path_escapes() {
    let base = json!({"_id":PICA_ID,"title":"T","thumb":{"fileServer":"https://storage1.picacomic.com","path":"cover/fixture.jpg"}});
    let (item, url) = protocol::work(Source::Pica, &base, false).unwrap();
    assert!(item.cover_available);
    assert_eq!(
        url.as_deref(),
        Some("https://storage1.picacomic.com/static/cover/fixture.jpg")
    );
    let serialized = serde_json::to_string(&item).unwrap();
    assert!(!serialized.contains("picacomic.com"));
    assert!(!serialized.contains("fileServer"));
    for host in [
        "http://storage1.picacomic.com",
        "https://evil.test",
        "https://storage1.picacomic.com.evil.test",
        "https://user@storage1.picacomic.com",
        "https://127.0.0.1",
        "https://storage1.picacomic.com/evil",
    ] {
        let mut bad = base.clone();
        bad["thumb"]["fileServer"] = json!(host);
        assert!(
            !protocol::work(Source::Pica, &bad, false)
                .unwrap()
                .0
                .cover_available
        );
    }
    for path in [
        "../secret",
        "/secret",
        "a/../../b.jpg",
        "a%2fb.jpg",
        "a?token=s",
        "a\\b.jpg",
        "a//b.jpg",
    ] {
        let mut bad = base.clone();
        bad["thumb"]["path"] = json!(path);
        assert!(
            !protocol::work(Source::Pica, &bad, false)
                .unwrap()
                .0
                .cover_available
        );
    }
}

#[test]
fn encrypted_metadata_requires_valid_pkcs7_and_json() {
    let timestamp = 1_700_000_000;
    let mut bytes = br#"{"uid":"1","username":"fixture"}"#.to_vec();
    let padding = 16 - bytes.len() % 16;
    bytes.extend(std::iter::repeat_n(padding as u8, padding));
    let key = format!("{:x}", md5::compute(format!("{timestamp}185Hcomic3PAPP7R")));
    let cipher = Aes256::new_from_slice(key.as_bytes()).unwrap();
    let mut invalid = bytes.clone();
    *invalid.last_mut().unwrap() = 0;
    for value in [&mut bytes, &mut invalid] {
        for block in value.as_chunks_mut::<16>().0 {
            cipher.encrypt_block(GenericArray::from_mut_slice(block));
        }
    }
    assert_eq!(
        protocol::decode_jm(timestamp, &STANDARD.encode(bytes)).unwrap()["uid"],
        "1"
    );
    assert!(protocol::decode_jm(timestamp, &STANDARD.encode(invalid)).is_err());
    assert!(protocol::decode_jm(timestamp, "not-base64").is_err());
}

#[test]
fn signatures_bind_query_method_and_timestamp() {
    let first =
        protocol::pica_signature("users/favourite?s=dd&page=1", "GET", 1_700_000_000).unwrap();
    assert_eq!(first.len(), 64);
    assert_ne!(
        first,
        protocol::pica_signature("users/favourite?s=dd&page=2", "GET", 1_700_000_000).unwrap()
    );
    assert_ne!(
        first,
        protocol::pica_signature("users/favourite?s=dd&page=1", "POST", 1_700_000_000).unwrap()
    );
    assert_ne!(
        first,
        protocol::pica_signature("users/favourite?s=dd&page=1", "GET", 1_700_000_001).unwrap()
    );
}

#[test]
fn covers_are_static_validated_bounded_rasters() {
    let mut png = vec![];
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&[1, 2, 3], 1, 1, image::ExtendedColorType::Rgb8)
        .unwrap();
    assert!(thumbnail::data_url(&png)
        .unwrap()
        .starts_with("data:image/jpeg;base64,"));
    assert!(thumbnail::data_url(b"<svg onload='evil()'></svg>").is_err());
    assert!(thumbnail::data_url(&png[..png.len() / 2]).is_err());
    assert!(thumbnail::data_url(&vec![0; thumbnail::MAX_COVER_BYTES + 1]).is_err());
}

#[test]
fn cover_output_fits_512_pixels_and_256_kib() {
    let mut png = vec![];
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &vec![90; 1024 * 768 * 3],
            1024,
            768,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    assert!(png.len() <= thumbnail::MAX_COVER_BYTES);
    let data_url = thumbnail::data_url(&png).unwrap();
    let jpeg = STANDARD
        .decode(data_url.strip_prefix("data:image/jpeg;base64,").unwrap())
        .unwrap();
    assert!(jpeg.len() <= thumbnail::MAX_OUTPUT_BYTES);
    let image = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg).unwrap();
    assert_eq!((image.width(), image.height()), (512, 384));
}

#[tokio::test]
async fn login_and_restore_validate_profile_and_never_return_passwords() {
    let sources = scripted(vec![
        Ok(json!({"token":"fixture-token"})),
        Ok(json!({"user":{"_id":"account","name":"Name"}})),
        Ok(json!({"user":{"_id":"account","name":"Name"}})),
    ]);
    let login = sources
        .login(Source::Pica, "email@example.invalid", "fixture-password")
        .await
        .unwrap();
    assert_eq!(login.credential.kind(), CredentialKind::SessionToken);
    assert_eq!(login.credential.secret(), "fixture-token");
    assert_eq!(
        sources
            .restore(Source::Pica, &login.credential)
            .await
            .unwrap()
            .account,
        login.account
    );
    let account = serde_json::to_string(&login.account).unwrap();
    assert!(!account.contains("fixture-token"));
    assert!(!account.contains("fixture-password"));
    assert_eq!(sources.recorded.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn credential_kind_mismatch_never_makes_a_request() {
    let sources = scripted(vec![]);
    let wrong = StoredCredential::new("name", CredentialKind::SessionCookie, "session").unwrap();
    assert_eq!(
        sources
            .restore(Source::Pica, &wrong)
            .await
            .err()
            .unwrap()
            .code,
        "SOURCE_CREDENTIAL_INVALID"
    );
    assert!(sources.recorded.lock().unwrap().is_empty());
}

#[tokio::test]
async fn desired_favorite_avoids_toggle_when_already_satisfied() {
    let sources = scripted(vec![Ok(detail_value(Source::Pica, Some(true)))]);
    let result = sources
        .set_favorite(&session(Source::Pica), PICA_ID, true)
        .await
        .unwrap();
    assert!(!result.changed);
    assert!(result.verified);
    assert_eq!(sources.recorded.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn favorite_mutation_is_one_toggle_followed_by_readback() {
    for source in [Source::Jm, Source::Pica] {
        let id = if source == Source::Jm { "123" } else { PICA_ID };
        let toggle = if source == Source::Jm {
            json!({"type":"add"})
        } else {
            json!({"action":"favourite"})
        };
        let sources = scripted(vec![
            Ok(detail_value(source, Some(false))),
            Ok(toggle),
            Ok(detail_value(source, Some(true))),
        ]);
        assert!(
            sources
                .set_favorite(&session(source), id, true)
                .await
                .unwrap()
                .changed
        );
        let requests = sources.recorded.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert_eq!(
            requests
                .iter()
                .filter(|(_, method, _)| method == Method::POST)
                .count(),
            1
        );
        assert_eq!(requests[2].1, Method::GET);
    }
}

#[tokio::test]
async fn ambiguous_toggle_reconciles_without_retrying_post() {
    let sources = scripted(vec![
        Ok(detail_value(Source::Pica, Some(false))),
        Err(error("SOURCE_TIMEOUT")),
        Ok(detail_value(Source::Pica, Some(true))),
    ]);
    assert!(
        sources
            .set_favorite(&session(Source::Pica), PICA_ID, true)
            .await
            .unwrap()
            .verified
    );
    assert_eq!(
        sources
            .recorded
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, method, _)| method == Method::POST)
            .count(),
        1
    );
    let sources = scripted(vec![
        Ok(detail_value(Source::Pica, Some(false))),
        Err(error("SOURCE_TIMEOUT")),
        Err(error("SOURCE_TIMEOUT")),
    ]);
    assert_eq!(
        sources
            .set_favorite(&session(Source::Pica), PICA_ID, true)
            .await
            .unwrap_err()
            .code,
        "FAVORITE_OUTCOME_UNKNOWN"
    );
    assert_eq!(sources.recorded.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn unknown_favorite_state_never_toggles_and_expiry_propagates() {
    let sources = scripted(vec![Ok(detail_value(Source::Pica, None))]);
    assert_eq!(
        sources
            .set_favorite(&session(Source::Pica), PICA_ID, true)
            .await
            .unwrap_err()
            .code,
        "FAVORITE_STATE_UNKNOWN"
    );
    assert_eq!(sources.recorded.lock().unwrap().len(), 1);
    let sources = scripted(vec![
        Ok(detail_value(Source::Pica, Some(false))),
        Err(error("SESSION_EXPIRED")),
    ]);
    assert_eq!(
        sources
            .set_favorite(&session(Source::Pica), PICA_ID, true)
            .await
            .unwrap_err()
            .code,
        "SESSION_EXPIRED"
    );
    assert_eq!(sources.recorded.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn response_identity_mismatch_cannot_populate_cover_cache() {
    let sources = scripted(vec![Ok(json!({"id":"456","name":"Wrong"}))]);
    let session = session(Source::Jm);
    assert_eq!(
        sources.detail(&session, "123").await.unwrap_err().code,
        "SOURCE_RESPONSE_ID_MISMATCH"
    );
    assert!(session.covers.lock().unwrap().is_empty());
}

#[tokio::test]
async fn unknown_thumbnail_id_never_performs_an_api_call() {
    let sources = scripted(vec![]);
    assert_eq!(
        sources
            .thumbnail(&session(Source::Pica), PICA_ID)
            .await
            .unwrap_err()
            .code,
        "WORK_NOT_LOADED"
    );
    assert!(sources.recorded.lock().unwrap().is_empty());
}

#[test]
fn session_thumbnail_descriptors_are_bounded_and_separate() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SourceSession>();
    assert_send_sync::<WorkbenchSources>();
    let first = session(Source::Jm);
    let second = session(Source::Jm);
    first
        .remember_covers((1..=1001).map(|id| {
            (
                id.to_string(),
                Some(format!(
                    "https://cdn-msp3.18comic.vip/media/albums/{id}_3x4.jpg"
                )),
            )
        }))
        .unwrap();
    assert_eq!(first.covers.lock().unwrap().len(), MAX_KNOWN_WORKS);
    assert!(
        first
            .covers
            .lock()
            .unwrap()
            .values()
            .filter(|url| url.is_some())
            .count()
            <= MAX_COVER_DESCRIPTORS
    );
    assert!(second.covers.lock().unwrap().is_empty());
}

#[tokio::test]
async fn jm_login_uses_avs_value_then_validates_profile() {
    let sources = scripted(vec![
        Ok(json!({"uid":"7","username":"Fixture","s":"fixture-avs"})),
        Ok(json!({"uid":"7","username":"Fixture"})),
    ]);
    let result = sources
        .login(Source::Jm, "fixture", "fixture-password")
        .await
        .unwrap();
    assert_eq!(result.credential.kind(), CredentialKind::SessionCookie);
    assert_eq!(result.credential.secret(), "fixture-avs");
    assert_eq!(result.account.account_id, "7");
    assert_eq!(
        sources.recorded.lock().unwrap().as_slice(),
        &[
            (Source::Jm, Method::POST, "/login".to_owned()),
            (Source::Jm, Method::POST, "/login".to_owned()),
        ]
    );
}

#[tokio::test]
async fn unscripted_tests_cannot_reach_account_or_cover_network() {
    let sources = WorkbenchSources::new().unwrap();
    assert_eq!(
        sources
            .login(Source::Jm, "fixture", "fixture-password")
            .await
            .err()
            .unwrap()
            .code,
        "SOURCE_LIVE_REQUESTS_DISABLED"
    );
    let session = session(Source::Jm);
    session
        .remember_covers([(
            "123".to_owned(),
            Some("https://cdn-msp3.18comic.vip/media/albums/123_3x4.jpg".to_owned()),
        )])
        .unwrap();
    assert_eq!(
        sources.thumbnail(&session, "123").await.unwrap_err().code,
        "SOURCE_LIVE_REQUESTS_DISABLED"
    );
    assert!(sources.recorded.lock().unwrap().is_empty());
}

#[tokio::test]
async fn known_work_without_cover_returns_none_without_network() {
    let sources = scripted(vec![Ok(detail_value(Source::Pica, Some(false)))]);
    let session = session(Source::Pica);
    assert!(
        !sources
            .detail(&session, PICA_ID)
            .await
            .unwrap()
            .cover_available
    );
    assert_eq!(sources.thumbnail(&session, PICA_ID).await.unwrap(), None);
    assert_eq!(sources.recorded.lock().unwrap().len(), 1);
}
