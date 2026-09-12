//! Account and catalog operations only. No task authority, chapter/media routes,
//! filesystem writes, automatic pagination, retries, or production-state access.
mod cover;
mod protocol;
mod thumbnail;
mod types;

pub use protocol::parse_work_id;
pub use types::*;

use protocol::{error, JM_HOST, PICA_HOST, PICA_KEY, PICA_NONCE};
use reqwest::{header::HeaderValue, Client, Method, Url};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex as AsyncMutex;
use workbench_credentials::{CredentialKind, StoredCredential};

const MAX_RESPONSE: usize = 8 * 1024 * 1024;
const MAX_KNOWN_WORKS: usize = 20_000;
const MAX_COVER_DESCRIPTORS: usize = 128;

/// An opaque session. Secrets and remote cover URLs are never serialized.
pub struct SourceSession {
    source: Source,
    credential: StoredCredential,
    operation: AsyncMutex<()>,
    covers: Mutex<CoverCache>,
}

#[derive(Default)]
struct CoverCache {
    known: HashMap<String, bool>,
    known_order: VecDeque<String>,
    urls: HashMap<String, String>,
    url_order: VecDeque<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum CoverLookup {
    Unknown,
    Missing,
    Evicted,
    Ready(String),
}

impl CoverCache {
    fn remember(&mut self, work_id: String, url: Option<String>) {
        if !self.known.contains_key(&work_id) {
            if self.known.len() >= MAX_KNOWN_WORKS {
                if let Some(oldest) = self.known_order.pop_front() {
                    self.known.remove(&oldest);
                    self.urls.remove(&oldest);
                    self.url_order.retain(|id| id != &oldest);
                }
            }
            self.known_order.push_back(work_id.clone());
        }
        self.known.insert(work_id.clone(), url.is_some());
        self.url_order.retain(|id| id != &work_id);
        if let Some(url) = url {
            if !self.urls.contains_key(&work_id) && self.urls.len() >= MAX_COVER_DESCRIPTORS {
                if let Some(oldest) = self.url_order.pop_front() {
                    self.urls.remove(&oldest);
                }
            }
            self.url_order.push_back(work_id.clone());
            self.urls.insert(work_id, url);
        } else {
            self.urls.remove(&work_id);
        }
    }

    fn lookup(&mut self, work_id: &str) -> CoverLookup {
        match self.known.get(work_id) {
            None => CoverLookup::Unknown,
            Some(false) => CoverLookup::Missing,
            Some(true) => match self.urls.get(work_id) {
                Some(url) => {
                    let url = url.clone();
                    self.url_order.retain(|id| id != work_id);
                    self.url_order.push_back(work_id.to_owned());
                    CoverLookup::Ready(url)
                }
                None => CoverLookup::Evicted,
            },
        }
    }
}

impl SourceSession {
    pub fn source(&self) -> Source {
        self.source
    }

    /// Native-only transfer to an explicitly confirmed desktop worker. This
    /// secret has no renderer/serialization path and grants no task authority.
    pub fn pica_download_credential(&self) -> Option<StoredCredential> {
        (self.source == Source::Pica).then(|| self.credential.clone())
    }

    fn remember_covers(
        &self,
        works: impl IntoIterator<Item = (String, Option<String>)>,
    ) -> SourceResult<()> {
        let mut cache = self
            .covers
            .lock()
            .map_err(|_| error("SOURCE_SESSION_FAILED"))?;
        for (work_id, url) in works {
            cache.remember(work_id, url);
        }
        Ok(())
    }
}

pub struct LoginResult {
    pub session: SourceSession,
    pub account: SourceAccount,
    pub credential: StoredCredential,
}

pub struct WorkbenchSources {
    api: Client,
    covers: Client,
    #[cfg(test)]
    script: Option<Mutex<std::collections::VecDeque<SourceResult<Value>>>>,
    #[cfg(test)]
    recorded: Mutex<Vec<(Source, Method, String)>>,
    #[cfg(test)]
    cover_script: Option<Mutex<VecDeque<SourceResult<cover::CoverResponse>>>>,
    #[cfg(test)]
    cover_recorded: Mutex<Vec<Url>>,
}

impl WorkbenchSources {
    pub fn new() -> SourceResult<Self> {
        fn client() -> SourceResult<Client> {
            Client::builder()
                .timeout(Duration::from_secs(30))
                .https_only(true)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| error("SOURCE_CLIENT_FAILED"))
        }
        // Neither client enables a cookie jar. Account credentials are attached
        // only to the fixed API request; the cover client has none.
        Ok(Self {
            api: client()?,
            covers: client()?,
            #[cfg(test)]
            script: None,
            #[cfg(test)]
            recorded: Mutex::new(vec![]),
            #[cfg(test)]
            cover_script: None,
            #[cfg(test)]
            cover_recorded: Mutex::new(vec![]),
        })
    }

