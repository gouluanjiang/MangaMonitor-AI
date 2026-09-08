use cloud_monitor::{
    local_inventory_rescan,
    local_library_import_gate::LocalLibraryImportReceipt,
};
use std::{env, fs, path::PathBuf};

fn usage() -> ! {
    eprintln!(
        "usage: mangamonitor-local-rescan --receipt <local-import-receipt.json> --library-root <existing-dir>\nRead-only V1.4 rescan. It never mutates monitor-state or inventory."
    );
    std::process::exit(2);
}

fn github_actions_runtime_forbidden(value: Option<&str>) -> bool {
    value == Some("true")
}

fn main() {
    if github_actions_runtime_forbidden(env::var("GITHUB_ACTIONS").ok().as_deref()) {
        eprintln!("LOCAL_RESCAN_GITHUB_ACTIONS_FORBIDDEN");
        std::process::exit(77);
    }
    if !cfg!(windows) {
        eprintln!("LOCAL_RESCAN_V1_4_WINDOWS_ONLY");
        std::process::exit(78);
    }

    let mut args = env::args().skip(1);
    let mut receipt: Option<PathBuf> = None;
    let mut library_root: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--receipt" => receipt = args.next().map(PathBuf::from),
            "--library-root" => library_root = args.next().map(PathBuf::from),
            _ => usage(),
        }
    }
    let receipt_path = receipt.unwrap_or_else(|| usage());
    let library_root = library_root.unwrap_or_else(|| usage());

    let result = (|| {
        let receipt: LocalLibraryImportReceipt = serde_json::from_slice(
            &fs::read(receipt_path).map_err(|_| "LOCAL_RESCAN_RECEIPT_READ")?,
        )
        .map_err(|_| "LOCAL_RESCAN_RECEIPT_JSON")?;
        let report = local_inventory_rescan::rescan(&library_root, &receipt)?;
        serde_json::to_string_pretty(&report).map_err(|_| "LOCAL_RESCAN_REPORT_SERIALIZE".to_string())
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
