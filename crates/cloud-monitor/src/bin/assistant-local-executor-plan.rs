use cloud_monitor::{executor_handoff::ExecutorCommand, local_executor};
use std::{fs, path::PathBuf};

fn usage() -> ! {
    eprintln!("usage: assistant-local-executor-plan --command <executor-command.json>");
    std::process::exit(2);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut command_path: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--command" => command_path = args.next().map(PathBuf::from),
            _ => usage(),
        }
    }
    let command_path = command_path.unwrap_or_else(|| usage());
    let result = (|| -> Result<_, String> {
        let bytes = fs::read(command_path).map_err(|_| "LOCAL_EXECUTOR_COMMAND_READ")?;
        let command: ExecutorCommand =
            serde_json::from_slice(&bytes).map_err(|_| "LOCAL_EXECUTOR_COMMAND_JSON")?;
        local_executor::plan(&command)
    })();
    match result {
        Ok(plan) => println!(
            "{}",
            serde_json::to_string_pretty(&plan).expect("local executor plan serializes")
        ),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
