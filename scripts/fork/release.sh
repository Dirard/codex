#!/usr/bin/env bash
# Release steps of .github/workflows/fork-rust-release.yml that talk to GitHub.
#
#   scripts/fork/release.sh provenance --repo OWNER/NAME --sha SHA --base SHA
#                                      [--repo-dir DIR]
#   scripts/fork/release.sh preflight --version X.Y.Z --sha SHA [--repo OWNER/NAME]
#                                     [--matrix JSON] [--create-draft true|false]
#                                     [--rebuild true|false] [--output FILE]
#   scripts/fork/release.sh preflight --mode prebuild --sha SHA [--repo OWNER/NAME]
#                                     [--output FILE]
#   scripts/fork/release.sh find-run --repo OWNER/NAME --workflow FILE --sha SHA
#                                    --event EVENT --branch BRANCH
#   scripts/fork/release.sh fetch-parts --repo OWNER/NAME --parts JSON --dir DIR
#   scripts/fork/release.sh draft-check --repo OWNER/NAME --version X.Y.Z --sha SHA
#   scripts/fork/release.sh draft --repo OWNER/NAME --version X.Y.Z --sha SHA
#                                 --dist DIR [--run-url URL] [--ci-run-url URL]
#                                 [--reused-runs "URL..."] [--dry-run]
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
#              build matrix (release targets, allowed runners). The output matrix
#              has one entry per target and bundle (release-package.sh bins);
#              targets and full_matrix count targets. With --repo and
#              --create-draft true (the default) it also requires that
#              rust-vX.Y.Z and sdk/go/vX.Y.Z are absent or point at SHA and that
#              rust-vX.Y.Z is not published yet; --create-draft false skips those
#              checks, so a dispatch can rehearse a released version. With --repo
#              it also looks for a successful push run of fork-ci.yml for SHA
#              (ci_run_url) and, for the full matrix unless --rebuild true, for
#              parts it can reuse (see below).
#              --mode prebuild (a push to codex/fork) takes the version from
#              codex-rs/Cargo.toml, builds the default matrix and checks no tags,
#              releases or fork-ci runs; with --repo it looks for parts to reuse,
#              without it makes no GitHub calls.
#              Reuse: for each part of the matrix, the newest unexpired artifact
#              codex-parts-<version>-<target>-<bundle> of a push run of
#              fork-rust-release.yml on codex/fork, of any status, whose
#              commit has the same build inputs as SHA (build_input_paths and the
#              build job's FORK_BUILD_RECIPE, compared through GitHub compare),
#              whose build job succeeded on the part's runner, and that stays
#              unexpired for 12 more hours; both Linux parts of a target come
#              from one run. The output matrix holds only the parts left to
#              build.
#              Writes version, sha, matrix, targets, full_matrix, ci_run_url,
#              build (true when the matrix is not empty), reused_parts (JSON
#              [{name, run_id, run_url}]) and reused_runs (run URLs) as
#              key=value lines to --output (for $GITHUB_OUTPUT) and to stdout.
# find-run     prints "ID URL" of the newest completed, successful run of
#              .github/workflows/FILE for SHA with that event and branch, or
#              nothing.
# fetch-parts  downloads each reused part of JSON ([{name, run_id}], as
#              preflight's reused_parts) from its run into DIR as NAME.tar.
# draft-check  read-only part of draft for the start of the run (needs a token
#              that sees drafts): repeats the tag checks and fails when a
#              release for rust-vX.Y.Z is published or exists as a draft that is
#              not resumable. Reports resumable drafts that draft will replace.
# draft        checks DIR against its SHA256SUMS (one package per target of the
#              default matrix plus the sums file), runs the draft-check
#              classification, deletes resumable drafts (re-read by id first),
#              creates a draft release at SHA with every file of DIR as an
#              asset, and compares the uploaded asset names, sizes and digests
#              with SHA256SUMS. It never creates or
#              deletes tags and never publishes or edits a release; the summary
#              lists the manual publication steps. --ci-run-url names a reused
#              fork-ci run and --reused-runs the prebuild runs whose bundle parts
#              were reused, in the notes and the summary. --dry-run performs only
#              the read-only checks and prints the commands.
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

