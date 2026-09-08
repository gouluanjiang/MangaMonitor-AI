use cloud_monitor::{
    local_inventory_apply,
    local_inventory_apply_authorization::InventoryApplyAuthorization,
    local_inventory_rescan::LocalInventoryRescanReport,
    local_inventory_update_candidate::InventoryUpdateCandidate,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{env, fs, path::PathBuf};

fn usage() -> ! {
    eprintln!(
        "usage: mangamonitor-local-inventory-apply --state <state-dir> --rescan-report <rescan.json> --candidate <candidate.json> --authorization <authorization.json>\nV1.7 local-only add-only inventory apply. It writes only inventory_index.json and never completes a task."
    );
    std::process::exit(2);
}

fn github_actions_runtime_forbidden(value: Option<&str>) -> bool {
    value == Some("true")
}

fn read_exact<T>(
    path: PathBuf,
    read_error: &'static str,
    json_error: &'static str,
    shape_error: &'static str,
) -> Result<T, String>
where
    T: DeserializeOwned + Serialize,
{
    let bytes = fs::read(path).map_err(|_| read_error.to_string())?;
    let raw: Value = serde_json::from_slice(&bytes).map_err(|_| json_error.to_string())?;
    let parsed: T = serde_json::from_value(raw.clone()).map_err(|_| json_error.to_string())?;
    let normalized = serde_json::to_value(&parsed).map_err(|_| shape_error.to_string())?;
    if normalized != raw {
        return Err(shape_error.into());
    }
    Ok(parsed)
}

fn main() {
    // This refusal intentionally precedes argument parsing and every filesystem
    // read so a GitHub Actions runtime cannot be pointed at real local state.
    if github_actions_runtime_forbidden(env::var("GITHUB_ACTIONS").ok().as_deref()) {
        eprintln!("INVENTORY_V1_7_GITHUB_ACTIONS_FORBIDDEN");
        std::process::exit(77);
    }
    if !cfg!(windows) {
        eprintln!("INVENTORY_V1_7_WINDOWS_ONLY");
        std::process::exit(78);
    }

    let mut args = env::args().skip(1);
    let mut state_dir = None;
    let mut rescan_report = None;
    let mut candidate_file = None;
    let mut authorization_file = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => state_dir = args.next().map(PathBuf::from),
            "--rescan-report" => rescan_report = args.next().map(PathBuf::from),
            "--candidate" => candidate_file = args.next().map(PathBuf::from),
            "--authorization" => authorization_file = args.next().map(PathBuf::from),
            _ => usage(),
        }
    }
    let state_dir = state_dir.unwrap_or_else(|| usage());
    let rescan_report = rescan_report.unwrap_or_else(|| usage());
    let candidate_file = candidate_file.unwrap_or_else(|| usage());
    let authorization_file = authorization_file.unwrap_or_else(|| usage());

    let result: Result<String, String> = (|| {
        let report: LocalInventoryRescanReport = read_exact(
            rescan_report,
            "INVENTORY_V1_7_RESCAN_REPORT_READ",
            "INVENTORY_V1_7_RESCAN_REPORT_JSON",
            "INVENTORY_V1_7_RESCAN_REPORT_SHAPE",
        )?;
        let candidate: InventoryUpdateCandidate = read_exact(
            candidate_file,
            "INVENTORY_V1_7_CANDIDATE_READ",
            "INVENTORY_V1_7_CANDIDATE_JSON",
            "INVENTORY_V1_7_CANDIDATE_SHAPE",
        )?;
        let authorization: InventoryApplyAuthorization = read_exact(
            authorization_file,
            "INVENTORY_V1_7_AUTHORIZATION_READ",
            "INVENTORY_V1_7_AUTHORIZATION_JSON",
            "INVENTORY_V1_7_AUTHORIZATION_SHAPE",
        )?;
        let receipt =
            local_inventory_apply::apply(&state_dir, &report, &candidate, &authorization)?;
        serde_json::to_string_pretty(&receipt)
            .map_err(|_| "INVENTORY_V1_7_RECEIPT_SERIALIZE".to_string())
    })();

    match result {
        Ok(output) => println!("{output}"),
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
