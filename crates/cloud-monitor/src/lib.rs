pub mod assistant;
pub mod assistant_author;
pub mod assistant_decision;
pub mod assistant_publication;
pub mod assistant_task_gate;
pub mod authority_recovery;
pub mod executor_handoff;
pub mod filesystem_verifier;
pub mod image_download_authorization;
pub mod isolated_staging_execution;
mod jm_media_transform;
pub mod live_media_descriptors;
pub mod live_media_fetch;
mod live_media_transport;
pub mod live_source_preflight;
pub mod local_execution_orchestrator;
pub mod local_executor;
pub mod local_inventory_apply;
pub mod local_inventory_apply_authorization;
pub mod local_inventory_rescan;
pub mod local_inventory_update_candidate;
#[expect(
    dead_code,
    clippy::too_many_arguments,
    reason = "V1.3 private importer keeps the audited plan in its validation bundle and passes all safety bindings explicitly with the current-state reload callback"
)]
mod local_library_import;
#[expect(
    clippy::too_many_arguments,
    reason = "V1.3 public gate keeps every safety binding explicit, including the current-state reload callback"
)]
pub mod local_library_import_gate;
pub mod local_task_completion;
pub mod matcher_m2;
pub(crate) mod media_validation;
pub mod monitor;
mod parallel_media_processing;
pub mod persistence;
pub mod runner;
pub mod scope_certificates;
pub mod source_bridge_request;
pub mod source_completion;
pub mod source_media_descriptors;
pub mod source_preflight;
pub mod source_preflight_authorization;
pub mod staging_manifest;
pub mod verified_execution_receipt;
