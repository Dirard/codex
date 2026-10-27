#!/usr/bin/env bash
# Release steps of .github/workflows/fork-rust-release.yml that talk to GitHub.
#
#   scripts/fork/release.sh provenance --repo OWNER/NAME --sha SHA --base SHA
#                                      [--repo-dir DIR]
#   scripts/fork/release.sh preflight --version X.Y.Z --sha SHA [--repo OWNER/NAME]
#                                     [--matrix JSON] [--output FILE]
#   scripts/fork/release.sh draft-check --repo OWNER/NAME --version X.Y.Z --sha SHA
#   scripts/fork/release.sh draft --repo OWNER/NAME --version X.Y.Z --sha SHA
#                                 --dist DIR [--run-url URL] [--dry-run]
#   scripts/fork/release.sh machinery
#
# provenance   requires SHA to be the dispatched codex/fork commit --base or one
#              of its ancestors (GitHub compare), with the same release
#              machinery (the paths `machinery` prints) as --base. The release
#              workflow runs it from the dispatched commit, before any code of
#              SHA runs. Git commands use --repo-dir (default: this checkout).
# preflight    validates the dispatch inputs before hours of building: the
#              version format, a full 40-character commit SHA that matches the
#              checkout, the workspace version in codex-rs/Cargo.toml, and the
#              build matrix (known targets, allowed runners). With --repo it also
#              requires that rust-vX.Y.Z and sdk/go/vX.Y.Z are absent or point at
#              SHA and that rust-vX.Y.Z is not published yet. Writes version, sha,
#              matrix, targets and full_matrix as key=value lines to --output
#              (for $GITHUB_OUTPUT) and to stdout.
# draft-check  read-only part of draft for the start of the run (needs a token
#              that sees drafts): repeats the tag checks and fails when a
#              release for rust-vX.Y.Z is published or exists as a draft that is
#              not resumable. Reports resumable drafts that draft will replace.
# draft        checks DIR against its SHA256SUMS (all five packages plus the sums
#              file), runs the draft-check classification, deletes resumable
#              drafts (re-read by id first), creates a draft release at SHA with
#              every file of DIR as an asset, and compares the uploaded asset
#              names, sizes and digests with SHA256SUMS. It never creates or
#              deletes tags and never publishes or edits a release; the summary
#              lists the manual publication steps. --dry-run performs only the
#              read-only checks and prints the commands.
# machinery    prints the release machinery paths, one per line.
#
# A resumable draft is one this workflow staged and nobody touched since: a
# draft that was never published, targets SHA, and still has the placeholder
# body marker. A partial failure (broken upload, draft not yet listed, digest
# mismatch) leaves such a draft, and a re-run replaces it.
#
# GitHub access goes through the gh CLI (GH_TOKEN in CI).
# Requires Linux tools: bash >= 4.4, GNU-compatible coreutils (GNU or uutils:
# stat -c, sha256sum --strict) and findutils (GNU or bfs: find -printf).

# Written for bash 3.2 too, so an old bash reaches this message.
if ((BASH_VERSINFO[0] < 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] < 4))); then
  printf '%s: requires bash >= 4.4 (running %s); run it on Linux\n' "${0##*/}" "${BASH_VERSION}" >&2
  exit 1
fi

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"

version_re='^[0-9]+\.[0-9]+\.[0-9]+(-(alpha(\.[0-9]+){0,2}|beta(\.[0-9]+)?))?$'
sha_re='^[0-9a-f]{40}$'
repo_re='^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$'

# Runner and target pairs built by default; the order is the build order.
default_matrix='[
  {"runner":"ubuntu-24.04","target":"x86_64-unknown-linux-gnu"},
  {"runner":"ubuntu-24.04-arm","target":"aarch64-unknown-linux-gnu"},
  {"runner":"macos-15-intel","target":"x86_64-apple-darwin"},
  {"runner":"macos-15","target":"aarch64-apple-darwin"},
  {"runner":"windows-2025","target":"x86_64-pc-windows-msvc"}
]'

# GitHub-hosted runners allowed per target: the same OS and architecture as
# the default runner (listed first). Self-hosted labels are never allowed.
runner_allowlist='{
  "x86_64-unknown-linux-gnu": ["ubuntu-24.04", "ubuntu-22.04"],
  "aarch64-unknown-linux-gnu": ["ubuntu-24.04-arm", "ubuntu-22.04-arm"],
  "x86_64-apple-darwin": ["macos-15-intel"],
  "aarch64-apple-darwin": ["macos-15", "macos-14"],
  "x86_64-pc-windows-msvc": ["windows-2025", "windows-2022"]
}'

