use cloud_monitor::{assistant_task_gate, persistence};
use serde_json::Value;
use std::{fs, path::{Path, PathBuf}};

fn usage() -> ! {
    eprintln!(
        "usage: assistant-task-gate-edit --state <dir> --output <new-dir> --operation <recommend|clear-recommendation|approve|revoke> --task-id <id> --task-revision <n> --target-hash <sha256> [--gates <assistant-task-gates.json>]"
    );
    std::process::exit(2);
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| "ASSISTANT_TASK_GATE_SERIALIZE")?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|_| format!("ASSISTANT_TASK_GATE_WRITE:{}", path.display()))
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

fn run(
    state_dir: &Path,
    gates: Option<&Path>,
    output: &Path,
    operation: &str,
    task_id: &str,
    revision: u64,
    target_hash: &str,
) -> Result<(), String> {
    if output.exists() {
        return Err("ASSISTANT_TASK_GATE_OUTPUT_EXISTS".into());
    }
    let state = persistence::load(state_dir)?;
    let ledger = load_ledger(state_dir, gates)?;
    let (proposed, audit, preview) = assistant_task_gate::plan(
        &state,
        &ledger,
        operation,
        task_id,
        revision,
        target_hash,
    )?;
    fs::create_dir_all(output).map_err(|_| "ASSISTANT_TASK_GATE_CREATE_OUTPUT")?;
    write_json(&output.join("assistant-task-gates.json"), &proposed)?;
    write_json(&output.join("task-gate-change.json"), &audit)?;
    write_json(&output.join("executor-preview.json"), &preview)?;
    Ok(())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut state: Option<PathBuf> = None;
    let mut gates: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut operation: Option<String> = None;
    let mut task_id: Option<String> = None;
    let mut revision: Option<u64> = None;
    let mut target_hash: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => state = args.next().map(PathBuf::from),
            "--gates" => gates = args.next().map(PathBuf::from),
            "--output" => output = args.next().map(PathBuf::from),
            "--operation" => operation = args.next(),
            "--task-id" => task_id = args.next(),
            "--task-revision" => revision = args.next().and_then(|value| value.parse().ok()),
            "--target-hash" => target_hash = args.next(),
            _ => usage(),
        }
    }
    let state = state.unwrap_or_else(|| usage());
    let output = output.unwrap_or_else(|| usage());
    let operation = operation.unwrap_or_else(|| usage());
    let task_id = task_id.unwrap_or_else(|| usage());
    let revision = revision.unwrap_or_else(|| usage());
    let target_hash = target_hash.unwrap_or_else(|| usage());
    if let Err(error) = run(
        &state,
        gates.as_deref(),
        &output,
        &operation,
        &task_id,
        revision,
        &target_hash,
    ) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