    pub async fn login(
        &self,
        source: Source,
        username: &str,
        password: &str,
    ) -> SourceResult<LoginResult> {
        if username.trim().is_empty()
            || password.is_empty()
            || username.len() > 2048
            || password.len() > 4096
            || username.chars().any(char::is_control)
            || password.chars().any(char::is_control)
        {
            return Err(error("LOGIN_INPUT_INVALID"));
        }
        let (route, payload) = match source {
            Source::Jm => ("/login", json!({"username":username,"password":password})),
            Source::Pica => (
                "auth/sign-in",
                json!({"email":username,"password":password}),
            ),
        };
        let data = self
            .request(source, None, Method::POST, route, Some(payload), true)
            .await?;
        let secret = protocol::required_text(
            &data[match source {
                Source::Jm => "s",
                Source::Pica => "token",
            }],
        )?;
        let kind = expected_kind(source);
        validate_secret(source, &secret)?;
        let credential = StoredCredential::new(username, kind, secret)
            .map_err(|_| error("SOURCE_CREDENTIAL_INVALID"))?;
        let session = new_session(source, credential.clone());
        // A login response is insufficient to publish a connected account.
        let account = self.profile(&session).await?;
        Ok(LoginResult {
            session,
            account,
            credential,
        })
    }

    pub async fn restore(
        &self,
        source: Source,
        credential: &StoredCredential,
    ) -> SourceResult<LoginResult> {
        if credential.kind() != expected_kind(source) {
            return Err(error("SOURCE_CREDENTIAL_INVALID"));
        }
        validate_secret(source, credential.secret())?;
        let session = new_session(source, credential.clone());
        let account = self.profile(&session).await?;
        Ok(LoginResult {
            session,
            account,
            credential: credential.clone(),
        })
    }

    pub async fn profile(&self, session: &SourceSession) -> SourceResult<SourceAccount> {
        let _operation = session.operation.lock().await;
        let (method, route) = match session.source {
            Source::Jm => (Method::POST, "/login"),
            Source::Pica => (Method::GET, "users/profile"),
        };
        let data = self
            .request(session.source, Some(session), method, route, None, false)
            .await?;
        protocol::account(session.source, &data)
    }

    pub async fn favorites(
        &self,
        session: &SourceSession,
        request: FavoritePageRequest,
    ) -> SourceResult<SourcePage> {
        protocol::validate_page(request.page)?;
        let route = match session.source {
            Source::Jm => {
                if request.reverse {
                    return Err(error("SOURCE_REVERSE_UNSUPPORTED"));
                }
                let folder = request.folder_id.as_deref().unwrap_or("0");
                protocol::validate_folder(folder)?;
                format!("/favorite?page={}&o=mr&folder_id={folder}", request.page)
            }
            Source::Pica => {
                if request.folder_id.is_some() {
                    return Err(error("SOURCE_FOLDER_UNSUPPORTED"));
                }
                let order = if request.reverse { "da" } else { "dd" };
                format!("users/favourite?s={order}&page={}", request.page)
            }
        };
        let _operation = session.operation.lock().await;
        let data = self
            .request(
                session.source,
                Some(session),
                Method::GET,
                &route,
                None,
                false,
            )
            .await?;
        let (page, covers) = protocol::page(session.source, &data, request.page, true)?;
        session.remember_covers(page.items.iter().map(|work| {
            (
                work.work_id.clone(),
                covers
                    .iter()
                    .find(|(id, _)| id == &work.work_id)
                    .map(|(_, url)| url.clone()),
            )
        }))?;
        Ok(page)
    }

