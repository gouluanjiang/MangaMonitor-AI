use base64::{engine::general_purpose::STANDARD, Engine};
use cloud_monitor::scope_certificates::{self, ScopeCertificateDocument};
use std::path::PathBuf;

fn usage() -> ! {
    eprintln!("usage: assistant-scope-certificate-stage --state <monitor-state> --output <dir> --candidate-b64 <base64-json>");
    std::process::exit(2);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut state = None;
    let mut output = None;
    let mut candidate = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => state = args.next().map(PathBuf::from),
            "--output" => output = args.next().map(PathBuf::from),
            "--candidate-b64" => candidate = args.next(),
            _ => usage(),
        }
    }
    let state = state.unwrap_or_else(|| usage());
    let output = output.unwrap_or_else(|| usage());
    let candidate = candidate.unwrap_or_else(|| usage());
    let result = (|| {
        let bytes = STANDARD
            .decode(candidate)
            .map_err(|_| "SCOPE_CERTIFICATE_CANDIDATE_B64")?;
        let document: ScopeCertificateDocument =
            serde_json::from_slice(&bytes).map_err(|_| "SCOPE_CERTIFICATE_CANDIDATE_JSON")?;
        scope_certificates::stage_candidate(&state, &output, document)
    })();
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
