use super::format_captured_exec_output_for_model;
use crate::exec::ExecCaptureMetadata;
use codex_protocol::exec_output::ExecToolCallOutput;
use codex_protocol::exec_output::StreamOutput;
use codex_utils_output_truncation::OutputTruncation;
use codex_utils_output_truncation::TruncationPolicy;
use pretty_assertions::assert_eq;
use std::time::Duration;

fn exec_output(output: &str) -> ExecToolCallOutput {
    ExecToolCallOutput {
        exit_code: 0,
        stdout: StreamOutput::new(output.to_string()),
        stderr: StreamOutput::new(String::new()),
        aggregated_output: StreamOutput::new(output.to_string()),
        duration: Duration::ZERO,
        timed_out: false,
    }
}

fn test_truncation() -> OutputTruncation {
    OutputTruncation::new(TruncationPolicy::Bytes(10_000), /*max_lines*/ None)
}

#[test]
fn legacy_exec_formatter_reports_exact_capture_omission() {
    let formatted = format_captured_exec_output_for_model(
        &exec_output("retained"),
        Some(&ExecCaptureMetadata {
            observed_bytes: 2_000_000,
            omitted_bytes: 1_000_000,
            estimated_original_token_count: 500_000,
            capture_incomplete: false,
        }),
        test_truncation(),
    );

    assert!(
        formatted
            .starts_with("Capture warning: observed total bytes: 2000000; bytes omitted: 1000000;")
    );
    assert!(formatted.contains("estimated original token count: approximately 500000"));
}

#[test]
fn legacy_exec_formatter_marks_incomplete_capture_as_lower_bound() {
    let formatted = format_captured_exec_output_for_model(
        &exec_output("partial"),
        Some(&ExecCaptureMetadata {
            observed_bytes: 7,
            omitted_bytes: 0,
            estimated_original_token_count: 2,
            capture_incomplete: true,
        }),
        test_truncation(),
    );

    assert!(formatted.contains("observed bytes lower bound: 7"));
    assert!(formatted.contains("observed omitted bytes lower bound: 0"));
    assert!(formatted.contains("full size unknown"));
    assert!(!formatted.contains("observed total bytes"));
}

#[test]
fn legacy_exec_formatter_omits_warning_for_complete_untruncated_capture() {
    let formatted = format_captured_exec_output_for_model(
        &exec_output("retained"),
        Some(&ExecCaptureMetadata {
            observed_bytes: 8,
            omitted_bytes: 0,
            estimated_original_token_count: 2,
            capture_incomplete: false,
        }),
        test_truncation(),
    );

    assert_eq!(
        formatted,
        "Exit code: 0\nWall time: 0 seconds\nOutput:\nretained"
    );
}

#[test]
fn legacy_exec_formatter_keeps_warning_outside_presentation_truncation() {
    let output = "0123456789".repeat(100);
    let formatted = format_captured_exec_output_for_model(
        &exec_output(&output),
        Some(&ExecCaptureMetadata {
            observed_bytes: 2_000,
            omitted_bytes: 1_000,
            estimated_original_token_count: 500,
            capture_incomplete: false,
        }),
        OutputTruncation::new(TruncationPolicy::Bytes(16), /*max_lines*/ None),
    );

    assert!(
        formatted.starts_with("Capture warning: observed total bytes: 2000; bytes omitted: 1000;")
    );
    assert!(!formatted.contains(&output));
}
