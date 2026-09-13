//! Local maintenance entry point; the same reconciliation service used by the UI.
//! Reads an explicit metadata input and writes a private report. Never downloads media.
use serde::Deserialize;
use std::{fs, io::Read, path::PathBuf};
use workbench_library::LibraryService;
use workbench_storage::{LibraryMatchWork, WorkbenchStore};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    app_data_root: PathBuf,
    root_id: String,
    generation: u64,
    revision: u64,
    works: Vec<LibraryMatchWork>,
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 || (args[2] != "--preview" && args[2] != "--apply") {
        return Err("expected INPUT REPORT --preview|--apply".into());
    }
    let input_file = fs::File::open(&args[0])?;
    let length = input_file.metadata()?.len();
    if length > 32 * 1024 * 1024 {
        return Err("INPUT_TOO_LARGE".into());
    }
    let mut bytes = Vec::new();
    input_file.take(length + 1).read_to_end(&mut bytes)?;
    let input: Input = serde_json::from_slice(&bytes)?;
    let store = WorkbenchStore::open(input.app_data_root)?;
    if store.read_library()?.revision != input.revision {
        return Err("LIBRARY_STALE_SNAPSHOT".into());
    }
    // Reserve a new report before applying; no prior audit is overwritten.
    let mut report = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    let result = LibraryService::new().reconcile(
        &store,
        &input.root_id,
        input.generation,
        &input.works,
        args[2] == "--apply",
    )?;
    serde_json::to_writer(&mut report, &result)?;
    report.sync_all()?;
    println!("examined={} linked={}", result.examined, result.linked);
    Ok(())
}
fn main() {
    if run().is_err() {
        eprintln!("LIBRARY_RECONCILE_FAILED");
        std::process::exit(1);
    }
}
