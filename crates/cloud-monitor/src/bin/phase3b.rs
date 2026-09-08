//! Phase 3B staged production entry point. It never mutates its input directory.
#[tokio::main]
async fn main() {
    if let Err(error) = cloud_monitor::runner::run(
        std::env::args().collect(),
        cloud_monitor::runner::Profile::Phase3B,
    )
    .await
    {
        eprintln!("PHASE3B_ERROR: {error}");
        std::process::exit(1);
    }
}