# Runner and target pairs built by default; the order is the build order. These
# are the release targets: a release holds one package per target.
default_matrix='[
  {"runner":"ubuntu-24.04","target":"x86_64-unknown-linux-gnu"},
  {"runner":"ubuntu-24.04-arm","target":"aarch64-unknown-linux-gnu"},
  {"runner":"windows-2025","target":"x86_64-pc-windows-msvc"}
]'

# GitHub-hosted runners allowed per target: the same OS and architecture as
# the default runner (listed first). Self-hosted labels are never allowed.
runner_allowlist='{
  "x86_64-unknown-linux-gnu": ["ubuntu-24.04", "ubuntu-22.04"],
  "aarch64-unknown-linux-gnu": ["ubuntu-24.04-arm", "ubuntu-22.04-arm"],
  "x86_64-pc-windows-msvc": ["windows-2025", "windows-2022"]
}'

# Bundles every target is built in (R1.1); release-package.sh owns their
# binaries, packs them on the build runner and merges them in verify.
release_package="${repo_root}/scripts/fork/release-package.sh"
bundles=(primary app-server)

# Runs that a release dispatch reuses come from pushes to this branch (R3.1, R4.3).
reuse_branch=codex/fork
ci_workflow=fork-ci.yml
release_workflow=fork-rust-release.yml

# Files whose content decides what a build job produces (R6.2). codex-rs/ is
# self-contained: its include_str! paths and build scripts stay inside it;
# cargo also reads the root .cargo/config.toml, and setup-rusty-v8 checks the
# V8 archives against the manifests in third_party/v8/.
build_input_paths=(
  codex-rs/
  .cargo/
  third_party/v8/
  scripts/fork/release-package.sh
  scripts/fork/rust-toolchain.sh
  .github/actions/fork-rust-toolchain/
  .github/actions/setup-msvc-env/
  .github/actions/setup-rusty-v8/
  .github/scripts/rusty_v8_bazel.py
)
# The build job's steps are compared through this env key of the job instead;
# a commit without it builds with recipe 1.
recipe_key=FORK_BUILD_RECIPE
# GitHub compare lists at most 300 files; a longer list may be truncated.
compare_file_limit=300
# Newest push runs searched for parts to reuse.
reuse_run_limit=20
# A reused part must stay unexpired this many more seconds, so that it outlives
# the longest build job (240 minutes) and its queue.
reuse_expiry_margin=$((12 * 3600))

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

# bundle_matrix MATRIX: expands the normalized [{runner, target}] matrix into
# one {runner, target, bundle, binaries} entry per target and bundle, in order.
bundle_matrix() {
  local runner target bundle binaries entries=()
  while IFS=$'\t' read -r runner target; do
    for bundle in "${bundles[@]}"; do
      binaries="$("${release_package}" bins --target "${target}" --bundle "${bundle}")"
      entries+=("$(jq -cn --arg runner "${runner}" --arg target "${target}" \
        --arg bundle "${bundle}" --arg binaries "${binaries}" \
        '{runner: $runner, target: $target, bundle: $bundle, binaries: $binaries}')")
    done
  done < <(jq -r '.[] | [.runner, .target] | @tsv' <<<"$1")
  printf '%s\n' "${entries[@]}" | jq -cs .
}

# find_run REPO WORKFLOW SHA EVENT BRANCH: see find-run.
find_run() {
  local repo="$1" workflow="$2" sha="$3" event="$4" branch="$5"
  local runs
  runs="$(gh api "repos/${repo}/actions/workflows/${workflow}/runs?head_sha=${sha}&event=${event}&branch=${branch}&status=success&per_page=100")" ||
    die "cannot list the ${workflow} runs of ${repo}"
  # GitHub filters by the query; the same conditions are required here too.
  jq -r --arg path ".github/workflows/${workflow}" --arg sha "${sha}" \
    --arg event "${event}" --arg branch "${branch}" '
    [.workflow_runs[]
      | select(.path == $path and .head_sha == $sha and .event == $event
          and .head_branch == $branch and .status == "completed"
          and .conclusion == "success")]
    | sort_by(.created_at) | reverse | first // empty | "\(.id) \(.html_url)"
  ' <<<"${runs}" || die "cannot read the ${workflow} runs of ${repo}"
}

