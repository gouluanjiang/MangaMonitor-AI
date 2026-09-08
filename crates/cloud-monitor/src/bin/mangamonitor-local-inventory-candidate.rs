use cloud_monitor::{
    local_inventory_rescan::LocalInventoryRescanReport,
    local_inventory_update_candidate,
    persistence,
};
use std::{env, fs, path::PathBuf};

fn usage() -> ! {
    eprintln!(
        "usage: mangamonitor-local-inventory-candidate --state <state-dir> --rescan-report <rescan.json>\nRead-only V1.5 candidate builder. It never writes monitor-state or inventory."
    );
    std::process::exit(2);
}

fn github_actions_runtime_forbidden(value: Option<&str>) -> bool {
    value == Some("true")
}

fn main() {
    if github_actions_runtime_forbidden(env::var("GITHUB_ACTIONS").ok().as_deref()) {
        eprintln!("INVENTORY_V1_5_GITHUB_ACTIONS_FORBIDDEN");
        std::process::exit(77);
    }
    if !cfg!(windows) {
        eprintln!("INVENTORY_V1_5_WINDOWS_ONLY");
        std::process::exit(78);
    }

    let mut args = env::args().skip(1);
    let mut state_dir: Option<PathBuf> = None;
    let mut rescan_report: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => state_dir = args.next().map(PathBuf::from),
            "--rescan-report" => rescan_report = args.next().map(PathBuf::from),
            _ => usage(),
        }
    }
    let state_dir = state_dir.unwrap_or_else(|| usage());
    let rescan_report = rescan_report.unwrap_or_else(|| usage());

    let result = (|| {
        let state = persistence::load(&state_dir)?;
        let report: LocalInventoryRescanReport = serde_json::from_slice(
            &fs::read(rescan_report).map_err(|_| "INVENTORY_V1_5_RESCAN_REPORT_READ")?,
        )
        .map_err(|_| "INVENTORY_V1_5_RESCAN_REPORT_JSON")?;
        let candidate = local_inventory_update_candidate::build(&state, &report)?;
        serde_json::to_string_pretty(&candidate)
            .map_err(|_| "INVENTORY_V1_5_CANDIDATE_SERIALIZE".to_string())
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
