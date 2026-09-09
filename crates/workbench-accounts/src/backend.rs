use crate::{AccountError, Result, Source};
use std::future::Future;
use workbench_credentials::StoredCredential;
use workbench_sources::{
    FavoritePageRequest, FavoriteUpdate, SourceAccount, SourcePage, SourceSession, SourceWork,
    WorkbenchSources,
};

pub struct Authenticated<S> {
    pub session: S,
    pub account: SourceAccount,
    pub credential: StoredCredential,
}

// Associated sessions keep fake tests independent of live authentication material.
// Every returned future is Send so Tauri may run the controller off the UI thread.
pub trait SourceBackend: Send + Sync + 'static {
    type Session: Send + Sync + 'static;

    fn login(
        &self,
        source: Source,
        username: &str,
        password: &str,
    ) -> impl Future<Output = Result<Authenticated<Self::Session>>> + Send;
    fn restore(
        &self,
        source: Source,
        credential: &StoredCredential,
    ) -> impl Future<Output = Result<Authenticated<Self::Session>>> + Send;
    fn favorites(
        &self,
        session: &Self::Session,
        request: FavoritePageRequest,
    ) -> impl Future<Output = Result<SourcePage>> + Send;
    fn search(
        &self,
        session: &Self::Session,
        query: &str,
        page: u64,
    ) -> impl Future<Output = Result<SourcePage>> + Send;
    fn detail(
        &self,
        session: &Self::Session,
        input: &str,
    ) -> impl Future<Output = Result<SourceWork>> + Send;
    fn favorite(
        &self,
        session: &Self::Session,
        work_id: &str,
        desired: bool,
    ) -> impl Future<Output = Result<FavoriteUpdate>> + Send;
    fn cover(
        &self,
        session: &Self::Session,
        work_id: &str,
    ) -> impl Future<Output = Result<Option<String>>> + Send;
}

impl SourceBackend for WorkbenchSources {
    type Session = SourceSession;

    async fn login(
        &self,
        source: Source,
        username: &str,
        password: &str,
    ) -> Result<Authenticated<Self::Session>> {
        let result = WorkbenchSources::login(self, source, username, password)
            .await
            .map_err(|error| AccountError::new(error.code))?;
        Ok(Authenticated {
            session: result.session,
            account: result.account,
            credential: result.credential,
        })
    }
    async fn restore(
        &self,
        source: Source,
        credential: &StoredCredential,
    ) -> Result<Authenticated<Self::Session>> {
        let result = WorkbenchSources::restore(self, source, credential)
            .await
            .map_err(|error| AccountError::new(error.code))?;
        Ok(Authenticated {
            session: result.session,
            account: result.account,
            credential: result.credential,
        })
    }
    async fn favorites(
        &self,
        session: &Self::Session,
        request: FavoritePageRequest,
    ) -> Result<SourcePage> {
        WorkbenchSources::favorites(self, session, request)
            .await
            .map_err(|error| AccountError::new(error.code))
    }
    async fn search(&self, session: &Self::Session, query: &str, page: u64) -> Result<SourcePage> {
        WorkbenchSources::search(self, session, query, page)
            .await
            .map_err(|error| AccountError::new(error.code))
    }
    async fn detail(&self, session: &Self::Session, input: &str) -> Result<SourceWork> {
        WorkbenchSources::detail(self, session, input)
            .await
            .map_err(|error| AccountError::new(error.code))
    }
    async fn favorite(
        &self,
        session: &Self::Session,
        work_id: &str,
        desired: bool,
    ) -> Result<FavoriteUpdate> {
        WorkbenchSources::set_favorite(self, session, work_id, desired)
            .await
            .map_err(|error| AccountError::new(error.code))
    }
    async fn cover(&self, session: &Self::Session, work_id: &str) -> Result<Option<String>> {
        WorkbenchSources::thumbnail(self, session, work_id)
            .await
            .map_err(|error| AccountError::new(error.code))
    }
}