# Files that decide what the release workflow builds, checks and runs with a
# write token: the workflows, the local actions they use and these scripts.
machinery_paths=(
  .github/workflows/fork-rust-release.yml
  .github/workflows/fork-ci.yml
  .github/actions/fork-rust-toolchain/
  .github/actions/fork-setup-rust/
  .github/actions/setup-rusty-v8/
  .github/actions/setup-msvc-env/
  .github/actions/check-clean-worktree/
  scripts/fork/
)

# Placeholder body line that marks a draft as staged by this workflow.
draft_marker='Draft staged by fork-rust-release.'

# Seconds between listings while a created draft is not visible yet.
read -r -a visibility_delays <<<"${RELEASE_VISIBILITY_DELAYS:-2 4 8 16}"

# Temporary files removed on exit, whatever the outcome.
cleanup_paths=()
trap 'rm -rf -- "${cleanup_paths[@]}"' EXIT

log() { printf '==> %s\n' "$*" >&2; }
die() {
  printf 'release: %s\n' "$*" >&2
  exit 1
}

usage() {
  sed -n '2,/^$/p' "${BASH_SOURCE[0]}" | sed -E 's/^# ?//'
}

# Requires the GNU-compatible options draft uses; BSD and macOS tools lack them.
require_tool_features() {
  local probe
  probe="$(mktemp -d "${TMPDIR:-/tmp}/release-probe.XXXXXX")"
  cleanup_paths+=("${probe}")
  printf 'x' >"${probe}/f"
  [[ "$(stat -c %s "${probe}/f" 2>/dev/null)" == 1 ]] ||
    die "requires GNU-compatible coreutils (stat -c): GNU or uutils coreutils, as on Linux"
  [[ "$(find "${probe}" -mindepth 1 -maxdepth 1 -name f -printf '%f\n' 2>/dev/null)" == f ]] ||
    die "requires GNU-compatible findutils (find -printf): GNU findutils or bfs, as on Linux"
  (cd "${probe}" && sha256sum f >sums && sha256sum --quiet --strict -c sums) >/dev/null 2>&1 ||
    die "requires GNU-compatible coreutils (sha256sum --strict): GNU or uutils coreutils, as on Linux"
}

all_targets() {
  jq -r '.[].target' <<<"${default_matrix}" | LC_ALL=C sort
}

package_name() {
  if [[ "$1" == *windows* ]]; then
    printf 'codex-package-%s.zip\n' "$1"
  else
    printf 'codex-package-%s.tar.gz\n' "$1"
  fi
}

# Prints the commit a tag points at (peeling annotated tags), or nothing when
# the tag does not exist.
tag_commit() {
  local repo="$1" tag="$2" ref kind sha
  ref="$(gh api "repos/${repo}/git/matching-refs/tags/${tag}" \
    --jq ".[] | select(.ref == \"refs/tags/${tag}\") | .object.type + \" \" + .object.sha")" ||
    die "cannot look up tag ${tag} in ${repo}"
  [[ -n "${ref}" ]] || return 0
  read -r kind sha <<<"${ref}"
  local depth=0
  while [[ "${kind}" == "tag" ]]; do
    depth=$((depth + 1))
    [[ ${depth} -le 5 ]] || die "tag ${tag} nests too deeply"
    ref="$(gh api "repos/${repo}/git/tags/${sha}" --jq '.object.type + " " + .object.sha')" ||
      die "cannot read annotated tag ${tag}"
    read -r kind sha <<<"${ref}"
  done
  [[ "${kind}" == "commit" ]] || die "tag ${tag} points at a ${kind}, not a commit"
  printf '%s\n' "${sha}"
}

# Requires each release tag to be absent or to point at the release commit.
check_tags() {
  local repo="$1" version="$2" sha="$3" tag commit
  for tag in "rust-v${version}" "sdk/go/v${version}"; do
    commit="$(tag_commit "${repo}" "${tag}")"
    if [[ -z "${commit}" ]]; then
      log "tag ${tag}: absent"
    elif [[ "${commit}" == "${sha}" ]]; then
      log "tag ${tag}: already points at ${sha}"
    else
      die "tag ${tag} points at ${commit}, not ${sha}"
    fi
  done
}

