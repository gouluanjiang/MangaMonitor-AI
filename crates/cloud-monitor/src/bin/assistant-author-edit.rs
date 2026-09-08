use cloud_monitor::assistant_author;
use serde_json::Value;
use std::{fs, path::{Path, PathBuf}};

fn usage() -> ! {
    eprintln!(
        "usage: assistant-author-edit --input <authors.json> --output <new-dir> --operation <add|enable|disable> --name <author>"
    );
    std::process::exit(2);
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| "ASSISTANT_AUTHOR_SERIALIZE")?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|_| format!("ASSISTANT_AUTHOR_WRITE:{}", path.display()))
}

fn run(input: &Path, output: &Path, operation: &str, name: &str) -> Result<(), String> {
    if output.exists() {
        return Err("ASSISTANT_AUTHOR_OUTPUT_EXISTS".into());
    }
    let bytes = fs::read(input).map_err(|_| "ASSISTANT_AUTHOR_READ_INPUT")?;
    let document: Value = serde_json::from_slice(&bytes).map_err(|_| "ASSISTANT_AUTHOR_INVALID_JSON")?;
    let (proposed, audit) = assistant_author::plan(&document, operation, name)?;
    fs::create_dir_all(output).map_err(|_| "ASSISTANT_AUTHOR_CREATE_OUTPUT")?;
    write_json(&output.join("authors.json"), &proposed)?;
    write_json(&output.join("author-change.json"), &audit)?;
    Ok(())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut operation: Option<String> = None;
    let mut name: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input = args.next().map(PathBuf::from),
            "--output" => output = args.next().map(PathBuf::from),
            "--operation" => operation = args.next(),
            "--name" => name = args.next(),
            _ => usage(),
        }
    }
    let input = input.unwrap_or_else(|| usage());
    let output = output.unwrap_or_else(|| usage());
    let operation = operation.unwrap_or_else(|| usage());
    let name = name.unwrap_or_else(|| usage());
    if let Err(error) = run(&input, &output, &operation, &name) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
