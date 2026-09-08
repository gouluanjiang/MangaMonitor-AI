use cloud_monitor::{assistant_decision, persistence};
use serde_json::Value;
use std::{fs, path::{Path, PathBuf}};

fn usage() -> ! {
    eprintln!(
        "usage: assistant-decision-edit --state <dir> --output <new-dir> --review-id <id> --decision <same|not-same|ignore> [--work-id <id>]"
    );
    std::process::exit(2);
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| "ASSISTANT_DECISION_SERIALIZE")?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|_| format!("ASSISTANT_DECISION_WRITE:{}", path.display()))
}

fn run(
    state_dir: &Path,
    output: &Path,
    review_id: &str,
    decision: &str,
    work_id: Option<&str>,
) -> Result<(), String> {
    if output.exists() {
        return Err("ASSISTANT_DECISION_OUTPUT_EXISTS".into());
    }
    let state = persistence::load(state_dir)?;
    let (decisions, audit, preview) =
        assistant_decision::plan(&state, review_id, decision, work_id)?;
    fs::create_dir_all(output).map_err(|_| "ASSISTANT_DECISION_CREATE_OUTPUT")?;
    write_json(&output.join("decisions.json"), &decisions)?;
    write_json(&output.join("decision-change.json"), &audit)?;
    write_json(&output.join("reanalyze-preview.json"), &preview)?;
    Ok(())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut state: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut review_id: Option<String> = None;
    let mut decision: Option<String> = None;
    let mut work_id: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => state = args.next().map(PathBuf::from),
            "--output" => output = args.next().map(PathBuf::from),
            "--review-id" => review_id = args.next(),
            "--decision" => decision = args.next(),
            "--work-id" => work_id = args.next(),
            _ => usage(),
        }
    }
    let state = state.unwrap_or_else(|| usage());
    let output = output.unwrap_or_else(|| usage());
    let review_id = review_id.unwrap_or_else(|| usage());
    let decision = decision.unwrap_or_else(|| usage());
    if let Err(error) = run(
        &state,
        &output,
        &review_id,
        &decision,
        work_id.as_deref(),
    ) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
