//! Tests locale-independent summaries and agent branch validation.

use pretty_assertions::assert_eq;

use super::branch_ref;
use super::format_shortstat;

#[test]
fn shortstat_matches_git_wording() {
    assert_eq!(format_shortstat(1, 1, 0), "1 file changed, 1 insertion(+)");
    assert_eq!(
        format_shortstat(2, 3, 1),
        "2 files changed, 3 insertions(+), 1 deletion(-)"
    );
    assert_eq!(format_shortstat(1, 0, 0), "1 file changed");
}

#[test]
fn agent_branches_stay_in_their_namespace() {
    assert_eq!(
        branch_ref("codex/agents/probe-01a1748c").expect("valid branch"),
        "refs/heads/codex/agents/probe-01a1748c"
    );
    for invalid in [
        "main",
        "codex/agents/../main",
        "codex/agents/with space",
        "codex/agents/a:b",
    ] {
        assert!(branch_ref(invalid).is_err(), "{invalid} must be rejected");
    }
}