    pub async fn search(
        &self,
        session: &SourceSession,
        keyword: &str,
        page: u64,
    ) -> SourceResult<SourcePage> {
        protocol::validate_page(page)?;
        if keyword.trim().is_empty()
            || keyword.len() > 1024
            || keyword.chars().any(char::is_control)
        {
            return Err(error("SOURCE_QUERY_INVALID"));
        }
        let _operation = session.operation.lock().await;
        let (method, route, payload) = match session.source {
            Source::Jm => {
                let mut url = Url::parse(&format!("https://{JM_HOST}/search"))
                    .map_err(|_| error("SOURCE_CLIENT_FAILED"))?;
                url.query_pairs_mut()
                    .append_pair("main_tag", "0")
                    .append_pair("search_query", keyword)
                    .append_pair("page", &page.to_string())
                    .append_pair("o", "mr");
                (
                    Method::GET,
                    format!("/search?{}", url.query().unwrap_or_default()),
                    None,
                )
            }
            Source::Pica => (
                Method::POST,
                format!("comics/advanced-search?page={page}"),
                Some(json!({"keyword":keyword,"sort":"dd","categories":[]})),
            ),
        };
        let data = self
            .request(
                session.source,
                Some(session),
                method,
                &route,
                payload,
                false,
            )
            .await?;
        if session.source == Source::Jm && !data["redirect_aid"].is_null() {
            if page != 1 {
                return Err(error("SOURCE_PAGINATION_INVALID"));
            }
            let id = protocol::required_text(&data["redirect_aid"])?;
            let work = self.detail_inner(session, &id).await?;
            return Ok(SourcePage {
                page,
                total: Some(1),
                pages: Some(1),
                has_more: Some(false),
                folders: vec![],
                items: vec![work],
            });
        }
        let (page, covers) = protocol::page(session.source, &data, page, false)?;
        session.remember_covers(page.items.iter().map(|work| {
            (
                work.work_id.clone(),
                covers
                    .iter()
                    .find(|(id, _)| id == &work.work_id)
                    .map(|(_, url)| url.clone()),
            )
        }))?;
        Ok(page)
    }

    pub async fn detail(
        &self,
        session: &SourceSession,
        id_or_link: &str,
    ) -> SourceResult<SourceWork> {
        let _operation = session.operation.lock().await;
        self.detail_inner(session, id_or_link).await
    }

    async fn detail_inner(
        &self,
        session: &SourceSession,
        id_or_link: &str,
    ) -> SourceResult<SourceWork> {
        let id = parse_work_id(session.source, id_or_link)?;
        let route = match session.source {
            Source::Jm => format!("/album?id={id}"),
            Source::Pica => format!("comics/{id}"),
        };
        let data = self
            .request(
                session.source,
                Some(session),
                Method::GET,
                &route,
                None,
                false,
            )
            .await?;
        let record = match session.source {
            Source::Jm => &data,
            Source::Pica => &data["comic"],
        };
        let (work, cover) = protocol::work(session.source, record, false)?;
        if work.work_id != id {
            return Err(error("SOURCE_RESPONSE_ID_MISMATCH"));
        }
        session.remember_covers([(id, cover)])?;
        Ok(work)
    }

