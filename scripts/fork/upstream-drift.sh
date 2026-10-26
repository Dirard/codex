#!/usr/bin/env bash
# Reports how far the fork lags behind upstream main.
#
#   scripts/fork/upstream-drift.sh [--max-age-days N] [--summary FILE] FORK_REF UPSTREAM_REF
#
# Prints the upstream pin of the fork (the merge base of the two refs), the
# number of upstream commits after it, since when the fork has been behind
# (the committer date of the oldest of those commits), and the files that a
# merge of upstream into the fork would conflict on (git merge-tree). With
# --summary the same report is appended to FILE as Markdown.
#
# Exit status: 0 when the fork is current or behind for less than N days
# (default 7), 3 when a rebase is due, 2 on usage or git errors.
# UPSTREAM_DRIFT_NOW (seconds since the epoch) replaces the current time.

set -euo pipefail

usage() {
  sed -n '2,15s/^# \{0,1\}//p' "${BASH_SOURCE[0]}"
}

die() {
  printf 'upstream-drift: %s\n' "$*" >&2
  exit 2
}

max_age_days=7
summary=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --max-age-days)
      [[ $# -ge 2 && "$2" =~ ^[0-9]+$ ]] || die "--max-age-days needs a whole number of days"
      max_age_days="$2"
      shift 2
      ;;
    --summary)
      [[ $# -ge 2 ]] || die "--summary needs a file"
      summary="$2"
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    -*) die "unknown option: $1" ;;
    *) break ;;
  esac
done
[[ $# -eq 2 ]] || {
  usage >&2
  exit 2
}
fork_ref="$1"
upstream_ref="$2"

fork="$(git rev-parse --verify --quiet "${fork_ref}^{commit}")" || die "unknown fork ref: ${fork_ref}"
upstream="$(git rev-parse --verify --quiet "${upstream_ref}^{commit}")" || die "unknown upstream ref: ${upstream_ref}"
pin="$(git merge-base "${fork}" "${upstream}")" || die "${fork_ref} and ${upstream_ref} have no common history"
now="${UPSTREAM_DRIFT_NOW:-$(date +%s)}"

behind="$(git rev-list --count "${pin}..${upstream}")"
behind_since=""
age_days=0
if [[ "${behind}" -gt 0 ]]; then
  behind_since="$(git log --format=%ct "${pin}..${upstream}" | sort -n | head -n 1)"
  age_days=$(((now - behind_since) / 86400))
fi

# merge-tree exits 1 when the merge has conflicts; its first line is the tree.
conflicts=()
merge_status=0
merge_output="$(git merge-tree --write-tree --name-only --no-messages "${fork}" "${upstream}")" || merge_status=$?
if [[ ${merge_status} -gt 1 ]]; then
  die "git merge-tree failed (exit ${merge_status})"
fi
if [[ ${merge_status} -eq 1 ]]; then
  mapfile -t conflicts < <(tail -n +2 <<<"${merge_output}" | sed '/^$/d')
fi

due=0
if [[ "${behind}" -gt 0 && "${age_days}" -ge "${max_age_days}" ]]; then
  due=1
fi

iso_date() { date -u -d "@$1" +%Y-%m-%dT%H:%M:%SZ; }

report() {
  printf 'fork:      %s (%s)\n' "${fork_ref}" "$(git rev-parse --short "${fork}")"
  printf 'upstream:  %s (%s)\n' "${upstream_ref}" "$(git rev-parse --short "${upstream}")"
  printf 'pin:       %s %s\n' "$(git rev-parse --short "${pin}")" "$(git log -1 --format=%cs "${pin}")"
  if [[ "${behind}" -eq 0 ]]; then
    printf 'behind:    0 commits\n'
  else
    printf 'behind:    %s commits, since %s (%s days)\n' "${behind}" "$(iso_date "${behind_since}")" "${age_days}"
  fi
  printf 'conflicts: %s files\n' "${#conflicts[@]}"
  local file
  for file in "${conflicts[@]}"; do
    printf '  %s\n' "${file}"
  done
  if [[ ${due} -eq 1 ]]; then
    printf 'rebase due: behind for %s days, limit %s\n' "${age_days}" "${max_age_days}"
  fi
}

report

if [[ -n "${summary}" ]]; then
  # Backticks are Markdown code spans, not command substitutions.
  # shellcheck disable=SC2016
  {
    printf '## Upstream drift\n\n'
    printf '| | |\n|---|---|\n'
    printf '| Fork | `%s` `%s` |\n' "${fork_ref}" "$(git rev-parse --short "${fork}")"
    printf '| Upstream | `%s` `%s` |\n' "${upstream_ref}" "$(git rev-parse --short "${upstream}")"
    printf '| Pin | `%s` %s |\n' "$(git rev-parse --short "${pin}")" "$(git log -1 --format=%cs "${pin}")"
    if [[ "${behind}" -eq 0 ]]; then
      printf '| Behind | 0 commits |\n'
    else
      printf '| Behind | %s commits since %s (%s days) |\n' "${behind}" "$(iso_date "${behind_since}")" "${age_days}"
    fi
    printf '| Conflicting files | %s |\n\n' "${#conflicts[@]}"
    for file in "${conflicts[@]}"; do
      printf -- '- `%s`\n' "${file}"
    done
    if [[ ${due} -eq 1 ]]; then
      printf '\n**Rebase due:** the fork has been behind upstream for %s days (limit %s).\n' "${age_days}" "${max_age_days}"
    fi
  } >>"${summary}"
fi

if [[ ${due} -eq 1 ]]; then
  exit 3
fi