cmd_find_run() {
  local repo="" workflow="" sha="" event="" branch=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --repo) repo="${2:?--repo requires a value}"; shift 2 ;;
      --workflow) workflow="${2:?--workflow requires a file name}"; shift 2 ;;
      --sha) sha="${2:?--sha requires a value}"; shift 2 ;;
      --event) event="${2:?--event requires a value}"; shift 2 ;;
      --branch) branch="${2:?--branch requires a value}"; shift 2 ;;
      *) die "unknown find-run option: $1" ;;
    esac
  done
  [[ "${repo}" =~ ${repo_re} ]] || die "invalid repository '${repo}'"
  [[ "${workflow}" =~ ^[A-Za-z0-9_.-]+\.ya?ml$ ]] || die "invalid workflow file '${workflow}'"
  [[ "${sha}" =~ ${sha_re} ]] || die "invalid commit SHA '${sha}'"
  [[ "${event}" =~ ^[a-z_]+$ ]] || die "invalid event '${event}'"
  [[ "${branch}" =~ ^[A-Za-z0-9_][A-Za-z0-9_./-]*$ ]] || die "invalid branch '${branch}'"
  find_run "${repo}" "${workflow}" "${sha}" "${event}" "${branch}"
}

# build_recipe: the FORK_BUILD_RECIPE value of the workflow on stdin, 1 when it
# has none.
build_recipe() {
  local value
  value="$(sed -nE "/^[[:space:]]*${recipe_key}:/{s/^[[:space:]]*${recipe_key}:[[:space:]]*\"?([^\"[:space:]#]*)\"?.*/\\1/p;q}")"
  printf '%s\n' "${value:-1}"
}

