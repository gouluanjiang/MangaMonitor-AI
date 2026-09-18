//! Local-only one-time reviewed-library import, separate from the desktop's
//! download queue and from any automatic matching or scheduled work.
use std::{env, path::PathBuf};
use workbench_library::{import_reviewed_library, preview_reviewed_library};
use workbench_storage::{library_path_mapping_bytes, WorkbenchStore};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let mut app_data = None;
    let mut manifest = None;
    let mut revision = None;
    let mut digest = None;
    let mut apply = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--app-data") if app_data.is_none() => {
                app_data = Some(PathBuf::from(args.next().ok_or("MISSING_ARGUMENT")?))
            }
            Some("--manifest") if manifest.is_none() => {
                manifest = Some(PathBuf::from(args.next().ok_or("MISSING_ARGUMENT")?))
            }
            Some("--expected-revision") if revision.is_none() => {
                revision = Some(
                    args.next()
                        .ok_or("MISSING_ARGUMENT")?
                        .to_str()
                        .ok_or("INVALID_ARGUMENT")?
                        .parse::<u64>()?,
                )
            }
            Some("--manifest-sha256") if digest.is_none() => {
                digest = Some(
                    args.next()
                        .ok_or("MISSING_ARGUMENT")?
                        .into_string()
                        .map_err(|_| "INVALID_ARGUMENT")?,
                )
            }
            Some("--apply") if !apply => apply = true,
            Some("--help") => {
                println!("Preview: --app-data DIR --manifest JSON\nApply locally: add --apply --expected-revision N --manifest-sha256 SHA256\nOnly explicitly reviewed references are imported; manga and download history are never modified.");
                return Ok(());
            }
            _ => return Err("INVALID_ARGUMENT".into()),
        }
    }
    // CI exercises synthetic library API tests, never this real-store write path.
    if apply && (env::var_os("GITHUB_ACTIONS").is_some() || env::var_os("CI").is_some()) {
        return Err("LOCAL_REVIEW_IMPORT_REQUIRED".into());
    }
    let app_data = app_data.ok_or("APP_DATA_REQUIRED")?;
    if !app_data.is_absolute()
        || !app_data
            .join(workbench_storage::PRIVATE_DIRECTORY)
            .join("library.json")
            .is_file()
    {
        return Err("EXISTING_LIBRARY_STORE_REQUIRED".into());
    }
    let path = manifest.ok_or("MANIFEST_REQUIRED")?;
    let bytes = library_path_mapping_bytes(&path)?;
    let store = WorkbenchStore::open(app_data)?;
    let report = if apply {
        import_reviewed_library(
            &store,
            &bytes,
            revision.ok_or("EXPECTED_REVISION_REQUIRED")?,
            digest.as_deref().ok_or("MANIFEST_SHA256_REQUIRED")?,
        )?
    } else {
        if revision.is_some() || digest.is_some() {
            return Err("APPLY_REQUIRED".into());
        }
        preview_reviewed_library(&store, &bytes)?
    };
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}

fn main() {
    if let Err(problem) = run() {
        eprintln!("{problem}");
        std::process::exit(1);
    }
}
