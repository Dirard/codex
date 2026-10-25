#!/usr/bin/env bash
# Release steps of .github/workflows/fork-rust-release.yml that talk to GitHub.
#
#   scripts/fork/release.sh preflight --version X.Y.Z --sha SHA [--repo OWNER/NAME]
#                                     [--matrix JSON] [--output FILE]
#   scripts/fork/release.sh draft --repo OWNER/NAME --version X.Y.Z --sha SHA
#                                 --dist DIR [--run-url URL] [--dry-run]
#
# preflight  validates the dispatch inputs before hours of building: the version
#            format, a full 40-character commit SHA that matches the checkout, the
#            workspace version in codex-rs/Cargo.toml, and the build matrix. With
#            --repo it also requires that rust-vX.Y.Z and sdk/go/vX.Y.Z are absent
#            or point at SHA and that rust-vX.Y.Z is not published yet. Writes
#            version, sha, matrix, targets and full_matrix as key=value lines to
#            --output (for $GITHUB_OUTPUT) and to stdout.
# draft      checks DIR against its SHA256SUMS (all five packages plus the sums
#            file), repeats the tag checks, refuses to touch an existing release
#            for rust-vX.Y.Z (draft or published), creates a draft release at SHA
#            with every file of DIR as an asset, and compares the uploaded asset
#            names, sizes and digests with SHA256SUMS. It never creates tags and
#            never publishes; the summary lists the manual publication steps.
#            --dry-run performs only the read-only checks and prints the commands.
#
# GitHub access goes through the gh CLI (GH_TOKEN in CI).

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

log() { printf '==> %s\n' "$*" >&2; }
die() {
  printf 'release: %s\n' "$*" >&2
  exit 1
}

usage() {
  sed -n '2,/^$/p' "${BASH_SOURCE[0]}" | sed -E 's/^# ?//'
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

# Prints "id<TAB>draft<TAB>target<TAB>url" for every release (drafts included
# when the token has push access) whose tag is rust-vVERSION.
releases_for_tag() {
  local repo="$1" tag="$2"
  gh api --paginate "repos/${repo}/releases?per_page=100" \
    --jq ".[] | select(.tag_name == \"${tag}\") | [(.id | tostring), (.draft | tostring), .target_commitish, .html_url] | @tsv"
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
  local known
  known="$(all_targets | jq -R . | jq -cs .)"
  matrix="$(jq -ce --argjson known "${known}" '
    if type == "array" and length > 0
      and all(.[]; type == "object"
        and (.runner | type) == "string" and (.runner | length) > 0
        and (.target | type) == "string"
        and (.target as $t | $known | index($t)) != null)
      and ([.[].target] | unique | length) == length
    then [.[] | {runner, target}]
    else error("invalid")
    end
  ' <<<"${matrix}" 2>/dev/null)" ||
    die "build_matrix must be a non-empty JSON array of {runner, target} with distinct known targets: $(all_targets | xargs)"
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
  [[ "${repo}" =~ ${repo_re} ]] || die "invalid repository '${repo}'"
  [[ "${version}" =~ ${version_re} ]] || die "invalid release version '${version}'"
  [[ "${sha}" =~ ${sha_re} ]] || die "invalid commit SHA '${sha}'"
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
  local existing
  existing="$(releases_for_tag "${repo}" "${tag}")"
  if [[ -n "${existing}" ]]; then
    if awk -F '\t' '$2 == "false"' <<<"${existing}" | grep -q .; then
      die "a release for ${tag} already exists and is published:
${existing}"
    fi
    die "a release for ${tag} already exists as a draft; review it, delete it (gh api -X DELETE repos/${repo}/releases/<id>) and re-run:
${existing}"
  fi
  log "no existing release for ${tag}"

  local notes
  notes="$(mktemp "${TMPDIR:-/tmp}/release-notes.XXXXXX")"
  # shellcheck disable=SC2016 # backticks in the notes are Markdown code spans
  {
    printf '> [!WARNING]\n> Draft staged by fork-rust-release. Replace this body with the curated release notes before publishing.\n\n'
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
    log "dry run: would run"
    printf '%q ' "${create[@]}"
    printf '\n--- draft notes ---\n'
    cat "${notes}"
    rm -f "${notes}"
    return 0
  fi

  log "create draft ${tag} at ${sha}"
  "${create[@]}"
  rm -f "${notes}"

  local id url
  read -r id _ _ url < <(releases_for_tag "${repo}" "${tag}" | awk -F '\t' '$2 == "true"') || true
  [[ -n "${id:-}" ]] || die "created draft for ${tag} is not visible"

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
  [[ "${uploaded}" == "${want}" ]] || die "uploaded assets differ from the verified files
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
  command -v jq >/dev/null 2>&1 || die "jq is required"
  local command="$1"
  shift
  case "${command}" in
    preflight) cmd_preflight "$@" ;;
    draft)
      command -v gh >/dev/null 2>&1 || die "gh is required"
      cmd_draft "$@"
      ;;
    -h | --help | help) usage ;;
    *)
      usage >&2
      die "unknown command: ${command}"
      ;;
  esac
}

main "$@"
