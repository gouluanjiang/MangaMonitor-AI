use cloud_monitor::assistant_publication;
use std::path::PathBuf;

fn usage() -> ! {
    eprintln!(
        "usage: assistant-publication-check --state <monitor-state> --staging <dir> --kind <author|decision|task-gate>"
    );
    std::process::exit(2);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut state: Option<PathBuf> = None;
    let mut staging: Option<PathBuf> = None;
    let mut kind: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => state = args.next().map(PathBuf::from),
            "--staging" => staging = args.next().map(PathBuf::from),
            "--kind" => kind = args.next(),
            _ => usage(),
        }
    }
    let state = state.unwrap_or_else(|| usage());
    let staging = staging.unwrap_or_else(|| usage());
    let kind = kind.unwrap_or_else(|| usage());
    match assistant_publication::check_json(&state, &staging, &kind) {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
