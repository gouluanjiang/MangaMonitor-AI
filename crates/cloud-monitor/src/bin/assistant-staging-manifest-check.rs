use cloud_monitor::{local_executor::LocalExecutionPlan, staging_manifest};
use std::{fs, path::PathBuf};

fn usage() -> ! {
    eprintln!("usage: assistant-staging-manifest-check --plan <local-plan.json> --manifest <staging-manifest.json>");
    std::process::exit(2);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut plan_path: Option<PathBuf> = None;
    let mut manifest_path: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--plan" => plan_path = args.next().map(PathBuf::from),
            "--manifest" => manifest_path = args.next().map(PathBuf::from),
            _ => usage(),
        }
    }
    let plan_path = plan_path.unwrap_or_else(|| usage());
    let manifest_path = manifest_path.unwrap_or_else(|| usage());
    let result = (|| -> Result<_, String> {
        let plan: LocalExecutionPlan = serde_json::from_slice(
            &fs::read(plan_path).map_err(|_| "STAGING_PLAN_READ")?,
        )
        .map_err(|_| "STAGING_PLAN_JSON")?;
        let manifest: staging_manifest::StagingManifest = serde_json::from_slice(
            &fs::read(manifest_path).map_err(|_| "STAGING_MANIFEST_READ")?,
        )
        .map_err(|_| "STAGING_MANIFEST_JSON")?;
        staging_manifest::validate(&plan, &manifest)
    })();
    match result {
        Ok(value) => println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("staging validation serializes")
        ),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
