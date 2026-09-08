use cloud_monitor::{
    local_executor::LocalExecutionPlan,
    source_completion::{self, SourceCompletionTranscript},
};
use std::{env, fs, path::PathBuf};

fn value_after(args: &[String], flag: &str) -> Result<String, String> {
    let index = args
        .iter()
        .position(|value| value == flag)
        .ok_or_else(|| format!("missing {flag}"))?;
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let plan_path = PathBuf::from(value_after(&args, "--plan")?);
    let transcript_path = PathBuf::from(value_after(&args, "--transcript")?);

    let plan: LocalExecutionPlan = serde_json::from_slice(
        &fs::read(plan_path).map_err(|_| "PLAN_READ_FAILED".to_string())?,
    )
    .map_err(|_| "PLAN_PARSE_FAILED".to_string())?;
    let transcript: SourceCompletionTranscript = serde_json::from_slice(
        &fs::read(transcript_path).map_err(|_| "TRANSCRIPT_READ_FAILED".to_string())?,
    )
    .map_err(|_| "TRANSCRIPT_PARSE_FAILED".to_string())?;

    let proof = source_completion::normalize(&plan, &transcript)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&proof).map_err(|_| "OUTPUT_SERIALIZE_FAILED".to_string())?
    );
    Ok(())
}
