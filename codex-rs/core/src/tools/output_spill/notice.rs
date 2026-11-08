//! Fork: the line that tells the model where the full output of a truncated tool result is
//! and how to read it (`specs/tool-output-spill-2026-11-07.md`, use case 4, Data `spill_notice`;
//! reading guidance from `specs/fork-tool-guidance-2026-11-08.md` UC4, R4.1).

use super::store::SpillRecord;

/// Average bytes per line of a saved file above which line ranges no longer help, such as
/// compact JSON from an MCP `structuredContent` or a minified file: the notice then names
/// byte- and match-oriented reads (`specs/fork-tool-guidance-2026-11-08.md` UC4, R4.1).
const LONG_LINE_BYTES: u64 = 2048;

/// `spill_notice`, with `spill_notice_when_capped` after the token count when `max_file_bytes`
/// cut the middle of the file (R3.3, R4.2). `reader` is how the model calls `exec_command`:
/// `exec_command`, or `tools.exec_command` from a Code Mode cell (R7.4).
///
/// `specs/fork-tool-guidance-2026-11-08.md` UC4, step "read a spilled file in parts" (R4.1):
/// the notice tells the model to read only the part it needs, at most the size of the preview,
/// because a larger read is cut again; a file of very long lines gets byte- and match-oriented
/// reads instead of line ranges.
pub(crate) fn spill_notice(record: &SpillRecord, reader: &str) -> String {
    let SpillRecord {
        path,
        lines,
        bytes,
        tokens,
        capped,
    } = record;
    let capped = capped
        .map(|capped| {
            format!(
                "; the file keeps the first and last {} bytes of {}",
                capped.kept_half, capped.original_bytes
            )
        })
        .unwrap_or_default();
    let how_to_read = if has_long_lines(*lines, *bytes) {
        format!(
            "The lines are very long: read only the part you need with {reader}, e.g. jq for JSON, rg -o '.{{0,200}}PATTERN.{{0,200}}' or head -c N; a larger read is cut again."
        )
    } else {
        format!(
            "Read only the part you need with {reader}: rg -n 'PATTERN' to find it, then sed -n 'A,Bp' for a range no larger than the preview above; a larger read is cut again."
        )
    };
    format!(
        "Full output saved to: {} ({lines} lines, {bytes} bytes, ~{tokens} tokens{capped}). {how_to_read}",
        path.display()
    )
}

/// Whether the lines of a saved file are on average longer than [`LONG_LINE_BYTES`].
fn has_long_lines(lines: usize, bytes: u64) -> bool {
    let lines = u64::try_from(lines.max(1)).unwrap_or(u64::MAX);
    bytes / lines > LONG_LINE_BYTES
}
