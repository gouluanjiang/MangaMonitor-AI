use cloud_monitor::{
    local_executor::LocalExecutionPlan,
    source_bridge_request::SourceBridgeRequest,
    source_preflight::{self, SourcePreflightEvidence},
};
use std::{env, fs, path::PathBuf};

fn arg_path(args: &[String], name: &str) -> Result<PathBuf, String> {
    let index = args
        .iter()
        .position(|value| value == name)
        .ok_or_else(|| format!("missing {name}"))?;
    Ok(PathBuf::from(
        args.get(index + 1)
            .ok_or_else(|| format!("missing value for {name}"))?,
    ))
}

fn read_json<T: serde::de::DeserializeOwned>(path: PathBuf, label: &str) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|_| format!("{label}_READ_FAILED"))?;
    serde_json::from_slice(&bytes).map_err(|_| format!("{label}_PARSE_FAILED"))
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let plan: LocalExecutionPlan = read_json(arg_path(&args, "--plan")?, "PLAN")?;
    let request: SourceBridgeRequest = read_json(arg_path(&args, "--request")?, "REQUEST")?;
    let evidence: SourcePreflightEvidence =
        read_json(arg_path(&args, "--evidence")?, "EVIDENCE")?;
    let proof = source_preflight::validate(&plan, &request, &evidence)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&proof).map_err(|_| "OUTPUT_SERIALIZE_FAILED".to_string())?
    );
    Ok(())
}
