use cloud_monitor::{
    assistant_task_gate::{self, GateLedger},
    executor_handoff::ExecutorCommand,
    local_execution_orchestrator::LocalExecutionReport,
    local_library_import_gate, persistence,
};
use serde_json::Value;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn usage() -> ! {
    eprintln!(
        "usage: mangamonitor-local-import --state <monitor-state-dir> --command <executor-command.json> --report <local-execution-report.json> --staging-root <dir> --library-root <existing-dir> [--gates <assistant-task-gates.json>] [--confirm-import <command_id>]\nWithout --confirm-import the command performs a read-only dry-run. V1.3 accepts add-only download tasks; upgrade/replacement remains disabled. Mutating import also requires <state-parent>/local-materialization-policy.json with schema_version=1, materialization_enabled=true, and exact command_id/task_id/task_revision/target_hash/manifest_hash bindings for the supplied command and accepted staging report."
    );
    std::process::exit(2);
}

fn github_actions_runtime_forbidden(value: Option<&str>) -> bool {
    value == Some("true")
}

fn load_ledger(state_dir: &Path, explicit: Option<&Path>) -> Result<GateLedger, String> {
    let path = explicit
        .map(Path::to_path_buf)
        .unwrap_or_else(|| state_dir.join("assistant-task-gates.json"));
    if !path.exists() {
        if explicit.is_some() {
            return Err("LOCAL_IMPORT_GATE_INPUT_MISSING".into());
        }
        return Ok(GateLedger::default());
    }
    let value: Value = serde_json::from_slice(
        &fs::read(&path).map_err(|_| "LOCAL_IMPORT_GATE_INPUT_READ")?,
    )
    .map_err(|_| "LOCAL_IMPORT_GATE_INPUT_JSON")?;
    assistant_task_gate::parse_ledger(value)
}

fn load_current(
    state_dir: &Path,
    gates: Option<&Path>,
) -> Result<(cloud_monitor::monitor::State, GateLedger), String> {
    Ok((persistence::load(state_dir)?, load_ledger(state_dir, gates)?))
}

fn load_command(path: &Path) -> Result<ExecutorCommand, String> {
    serde_json::from_slice(&fs::read(path).map_err(|_| "LOCAL_IMPORT_COMMAND_READ")?)
        .map_err(|_| "LOCAL_IMPORT_COMMAND_JSON".into())
}

fn load_report(path: &Path) -> Result<LocalExecutionReport, String> {
    serde_json::from_slice(&fs::read(path).map_err(|_| "LOCAL_IMPORT_REPORT_READ")?)
        .map_err(|_| "LOCAL_IMPORT_REPORT_JSON".into())
}

fn main() {
    if github_actions_runtime_forbidden(env::var("GITHUB_ACTIONS").ok().as_deref()) {
        eprintln!("LOCAL_IMPORT_GITHUB_ACTIONS_FORBIDDEN");
        std::process::exit(77);
    }

    let mut args = env::args().skip(1);
    let mut state: Option<PathBuf> = None;
    let mut gates: Option<PathBuf> = None;
    let mut command: Option<PathBuf> = None;
    let mut report: Option<PathBuf> = None;
    let mut staging_root: Option<PathBuf> = None;
    let mut library_root: Option<PathBuf> = None;
    let mut confirm_import: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => state = args.next().map(PathBuf::from),
            "--gates" => gates = args.next().map(PathBuf::from),
            "--command" => command = args.next().map(PathBuf::from),
            "--report" => report = args.next().map(PathBuf::from),
            "--staging-root" => staging_root = args.next().map(PathBuf::from),
            "--library-root" => library_root = args.next().map(PathBuf::from),
            "--confirm-import" => confirm_import = args.next(),
            _ => usage(),
        }
    }

    let state_dir = state.unwrap_or_else(|| usage());
    let command_path = command.unwrap_or_else(|| usage());
    let report_path = report.unwrap_or_else(|| usage());
    let staging_root = staging_root.unwrap_or_else(|| usage());
    let library_root = library_root.unwrap_or_else(|| usage());

    let result = (|| {
        let (current_state, ledger) = load_current(&state_dir, gates.as_deref())?;
        let command = load_command(&command_path)?;
        let report = load_report(&report_path)?;

        if let Some(confirmation) = confirm_import.as_deref() {
            let authority =
                local_library_import_gate::load_materialization_authority(&state_dir)?;
            let receipt = local_library_import_gate::execute_add_only(
                &current_state,
                &ledger,
                &command,
                &report,
                &staging_root,
                &library_root,
                &authority,
                confirmation,
                || load_current(&state_dir, gates.as_deref()),
            )?;
            serde_json::to_value(receipt).map_err(|_| "LOCAL_IMPORT_RECEIPT_SERIALIZE".to_string())
        } else {
            let plan = local_library_import_gate::plan(
                &current_state,
                &ledger,
                &command,
                &report,
                &staging_root,
                &library_root,
            )?;
            serde_json::to_value(plan).map_err(|_| "LOCAL_IMPORT_PLAN_SERIALIZE".to_string())
        }
    })();

    match result {
        Ok(value) => println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("local import output serializes")
        ),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::github_actions_runtime_forbidden;

    #[test]
    fn github_actions_runtime_is_explicitly_forbidden() {
        assert!(github_actions_runtime_forbidden(Some("true")));
        assert!(!github_actions_runtime_forbidden(Some("false")));
        assert!(!github_actions_runtime_forbidden(None));
    }
}
