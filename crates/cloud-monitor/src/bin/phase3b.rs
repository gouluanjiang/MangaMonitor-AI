//! Phase 3B staged production entry point. It never mutates its input directory.
#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--resume-preflight") {
        println!("{}", serde_json::to_string(&cloud_monitor::runner::resume_preflight(&args)).expect("preflight serializes"));
        return;
    }
    if let Err(error) = cloud_monitor::runner::run(
        args,
        cloud_monitor::runner::Profile::Phase3B,
    )
    .await
    {
        eprintln!("PHASE3B_ERROR: {error}");
        std::process::exit(1);
    }
}
