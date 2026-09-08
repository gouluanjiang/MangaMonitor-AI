use cloud_monitor::{assistant, persistence};
use std::path::PathBuf;

fn usage() -> ! {
    eprintln!(
        "usage: assistant-view --state <dir> --view <scan-summary|review-summary|review-batch|pending|collection> [--offset N] [--limit N]"
    );
    std::process::exit(2);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut state_dir: Option<PathBuf> = None;
    let mut view: Option<String> = None;
    let mut offset = 0usize;
    let mut limit = 25usize;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => state_dir = args.next().map(PathBuf::from),
            "--view" => view = args.next(),
            "--offset" => {
                offset = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_else(|| usage())
            }
            "--limit" => {
                limit = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_else(|| usage())
            }
            _ => usage(),
        }
    }

    let state_dir = state_dir.unwrap_or_else(|| usage());
    let view = view.unwrap_or_else(|| usage());
    let state = persistence::load(&state_dir).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    let value = match view.as_str() {
        "scan-summary" => Ok(assistant::scan_summary(&state)),
        "review-summary" => Ok(assistant::review_backlog_summary(&state)),
        "review-batch" => assistant::review_batch(&state, offset, limit),
        "pending" => Ok(assistant::pending_task_summary(&state)),
        "collection" => assistant::collection_summary(&state),
        _ => {
            usage();
        }
    }
    .unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    println!("{}", serde_json::to_string_pretty(&value).unwrap());
}
