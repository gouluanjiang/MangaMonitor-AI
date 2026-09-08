use cloud_monitor::{local_executor::LocalExecutionPlan, source_bridge_request};
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let index = args
        .iter()
        .position(|value| value == "--plan")
        .ok_or("missing --plan")?;
    let path = PathBuf::from(args.get(index + 1).ok_or("missing value for --plan")?);
    let plan: LocalExecutionPlan = serde_json::from_slice(
        &fs::read(path).map_err(|_| "PLAN_READ_FAILED".to_string())?,
    )
    .map_err(|_| "PLAN_PARSE_FAILED".to_string())?;
    let request = source_bridge_request::build(&plan)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&request).map_err(|_| "OUTPUT_SERIALIZE_FAILED".to_string())?
    );
    Ok(())
}