# compare_listing REPO BASE HEAD: prints the GitHub compare status of
# BASE...HEAD, the number of changed files, then each changed file name and the
# previous name of each renamed file, one per line.
compare_listing() {
  local json
  json="$(gh api "repos/$1/compare/$2...$3")" || return 1
  # A response without a files array lists nothing it can vouch for.
  jq -er 'if (.files | type) != "array" then error("no files") else
    .status, (.files | length), (.files[] | .filename, (.previous_filename // empty)) end' <<<"${json}"
}

# same_build_inputs REPO CANDIDATE SHA: succeeds when commit CANDIDATE has the
# same build inputs as SHA, the checked-out commit (R6.2). It runs as an if
# condition, where set -e does not apply, so every step checks its status.
same_build_inputs() {
  local repo="$1" candidate="$2" sha="$3"
  [[ "${candidate}" != "${sha}" ]] || return 0
  local listing status
  if ! listing="$(compare_listing "${repo}" "${candidate}" "${sha}")"; then
    log "cannot compare ${candidate} with ${sha}; its parts are not reused"
    return 1
  fi
  status="$(head -n 1 <<<"${listing}")"
  if [[ "${status}" == behind ]]; then
    # CANDIDATE is newer: list the changes from SHA to it instead.
    if ! listing="$(compare_listing "${repo}" "${sha}" "${candidate}")"; then
      log "cannot compare ${sha} with ${candidate}; its parts are not reused"
      return 1
    fi
    status="$(head -n 1 <<<"${listing}")"
  fi
  case "${status}" in
    identical) return 0 ;;
    ahead) ;;
    *)
      log "${candidate} and ${sha} are not in one line of history (compare: ${status}); its parts are not reused"
      return 1
      ;;
  esac
  local count
  count="$(sed -n 2p <<<"${listing}")"
  if [[ ! "${count}" =~ ^[0-9]+$ ]] || ((count >= compare_file_limit)); then
    log "compare of ${candidate} and ${sha} lists ${count} files, possibly truncated; its parts are not reused"
    return 1
  fi
  local file path changed=() workflow_changed=false
  while IFS= read -r file; do
    [[ -n "${file}" ]] || continue
    [[ "${file}" != ".github/workflows/${release_workflow}" ]] || workflow_changed=true
    for path in "${build_input_paths[@]}"; do
      if [[ "${file}" == "${path}" || ("${path}" == */ && "${file}" == "${path}"*) ]]; then
        changed+=("${file}")
        break
      fi
    done
  done < <(tail -n +3 <<<"${listing}")
  if [[ ${#changed[@]} -gt 0 ]]; then
    log "build inputs differ between ${candidate} and ${sha} (${#changed[@]} files, e.g. ${changed[0]}); its parts are not reused"
    return 1
  fi
  if [[ "${workflow_changed}" == true ]]; then
    local mine theirs content
    mine="$(build_recipe <"${repo_root}/.github/workflows/${release_workflow}")"
    if ! content="$(gh api -H "Accept: application/vnd.github.raw+json" \
      "repos/${repo}/contents/.github/workflows/${release_workflow}?ref=${candidate}")"; then
      log "cannot read the ${release_workflow} of ${candidate}; its parts are not reused"
      return 1
    fi
    theirs="$(build_recipe <<<"${content}")"
    if [[ "${theirs}" != "${mine}" ]]; then
      log "build recipe ${theirs} of ${candidate} is not ${mine}; its parts are not reused"
      return 1
    fi
  fi
}

# run_usable_parts REPO RUN_ID: prints "name<TAB>runner labels" of each part the
# run can give: an artifact that stays unexpired for reuse_expiry_margin more
# seconds, built by a "Build <target> <bundle>" job of the run's latest attempt
# that succeeded. Fails when a listing cannot be read.
run_usable_parts() {
  local repo="$1" id="$2" artifacts jobs
  artifacts="$(gh api "repos/${repo}/actions/runs/${id}/artifacts?per_page=100")" || return 1
  jobs="$(gh api "repos/${repo}/actions/runs/${id}/jobs?filter=latest&per_page=100")" || return 1
  jq -rn --argjson artifacts "${artifacts}" --argjson jobs "${jobs}" \
    --argjson margin "${reuse_expiry_margin}" '
    ([$artifacts.artifacts[]
      | select(.expired == false
          and ((.expires_at // "") | sub("\\.[0-9]+Z$"; "Z") | try fromdateiso8601 catch 0) > (now + $margin))
      | .name]) as $names
    | $jobs.jobs[]
    | select(.conclusion == "success")
    | (.name | capture("^Build (?<target>[^ ]+) (?<bundle>[^ ]+)$")) as $job
    | ($names[] | select(endswith("-\($job.target)-\($job.bundle)"))) as $name
    | [$name, (.labels | join(" "))] | @tsv
  '
}

# reusable_parts REPO SHA VERSION MATRIX: prints the JSON array of the parts of
# the bundle MATRIX found in earlier push runs with the same build inputs as SHA
# (R6.1), each {name, run_id, run_url} from the newest run that can give it:
# its build job ran on the part's runner of MATRIX and succeeded (R6.6). Both
# Linux parts of a target come from one run, since each embeds the digest of
# the bwrap that its own job built (R6.7). A failed lookup reuses less; it never
# stops the release.
reusable_parts() {
  local repo="$1" sha="$2" version="$3" matrix="$4"
  local -A runner_of=() groups=() verdict=()
  local name runner target group
  while IFS=$'\t' read -r name runner target; do
    runner_of["${name}"]="${runner}"
    group="${name}"
    [[ "${target}" != *-linux-* ]] || group="${target}"
    groups["${group}"]+="${groups[${group}]:+ }${name}"
  done < <(jq -r --arg v "${version}" \
    '.[] | ["codex-parts-\($v)-\(.target)-\(.bundle)", .runner, .target] | @tsv' <<<"${matrix}")

  local runs candidates
  # Runs of any status: a newer push cancels the older prebuild, whose finished
  # parts are final artifacts already (unfinished uploads are not listed).
  if ! runs="$(gh api "repos/${repo}/actions/workflows/${release_workflow}/runs?event=push&branch=${reuse_branch}&per_page=${reuse_run_limit}")" ||
    ! candidates="$(jq -r --arg path ".github/workflows/${release_workflow}" --arg branch "${reuse_branch}" '
      [.workflow_runs[]
        | select(.path == $path and .event == "push" and .head_branch == $branch)]
      | sort_by(.created_at) | reverse | .[] | "\(.id) \(.head_sha) \(.html_url)"
    ' <<<"${runs}")"; then
    log "cannot list the ${release_workflow} runs of ${repo}; every part is built"
    printf '[]\n'
    return 0
  fi

  local id candidate url usable labels part ok found=()
  while read -r id candidate url; do
    [[ ${#groups[@]} -gt 0 ]] || break
    [[ "${id}" =~ ^[0-9]+$ && "${candidate}" =~ ${sha_re} ]] || continue
    if [[ -z "${verdict[${candidate}]+set}" ]]; then
      verdict["${candidate}"]=different
      if same_build_inputs "${repo}" "${candidate}" "${sha}"; then
        verdict["${candidate}"]=same
      fi
    fi
    [[ "${verdict[${candidate}]}" == same ]] || continue
    if ! usable="$(run_usable_parts "${repo}" "${id}")"; then
      log "cannot list the artifacts or jobs of run ${id}; its parts are not reused"
      continue
    fi
    local -A labels_of=()
    while IFS=$'\t' read -r name labels; do
      [[ -z "${name}" ]] || labels_of["${name}"]="${labels}"
    done <<<"${usable}"
    for group in "${!groups[@]}"; do
      ok=true
      for part in ${groups[${group}]}; do
        if [[ -z "${labels_of[${part}]+set}" || " ${labels_of[${part}]} " != *" ${runner_of[${part}]} "* ]]; then
          ok=false
          break
        fi
      done
      [[ "${ok}" == true ]] || continue
      for part in ${groups[${group}]}; do
        found+=("$(jq -cn --arg name "${part}" --argjson id "${id}" --arg url "${url}" \
          '{name: $name, run_id: $id, run_url: $url}')")
      done
      unset 'groups[${group}]'
    done
    unset labels_of
  done <<<"${candidates}"
  if [[ ${#found[@]} -eq 0 ]]; then
    printf '[]\n'
  else
    printf '%s\n' "${found[@]}" | jq -cs 'sort_by(.name)'
  fi
}

cmd_fetch_parts() {
  local repo="" parts="" dir=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --repo) repo="${2:?--repo requires a value}"; shift 2 ;;
      --parts) parts="${2:?--parts requires a JSON array}"; shift 2 ;;
      --dir) dir="${2:?--dir requires a directory}"; shift 2 ;;
      *) die "unknown fetch-parts option: $1" ;;
    esac
  done
  [[ "${repo}" =~ ${repo_re} ]] || die "invalid repository '${repo}'"
  [[ -n "${dir}" ]] || die "--dir is required"
  local entries
  entries="$(jq -r '
    if type == "array"
      and all(.[]; type == "object"
        and (.name | type) == "string" and (.name | test("^codex-parts-[A-Za-z0-9._-]+$"))
        and (.run_id | type) == "number" and .run_id == (.run_id | floor) and .run_id > 0)
    then .[] | "\(.run_id) \(.name)"
    else error("invalid")
    end
  ' <<<"${parts}" 2>/dev/null)" ||
    die "--parts must be a JSON array of {name: codex-parts-..., run_id: N}, got '${parts}'"
  if [[ -z "${entries}" ]]; then
    log "no reused parts to fetch"
    return 0
  fi
  mkdir -p "${dir}"
  local id name tmp tars
  while read -r id name; do
    [[ -n "${id}" ]] || continue
    tmp="$(mktemp -d "${TMPDIR:-/tmp}/release-part.XXXXXX")"
    cleanup_paths+=("${tmp}")
    gh run download "${id}" --repo "${repo}" --name "${name}" --dir "${tmp}" ||
      die "cannot download part ${name} of run ${id}"
    tars="$(find "${tmp}" -type f -name "${name}.tar")"
    [[ -n "${tars}" && "$(wc -l <<<"${tars}")" -eq 1 ]] ||
      die "artifact ${name} of run ${id} does not hold exactly one ${name}.tar"
    mv -- "${tars}" "${dir}/${name}.tar"
    log "fetched ${name} from run ${id}"
  done <<<"${entries}"
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
  local mode=release version="" sha="" repo="" matrix="" output="" create_draft=true rebuild=false
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --mode) mode="${2:?--mode requires release or prebuild}"; shift 2 ;;
      --version) version="${2:?--version requires a value}"; shift 2 ;;
      --sha) sha="${2:?--sha requires a value}"; shift 2 ;;
      --repo) repo="${2:?--repo requires a value}"; shift 2 ;;
      --matrix) matrix="${2-}"; shift 2 ;;
      --create-draft) create_draft="${2:?--create-draft requires true or false}"; shift 2 ;;
      --rebuild) rebuild="${2:?--rebuild requires true or false}"; shift 2 ;;
      --output) output="${2:?--output requires a file}"; shift 2 ;;
      *) die "unknown preflight option: $1" ;;
    esac
  done
  [[ "${create_draft}" == true || "${create_draft}" == false ]] ||
    die "--create-draft must be true or false, got '${create_draft}'"
  [[ "${rebuild}" == true || "${rebuild}" == false ]] || die "--rebuild must be true or false, got '${rebuild}'"

  local workspace_version
  workspace_version="$(awk '
    /^\[workspace\.package\]/ { section = 1; next }
    /^\[/ { section = 0 }
    section && $1 == "version" { gsub(/"/, "", $3); print $3; exit }
  ' "${repo_root}/codex-rs/Cargo.toml")"
  case "${mode}" in
    release) ;;
    prebuild)
      # A push to codex/fork (R4.1): no tags, releases or fork-ci runs to look
      # at; with --repo only parts to reuse (R6.1).
      [[ -z "${matrix}" ]] || die "prebuild mode builds the default matrix; drop --matrix"
      [[ -n "${version}" ]] || version="${workspace_version}"
      ;;
    *) die "--mode must be release or prebuild, got '${mode}'" ;;
  esac
  [[ "${version}" =~ ${version_re} ]] || die "invalid release version '${version}'"
  [[ "${sha}" =~ ${sha_re} ]] || die "checkout_ref must be a full 40-character lowercase commit SHA, got '${sha}'"
  [[ -z "${repo}" || "${repo}" =~ ${repo_re} ]] || die "invalid repository '${repo}'"

  local head
  head="$(git -C "${repo_root}" rev-parse HEAD)"
  [[ "${head}" == "${sha}" ]] || die "checkout is at ${head}, not ${sha}"

  [[ "${workspace_version}" == "${version}" ]] ||
    die "codex-rs/Cargo.toml has workspace version '${workspace_version}', not '${version}'"

  [[ -n "${matrix}" ]] || matrix="${default_matrix}"
  # A target outside the release set is named first (R1.2); the check below
  # rejects every other malformed matrix.
  local unknown
  unknown="$(jq -r --argjson allowed "${runner_allowlist}" '
    [.[] | .target | strings | select($allowed[.] == null)] | unique | join(" ")
  ' <<<"${matrix}" 2>/dev/null)" || unknown=""
  [[ -z "${unknown}" ]] ||
    die "build_matrix names targets outside the release set: ${unknown} (release targets: $(all_targets | xargs))"
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
  # targets and full_matrix count targets (R5.3); the build runs per bundle.
  matrix="$(bundle_matrix "${matrix}")"

  local ci_run_url="" reused='[]'
  if [[ -n "${repo}" ]]; then
    command -v gh >/dev/null 2>&1 || die "gh is required with --repo"
  fi
  if [[ -n "${repo}" && "${mode}" == release ]]; then
    # R4.5: a build-only dispatch can rehearse a version that is out already.
    if [[ "${create_draft}" == true ]]; then
      check_tags "${repo}" "${version}" "${sha}"
      local err
      if err="$(gh api "repos/${repo}/releases/tags/rust-v${version}" --jq .html_url 2>&1)"; then
        die "rust-v${version} is already published: ${err}"
      elif [[ "${err}" != *"HTTP 404"* ]]; then
        die "cannot check for a published rust-v${version}: ${err}"
      fi
      log "no published release rust-v${version}"
    else
      log "create_draft is false: tags and releases of ${version} are not checked"
    fi

    # R3.1: fork-ci of this commit already passed on its push to codex/fork.
    local found
    found="$(find_run "${repo}" "${ci_workflow}" "${sha}" push "${reuse_branch}")"
    if [[ -n "${found}" ]]; then
      ci_run_url="${found#* }"
      log "fork-ci passed for ${sha} in ${ci_run_url}; the ci job is skipped"
    else
      log "no successful push run of ${ci_workflow} for ${sha}; the ci job runs fork-ci"
    fi
  fi

  # R6.1, R6.5: reuse the parts that earlier push runs built from the same inputs.
  if [[ -z "${repo}" ]]; then
    :
  elif [[ "${full_matrix}" != true ]]; then
    log "partial matrix: every bundle is built"
  elif [[ "${rebuild}" == true ]]; then
    log "rebuild is true: every bundle is built"
  else
    reused="$(reusable_parts "${repo}" "${sha}" "${version}" "${matrix}")"
    local reused_count total_count
    reused_count="$(jq length <<<"${reused}")"
    total_count="$(jq length <<<"${matrix}")"
    if [[ "${reused_count}" -eq 0 ]]; then
      log "no part to reuse: every bundle is built"
    else
      log "reusing ${reused_count} of ${total_count} parts:"
      jq -r '.[] | "    \(.name) from \(.run_url)"' <<<"${reused}" >&2
      matrix="$(jq -c --argjson reused "${reused}" --arg v "${version}" '
        ($reused | map(.name)) as $names
        | map(select(("codex-parts-\($v)-\(.target)-\(.bundle)") as $n | $names | index($n) | not))
      ' <<<"${matrix}")"
      if [[ "${reused_count}" -eq "${total_count}" ]]; then
        log "every part is reused; the build job is skipped"
      else
        log "building $(jq -r '[.[] | "\(.target) \(.bundle)"] | join(", ")' <<<"${matrix}")"
      fi
    fi
  fi
  local build=true reused_runs
  [[ "${matrix}" != "[]" ]] || build=false
  reused_runs="$(jq -r '[.[].run_url] | unique | join(" ")' <<<"${reused}")"

  local lines
  lines="$(printf 'version=%s\nsha=%s\nmatrix=%s\ntargets=%s\nfull_matrix=%s\nci_run_url=%s\nbuild=%s\nreused_parts=%s\nreused_runs=%s\n' \
    "${version}" "${sha}" "${matrix}" "${targets}" "${full_matrix}" \
    "${ci_run_url}" "${build}" "${reused}" "${reused_runs}")"
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
  local repo="" version="" sha="" dist="" run_url="" ci_run_url="" reused_runs="" dry_run=0
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --repo) repo="${2:?--repo requires a value}"; shift 2 ;;
      --version) version="${2:?--version requires a value}"; shift 2 ;;
      --sha) sha="${2:?--sha requires a value}"; shift 2 ;;
      --dist) dist="${2:?--dist requires a directory}"; shift 2 ;;
      --run-url) run_url="${2:?--run-url requires a value}"; shift 2 ;;
      # Empty when this run ran fork-ci or built the bundles itself.
      --ci-run-url) ci_run_url="${2-}"; shift 2 ;;
      --reused-runs) reused_runs="${2-}"; shift 2 ;;
      --dry-run) dry_run=1; shift ;;
      *) die "unknown draft option: $1" ;;
    esac
  done
  parse_release_target
  require_tool_features
  [[ -d "${dist}" ]] || die "missing dist directory ${dist}"
  dist="$(cd -- "${dist}" && pwd)"
  local tag="rust-v${version}"

  # The assets are exactly the packages of the release targets, listed in
  # SHA256SUMS, plus that file.
  [[ -f "${dist}/SHA256SUMS" ]] || die "missing ${dist}/SHA256SUMS"
  local expected listed actual
  expected="$(all_targets | while read -r target; do package_name "${target}"; done | LC_ALL=C sort)"
  listed="$(awk '{print $2}' "${dist}/SHA256SUMS" | LC_ALL=C sort)"
  [[ "${listed}" == "${expected}" ]] || die "SHA256SUMS does not list exactly the $(grep -c . <<<"${expected}") release packages
