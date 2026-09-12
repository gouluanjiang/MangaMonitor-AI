use crate::{error, source_key, Result};
use cloud_monitor::{
    assistant_task_gate::{self, GateLedger},
    executor_handoff::{self, ExecutorCommand},
    monitor::{State, Target, Task},
};
use serde_json::json;
use std::collections::BTreeMap;
use workbench_storage::DownloadRecord;

pub(crate) fn current(record: &DownloadRecord) -> Result<(State, GateLedger, ExecutorCommand)> {
    // Manual authority is represented only as a pending task. No matcher entry,
    // scope certificate, source catalog, or PROVEN_NEW evidence is synthesized.
    let target = Target {
        source_key: format!("{}:{}", source_key(record.source), record.metadata.work_id),
        author: record.metadata.authors.join(", "),
        title: record.metadata.title.clone(),
        version: Default::default(),
        coverage: serde_json::Value::Null,
    };
    let work_id = format!("MANUAL_{}", record.id);
    let task = Task {
        task_id: format!("TASK_{}", record.id),
        work_id: work_id.clone(),
        first_seen: String::new(),
        task_revision: record.approval_revision,
        target,
        action: "download".into(),
        status: "pending".into(),
        old_local_item_ids: Vec::new(),
        binding_authority_hash: record.target_hash.clone(),
    };
    let state = State {
        authors: json!({"authors": []}),
        inventory: json!({"works": []}),
        catalog: BTreeMap::new(),
        pending: BTreeMap::from([(work_id, task.clone())]),
        review: BTreeMap::new(),
        cleanup_review: json!([]),
        decisions: Default::default(),
        scan: Default::default(),
    };
    let (ledger, _, _) = assistant_task_gate::plan(
        &state,
        &GateLedger::default(),
        "approve",
        &task.task_id,
        task.task_revision,
        &assistant_task_gate::target_hash(&task),
    )
    .map_err(|_| error("DOWNLOAD_APPROVAL_INVALID"))?;
    let queue = executor_handoff::queue_view(&state, &ledger, 0, 1)
        .map_err(|_| error("DOWNLOAD_APPROVAL_INVALID"))?;
    let command = serde_json::from_value(queue["commands"][0].clone())
        .map_err(|_| error("DOWNLOAD_APPROVAL_INVALID"))?;
    Ok((state, ledger, command))
}
