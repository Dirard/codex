use anyhow::Context;
use anyhow::Result;
use codex_app_server_protocol::SchemaFixtureOptions;
use codex_app_server_protocol::go_manifest;
use codex_app_server_protocol::write_schema_fixtures_with_options;
use pretty_assertions::assert_eq;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::OnceLock;

#[test]
fn write_schema_fixtures_check_accepts_crlf_equivalent_fixture() -> Result<()> {
    let _guard = fixture_writer_lock()?;
    let temp_dir = tempfile::tempdir().context("create temp dir")?;
    let schema_root = temp_dir.path().join("schema");
    write_schema_fixtures_with_options(&schema_root, None, SchemaFixtureOptions::default())
        .context("seed schema fixture tree")?;

    let typescript_file = schema_root.join("typescript/index.ts");
    let json_file = schema_root.join("json/codex_app_server_protocol.schemas.json");
    rewrite_lf_file_as_crlf(&typescript_file)?;
    rewrite_lf_file_as_crlf(&json_file)?;
    let typescript_before = fs::read(&typescript_file)
        .with_context(|| format!("read {}", typescript_file.display()))?;
    let json_before =
        fs::read(&json_file).with_context(|| format!("read {}", json_file.display()))?;

    let output = write_schema_fixtures_command()?
        .arg("--check")
        .arg("--schema-root")
        .arg(&schema_root)
        .output()
        .context("run write_schema_fixtures --check")?;

    assert!(
        output.status.success(),
        "write_schema_fixtures --check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(&typescript_file)?, typescript_before);
    assert_eq!(fs::read(&json_file)?, json_before);

    Ok(())
}

#[test]
fn write_schema_fixtures_check_rejects_missing_typescript_header() -> Result<()> {
    let _guard = fixture_writer_lock()?;
    let temp_dir = tempfile::tempdir().context("create temp dir")?;
    let schema_root = temp_dir.path().join("schema");
    write_schema_fixtures_with_options(&schema_root, None, SchemaFixtureOptions::default())
        .context("seed schema fixture tree")?;

    let typescript_file = schema_root.join("typescript/index.ts");
    let text = fs::read_to_string(&typescript_file)
        .with_context(|| format!("read {}", typescript_file.display()))?;
    let text = text
        .strip_prefix("// GENERATED CODE! DO NOT MODIFY BY HAND!\n\n")
        .context("fixture should start with generated header")?;
    fs::write(&typescript_file, text)
        .with_context(|| format!("write {}", typescript_file.display()))?;

    let output = write_schema_fixtures_command()?
        .arg("--check")
        .arg("--schema-root")
        .arg(&schema_root)
        .output()
        .context("run write_schema_fixtures --check")?;

    assert!(
        !output.status.success(),
        "write_schema_fixtures --check unexpectedly accepted missing TypeScript header"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("schema fixture drift"),
        "stderr did not explain drift:\n{stderr}"
    );

    Ok(())
}

#[test]
fn write_schema_fixtures_check_rejects_changed_content() -> Result<()> {
    let _guard = fixture_writer_lock()?;
    let temp_dir = tempfile::tempdir().context("create temp dir")?;
    let schema_root = temp_dir.path().join("schema");
    write_schema_fixtures_with_options(&schema_root, None, SchemaFixtureOptions::default())
        .context("seed schema fixture tree")?;

    let typescript_file = schema_root.join("typescript/index.ts");
    let mut text = fs::read_to_string(&typescript_file)
        .with_context(|| format!("read {}", typescript_file.display()))?;
    text.push_str("\nexport type DriftSentinel = string;\n");
    fs::write(&typescript_file, text)
        .with_context(|| format!("write {}", typescript_file.display()))?;

    let output = write_schema_fixtures_command()?
        .arg("--check")
        .arg("--schema-root")
        .arg(&schema_root)
        .output()
        .context("run write_schema_fixtures --check")?;

    assert!(
        !output.status.success(),
        "write_schema_fixtures --check unexpectedly succeeded"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("schema fixture drift"),
        "stderr did not explain drift:\n{stderr}"
    );

    Ok(())
}

#[test]
fn write_go_sdk_manifest_check_accepts_crlf_equivalent_manifest() -> Result<()> {
    let _guard = fixture_writer_lock()?;
    let temp_dir = tempfile::tempdir().context("create temp dir")?;
    let manifest_path = temp_dir.path().join("app_server_protocol_manifest.json");
    fs::write(
        &manifest_path,
        go_manifest::canonical_pretty_manifest_json(&go_manifest::go_sdk_manifest())?,
    )
    .with_context(|| format!("write {}", manifest_path.display()))?;
    rewrite_lf_file_as_crlf(&manifest_path)?;

    let output = write_go_sdk_manifest_command()?
        .arg("--check")
        .arg("--output")
        .arg(&manifest_path)
        .output()
        .context("run write_go_sdk_manifest --check")?;

    assert!(
        output.status.success(),
        "write_go_sdk_manifest --check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}

#[test]
fn write_go_sdk_manifest_check_rejects_changed_content() -> Result<()> {
    let _guard = fixture_writer_lock()?;
    let temp_dir = tempfile::tempdir().context("create temp dir")?;
    let manifest_path = temp_dir.path().join("app_server_protocol_manifest.json");
    let mut json = go_manifest::canonical_pretty_manifest_json(&go_manifest::go_sdk_manifest())?;
    json = json.replacen(
        "\"manifestSchemaVersion\": 1",
        "\"manifestSchemaVersion\": 2",
        1,
    );
    fs::write(&manifest_path, json)
        .with_context(|| format!("write {}", manifest_path.display()))?;

    let output = write_go_sdk_manifest_command()?
        .arg("--check")
        .arg("--output")
        .arg(&manifest_path)
        .output()
        .context("run write_go_sdk_manifest --check")?;

    assert!(
        !output.status.success(),
        "write_go_sdk_manifest --check unexpectedly succeeded"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Go SDK manifest drift"),
        "stderr did not explain manifest drift:\n{stderr}"
    );

    Ok(())
}

fn fixture_writer_lock() -> Result<MutexGuard<'static, ()>> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|err| anyhow::anyhow!("fixture writer lock poisoned: {err}"))
}

fn write_schema_fixtures_command() -> Result<Command> {
    Ok(Command::new(codex_utils_cargo_bin::cargo_bin(
        "write_schema_fixtures",
    )?))
}

fn write_go_sdk_manifest_command() -> Result<Command> {
    Ok(Command::new(codex_utils_cargo_bin::cargo_bin(
        "write_go_sdk_manifest",
    )?))
}

fn rewrite_lf_file_as_crlf(path: &Path) -> Result<()> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    fs::write(path, text.replace('\n', "\r\n"))
        .with_context(|| format!("rewrite {} with CRLF", path.display()))
}
