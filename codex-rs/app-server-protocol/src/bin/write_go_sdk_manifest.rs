use std::fs;
use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    output: PathBuf,

    #[arg(long)]
    check: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let manifest = codex_app_server_protocol::go_manifest::go_sdk_manifest();
    let json = codex_app_server_protocol::go_manifest::canonical_pretty_manifest_json(&manifest)?;
    if args.check {
        let existing = fs::read_to_string(&args.output)?;
        let existing =
            codex_app_server_protocol::go_manifest::canonical_manifest_json_from_str(&existing)?;
        let generated =
            codex_app_server_protocol::go_manifest::canonical_manifest_json_from_str(&json)?;
        anyhow::ensure!(
            existing == generated,
            "Go SDK manifest drift: {}",
            args.output.display()
        );
        return Ok(());
    }
    if let Some(parent) = args.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.output, json)?;
    Ok(())
}
