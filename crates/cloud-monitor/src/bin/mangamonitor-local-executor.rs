use cloud_monitor::{
    assistant_task_gate::{self, GateLedger},
    executor_handoff::ExecutorCommand,
    local_execution_orchestrator, persistence,
};
use serde_json::Value;
use std::{env, fs, path::{Path, PathBuf}};

fn usage() -> ! {
    eprintln!(
        "usage: mangamonitor-local-executor --state <monitor-state-dir> --command <executor-command.json> --staging-root <dir> [--gates <assistant-task-gates.json>] [--completed-at <RFC3339>]\nPica commands require PICA_TOKEN in the local process environment."
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
            return Err("LOCAL_EXECUTOR_GATE_INPUT_MISSING".into());
        }
        return Ok(GateLedger::default());
    }
    let value: Value = serde_json::from_slice(
        &fs::read(&path).map_err(|_| "LOCAL_EXECUTOR_GATE_INPUT_READ")?,
    )
    .map_err(|_| "LOCAL_EXECUTOR_GATE_INPUT_JSON")?;
    assistant_task_gate::parse_ledger(value)
}

fn load_current(
    state_dir: &Path,
    gates: Option<&Path>,
) -> Result<(cloud_monitor::monitor::State, GateLedger), String> {
    Ok((persistence::load(state_dir)?, load_ledger(state_dir, gates)?))
}

fn load_command(path: &Path) -> Result<ExecutorCommand, String> {
    serde_json::from_slice(&fs::read(path).map_err(|_| "LOCAL_EXECUTOR_COMMAND_READ")?)
        .map_err(|_| "LOCAL_EXECUTOR_COMMAND_JSON".into())
}

#[tokio::main]
async fn main() {
    if github_actions_runtime_forbidden(env::var("GITHUB_ACTIONS").ok().as_deref()) {
        eprintln!("LOCAL_EXECUTOR_GITHUB_ACTIONS_FORBIDDEN");
        std::process::exit(77);
    }

    let mut args = env::args().skip(1);
    let mut state: Option<PathBuf> = None;
    let mut gates: Option<PathBuf> = None;
    let mut command: Option<PathBuf> = None;
    let mut staging_root: Option<PathBuf> = None;
    let mut completed_at: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => state = args.next().map(PathBuf::from),
            "--gates" => gates = args.next().map(PathBuf::from),
            "--command" => command = args.next().map(PathBuf::from),
            "--staging-root" => staging_root = args.next().map(PathBuf::from),
            "--completed-at" => completed_at = args.next(),
            _ => usage(),
        }
    }

    let state_dir = state.unwrap_or_else(|| usage());
    let command_path = command.unwrap_or_else(|| usage());
    let staging_root = staging_root.unwrap_or_else(|| usage());

    let result = async {
        let command = load_command(&command_path)?;
        let (initial_state, initial_ledger) = load_current(&state_dir, gates.as_deref())?;
        let pica_token = if command.source == "pica" {
            Some(env::var("PICA_TOKEN").map_err(|_| "LOCAL_EXECUTOR_PICA_TOKEN_REQUIRED")?)
        } else {
            None
        };

        local_execution_orchestrator::execute_live(
            &initial_state,
            &initial_ledger,
            &command,
            &staging_root,
            pica_token.as_deref(),
            completed_at.as_deref(),
            || load_current(&state_dir, gates.as_deref()),
        )
        .await
    }
    .await;

    match result {
        Ok(report) => println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("local execution report serializes")
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