    pub async fn set_favorite(
        &self,
        session: &SourceSession,
        work_id: &str,
        desired: bool,
    ) -> SourceResult<FavoriteUpdate> {
        let _operation = session.operation.lock().await;
        let id = parse_work_id(session.source, work_id)?;
        let before = self
            .detail_inner(session, &id)
            .await?
            .favorite
            .ok_or(error("FAVORITE_STATE_UNKNOWN"))?;
        if before == desired {
            return Ok(FavoriteUpdate {
                work_id: id,
                favorite: desired,
                changed: false,
                verified: true,
            });
        }
        let (route, payload) = match session.source {
            Source::Jm => ("/favorite".to_owned(), Some(json!({"aid":id}))),
            Source::Pica => (format!("comics/{id}/favourite"), None),
        };
        // A toggle is not idempotent. Never retry it, even after a timeout.
        let response = self
            .request(
                session.source,
                Some(session),
                Method::POST,
                &route,
                payload,
                false,
            )
            .await;
        if response
            .as_ref()
            .is_err_and(|e| e.code == "SESSION_EXPIRED")
        {
            return Err(error("SESSION_EXPIRED"));
        }
        let reported = response
            .as_ref()
            .ok()
            .and_then(|data| toggle_result(session.source, data));
        // Also attempt one read-only reconciliation after an ambiguous response.
        let after = self.detail_inner(session, &id).await;
        if after.as_ref().is_err_and(|e| e.code == "SESSION_EXPIRED") {
            return Err(error("SESSION_EXPIRED"));
        }
        match after.ok().and_then(|work| work.favorite) {
            Some(state) if state == desired && (response.is_err() || reported == Some(desired)) => {
                Ok(FavoriteUpdate {
                    work_id: id,
                    favorite: desired,
                    changed: true,
                    verified: true,
                })
            }
            _ => Err(error("FAVORITE_OUTCOME_UNKNOWN")),
        }
    }

    /// Only previously validated metadata in this session can name a cover.
    pub async fn thumbnail(
        &self,
        session: &SourceSession,
        work_id: &str,
    ) -> SourceResult<Option<String>> {
        let id = parse_work_id(session.source, work_id)?;
        tokio::time::timeout(Duration::from_secs(30), self.thumbnail_inner(session, &id))
            .await
            .map_err(|_| error("SOURCE_TIMEOUT"))?
    }

    async fn request(
        &self,
        source: Source,
        session: Option<&SourceSession>,
        method: Method,
        route: &str,
        payload: Option<Value>,
        login: bool,
    ) -> SourceResult<Value> {
        #[cfg(test)]
        if let Some(script) = &self.script {
            self.recorded
                .lock()
                .unwrap()
                .push((source, method, route.to_owned()));
            return script
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected extra request");
        }
        check_live_environment()?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| error("SOURCE_CLOCK_INVALID"))?
            .as_secs();
        let host = match source {
            Source::Jm => JM_HOST,
            Source::Pica => PICA_HOST,
        };
        let separator = if route.starts_with('/') { "" } else { "/" };
        let mut request = self
            .api
            .request(method.clone(), format!("https://{host}{separator}{route}"));
        match source {
            Source::Jm => {
                request = request.header("token", format!("{:x}", md5::compute(format!("{timestamp}18comicAPP"))))
                    .header("tokenparam", format!("{timestamp},2.0.13"))
                    .header("user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36");
                if let Some(session) = session {
                    request = request.header(
                        "cookie",
                        sensitive_header(&format!("AVS={}", session.credential.secret()))?,
                    );
                }
                if let Some(payload) = payload {
                    request = request.form(&payload);
                }
            }
            Source::Pica => {
                request = request
                    .header("api-key", PICA_KEY)
                    .header("accept", "application/vnd.picacomic.com.v1+json")
                    .header("app-channel", "2")
                    .header("time", timestamp.to_string())
                    .header("nonce", PICA_NONCE)
                    .header("app-version", "2.2.1.2.3.3")
                    .header("app-uuid", "defaultUuid")
                    .header("app-platform", "android")
                    .header("app-build-version", "44")
                    .header("content-type", "application/json; charset=UTF-8")
                    .header("user-agent", "okhttp/3.8.1")
                    .header("image-quality", "original")
                    .header(
                        "signature",
                        protocol::pica_signature(route, method.as_str(), timestamp)?,
                    );
                if let Some(session) = session {
                    request = request.header(
                        "authorization",
                        sensitive_header(session.credential.secret())?,
                    );
                }
                if let Some(payload) = payload {
                    request = request.json(&payload);
                }
            }
        }
        let response = request.send().await.map_err(transport_error)?;
        let status = response.status();
        if status.is_redirection() {
            return Err(error("SOURCE_REDIRECT_REFUSED"));
        }
        if login && matches!(status.as_u16(), 400 | 401) {
            return Err(error("LOGIN_REJECTED"));
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(error("SESSION_EXPIRED"));
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            return Err(error("SOURCE_ACCESS_DENIED"));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(error("SOURCE_RATE_LIMITED"));
        }
        if !status.is_success() {
            return Err(error("SOURCE_REQUEST_FAILED"));
        }
        let bytes = bounded_bytes(response, MAX_RESPONSE).await?;
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| error("SOURCE_RESPONSE_INVALID"))?;
        let code = protocol::count(&value["code"])?;
        if code == Some(401) {
            return Err(error(if login {
                "LOGIN_REJECTED"
            } else {
                "SESSION_EXPIRED"
            }));
        }
        if code != Some(200) {
            return Err(error("SOURCE_API_REJECTED"));
        }
        match source {
            Source::Jm => protocol::decode_jm(
                timestamp,
                value["data"]
                    .as_str()
                    .ok_or(error("SOURCE_RESPONSE_INVALID"))?,
            ),
            Source::Pica => value
                .get("data")
                .filter(|value| value.is_object())
                .cloned()
                .ok_or(error("SOURCE_RESPONSE_INVALID")),
        }
    }
}

