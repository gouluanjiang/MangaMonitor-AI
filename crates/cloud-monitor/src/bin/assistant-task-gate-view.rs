use cloud_monitor::{assistant_task_gate, persistence};
use serde_json::Value;
use std::{fs, path::{Path, PathBuf}};

fn usage() -> ! {
    eprintln!(
        "usage: assistant-task-gate-view --state <dir> [--gates <assistant-task-gates.json>] [--task-id <id>] [--offset N --limit N]"
    );
    std::process::exit(2);
}

fn load_ledger(state_dir: &Path, explicit: Option<&Path>) -> Result<assistant_task_gate::GateLedger, String> {
    let path = explicit
        .map(Path::to_path_buf)
        .unwrap_or_else(|| state_dir.join("assistant-task-gates.json"));
    if !path.exists() {
        if explicit.is_some() {
            return Err("ASSISTANT_TASK_GATE_INPUT_MISSING".into());
        }
        return Ok(assistant_task_gate::GateLedger::default());
    }
    let value: Value = serde_json::from_slice(
        &fs::read(&path).map_err(|_| "ASSISTANT_TASK_GATE_INPUT_READ")?,
    )
    .map_err(|_| "ASSISTANT_TASK_GATE_INPUT_JSON")?;
    assistant_task_gate::parse_ledger(value)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut state: Option<PathBuf> = None;
    let mut gates: Option<PathBuf> = None;
    let mut task_id: Option<String> = None;
    let mut offset = 0usize;
    let mut limit = 50usize;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => state = args.next().map(PathBuf::from),
            "--gates" => gates = args.next().map(PathBuf::from),
            "--task-id" => task_id = args.next(),
            "--offset" => offset = args.next().and_then(|value| value.parse().ok()).unwrap_or_else(|| usage()),
            "--limit" => limit = args.next().and_then(|value| value.parse().ok()).unwrap_or_else(|| usage()),
            _ => usage(),
        }
    }
    let state_dir = state.unwrap_or_else(|| usage());
    let result = (|| -> Result<Value, String> {
        let state = persistence::load(&state_dir)?;
        let ledger = load_ledger(&state_dir, gates.as_deref())?;
        if let Some(task_id) = task_id {
            assistant_task_gate::task_view(&state, &ledger, &task_id)
        } else {
            assistant_task_gate::batch_view(&state, &ledger, offset, limit)
        }
    })();
    match result {
        Ok(value) => println!("{}", serde_json::to_string_pretty(&value).expect("view serializes")),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
