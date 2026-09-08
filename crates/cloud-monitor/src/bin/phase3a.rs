#[tokio::main]
async fn main() {
    if let Err(e) = cloud_monitor::runner::run(
        std::env::args().collect(),
        cloud_monitor::runner::Profile::Phase3A,
    )
    .await
    {
        eprintln!("PHASE3A_ERROR: {e}");
        std::process::exit(1);
    }
}
