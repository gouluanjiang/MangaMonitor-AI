//! Explicitly approved single-work JM/Pica downloads. No production inventory or
//! phone-library mutation, background scheduling, overwrite, or deletion.
mod adapter;
mod fs;
mod materialize;
mod presence;
mod service;
pub use presence::LocalFiles;
pub use service::{
    AwaitingIndexReceipt, Control, DownloadPlan, DownloadService, DownloadSnapshot, DownloadTask,
};
pub use workbench_storage::{DownloadPhase, JmDownloadMetadata, Source, StoreError};
pub type DownloadMetadata = JmDownloadMetadata;
pub type Result<T> = std::result::Result<T, StoreError>;
pub(crate) const fn error(code: &'static str) -> StoreError {
    StoreError { code }
}
pub(crate) fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) const fn source_key(source: Source) -> &'static str {
    match source {
        Source::Jm => "jm",
        Source::Pica => "pica",
    }
}
pub(crate) const fn source_label(source: Source) -> &'static str {
    match source {
        Source::Jm => "JM",
        Source::Pica => "Pica",
    }
}
pub(crate) fn now() -> Result<u64> {
    use std::time::{SystemTime, UNIX_EPOCH};
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| error("DOWNLOAD_CLOCK_INVALID"))?
            .as_millis(),
    )
    .ok()
    .filter(|v| *v <= workbench_storage::MAX_SAFE_INTEGER)
    .ok_or(error("DOWNLOAD_CLOCK_INVALID"))
}
