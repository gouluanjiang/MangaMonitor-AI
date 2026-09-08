use cloud_monitor::{
    filesystem_verifier,
    local_executor::LocalExecutionPlan,
    staging_manifest::StagingManifest,
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
    let manifest_path = PathBuf::from(value_after(&args, "--manifest")?);
    let staging_root = PathBuf::from(value_after(&args, "--staging-root")?);

    let plan: LocalExecutionPlan = serde_json::from_slice(
        &fs::read(&plan_path).map_err(|_| "PLAN_READ_FAILED".to_string())?,
    )
    .map_err(|_| "PLAN_PARSE_FAILED".to_string())?;
    let manifest: StagingManifest = serde_json::from_slice(
        &fs::read(&manifest_path).map_err(|_| "MANIFEST_READ_FAILED".to_string())?,
    )
    .map_err(|_| "MANIFEST_PARSE_FAILED".to_string())?;

    let result = filesystem_verifier::verify(&staging_root, &plan, &manifest)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&result).map_err(|_| "OUTPUT_SERIALIZE_FAILED".to_string())?
    );
    Ok(())
}