# Prints "id<TAB>draft<TAB>published_at<TAB>target<TAB>marker<TAB>url" for every
# release (drafts included when the token has push access) whose tag is TAG.
releases_for_tag() {
  local repo="$1" tag="$2"
  gh api --paginate "repos/${repo}/releases?per_page=100" \
    --jq ".[] | select(.tag_name == \"${tag}\") | [(.id | tostring), (.draft | tostring), (.published_at // \"null\"), .target_commitish, ((.body // \"\") | contains(\"${draft_marker}\") | tostring), .html_url] | @tsv"
}

# Classifies the releases for TAG: fails when one is published, was published
# or is a draft that is not resumable at SHA; otherwise prints the ids of the
# resumable drafts (possibly none).
classify_existing() {
  local repo="$1" tag="$2" sha="$3" existing
  existing="$(releases_for_tag "${repo}" "${tag}")" || die "cannot list releases of ${repo}"
  [[ -n "${existing}" ]] || return 0
  local id draft published target marker url bad=()
  while IFS=$'\t' read -r id draft published target marker url; do
    if [[ "${draft}" != "true" ]]; then
      die "a release for ${tag} exists and ${tag} is published, never touched here: ${url} (id ${id})"
    elif [[ "${published}" != "null" ]]; then
      die "draft ${id} for ${tag} was published at ${published}, never touched here: ${url}"
    elif [[ "${target}" != "${sha}" || "${marker}" != "true" ]]; then
      bad+=("${id} (target ${target}, placeholder body ${marker}) ${url}")
    fi
  done <<<"${existing}"
  if [[ ${#bad[@]} -gt 0 ]]; then
    die "a draft for ${tag} is not a resumable fork-rust-release draft for ${sha}; review it, delete it (gh api -X DELETE repos/${repo}/releases/<id>) and re-run:
$(printf '  %s\n' "${bad[@]}")"
  fi
  cut -f1 <<<"${existing}"
}

# Deletes each resumable draft after re-reading it by id: it must still be an
# unpublished draft at SHA with the placeholder body.
delete_resumable_drafts() {
  local repo="$1" sha="$2" id state
  shift 2
  for id in "$@"; do
    state="$(gh api "repos/${repo}/releases/${id}" \
      --jq "[(.draft | tostring), (.published_at // \"null\"), .target_commitish, ((.body // \"\") | contains(\"${draft_marker}\") | tostring)] | @tsv")" ||
      die "cannot re-read draft ${id}"
    [[ "${state}" == "true"$'\t'"null"$'\t'"${sha}"$'\t'"true" ]] ||
      die "release ${id} is no longer a resumable draft (draft, published_at, target, placeholder: ${state//$'\t'/, }); nothing deleted"
    log "delete resumable draft ${id} (left by an earlier, failed run)"
    gh api -X DELETE "repos/${repo}/releases/${id}" >/dev/null || die "cannot delete draft ${id}"
  done
}

# Prints the id of the release whose html_url is URL, listing again with the
# delays of RELEASE_VISIBILITY_DELAYS while it is not visible yet.
release_id_for_url() {
  local repo="$1" tag="$2" url="$3" id delay
  for delay in "${visibility_delays[@]}" ""; do
    id="$(releases_for_tag "${repo}" "${tag}" | awk -F '\t' -v url="${url}" '$6 == url { print $1; exit }')" ||
      die "cannot list releases of ${repo}"
    if [[ -n "${id}" ]]; then
      printf '%s\n' "${id}"
      return 0
    fi
    [[ -n "${delay}" ]] || break
    log "created draft ${url} is not listed yet; retrying in ${delay}s"
    sleep "${delay}"
  done
  die "created draft ${url} for ${tag} is not visible in the release list; re-run the job: it replaces this draft"
}

cmd_machinery() {
  printf '%s\n' "${machinery_paths[@]}"
}

cmd_provenance() {
  local repo="" sha="" base="" repo_dir="${repo_root}"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --repo) repo="${2:?--repo requires a value}"; shift 2 ;;
      --sha) sha="${2:?--sha requires a value}"; shift 2 ;;
      --base) base="${2:?--base requires a value}"; shift 2 ;;
      --repo-dir) repo_dir="${2:?--repo-dir requires a directory}"; shift 2 ;;
      *) die "unknown provenance option: $1" ;;
    esac
  done
  [[ "${repo}" =~ ${repo_re} ]] || die "invalid repository '${repo}'"
  [[ "${sha}" =~ ${sha_re} ]] || die "checkout_ref must be a full 40-character lowercase commit SHA, got '${sha}'"
  [[ "${base}" =~ ${sha_re} ]] || die "invalid dispatched commit '${base}'"
  if [[ "${sha}" == "${base}" ]]; then
    log "release commit ${sha} is the dispatched commit"
    return 0
  fi

  local status
  status="$(gh api "repos/${repo}/compare/${sha}...${base}" --jq .status)" ||
    die "cannot compare ${sha} with the dispatched commit ${base}"
  [[ "${status}" == "ahead" || "${status}" == "identical" ]] ||
    die "release commit ${sha} is not an ancestor of the dispatched codex/fork commit ${base} (compare: ${status})"
  log "release commit ${sha} is an ancestor of the dispatched commit ${base}"

  if ! git -C "${repo_dir}" cat-file -e "${sha}^{commit}" 2>/dev/null; then
    git -C "${repo_dir}" fetch --quiet --no-tags --depth=1 origin "${sha}" ||
      die "cannot fetch ${sha}"
  fi
  local differing
  differing="$(git -C "${repo_dir}" diff --name-only "${sha}" "${base}" -- "${machinery_paths[@]}")" ||
    die "cannot compare the release machinery of ${sha} and ${base}"
  [[ -z "${differing}" ]] ||
    die "release machinery differs between ${sha} and the dispatched commit ${base}; dispatch fork-rust-release for a commit with the current workflows and scripts:
  ${differing//$'\n'/$'\n  '}"
  log "release machinery matches the dispatched commit"
}

cmd_preflight() {
  local version="" sha="" repo="" matrix="" output=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --version) version="${2:?--version requires a value}"; shift 2 ;;
      --sha) sha="${2:?--sha requires a value}"; shift 2 ;;
      --repo) repo="${2:?--repo requires a value}"; shift 2 ;;
      --matrix) matrix="${2-}"; shift 2 ;;
      --output) output="${2:?--output requires a file}"; shift 2 ;;
      *) die "unknown preflight option: $1" ;;
    esac
  done
  [[ "${version}" =~ ${version_re} ]] || die "invalid release version '${version}'"
  [[ "${sha}" =~ ${sha_re} ]] || die "checkout_ref must be a full 40-character lowercase commit SHA, got '${sha}'"
  [[ -z "${repo}" || "${repo}" =~ ${repo_re} ]] || die "invalid repository '${repo}'"

  local head
  head="$(git -C "${repo_root}" rev-parse HEAD)"
  [[ "${head}" == "${sha}" ]] || die "checkout is at ${head}, not ${sha}"

  local workspace_version
  workspace_version="$(awk '
    /^\[workspace\.package\]/ { section = 1; next }
    /^\[/ { section = 0 }
    section && $1 == "version" { gsub(/"/, "", $3); print $3; exit }
  ' "${repo_root}/codex-rs/Cargo.toml")"
  [[ "${workspace_version}" == "${version}" ]] ||
    die "codex-rs/Cargo.toml has workspace version '${workspace_version}', not '${version}'"

  [[ -n "${matrix}" ]] || matrix="${default_matrix}"
  matrix="$(jq -ce --argjson allowed "${runner_allowlist}" '
    if type == "array" and length > 0
      and all(.[]; type == "object"
        and (.runner | type) == "string"
        and (.target | type) == "string"
        and ((.target as $t | $allowed[$t]) | type) == "array"
        and (.runner as $r | $allowed[.target] | index($r)) != null)
      and ([.[].target] | unique | length) == length
    then [.[] | {runner, target}]
    else error("invalid")
    end
  ' <<<"${matrix}" 2>/dev/null)" ||
    die "build_matrix must be a non-empty JSON array of {runner, target} with distinct known targets and runners from the runner allowlist: $(jq -c . <<<"${runner_allowlist}")"
  local targets full_matrix=false
  targets="$(jq -r '[.[].target] | join(" ")' <<<"${matrix}")"
  if [[ "$(jq -r '.[].target' <<<"${matrix}" | LC_ALL=C sort)" == "$(all_targets)" ]]; then
    full_matrix=true
  fi

  if [[ -n "${repo}" ]]; then
    command -v gh >/dev/null 2>&1 || die "gh is required with --repo"
    check_tags "${repo}" "${version}" "${sha}"
    local err
    if err="$(gh api "repos/${repo}/releases/tags/rust-v${version}" --jq .html_url 2>&1)"; then
      die "rust-v${version} is already published: ${err}"
    elif [[ "${err}" != *"HTTP 404"* ]]; then
      die "cannot check for a published rust-v${version}: ${err}"
    fi
    log "no published release rust-v${version}"
  fi

  local lines
  lines="$(printf 'version=%s\nsha=%s\nmatrix=%s\ntargets=%s\nfull_matrix=%s\n' \
    "${version}" "${sha}" "${matrix}" "${targets}" "${full_matrix}")"
  printf '%s\n' "${lines}"
  if [[ -n "${output}" ]]; then
    printf '%s\n' "${lines}" >>"${output}"
  fi
}

# Validates the caller's repo, version and sha (from --repo, --version, --sha).
parse_release_target() {
  [[ "${repo}" =~ ${repo_re} ]] || die "invalid repository '${repo}'"
  [[ "${version}" =~ ${version_re} ]] || die "invalid release version '${version}'"
  [[ "${sha}" =~ ${sha_re} ]] || die "invalid commit SHA '${sha}'"
}

cmd_draft_check() {
  local repo="" version="" sha=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --repo) repo="${2:?--repo requires a value}"; shift 2 ;;
      --version) version="${2:?--version requires a value}"; shift 2 ;;
      --sha) sha="${2:?--sha requires a value}"; shift 2 ;;
      *) die "unknown draft-check option: $1" ;;
    esac
  done
  parse_release_target
  local tag="rust-v${version}" resumable
  check_tags "${repo}" "${version}" "${sha}"
  resumable="$(classify_existing "${repo}" "${tag}" "${sha}")"
  if [[ -z "${resumable}" ]]; then
    log "no release for ${tag}"
  else
    log "the draft job will replace resumable draft $(xargs <<<"${resumable}") for ${tag}"
  fi
}