fn new_session(source: Source, credential: StoredCredential) -> SourceSession {
    SourceSession {
        source,
        credential,
        operation: AsyncMutex::new(()),
        covers: Mutex::new(CoverCache::default()),
    }
}

fn expected_kind(source: Source) -> CredentialKind {
    match source {
        Source::Jm => CredentialKind::SessionCookie,
        Source::Pica => CredentialKind::SessionToken,
    }
}

fn validate_secret(source: Source, secret: &str) -> SourceResult<()> {
    if secret.is_empty() {
        return Err(error("AUTH_REQUIRED"));
    }
    sensitive_header(secret)?;
    if source == Source::Jm
        && !secret
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && !b";,\"\\".contains(&byte))
    {
        return Err(error("SOURCE_CREDENTIAL_INVALID"));
    }
    Ok(())
}

fn sensitive_header(value: &str) -> SourceResult<HeaderValue> {
    let mut header =
        HeaderValue::from_str(value).map_err(|_| error("SOURCE_CREDENTIAL_INVALID"))?;
    header.set_sensitive(true);
    Ok(header)
}

fn toggle_result(source: Source, value: &Value) -> Option<bool> {
    match source {
        Source::Jm => match value["type"].as_str()? {
            "add" => Some(true),
            "remove" => Some(false),
            _ => None,
        },
        Source::Pica => match value["action"].as_str()? {
            "favourite" => Some(true),
            "un_favourite" => Some(false),
            _ => None,
        },
    }
}

fn check_live_environment() -> SourceResult<()> {
    if cfg!(test) || std::env::var("GITHUB_ACTIONS").is_ok_and(|value| value == "true") {
        Err(error("SOURCE_LIVE_REQUESTS_DISABLED"))
    } else {
        Ok(())
    }
}

fn transport_error(error_value: reqwest::Error) -> SourceError {
    error(if error_value.is_timeout() {
        "SOURCE_TIMEOUT"
    } else if error_value.is_connect() {
        "SOURCE_CONNECTION_FAILED"
    } else {
        "SOURCE_REQUEST_FAILED"
    })
}

async fn bounded_bytes(mut response: reqwest::Response, limit: usize) -> SourceResult<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(error("SOURCE_RESPONSE_TOO_LARGE"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
        if bytes
            .len()
            .checked_add(chunk.len())
            .is_none_or(|length| length > limit)
        {
            return Err(error("SOURCE_RESPONSE_TOO_LARGE"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests;
