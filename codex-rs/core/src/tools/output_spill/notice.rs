//! Fork: the line that tells the model where the full output of a truncated tool result is
//! (`specs/tool-output-spill-2026-11-07.md`, use case 4, Data `spill_notice`).

use super::store::SpillRecord;

/// `spill_notice`, with `spill_notice_when_capped` after the token count when `max_file_bytes`
/// cut the middle of the file (R3.3, R4.2).
pub(crate) fn spill_notice(record: &SpillRecord) -> String {
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
    format!(
        "Full output saved to: {} ({lines} lines, {bytes} bytes, ~{tokens} tokens{capped}). Read the omitted part with exec_command, e.g. sed -n 'A,Bp' or rg -n 'PATTERN' on that file.",
        path.display()
    )
}
