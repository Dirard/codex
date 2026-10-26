#!/usr/bin/env bash
# Self-test for scripts/fork/upstream-drift.sh.
#
#   scripts/fork/test-upstream-drift.sh
#
# Builds a throwaway repository with an upstream line and a fork branch, with
# fixed commit dates, and checks the report and exit status of the drift
# script for a current fork, a fork behind for a few days, a fork behind past
# the limit, and a fork whose change conflicts with upstream.

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
drift="${repo_root}/scripts/fork/upstream-drift.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/test-upstream-drift.XXXXXX")"
trap 'rm -rf "${work}"' EXIT

passed=0
failures=0

day=86400
t0=1760000000

git_at() {
  local when="$1"
  shift
  GIT_AUTHOR_DATE="@${when} +0000" GIT_COMMITTER_DATE="@${when} +0000" git "$@"
}

repo="${work}/repo"
git init -q -b main "${repo}"
cd "${repo}"
git config user.email drift@example.invalid
git config user.name drift
git config commit.gpgsign false

printf 'one\n' >shared.txt
printf 'base\n' >upstream.txt
git add shared.txt upstream.txt
git_at "${t0}" commit -q -m base
git branch fork

git checkout -q fork
printf 'fork\n' >fork.txt
git add fork.txt
git_at $((t0 + day)) commit -q -m 'fork change'
git checkout -q main

# check NAME WANT_STATUS PATTERN NOW ARGS...: runs the drift script at time NOW
# and expects the exit status and an output line matching PATTERN.
check() {
  local name="$1" want="$2" pattern="$3" now="$4"
  shift 4
  local status=0
  UPSTREAM_DRIFT_NOW="${now}" "${drift}" "$@" >"${work}/${name}.log" 2>&1 || status=$?
  if [[ ${status} -eq ${want} ]] && grep -qE -- "${pattern}" "${work}/${name}.log"; then
    echo "ok   ${name}"
    passed=$((passed + 1))
  else
    echo "FAIL ${name}: status ${status}, want ${want} with /${pattern}/" >&2
    sed 's/^/    /' "${work}/${name}.log" >&2
    failures=$((failures + 1))
  fi
}

check current 0 '^behind:    0 commits$' $((t0 + 30 * day)) fork main
check current-no-conflicts 0 '^conflicts: 0 files$' $((t0 + 30 * day)) fork main

printf 'two\n' >upstream.txt
git add upstream.txt
git_at $((t0 + 2 * day)) commit -q -m 'upstream change'

check behind-recent 0 '^behind:    1 commits, since .* \(3 days\)$' $((t0 + 5 * day)) fork main
check behind-due 3 '^rebase due: behind for 10 days, limit 7$' $((t0 + 12 * day)) fork main
check behind-custom-limit 0 '^behind:    1 commits' $((t0 + 12 * day)) --max-age-days 30 fork main

git checkout -q fork
printf 'fork side\n' >shared.txt
git add shared.txt
git_at $((t0 + 3 * day)) commit -q -m 'fork edits shared'
git checkout -q main
printf 'upstream side\n' >shared.txt
git add shared.txt
git_at $((t0 + 4 * day)) commit -q -m 'upstream edits shared'

check conflict-listed 0 '^  shared.txt$' $((t0 + 5 * day)) fork main
check conflict-counted 0 '^conflicts: 1 files$' $((t0 + 5 * day)) fork main

summary="${work}/summary.md"
check summary-written 3 'rebase due' $((t0 + 20 * day)) --summary "${summary}" fork main
# shellcheck disable=SC2016 # a Markdown code span, not a substitution
if grep -q '^- `shared.txt`$' "${summary}" && grep -q '^\*\*Rebase due:\*\*' "${summary}"; then
  echo "ok   summary-content"
  passed=$((passed + 1))
else
  echo "FAIL summary-content" >&2
  sed 's/^/    /' "${summary}" >&2
  failures=$((failures + 1))
fi

check unknown-ref 2 'unknown fork ref' $((t0 + 5 * day)) nope main
check bad-limit 2 'whole number' $((t0 + 5 * day)) --max-age-days soon fork main

echo "upstream-drift.sh self-test: ${passed} passed, ${failures} failed"
[[ ${failures} -eq 0 ]]