cmd_draft() {
  local repo="" version="" sha="" dist="" run_url="" dry_run=0
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --repo) repo="${2:?--repo requires a value}"; shift 2 ;;
      --version) version="${2:?--version requires a value}"; shift 2 ;;
      --sha) sha="${2:?--sha requires a value}"; shift 2 ;;
      --dist) dist="${2:?--dist requires a directory}"; shift 2 ;;
      --run-url) run_url="${2:?--run-url requires a value}"; shift 2 ;;
      --dry-run) dry_run=1; shift ;;
      *) die "unknown draft option: $1" ;;
    esac
  done
  parse_release_target
  require_tool_features
  [[ -d "${dist}" ]] || die "missing dist directory ${dist}"
  dist="$(cd -- "${dist}" && pwd)"
  local tag="rust-v${version}"

  # The assets are exactly the five packages listed in SHA256SUMS plus that file.
  [[ -f "${dist}/SHA256SUMS" ]] || die "missing ${dist}/SHA256SUMS"
  local expected listed actual
  expected="$(all_targets | while read -r target; do package_name "${target}"; done | LC_ALL=C sort)"
  listed="$(awk '{print $2}' "${dist}/SHA256SUMS" | LC_ALL=C sort)"
  [[ "${listed}" == "${expected}" ]] || die "SHA256SUMS does not list exactly the five release packages:
${listed}"
  actual="$(find "${dist}" -mindepth 1 -maxdepth 1 ! -name SHA256SUMS -printf '%f\n' | LC_ALL=C sort)"
  [[ "${actual}" == "${expected}" ]] || die "unexpected files in ${dist}:
${actual}"
  (cd "${dist}" && sha256sum --quiet --strict -c SHA256SUMS) || die "packages do not match SHA256SUMS"
  log "packages match SHA256SUMS"

  check_tags "${repo}" "${version}" "${sha}"
  # A plain assignment, not a process substitution: a refusal inside
  # classify_existing must stop this script.
  local resumable_ids resumable=()
  resumable_ids="$(classify_existing "${repo}" "${tag}" "${sha}")"
  if [[ -n "${resumable_ids}" ]]; then
    mapfile -t resumable <<<"${resumable_ids}"
  else
    log "no existing release for ${tag}"
  fi

  local notes
  notes="$(mktemp "${TMPDIR:-/tmp}/release-notes.XXXXXX")"
  cleanup_paths+=("${notes}")
  # shellcheck disable=SC2016 # backticks in the notes are Markdown code spans
  {
    printf '> [!WARNING]\n> %s Replace this body with the curated release notes before publishing.\n\n' "${draft_marker}"
    printf -- '- Commit: `%s`\n' "${sha}"
    if [[ -n "${run_url}" ]]; then
      printf -- '- Build and verification run: %s\n' "${run_url}"
    fi
    printf '\n| Asset | Bytes | SHA-256 |\n|---|---:|---|\n'
    local hash file
    while read -r hash file; do
      # shellcheck disable=SC2016 # backticks are Markdown code spans
      printf '| `%s` | %s | `%s` |\n' "${file}" "$(stat -c %s "${dist}/${file}")" "${hash}"
    done <"${dist}/SHA256SUMS"
  } >"${notes}"

  local assets=()
  while IFS= read -r file; do
    assets+=("${dist}/${file}")
  done < <(printf '%s\nSHA256SUMS\n' "${expected}")

  local create=(gh release create "${tag}" --repo "${repo}" --draft --target "${sha}"
    --title "Codex fork ${version}" --notes-file "${notes}" "${assets[@]}")
  if [[ ${dry_run} -eq 1 ]]; then
    local id
    for id in "${resumable[@]}"; do
      log "dry run: would delete resumable draft ${id} after re-reading it"
    done
    log "dry run: would run"
    printf '%q ' "${create[@]}"
    printf '\n--- draft notes ---\n'
    cat "${notes}"
    return 0
  fi

  delete_resumable_drafts "${repo}" "${sha}" "${resumable[@]}"
  log "create draft ${tag} at ${sha}"
  local created url
  created="$("${create[@]}")" || die "gh release create failed; a partial draft may remain, and a re-run replaces it"
  url="$(grep -E '^https://' <<<"${created}" | tail -n 1)" ||
    die "gh release create printed no release URL: ${created}"

  local id
  id="$(release_id_for_url "${repo}" "${tag}" "${url}")"

  # Compare what GitHub stored with what was verified.
  local uploaded want
  uploaded="$(gh api "repos/${repo}/releases/${id}" \
    --jq '.assets[] | [.name, (.size | tostring), (.digest // "none"), .state] | @tsv' | LC_ALL=C sort)"
  want="$(while read -r hash file; do
    printf '%s\t%s\tsha256:%s\tuploaded\n' "${file}" "$(stat -c %s "${dist}/${file}")" "${hash}"
  done <"${dist}/SHA256SUMS"
  printf 'SHA256SUMS\t%s\tsha256:%s\tuploaded\n' "$(stat -c %s "${dist}/SHA256SUMS")" \
    "$(sha256sum "${dist}/SHA256SUMS" | cut -d' ' -f1)")"
  want="$(LC_ALL=C sort <<<"${want}")"
  [[ "${uploaded}" == "${want}" ]] || die "uploaded assets differ from the verified files (a re-run replaces draft ${id})