expected:
${expected}
listed:
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
    if [[ -n "${reused_runs}" ]]; then
      printf -- '- Bundle parts reused from prebuild runs with the same build inputs: %s\n' "${reused_runs// /, }"
    fi
    if [[ -n "${ci_run_url}" ]]; then
      printf -- '- fork-ci (push run of this commit): %s\n' "${ci_run_url}"
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

  local runs=""
  if [[ -n "${reused_runs}" ]]; then
    runs+=$'\n\n'"Bundle parts reused from prebuild runs ${reused_runs// /, }, assembled and verified in ${run_url:-this run}."
  fi
  if [[ -n "${ci_run_url}" ]]; then
    runs+=$'\n\n'"fork-ci passed for this commit in its push run ${ci_run_url}."
  fi
  local summary
  summary="$(cat <<EOF
### Draft release ${tag}

Draft: ${url} (id ${id}), target \`${sha}\`. Assets match SHA256SUMS (name, size, digest).${runs}

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
    provenance | preflight | find-run | fetch-parts | draft-check | draft)
      command -v jq >/dev/null 2>&1 || die "jq is required"
      case "${command}" in
        provenance)
          command -v gh >/dev/null 2>&1 || die "gh is required"
          cmd_provenance "$@"
          ;;
        preflight) cmd_preflight "$@" ;;
        find-run)
          command -v gh >/dev/null 2>&1 || die "gh is required"
          cmd_find_run "$@"
          ;;
        fetch-parts)
          command -v gh >/dev/null 2>&1 || die "gh is required"
          cmd_fetch_parts "$@"
          ;;
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
