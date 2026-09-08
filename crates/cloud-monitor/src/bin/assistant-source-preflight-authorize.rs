use cloud_monitor::{
    assistant_task_gate,
    executor_handoff::ExecutorCommand,
    local_executor::LocalExecutionPlan,
    persistence,
    source_bridge_request::SourceBridgeRequest,
    source_preflight_authorization,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn usage() -> ! {
    eprintln!(
        "usage: assistant-source-preflight-authorize --state <dir> [--gates <assistant-task-gates.json>] --command <command.json> --plan <plan.json> --request <request.json>"
    );
    std::process::exit(2);
}

fn read_json<T: DeserializeOwned>(path: &Path, label: &str) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|_| format!("{label}_READ_FAILED"))?;
    serde_json::from_slice(&bytes).map_err(|_| format!("{label}_PARSE_FAILED"))
}

fn load_ledger(
    state_dir: &Path,
    explicit: Option<&Path>,
) -> Result<assistant_task_gate::GateLedger, String> {
    let path = explicit
        .map(Path::to_path_buf)
        .unwrap_or_else(|| state_dir.join("assistant-task-gates.json"));
    if !path.exists() {
        return Err("ASSISTANT_TASK_GATE_INPUT_MISSING".into());
    }
    let value: Value = read_json(&path, "ASSISTANT_TASK_GATE_INPUT")?;
    assistant_task_gate::parse_ledger(value)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut state: Option<PathBuf> = None;
    let mut gates: Option<PathBuf> = None;
    let mut command: Option<PathBuf> = None;
    let mut plan: Option<PathBuf> = None;
    let mut request: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => state = args.next().map(PathBuf::from),
            "--gates" => gates = args.next().map(PathBuf::from),
            "--command" => command = args.next().map(PathBuf::from),
            "--plan" => plan = args.next().map(PathBuf::from),
            "--request" => request = args.next().map(PathBuf::from),
            _ => usage(),
        }
    }

    let state_dir = state.unwrap_or_else(|| usage());
    let command_path = command.unwrap_or_else(|| usage());
    let plan_path = plan.unwrap_or_else(|| usage());
    let request_path = request.unwrap_or_else(|| usage());

    let result = (|| -> Result<_, String> {
        // Authorization is security-sensitive: use the atomically committed
        // authoritative checkpoint, never reconstruct a potentially mixed
        // generation from the eight human-facing export files.
        let state = persistence::load_checkpoint(&state_dir)?;
        let ledger = load_ledger(&state_dir, gates.as_deref())?;
        let command: ExecutorCommand = read_json(&command_path, "COMMAND")?;
        let plan: LocalExecutionPlan = read_json(&plan_path, "PLAN")?;
        let request: SourceBridgeRequest = read_json(&request_path, "REQUEST")?;
        source_preflight_authorization::authorize(&state, &ledger, &command, &plan, &request)
    })();

    match result {
        Ok(value) => println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("authorization serializes")
        ),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