expected:
${want}
uploaded:
${uploaded}"
  log "draft ${id} has the verified assets: ${url}"

  local summary
  summary="$(cat <<EOF
### Draft release ${tag}

Draft: ${url} (id ${id}), target \`${sha}\`. Assets match SHA256SUMS (name, size, digest).

Publication stays manual:

1. Tag the release commit (annotated, atomic push):
   \`\`\`
   git tag -a rust-v${version} -m "Codex fork ${version}" ${sha}
   git tag -a sdk/go/v${version} -m "Codex fork Go SDK ${version}" ${sha}
   git push --atomic origin rust-v${version} sdk/go/v${version}
   \`\`\`
2. Replace the placeholder notes: \`gh api -X PATCH repos/${repo}/releases/${id} -F body=@<notes.md>\`
3. Publish: \`gh api -X PATCH repos/${repo}/releases/${id} -F draft=false -f make_latest=true\`
EOF
)"
  printf '%s\n' "${summary}"
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf '%s\n' "${summary}" >>"${GITHUB_STEP_SUMMARY}"
  fi
}

main() {
  [[ $# -ge 1 ]] || {
    usage >&2
    exit 2
  }
  local command="$1"
  shift
  case "${command}" in
    machinery) cmd_machinery "$@" ;;
    -h | --help | help) usage ;;
    provenance | preflight | draft-check | draft)
      command -v jq >/dev/null 2>&1 || die "jq is required"
      case "${command}" in
        provenance)
          command -v gh >/dev/null 2>&1 || die "gh is required"
          cmd_provenance "$@"
          ;;
        preflight) cmd_preflight "$@" ;;
        draft-check)
          command -v gh >/dev/null 2>&1 || die "gh is required"
          cmd_draft_check "$@"
          ;;
        draft)
          command -v gh >/dev/null 2>&1 || die "gh is required"
          cmd_draft "$@"
          ;;
      esac
      ;;
    *)
      usage >&2
      die "unknown command: ${command}"
      ;;
  esac
}

main "$@"
