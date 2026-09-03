use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
struct Args {
    output: PathBuf,
    experimental_schema_output: Option<PathBuf>,
    check: bool,
}

fn main() -> anyhow::Result<()> {
    let mut output = None;
    let mut experimental_schema_output = None;
    let mut check = false;
    let mut cli_args = env::args_os().skip(1);
    while let Some(arg) = cli_args.next() {
        match arg.to_str() {
            Some("--output") => {
                output =
                    Some(PathBuf::from(cli_args.next().ok_or_else(|| {
                        anyhow::anyhow!("--output requires a path")
                    })?));
            }
            Some("--check") => check = true,
            Some("--experimental-schema-output") => {
                experimental_schema_output =
                    Some(PathBuf::from(cli_args.next().ok_or_else(|| {
                        anyhow::anyhow!("--experimental-schema-output requires a path")
                    })?));
            }
            _ => anyhow::bail!("unexpected argument: {}", arg.to_string_lossy()),
        }
    }
    let args = Args {
        output: output.ok_or_else(|| anyhow::anyhow!("--output is required"))?,
        experimental_schema_output,
        check,
    };
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
    if let Some(schema_output) = args.experimental_schema_output {
        if schema_output.exists() {
            fs::remove_dir_all(&schema_output)?;
        }
        codex_app_server_protocol::generate_json_with_experimental(
            &schema_output.join("json"),
            /*experimental_api*/ true,
        )?;
    }
    if let Some(parent) = args.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.output, json)?;
    Ok(())
}
