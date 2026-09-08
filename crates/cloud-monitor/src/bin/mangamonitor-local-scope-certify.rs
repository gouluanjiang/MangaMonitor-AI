use cloud_monitor::scope_certificates::{self, CompletenessAttestation};
use std::{fs, path::PathBuf};

fn usage() -> ! {
    eprintln!(
        "usage: mangamonitor-local-scope-certify --author <name> --authors <json> --inventory <json> --snapshot <json> --attestation <json> --rule-version <version> --output <json>"
    );
    std::process::exit(2);
}

fn main() {
    if std::env::var("GITHUB_ACTIONS").is_ok_and(|value| value == "true") {
        eprintln!("LOCAL_CERTIFIER_FORBIDDEN_IN_GITHUB_ACTIONS");
        std::process::exit(1);
    }

    let mut args = std::env::args().skip(1);
    let mut author: Option<String> = None;
    let mut authors = None;
    let mut inventory = None;
    let mut snapshot = None;
    let mut attestation = None;
    let mut rule_version: Option<String> = None;
    let mut output = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--author" => author = args.next(),
            "--authors" => authors = args.next().map(PathBuf::from),
            "--inventory" => inventory = args.next().map(PathBuf::from),
            "--snapshot" => snapshot = args.next().map(PathBuf::from),
            "--attestation" => attestation = args.next().map(PathBuf::from),
            "--rule-version" => rule_version = args.next(),
            "--output" => output = args.next().map(PathBuf::from),
            _ => usage(),
        }
    }
    let author = author.unwrap_or_else(|| usage());
    let authors_path = authors.unwrap_or_else(|| usage());
    let inventory_path = inventory.unwrap_or_else(|| usage());
    let snapshot_path = snapshot.unwrap_or_else(|| usage());
    let attestation_path = attestation.unwrap_or_else(|| usage());
    let rule_version = rule_version.unwrap_or_else(|| usage());
    let output_path = output.unwrap_or_else(|| usage());

    let result = (|| {
        let authors: serde_json::Value = serde_json::from_slice(
            &fs::read(authors_path).map_err(|_| "LOCAL_CERTIFIER_AUTHORS_READ")?,
        )
        .map_err(|_| "LOCAL_CERTIFIER_AUTHORS_JSON")?;
        let inventory: serde_json::Value = serde_json::from_slice(
            &fs::read(inventory_path).map_err(|_| "LOCAL_CERTIFIER_INVENTORY_READ")?,
        )
        .map_err(|_| "LOCAL_CERTIFIER_INVENTORY_JSON")?;
        let attestation: CompletenessAttestation = serde_json::from_slice(
            &fs::read(attestation_path).map_err(|_| "LOCAL_CERTIFIER_ATTESTATION_READ")?,
        )
        .map_err(|_| "LOCAL_CERTIFIER_ATTESTATION_JSON")?;
        let record = scope_certificates::certify_snapshot_file(
            author.as_str(),
            &authors,
            &inventory,
            &snapshot_path,
            &attestation,
            &rule_version,
        )?;
        let bytes = serde_json::to_vec_pretty(&record).map_err(|_| "LOCAL_CERTIFIER_OUTPUT_JSON")?;
        fs::write(output_path, bytes).map_err(|_| "LOCAL_CERTIFIER_OUTPUT_WRITE")?;
        Ok::<(), &str>(())
    })();
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
