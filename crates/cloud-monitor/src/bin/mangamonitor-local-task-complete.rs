use cloud_monitor::{
    local_inventory_apply::InventoryApplyReceipt,
    local_inventory_apply_authorization::InventoryApplyAuthorization,
    local_inventory_update_candidate::InventoryUpdateCandidate,
    local_library_import_gate::LocalLibraryImportReceipt,
    local_task_completion,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{env, fs, path::PathBuf};

fn usage() -> ! {
    eprintln!(
        "usage: mangamonitor-local-task-complete --state <state-dir> --library-root <library-root> --import-receipt <import.json> --candidate <candidate.json> --authorization <authorization.json> --apply-receipt <apply-receipt.json>\nV1.8 Windows-local-only exact task completion. The only durable write is pending.json for one exact completed task revision."
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
    // Refuse cloud execution before argument parsing or any filesystem read so
    // neither local paths nor user-library contents can be consumed in Actions.
    if github_actions_runtime_forbidden(env::var("GITHUB_ACTIONS").ok().as_deref()) {
        eprintln!("TASK_V1_8_GITHUB_ACTIONS_FORBIDDEN");
        std::process::exit(77);
    }
    if !cfg!(windows) {
        eprintln!("TASK_V1_8_WINDOWS_ONLY");
        std::process::exit(78);
    }

    let mut args = env::args().skip(1);
    let mut state_dir = None;
    let mut library_root = None;
    let mut import_receipt_file = None;
    let mut candidate_file = None;
    let mut authorization_file = None;
    let mut apply_receipt_file = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => state_dir = args.next().map(PathBuf::from),
            "--library-root" => library_root = args.next().map(PathBuf::from),
            "--import-receipt" => import_receipt_file = args.next().map(PathBuf::from),
            "--candidate" => candidate_file = args.next().map(PathBuf::from),
            "--authorization" => authorization_file = args.next().map(PathBuf::from),
            "--apply-receipt" => apply_receipt_file = args.next().map(PathBuf::from),
            _ => usage(),
        }
    }
    let state_dir = state_dir.unwrap_or_else(|| usage());
    let library_root = library_root.unwrap_or_else(|| usage());
    let import_receipt_file = import_receipt_file.unwrap_or_else(|| usage());
    let candidate_file = candidate_file.unwrap_or_else(|| usage());
    let authorization_file = authorization_file.unwrap_or_else(|| usage());
    let apply_receipt_file = apply_receipt_file.unwrap_or_else(|| usage());

    let result: Result<String, String> = (|| {
        let import_receipt: LocalLibraryImportReceipt = read_exact(
            import_receipt_file,
            "TASK_V1_8_IMPORT_RECEIPT_READ",
            "TASK_V1_8_IMPORT_RECEIPT_JSON",
            "TASK_V1_8_IMPORT_RECEIPT_SHAPE",
        )?;
        let candidate: InventoryUpdateCandidate = read_exact(
            candidate_file,
            "TASK_V1_8_CANDIDATE_READ",
            "TASK_V1_8_CANDIDATE_JSON",
            "TASK_V1_8_CANDIDATE_SHAPE",
        )?;
        let authorization: InventoryApplyAuthorization = read_exact(
            authorization_file,
            "TASK_V1_8_AUTHORIZATION_READ",
            "TASK_V1_8_AUTHORIZATION_JSON",
            "TASK_V1_8_AUTHORIZATION_SHAPE",
        )?;
        let apply_receipt: InventoryApplyReceipt = read_exact(
            apply_receipt_file,
            "TASK_V1_8_APPLY_RECEIPT_READ",
            "TASK_V1_8_APPLY_RECEIPT_JSON",
            "TASK_V1_8_APPLY_RECEIPT_SHAPE",
        )?;
        let receipt = local_task_completion::complete(
            &state_dir,
            &library_root,
            &import_receipt,
            &candidate,
            &authorization,
            &apply_receipt,
        )?;
        serde_json::to_string_pretty(&receipt)
            .map_err(|_| "TASK_V1_8_RECEIPT_SERIALIZE".to_string())
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
