#!/usr/bin/env bash
# Self-test for scripts/fork/release.sh and the fork workflow definitions.
#
#   scripts/fork/test-release.sh
#
# Runs preflight (release and prebuild modes) against this checkout (HEAD and
# codex-rs/Cargo.toml), find-run against fake workflow run and artifact
# listings, provenance against a small temporary git repository, and draft
# against a small fake dist. Every GitHub call goes to a stub `gh` placed first
# on PATH, so the test is hermetic and never changes GitHub. The last section
# checks the fork workflows statically.

# Written for bash 3.2 too, so an old bash reaches this message.
if ((BASH_VERSINFO[0] < 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] < 4))); then
  printf '%s: requires bash >= 4.4 (running %s)\n' "${0##*/}" "${BASH_VERSION}" >&2
  exit 1
fi

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
release="${repo_root}/scripts/fork/release.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/test-release.XXXXXX")"
trap 'rm -rf "${work}"' EXIT

# The stub and the fixtures use GNU-compatible cp -al and stat -c.
mkdir -p "${work}/probe"
echo x >"${work}/probe/f"
if ! cp -al "${work}/probe" "${work}/probe-link" 2>/dev/null ||
  [[ "$(stat -c %s "${work}/probe/f" 2>/dev/null)" != 2 ]]; then
  echo "${0##*/}: requires GNU-compatible coreutils (cp -al, stat -c): GNU or uutils, as on Linux" >&2
  exit 1
fi
rm -rf "${work}/probe" "${work}/probe-link"

head="$(git -C "${repo_root}" rev-parse HEAD)"
version="$(awk '
  /^\[workspace\.package\]/ { section = 1; next }
  /^\[/ { section = 0 }
  section && $1 == "version" { gsub(/"/, "", $3); print $3; exit }
' "${repo_root}/codex-rs/Cargo.toml")"
other_sha="$(printf '%040d' 0 | tr 0 a)"
repo="example/codex"
marker="Draft staged by fork-rust-release."

# Stub gh: answers the calls release.sh makes with the post---jq output, using
# files under $FAKE_GH_STATE. Releases are lines of
#   id <TAB> draft <TAB> published_at <TAB> target <TAB> marker <TAB> html_url
# in $FAKE_GH_STATE/releases. Workflow runs and run artifacts are raw API JSON
# (release.sh filters them with jq): runs-<workflow file> and artifacts-<run id>.
# Unknown calls fail loudly.
mkdir -p "${work}/bin"
cat >"${work}/bin/gh" <<'GH'
#!/usr/bin/env bash
set -euo pipefail
state="${FAKE_GH_STATE:?}"
args=" $* "
echo "gh$args" >>"${state}/calls"
if [[ -f "${state}/fail-api" && "${args}" == *" api "* ]]; then
  echo "gh: Server Error (HTTP 500)" >&2
  exit 1
fi
touch "${state}/releases"
case "${args}" in
  " api repos/"*"/actions/workflows/"*"/runs?"*)
    workflow="${args#*/actions/workflows/}"
    workflow="${workflow%%/runs\?*}"
    if [[ -f "${state}/runs-${workflow}" ]]; then
      cat "${state}/runs-${workflow}"
    else
      echo '{"total_count":0,"workflow_runs":[]}'
    fi
    ;;
  " api repos/"*"/actions/runs/"*"/artifacts?"*)
    id="${args#*/actions/runs/}"
    id="${id%%/artifacts*}"
    if [[ -f "${state}/artifacts-${id}" ]]; then
      cat "${state}/artifacts-${id}"
    else
      echo '{"total_count":0,"artifacts":[]}'
    fi
    ;;
  *"/git/matching-refs/tags/"*)
    tag="${args#*/git/matching-refs/tags/}"
    tag="${tag%% *}"
    file="${state}/ref-${tag//\//_}"
    if [[ -f "${file}" ]]; then cat "${file}"; fi
    ;;
  *"/git/tags/"*)
    object="${args#*/git/tags/}"
    cat "${state}/tagobj-${object%% *}"
    ;;
  *"/releases/tags/"*)
    if [[ -f "${state}/published" ]]; then
      echo "https://github.com/example/codex/releases/tag/published"
    else
      echo "gh: Not Found (HTTP 404)" >&2
      exit 1
    fi
    ;;
  *"/compare/"*)
    cat "${state}/compare-status" 2>/dev/null || echo ahead
    ;;
  *" api --paginate "*"/releases?per_page=100 "*)
    lag=0
    if [[ -f "${state}/list-lag" ]]; then lag="$(cat "${state}/list-lag")"; fi
    if [[ ${lag} -gt 0 ]]; then
      echo $((lag - 1)) >"${state}/list-lag"
      head -n -1 "${state}/releases"
    else
      cat "${state}/releases"
    fi
    ;;
  " api -X DELETE "*"/releases/"*)
    id="${args##*/releases/}"
    id="${id%% *}"
    awk -F '\t' -v id="${id}" '$1 != id' "${state}/releases" >"${state}/releases.new"
    mv "${state}/releases.new" "${state}/releases"
    ;;
  *"/releases/"[0-9]*" --jq .assets"*)
    id="${args#*/releases/}"
    id="${id%% *}"
    while IFS= read -r asset; do
      hash="$(sha256sum "${asset}" | cut -d' ' -f1)"
      if [[ -f "${state}/corrupt-digest" ]]; then hash="0${hash:1}"; fi
      printf '%s\t%s\tsha256:%s\tuploaded\n' "$(basename "${asset}")" "$(stat -c %s "${asset}")" "${hash}"
    done <"${state}/assets-${id}"
    ;;
  *"/releases/"[0-9]*)
    id="${args#*/releases/}"
    id="${id%% *}"
    if [[ -f "${state}/recheck-${id}" ]]; then
      cat "${state}/recheck-${id}"
    else
      awk -F '\t' -v id="${id}" -v OFS='\t' '$1 == id { print $2, $3, $4, $5 }' "${state}/releases"
    fi
    ;;
  " release create "*)
    shift 3 # release create TAG
    target="" assets=() notes=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --target) target="$2"; shift 2 ;;
        --notes-file) notes="$2"; shift 2 ;;
        --repo | --title) shift 2 ;;
        --draft) shift ;;
        *) assets+=("$1"); shift ;;
      esac
    done
    cp "${notes}" "${state}/notes"
    id=$((4242 + $(grep -c '^gh release create ' "${state}/calls")))
    has_marker=false
    if grep -qF 'Draft staged by fork-rust-release.' "${notes}"; then has_marker=true; fi
    url="https://github.com/example/codex/releases/tag/untagged-${id}"
    printf '%s\ttrue\tnull\t%s\t%s\t%s\n' "${id}" "${target}" "${has_marker}" "${url}" >>"${state}/releases"
    if [[ -f "${state}/fail-create" ]]; then
      printf '%s\n' "${assets[0]}" >"${state}/assets-${id}"
      echo "HTTP 502: upload of ${assets[1]##*/} failed" >&2
      exit 1
    fi
    printf '%s\n' "${assets[@]}" >"${state}/assets-${id}"
    echo "${url}"
    ;;
  *)
    echo "stub gh: unexpected call:${args}" >&2
    exit 99
    ;;
esac
GH
chmod 0755 "${work}/bin/gh"
export PATH="${work}/bin:${PATH}"
# Releases in the self-test become visible on the third listing at the latest.
export RELEASE_VISIBILITY_DELAYS="0 0 0"

passed=0
failures=0

# new_state: fresh stub state; prints its directory.
new_state() {
  local dir
  dir="$(mktemp -d "${work}/state.XXXXXX")"
  : >"${dir}/calls"
  printf '%s\n' "${dir}"
}

# release_line ID DRAFT PUBLISHED_AT TARGET MARKER: one stub release.
release_line() {
  printf '%s\t%s\t%s\t%s\t%s\thttps://github.com/example/codex/releases/tag/untagged-%s\n' "$@" "$1"
}

# check NAME ok|fail PATTERN COMMAND...: runs COMMAND, expects the status and an
# output line matching the extended regular expression PATTERN.
check() {
  local name="$1" want="$2" pattern="$3"
  shift 3
  local status=0
  "$@" >"${work}/${name}.log" 2>&1 || status=$?
  local got=ok
  [[ ${status} -eq 0 ]] || got=fail
  if [[ "${got}" == "${want}" ]] && grep -qE -- "${pattern}" "${work}/${name}.log"; then
    echo "ok   ${name}"
    passed=$((passed + 1))
  else
    echo "FAIL ${name}: status ${status}, want ${want} with /${pattern}/" >&2
    sed 's/^/    /' "${work}/${name}.log" >&2
    failures=$((failures + 1))
  fi
}

# expect NAME COMMAND...: records whether COMMAND succeeds.
expect() {
  local name="$1"
  shift
  if "$@"; then
    echo "ok   ${name}"
    passed=$((passed + 1))
  else
    echo "FAIL ${name}" >&2
    if [[ -n "${FAKE_GH_STATE:-}" && -f "${FAKE_GH_STATE}/calls" ]]; then
      sed 's/^/    /' "${FAKE_GH_STATE}/calls" >&2
    fi
    failures=$((failures + 1))
  fi
}

calls_match() { grep -qE -- "$1" "${FAKE_GH_STATE}/calls"; }
no_calls_match() { ! grep -qE -- "$1" "${FAKE_GH_STATE}/calls"; }

# --- preflight -------------------------------------------------------------

check preflight-short-sha fail 'full 40-character' \
  "${release}" preflight --version "${version}" --sha "${head:0:12}"
check preflight-bad-version fail 'invalid release version' \
  "${release}" preflight --version "v${version}" --sha "${head}"
check preflight-checkout-mismatch fail "checkout is at ${head}" \
  "${release}" preflight --version "${version}" --sha "${other_sha}"
check preflight-cargo-version fail "not '${version}-beta.1'" \
  "${release}" preflight --version "${version}-beta.1" --sha "${head}"
check preflight-offline ok '^full_matrix=true$' \
  "${release}" preflight --version "${version}" --sha "${head}" --output "${work}/github-output"
expect preflight-output-file \
  grep -qx "sha=${head}" "${work}/github-output"
expect preflight-output-matrix \
  grep -q '^matrix=\[{"runner":"ubuntu-24.04","target":"x86_64-unknown-linux-gnu","bundle":"primary","binaries":"codex codex-responses-api-proxy bwrap"}' "${work}/github-output"

# output_value FILE KEY: the value of KEY= in a preflight output.
output_value() { sed -n "s/^$2=//p" "$1"; }

# R1.1: one matrix entry per target and bundle, with the bundle's binaries;
# targets and full_matrix still count targets.
bundle_binaries='{
  "primary": {"linux": "codex codex-responses-api-proxy bwrap",
              "windows": "codex codex-responses-api-proxy codex-command-runner codex-windows-sandbox-setup",
              "apple": "codex codex-responses-api-proxy"},
  "app-server": {"linux": "codex-app-server codex-code-mode-host",
                 "windows": "codex-app-server codex-code-mode-host",
                 "apple": "codex-app-server codex-code-mode-host"}
}'
# bundle_matrix_ok MATRIX COUNT: COUNT entries, each target twice (primary, then
# app-server, in target order), each with the binaries of the bundles table.
bundle_matrix_ok() {
  jq -e --argjson want "${bundle_binaries}" --argjson count "$2" '
    length == $count
    and ([.[] | .bundle] == ([range($count / 2)] | map("primary", "app-server")))
    and ([.[] | .target] | [range(0; length; 2) as $i | .[$i] == .[$i + 1]] | all)
    and all(.[]; keys == ["binaries", "bundle", "runner", "target"]
      and .binaries == $want[.bundle][.target | if test("linux") then "linux"
        elif test("windows") then "windows" else "apple" end])
  ' <<<"$1" >/dev/null
}
expect preflight-bundle-matrix-full \
  bundle_matrix_ok "$(output_value "${work}/github-output" matrix)" 10
expect preflight-targets-per-target test \
  "$(output_value "${work}/github-output" targets)" = \
  "x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin x86_64-pc-windows-msvc"
expect preflight-offline-no-reuse test -z \
  "$(output_value "${work}/github-output" ci_run_url)$(output_value "${work}/github-output" prebuilt_run_id)"

check preflight-partial-matrix ok '^full_matrix=false$' \
  "${release}" preflight --version "${version}" --sha "${head}" \
  --matrix '[{"runner":"ubuntu-24.04-arm","target":"aarch64-unknown-linux-gnu"}]' --output "${work}/partial-output"
expect preflight-bundle-matrix-partial \
  bundle_matrix_ok "$(output_value "${work}/partial-output" matrix)" 2
expect preflight-partial-targets test "$(output_value "${work}/partial-output" targets)" = aarch64-unknown-linux-gnu
check preflight-bad-mode fail '--mode must be release or prebuild' \
  "${release}" preflight --mode nightly --version "${version}" --sha "${head}"
check preflight-bad-create-draft fail '--create-draft must be true or false' \
  "${release}" preflight --version "${version}" --sha "${head}" --create-draft yes
check preflight-unknown-target fail 'build_matrix must be' \
  "${release}" preflight --version "${version}" --sha "${head}" \
  --matrix '[{"runner":"ubuntu-24.04","target":"riscv64gc-unknown-linux-gnu"}]'
check preflight-duplicate-target fail 'build_matrix must be' \
  "${release}" preflight --version "${version}" --sha "${head}" \
  --matrix '[{"runner":"macos-15-intel","target":"x86_64-apple-darwin"},{"runner":"macos-15-intel","target":"x86_64-apple-darwin"}]'
check preflight-self-hosted-runner fail 'runner allowlist' \
  "${release}" preflight --version "${version}" --sha "${head}" \
  --matrix '[{"runner":"self-hosted","target":"x86_64-unknown-linux-gnu"}]'
check preflight-runner-other-os fail 'runner allowlist' \
  "${release}" preflight --version "${version}" --sha "${head}" \
  --matrix '[{"runner":"macos-15","target":"x86_64-unknown-linux-gnu"}]'
check preflight-allowed-alternative-runner ok '^full_matrix=false$' \
  "${release}" preflight --version "${version}" --sha "${head}" \
  --matrix '[{"runner":"ubuntu-22.04","target":"x86_64-unknown-linux-gnu"}]'

FAKE_GH_STATE="$(new_state)"
export FAKE_GH_STATE
check preflight-github-clean ok 'no published release' \
  "${release}" preflight --version "${version}" --sha "${head}" --repo "${repo}"

FAKE_GH_STATE="$(new_state)"
printf 'tag 1111111111111111111111111111111111111111\n' >"${FAKE_GH_STATE}/ref-rust-v${version}"
printf 'commit %s\n' "${head}" >"${FAKE_GH_STATE}/tagobj-1111111111111111111111111111111111111111"
check preflight-annotated-tag-at-sha ok "tag rust-v${version}: already points at ${head}" \
  "${release}" preflight --version "${version}" --sha "${head}" --repo "${repo}"

FAKE_GH_STATE="$(new_state)"
printf 'commit %s\n' "${other_sha}" >"${FAKE_GH_STATE}/ref-sdk_go_v${version}"
check preflight-go-tag-elsewhere fail "tag sdk/go/v${version} points at ${other_sha}" \
  "${release}" preflight --version "${version}" --sha "${head}" --repo "${repo}"

FAKE_GH_STATE="$(new_state)"
touch "${FAKE_GH_STATE}/published"
check preflight-published fail "rust-v${version} is already published" \
  "${release}" preflight --version "${version}" --sha "${head}" --repo "${repo}"

FAKE_GH_STATE="$(new_state)"
touch "${FAKE_GH_STATE}/fail-api"
check preflight-api-error fail "cannot look up tag rust-v${version}" \
  "${release}" preflight --version "${version}" --sha "${head}" --repo "${repo}"

# --- prebuild mode (R4.1) and create_draft=false (R4.5) ----------------------

FAKE_GH_STATE="$(new_state)"
check preflight-prebuild ok "^version=${version//./\\.}$" \
  "${release}" preflight --mode prebuild --sha "${head}" --output "${work}/prebuild-output"
expect preflight-prebuild-no-gh-calls test ! -s "${FAKE_GH_STATE}/calls"
expect preflight-prebuild-full-matrix grep -qx 'full_matrix=true' "${work}/prebuild-output"
expect preflight-prebuild-bundles bundle_matrix_ok "$(output_value "${work}/prebuild-output" matrix)" 10
expect preflight-prebuild-no-reuse test -z \
  "$(output_value "${work}/prebuild-output" ci_run_url)$(output_value "${work}/prebuild-output" prebuilt_run_id)"
check preflight-prebuild-no-repo fail 'prebuild mode makes no GitHub calls' \
  "${release}" preflight --mode prebuild --sha "${head}" --repo "${repo}"
check preflight-prebuild-no-matrix fail 'prebuild mode builds the default matrix' \
  "${release}" preflight --mode prebuild --sha "${head}" \
  --matrix '[{"runner":"ubuntu-24.04","target":"x86_64-unknown-linux-gnu"}]'
check preflight-prebuild-version-mismatch fail "not '${version}-beta.1'" \
  "${release}" preflight --mode prebuild --sha "${head}" --version "${version}-beta.1"
check preflight-prebuild-checkout-mismatch fail "checkout is at ${head}" \
  "${release}" preflight --mode prebuild --sha "${other_sha}"
expect preflight-prebuild-never-called-gh test ! -s "${FAKE_GH_STATE}/calls"

# A released version: tags at the release commit, rust-vX published.
released_state() {
  local dir
  dir="$(new_state)"
  touch "${dir}/published"
  printf 'commit %s\n' "${head}" >"${dir}/ref-rust-v${version}"
  printf 'commit %s\n' "${head}" >"${dir}/ref-sdk_go_v${version}"
  printf '%s\n' "${dir}"
}
FAKE_GH_STATE="$(released_state)"
check preflight-rehearsal-of-released ok 'create_draft is false: tags and releases' \
  "${release}" preflight --version "${version}" --sha "${head}" --repo "${repo}" --create-draft false
expect preflight-rehearsal-skips-tags-and-releases no_calls_match 'matching-refs|/releases'
FAKE_GH_STATE="$(released_state)"
check preflight-draft-of-released fail "rust-v${version} is already published" \
  "${release}" preflight --version "${version}" --sha "${head}" --repo "${repo}" --create-draft true

# --- find-run and reuse (R3.1, R3.2, R4.3, R4.4) ---------------------------

ci_path=.github/workflows/fork-ci.yml
release_path=.github/workflows/fork-rust-release.yml
runs_url=https://github.com/example/codex/actions/runs

# run_json ID PATH EVENT BRANCH SHA CONCLUSION CREATED_AT: one workflow run.
run_json() {
  jq -cn --argjson id "$1" --arg path "$2" --arg event "$3" --arg branch "$4" --arg sha "$5" \
    --arg conclusion "$6" --arg created "$7" --arg url "${runs_url}/$1" '{
      id: $id, path: $path, event: $event, head_branch: $branch, head_sha: $sha,
      status: "completed", conclusion: $conclusion, created_at: $created, html_url: $url}'
}
# runs_listing RUN_JSON...: a workflow runs API response.
runs_listing() { printf '%s\n' "$@" | jq -cs '{total_count: length, workflow_runs: .}'; }
# parts_listing [DROP [EXPIRED]]: a run artifacts API response with the ten
# parts of ${version} (without DROP, with EXPIRED expired) and other artifacts.
parts_listing() {
  local target bundle
  {
    for target in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin \
      aarch64-apple-darwin x86_64-pc-windows-msvc; do
      for bundle in primary app-server; do
        printf 'codex-parts-%s-%s-%s\n' "${version}" "${target}" "${bundle}"
        printf 'codex-timings-%s-%s-%s\n' "${version}" "${target}" "${bundle}"
      done
    done
    printf 'codex-%s-packages\ncodex-%s-SHA256SUMS\n' "${version}" "${version}"
  } | jq -Rn --arg drop "${1-}" --arg expired "${2-}" '
    [inputs | select(. != $drop) | {name: ., expired: (. == $expired)}]
    | {total_count: length, artifacts: .}'
}
# finds_nothing WORKFLOW [find-run options...]: find-run succeeds without a run.
finds_nothing() {
  local workflow="$1" out
  shift
  out="$("${release}" find-run --repo "${repo}" --workflow "${workflow}" --sha "${head}" \
    --event push --branch codex/fork "$@" 2>/dev/null)" && [[ -z "${out}" ]]
}
find_ci=("${release}" find-run --repo "${repo}" --workflow fork-ci.yml --sha "${head}" --event push --branch codex/fork)
find_prebuild=("${release}" find-run --repo "${repo}" --workflow fork-rust-release.yml --sha "${head}"
  --event push --branch codex/fork --parts-version "${version}")
x86_part="codex-parts-${version}-x86_64-apple-darwin-primary"

FAKE_GH_STATE="$(new_state)"
runs_listing "$(run_json 101 "${ci_path}" push codex/fork "${head}" success 2026-10-28T01:00:00Z)" \
  >"${FAKE_GH_STATE}/runs-fork-ci.yml"
check find-run-ci-push-success ok "^101 ${runs_url}/101$" "${find_ci[@]}"
expect find-run-ci-query calls_match \
  "^gh api repos/${repo}/actions/workflows/fork-ci.yml/runs\?head_sha=${head}&event=push&branch=codex/fork&status=success&per_page=100 $"

for spec in "other-sha 102 push codex/fork ${other_sha} success" \
  "failure 103 push codex/fork ${head} failure" \
  "dispatch-event 104 workflow_dispatch codex/fork ${head} success" \
  "other-branch 105 push main ${head} success"; do
  read -r name id event branch sha conclusion <<<"${spec}"
  FAKE_GH_STATE="$(new_state)"
  runs_listing "$(run_json "${id}" "${ci_path}" "${event}" "${branch}" "${sha}" "${conclusion}" 2026-10-28T01:00:00Z)" \
    >"${FAKE_GH_STATE}/runs-fork-ci.yml"
  expect "find-run-ci-${name}-ignored" finds_nothing fork-ci.yml
done

FAKE_GH_STATE="$(new_state)"
runs_listing "$(run_json 106 "${ci_path}" push codex/fork "${head}" success 2026-10-28T01:00:00Z)" \
  "$(run_json 107 "${ci_path}" push codex/fork "${head}" success 2026-10-28T03:00:00Z)" \
  >"${FAKE_GH_STATE}/runs-fork-ci.yml"
check find-run-newest ok "^107 ${runs_url}/107$" "${find_ci[@]}"

FAKE_GH_STATE="$(new_state)"
runs_listing "$(run_json 201 "${release_path}" push codex/fork "${head}" success 2026-10-28T01:00:00Z)" \
  >"${FAKE_GH_STATE}/runs-fork-rust-release.yml"
parts_listing >"${FAKE_GH_STATE}/artifacts-201"
check find-run-prebuild-ten-parts ok "^201 ${runs_url}/201$" "${find_prebuild[@]}"

FAKE_GH_STATE="$(new_state)"
runs_listing "$(run_json 202 "${release_path}" push codex/fork "${head}" success 2026-10-28T01:00:00Z)" \
  >"${FAKE_GH_STATE}/runs-fork-rust-release.yml"
parts_listing "${x86_part}" >"${FAKE_GH_STATE}/artifacts-202"
expect find-run-prebuild-nine-parts finds_nothing fork-rust-release.yml --parts-version "${version}"

FAKE_GH_STATE="$(new_state)"
runs_listing "$(run_json 203 "${release_path}" push codex/fork "${head}" success 2026-10-28T01:00:00Z)" \
  >"${FAKE_GH_STATE}/runs-fork-rust-release.yml"
parts_listing "" "${x86_part}" >"${FAKE_GH_STATE}/artifacts-203"
expect find-run-prebuild-expired-part finds_nothing fork-rust-release.yml --parts-version "${version}"

FAKE_GH_STATE="$(new_state)"
runs_listing "$(run_json 204 .github/workflows/rust-release.yml push codex/fork "${head}" success 2026-10-28T01:00:00Z)" \
  >"${FAKE_GH_STATE}/runs-fork-rust-release.yml"
parts_listing >"${FAKE_GH_STATE}/artifacts-204"
expect find-run-prebuild-other-workflow finds_nothing fork-rust-release.yml --parts-version "${version}"

FAKE_GH_STATE="$(new_state)"
runs_listing "$(run_json 205 "${release_path}" push codex/fork "${head}" success 2026-10-28T03:00:00Z)" \
  "$(run_json 206 "${release_path}" push codex/fork "${head}" success 2026-10-28T01:00:00Z)" \
  >"${FAKE_GH_STATE}/runs-fork-rust-release.yml"
parts_listing "${x86_part}" >"${FAKE_GH_STATE}/artifacts-205"
parts_listing >"${FAKE_GH_STATE}/artifacts-206"
check find-run-prebuild-older-complete ok "^206 ${runs_url}/206$" "${find_prebuild[@]}"

FAKE_GH_STATE="$(new_state)"
touch "${FAKE_GH_STATE}/fail-api"
check find-run-api-error fail 'cannot list the fork-ci.yml runs' "${find_ci[@]}"
check find-run-bad-workflow fail 'invalid workflow file' \
  "${release}" find-run --repo "${repo}" --workflow ../x --sha "${head}" --event push --branch codex/fork

# reuse_state: a successful fork-ci push run 301 and a prebuild run 302 with
# every part, both for HEAD.
reuse_state() {
  local dir
  dir="$(new_state)"
  runs_listing "$(run_json 301 "${ci_path}" push codex/fork "${head}" success 2026-10-28T01:00:00Z)" \
    >"${dir}/runs-fork-ci.yml"
  runs_listing "$(run_json 302 "${release_path}" push codex/fork "${head}" success 2026-10-28T01:00:00Z)" \
    >"${dir}/runs-fork-rust-release.yml"
  parts_listing >"${dir}/artifacts-302"
  printf '%s\n' "${dir}"
}
FAKE_GH_STATE="$(reuse_state)"
check preflight-reuses-runs ok 'reusing the parts of prebuild run' \
  "${release}" preflight --version "${version}" --sha "${head}" --repo "${repo}" --output "${work}/reuse-output"
expect preflight-reuse-ci-url grep -qx "ci_run_url=${runs_url}/301" "${work}/reuse-output"
expect preflight-reuse-prebuilt-id grep -qx 'prebuilt_run_id=302' "${work}/reuse-output"
expect preflight-reuse-prebuilt-url grep -qx "prebuilt_run_url=${runs_url}/302" "${work}/reuse-output"

FAKE_GH_STATE="$(reuse_state)"
check preflight-rebuild ok 'rebuild is true' \
  "${release}" preflight --version "${version}" --sha "${head}" --repo "${repo}" --rebuild true \
  --output "${work}/rebuild-output"
expect preflight-rebuild-no-prebuild grep -qx 'prebuilt_run_id=' "${work}/rebuild-output"
expect preflight-rebuild-no-lookup no_calls_match 'fork-rust-release.yml/runs|/artifacts'
expect preflight-rebuild-keeps-ci grep -qx "ci_run_url=${runs_url}/301" "${work}/rebuild-output"

FAKE_GH_STATE="$(reuse_state)"
check preflight-partial-builds ok 'partial matrix: every bundle is built' \
  "${release}" preflight --version "${version}" --sha "${head}" --repo "${repo}" \
  --matrix '[{"runner":"ubuntu-24.04","target":"x86_64-unknown-linux-gnu"}]' --output "${work}/partial-reuse-output"
expect preflight-partial-no-prebuild grep -qx 'prebuilt_run_id=' "${work}/partial-reuse-output"
expect preflight-partial-no-lookup no_calls_match 'fork-rust-release.yml/runs|/artifacts'

FAKE_GH_STATE="$(new_state)"
check preflight-nothing-to-reuse ok 'the ci job runs fork-ci' \
  "${release}" preflight --version "${version}" --sha "${head}" --repo "${repo}" --output "${work}/fresh-output"
expect preflight-nothing-to-reuse-outputs test \
  "$(grep -E '^(ci_run_url|prebuilt_run_id|prebuilt_run_url)=' "${work}/fresh-output" | tr '\n' ' ')" = \
  "ci_run_url= prebuilt_run_id= prebuilt_run_url= "

# --- provenance ------------------------------------------------------------

# A repository whose commits a -> b change only code and b -> c change release
# machinery, so a and b share machinery and a and c do not.
prov="${work}/prov"
prov_git() {
  GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
    git -C "${prov}" -c user.name=test -c user.email=test@example.com -c commit.gpgsign=false "$@"
}
mkdir -p "${prov}/scripts/fork" "${prov}/codex-rs"
prov_git init -q
echo one >"${prov}/codex-rs/lib.rs"
echo one >"${prov}/scripts/fork/release.sh"
prov_git add -A
prov_git commit -q -m a
prov_a="$(prov_git rev-parse HEAD)"
echo two >"${prov}/codex-rs/lib.rs"
prov_git commit -q -am b
prov_b="$(prov_git rev-parse HEAD)"
echo two >"${prov}/scripts/fork/release.sh"
prov_git commit -q -am c
prov_c="$(prov_git rev-parse HEAD)"
provenance=("${release}" provenance --repo "${repo}" --repo-dir "${prov}")

FAKE_GH_STATE="$(new_state)"
check provenance-same-commit ok 'is the dispatched commit' "${provenance[@]}" --sha "${prov_c}" --base "${prov_c}"
expect provenance-same-commit-no-api no_calls_match ' api '

FAKE_GH_STATE="$(new_state)"
check provenance-ancestor-same-machinery ok 'release machinery matches' \
  "${provenance[@]}" --sha "${prov_a}" --base "${prov_b}"
expect provenance-compares-sha-with-base calls_match "/compare/${prov_a}\.\.\.${prov_b} "

FAKE_GH_STATE="$(new_state)"
check provenance-machinery-differs fail 'scripts/fork/release.sh' \
  "${provenance[@]}" --sha "${prov_a}" --base "${prov_c}"

FAKE_GH_STATE="$(new_state)"
echo diverged >"${FAKE_GH_STATE}/compare-status"
check provenance-diverged fail 'not an ancestor' "${provenance[@]}" --sha "${prov_a}" --base "${prov_b}"

FAKE_GH_STATE="$(new_state)"
echo behind >"${FAKE_GH_STATE}/compare-status"
check provenance-behind fail 'not an ancestor' "${provenance[@]}" --sha "${prov_b}" --base "${prov_a}"

FAKE_GH_STATE="$(new_state)"
touch "${FAKE_GH_STATE}/fail-api"
check provenance-api-error fail 'cannot compare' "${provenance[@]}" --sha "${prov_a}" --base "${prov_b}"

check provenance-bad-base fail 'invalid dispatched commit' "${provenance[@]}" --sha "${prov_a}" --base main

# --- draft ---------------------------------------------------------------

dist="${work}/dist"
mkdir -p "${dist}"
for target in aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu; do
  echo "package ${target}" >"${dist}/codex-package-${target}.tar.gz"
done
echo "package windows" >"${dist}/codex-package-x86_64-pc-windows-msvc.zip"
(cd "${dist}" && sha256sum codex-package-* >SHA256SUMS)

# variant_dist NAME: hard-linked copy of the good dist.
variant_dist() {
  cp -al "${dist}" "${work}/dist-$1"
  printf '%s\n' "${work}/dist-$1"
}

draft=("${release}" draft --repo "${repo}" --version "${version}" --sha "${head}")
draft_check=("${release}" draft-check --repo "${repo}" --version "${version}" --sha "${head}")

FAKE_GH_STATE="$(new_state)"
check draft-dry-run ok 'gh release create rust-v' "${draft[@]}" --dist "${dist}" --dry-run
expect draft-dry-run-no-mutation no_calls_match 'release create|-X DELETE'

FAKE_GH_STATE="$(new_state)"
check draft-create ok 'Publication stays manual' \
  "${draft[@]}" --dist "${dist}" --run-url https://github.com/example/codex/actions/runs/1
expect draft-create-arguments calls_match "^gh release create rust-v${version} --repo ${repo} --draft --target ${head} "
expect draft-create-six-assets test "$(wc -l <"${FAKE_GH_STATE}/assets-4243")" -eq 6
expect draft-create-no-tags-or-publish no_calls_match 'git tag|-F draft=false|-X DELETE|-X PATCH'
expect draft-notes-carry-marker grep -qF "${marker}" "${FAKE_GH_STATE}/notes"
expect draft-summary-uses-created-id grep -q 'releases/4243 -F draft=false' "${work}/draft-create.log"

# R3.1, UC4: the notes and the summary name the reused runs.
FAKE_GH_STATE="$(new_state)"
check draft-names-reused-runs ok "fork-ci passed for this commit in its push run ${runs_url}/301" \
  "${draft[@]}" --dist "${dist}" --run-url "${runs_url}/1" \
  --ci-run-url "${runs_url}/301" --parts-run-url "${runs_url}/302"
expect draft-summary-names-prebuild grep -qF \
  "Bundles built by the prebuild run ${runs_url}/302, assembled and verified in ${runs_url}/1." "${work}/draft-names-reused-runs.log"
expect draft-notes-name-prebuild grep -qxF -- "- Bundles built by the prebuild run: ${runs_url}/302" "${FAKE_GH_STATE}/notes"
expect draft-notes-name-ci grep -qxF -- "- fork-ci (push run of this commit): ${runs_url}/301" "${FAKE_GH_STATE}/notes"
FAKE_GH_STATE="$(new_state)"
check draft-empty-run-urls ok 'Publication stays manual' \
  "${draft[@]}" --dist "${dist}" --run-url "${runs_url}/1" --ci-run-url "" --parts-run-url ""
expect draft-notes-without-reuse bash -c "! grep -qE 'prebuild run|push run of this commit' '${FAKE_GH_STATE}/notes'"

FAKE_GH_STATE="$(new_state)"
release_line 77 false 2026-10-01T00:00:00Z "${head}" false >"${FAKE_GH_STATE}/releases"
check draft-published-release fail "rust-v${version} is published" "${draft[@]}" --dist "${dist}"
expect draft-published-untouched no_calls_match '-X DELETE|release create'

FAKE_GH_STATE="$(new_state)"
release_line 78 true null "${head}" false >"${FAKE_GH_STATE}/releases"
check draft-foreign-draft fail 'not a resumable fork-rust-release draft' "${draft[@]}" --dist "${dist}"
expect draft-foreign-draft-untouched no_calls_match '-X DELETE|release create'

FAKE_GH_STATE="$(new_state)"
release_line 79 true null "${head}" true >"${FAKE_GH_STATE}/releases"
release_line 80 true null "${head}" false >>"${FAKE_GH_STATE}/releases"
check draft-resumable-and-foreign fail 'not a resumable fork-rust-release draft' "${draft[@]}" --dist "${dist}"
expect draft-resumable-and-foreign-untouched no_calls_match '-X DELETE|release create'

FAKE_GH_STATE="$(new_state)"
release_line 81 true null "${other_sha}" true >"${FAKE_GH_STATE}/releases"
check draft-other-sha-draft fail 'not a resumable fork-rust-release draft' "${draft[@]}" --dist "${dist}"

FAKE_GH_STATE="$(new_state)"
release_line 82 true 2026-10-01T00:00:00Z "${head}" true >"${FAKE_GH_STATE}/releases"
check draft-once-published-draft fail "was published" "${draft[@]}" --dist "${dist}"
expect draft-once-published-untouched no_calls_match '-X DELETE|release create'

FAKE_GH_STATE="$(new_state)"
release_line 83 true null "${head}" true >"${FAKE_GH_STATE}/releases"
check draft-replaces-resumable ok 'Publication stays manual' "${draft[@]}" --dist "${dist}"
expect draft-deletes-only-resumable calls_match '^gh api -X DELETE repos/example/codex/releases/83 $'
expect draft-deletes-once test "$(grep -c -- '-X DELETE' "${FAKE_GH_STATE}/calls")" -eq 1

FAKE_GH_STATE="$(new_state)"
release_line 84 true null "${head}" true >"${FAKE_GH_STATE}/releases"
printf 'false\t2026-10-01T00:00:00Z\t%s\ttrue\n' "${head}" >"${FAKE_GH_STATE}/recheck-84"
check draft-published-before-delete fail 'is no longer a resumable draft' "${draft[@]}" --dist "${dist}"
expect draft-published-before-delete-untouched no_calls_match '-X DELETE|release create'

FAKE_GH_STATE="$(new_state)"
release_line 85 true null "${head}" true >"${FAKE_GH_STATE}/releases"
check draft-dry-run-resumable ok 'would delete resumable draft 85' "${draft[@]}" --dist "${dist}" --dry-run
expect draft-dry-run-resumable-no-mutation no_calls_match 'release create|-X DELETE'

# Partial failure: the upload breaks after the draft exists, then a re-run on
# the same GitHub state recovers.
FAKE_GH_STATE="$(new_state)"
notes_tmp="${work}/notes-tmp"
mkdir -p "${notes_tmp}"
touch "${FAKE_GH_STATE}/fail-create"
TMPDIR="${notes_tmp}" check draft-partial-upload fail 'HTTP 502' "${draft[@]}" --dist "${dist}"
expect draft-partial-upload-left-draft grep -q $'^4243\ttrue\tnull\t' "${FAKE_GH_STATE}/releases"
expect draft-notes-removed-on-failure test -z "$(find "${notes_tmp}" -mindepth 1 -print -quit)"
rm "${FAKE_GH_STATE}/fail-create"
TMPDIR="${notes_tmp}" check draft-rerun-recovers ok 'Publication stays manual' "${draft[@]}" --dist "${dist}"
expect draft-rerun-deleted-partial calls_match '^gh api -X DELETE repos/example/codex/releases/4243 $'
expect draft-rerun-one-release test "$(grep -c . "${FAKE_GH_STATE}/releases")" -eq 1
expect draft-notes-removed-on-success test -z "$(find "${notes_tmp}" -mindepth 1 -print -quit)"

FAKE_GH_STATE="$(new_state)"
echo 2 >"${FAKE_GH_STATE}/list-lag"
check draft-visibility-lag ok 'Publication stays manual' "${draft[@]}" --dist "${dist}"

FAKE_GH_STATE="$(new_state)"
echo 9 >"${FAKE_GH_STATE}/list-lag"
check draft-never-visible fail 'is not visible.*re-run' "${draft[@]}" --dist "${dist}"

FAKE_GH_STATE="$(new_state)"
touch "${FAKE_GH_STATE}/corrupt-digest"
check draft-digest-mismatch fail 'uploaded assets differ' "${draft[@]}" --dist "${dist}"

FAKE_GH_STATE="$(new_state)"
printf 'commit %s\n' "${other_sha}" >"${FAKE_GH_STATE}/ref-rust-v${version}"
check draft-tag-moved fail "tag rust-v${version} points at ${other_sha}" "${draft[@]}" --dist "${dist}"

FAKE_GH_STATE="$(new_state)"
bad="$(variant_dist tampered)"
rm "${bad}/codex-package-x86_64-apple-darwin.tar.gz"
echo tampered >"${bad}/codex-package-x86_64-apple-darwin.tar.gz"
check draft-tampered-package fail 'packages do not match SHA256SUMS' "${draft[@]}" --dist "${bad}"

FAKE_GH_STATE="$(new_state)"
bad="$(variant_dist missing)"
rm "${bad}/codex-package-x86_64-pc-windows-msvc.zip" "${bad}/SHA256SUMS"
(cd "${bad}" && sha256sum codex-package-* >SHA256SUMS)
check draft-partial-set fail 'does not list exactly the five release packages' "${draft[@]}" --dist "${bad}"

FAKE_GH_STATE="$(new_state)"
bad="$(variant_dist extra)"
echo notes >"${bad}/notes.md"
check draft-extra-file fail 'unexpected files' "${draft[@]}" --dist "${bad}"

# GNU-compatible stat -c and find -printf are required; BSD-like tools that
# reject those options are named up front instead of failing midway.
bsd_tool() {
  local tool="$1" option="$2" dir="$3" real
  real="$(command -v "${tool}")"
  mkdir -p "${dir}"
  cat >"${dir}/${tool}" <<STUB
#!/usr/bin/env bash
for arg in "\$@"; do
  if [[ "\${arg}" == ${option} ]]; then echo "${tool}: illegal option -- ${option#-}" >&2; exit 1; fi
done
exec "${real}" "\$@"
STUB
  chmod 0755 "${dir}/${tool}"
}
bsd_tool stat -c "${work}/bsd-stat"
bsd_tool find -printf "${work}/bsd-find"
FAKE_GH_STATE="$(new_state)"
PATH="${work}/bsd-stat:${PATH}" check draft-requires-stat-c fail 'requires GNU-compatible coreutils.*stat -c' \
  "${draft[@]}" --dist "${dist}" --dry-run
PATH="${work}/bsd-find:${PATH}" check draft-requires-find-printf fail 'requires GNU-compatible findutils.*find -printf' \
  "${draft[@]}" --dist "${dist}" --dry-run

# --- draft-check (preflight with a token that sees drafts) ------------------

FAKE_GH_STATE="$(new_state)"
check draft-check-clean ok "no release for rust-v${version}" "${draft_check[@]}"

FAKE_GH_STATE="$(new_state)"
release_line 90 true null "${head}" true >"${FAKE_GH_STATE}/releases"
check draft-check-resumable ok 'draft job will replace resumable draft 90' "${draft_check[@]}"

FAKE_GH_STATE="$(new_state)"
release_line 91 false 2026-10-01T00:00:00Z "${head}" false >"${FAKE_GH_STATE}/releases"
check draft-check-published fail "rust-v${version} is published" "${draft_check[@]}"

FAKE_GH_STATE="$(new_state)"
release_line 92 true null "${head}" false >"${FAKE_GH_STATE}/releases"
check draft-check-foreign fail 'not a resumable fork-rust-release draft' "${draft_check[@]}"

FAKE_GH_STATE="$(new_state)"
release_line 93 true null "${other_sha}" true >"${FAKE_GH_STATE}/releases"
check draft-check-other-sha fail 'not a resumable fork-rust-release draft' "${draft_check[@]}"

FAKE_GH_STATE="$(new_state)"
release_line 94 true 2026-10-01T00:00:00Z "${head}" true >"${FAKE_GH_STATE}/releases"
check draft-check-once-published fail 'was published' "${draft_check[@]}"
expect draft-check-read-only no_calls_match '-X DELETE|-X PATCH|-X POST|release create'

# --- workflows (static) ----------------------------------------------------

unset FAKE_GH_STATE
workflows="${repo_root}/.github/workflows"
fork_ci="${workflows}/fork-ci.yml"
fork_release="${workflows}/fork-rust-release.yml"
setup_rust="${repo_root}/.github/actions/fork-setup-rust/action.yml"
rust_toolchain="${repo_root}/.github/actions/fork-rust-toolchain/action.yml"

# job_block FILE JOB: the lines of one job (two-space key) of a workflow.
job_block() {
  awk -v job="  $2:" '
    $0 == job { inside = 1; print; next }
    inside && /^  [A-Za-z0-9_-]+:/ { exit }
    inside { print }
  ' "$1"
}

nextest_pin="$(awk -F= '/^nextest_version=/ { print $2; exit }' "${repo_root}/scripts/fork/ci-local.sh")"
expect workflow-nextest-pinned-in-tool grep -qE "^ +tool: nextest@${nextest_pin//./\\.}$" "${fork_ci}"
expect workflow-no-install-action-version-input bash -c "! grep -qE '^ +version:' '${fork_ci}'"

go_runtime="$(job_block "${fork_ci}" go-runtime)"
expect workflow-go-runtime-reads-test-cache grep -qE '^ +shared-key: fork-ci-test$' <<<"${go_runtime}"
expect workflow-go-runtime-never-saves grep -qE '^ +save-if: "?false"?$' <<<"${go_runtime}"
app_server_job="$(job_block "${fork_ci}" rust-app-server)"
expect workflow-app-server-reads-test-cache grep -qE '^ +shared-key: fork-ci-test$' <<<"${app_server_job}"
expect workflow-app-server-never-saves grep -qE '^ +save-if: "?false"?$' <<<"${app_server_job}"
expect workflow-two-cache-families \
  test "$(grep -oE 'shared-key: [a-z0-9-]+' "${fork_ci}" | sort -u | wc -l)" -eq 2

# fork-ci result (and with it the release gate) needs every other job.
ci_jobs="$(awk '/^jobs:/ { inside = 1; next } inside && /^  [A-Za-z0-9_-]+:$/ { sub(/:$/, ""); print $1 }' \
  "${fork_ci}" | grep -vx result | LC_ALL=C sort)"
result_needs="$(job_block "${fork_ci}" result | sed -nE 's/^ +needs: \[(.*)\]$/\1/p' | tr ',' '\n' |
  tr -d ' ' | LC_ALL=C sort)"
expect workflow-result-needs-every-job test -n "${ci_jobs}" -a "${ci_jobs}" = "${result_needs}"

draft_job="$(job_block "${fork_release}" draft)"
expect workflow-draft-cancellable grep -q '!cancelled()' <<<"${draft_job}"
expect workflow-draft-not-always bash -c "! grep -q 'always()' <<<'${draft_job//\'/}'"
expect workflow-result-always grep -qE '^ +if: always\(\)$' <<<"$(job_block "${fork_ci}" result)"

# top_block FILE KEY: the lines under a top-level KEY of a workflow.
top_block() {
  awk -v key="$2:" '$0 == key { inside = 1; next } inside && /^#/ { next } inside && /^[^ ]/ { exit } inside { print }' "$1"
}
# job_if FILE JOB: the job-level if expression of JOB, on one line.
job_if() {
  job_block "$1" "$2" | awk '
    /^    if:/ { inside = 1; sub(/^    if: *(>-)? */, ""); if ($0 != "") print; next }
    inside && /^      / { sub(/^ +/, ""); print; next }
    inside { exit }
  ' | tr '\n' ' '
}
# step_if BLOCK NAME: the if expression of the step named NAME in BLOCK.
step_if() {
  awk -v want="- name: $2" '
    { line = $0; sub(/^ +/, "", line) }
    line == want { inside = 1; next }
    inside && line ~ /^- / { exit }
    inside && line ~ /^if: / { sub(/^if: /, "", line); print line; exit }
  ' <<<"$1"
}
# in_order BLOCK FIRST SECOND: FIRST occurs on an earlier line of BLOCK than SECOND.
in_order() {
  local first second
  first="$(grep -m 1 -nF -- "$2" <<<"$1" | cut -d: -f1)"
  second="$(grep -m 1 -nF -- "$3" <<<"$1" | cut -d: -f1)"
  [[ -n "${first}" && -n "${second}" && "${first}" -lt "${second}" ]]
}
# contains TEXT FIXED: TEXT contains the fixed string FIXED.
contains() { [[ "$1" == *"$2"* ]]; }
# lacks TEXT ERE: no line of TEXT matches ERE.
lacks() { ! grep -qE -- "$2" <<<"$1"; }

release_jobs="$(awk '/^jobs:/ { inside = 1; next } inside && /^  [A-Za-z0-9_-]+:$/ { sub(/:$/, ""); print $1 }' "${fork_release}")"
preflight_job="$(job_block "${fork_release}" preflight)"
build_job="$(job_block "${fork_release}" build)"
verify_job="$(job_block "${fork_release}" verify)"

# R4.1, R4.2: pushes to codex/fork prebuild; only prebuilds cancel prebuilds.
expect workflow-push-only-codex-fork test \
  "$(awk '/^  push:$/ { inside = 1; next } inside && /^  [^ ]/ { exit } inside { print }' <<<"$(top_block "${fork_release}" on)")" = \
  "    branches: [codex/fork]"
expect workflow-dispatch-rebuild-input grep -qE '^      rebuild:$' <<<"$(top_block "${fork_release}" on)"
expect workflow-prebuild-concurrency-group grep -qxF \
  "  group: \${{ github.event_name == 'push' && 'fork-rust-prebuild' || format('fork-rust-release-{0}', inputs.release_version) }}" \
  <<<"$(top_block "${fork_release}" concurrency)"
expect workflow-prebuild-cancel-in-progress grep -qxF "  cancel-in-progress: \${{ github.event_name == 'push' }}" \
  <<<"$(top_block "${fork_release}" concurrency)"
expect workflow-top-permissions-read-only test "$(top_block "${fork_release}" permissions | grep .)" = "  contents: read"
write_jobs=0
push_writes=""
for job in ${release_jobs}; do
  if job_block "${fork_release}" "${job}" | grep -qE '^ +contents: write$'; then
    write_jobs=$((write_jobs + 1))
    contains "$(job_if "${fork_release}" "${job}")" "github.event_name != 'push'" || push_writes+=" ${job}"
  fi
done
expect workflow-push-run-no-write-token test -z "${push_writes}" -a "${write_jobs}" -eq 2
expect workflow-draft-precheck-skipped-on-push test "$(job_if "${fork_release}" draft-precheck)" = "github.event_name != 'push' "
# shellcheck disable=SC2016 # ${SHA} is workflow shell text
expect workflow-preflight-prebuild-mode grep -qF 'release.sh preflight --mode prebuild --sha "${SHA}"' <<<"${preflight_job}"
expect workflow-preflight-reads-actions grep -qxF '      actions: read' <<<"${preflight_job}"
expect workflow-uploads-kept-30-days-on-push test \
  "$(grep -c 'uses: actions/upload-artifact@' "${fork_release}")" -eq \
  "$(grep -cxF "          retention-days: \${{ github.event_name == 'push' && 30 || '' }}" "${fork_release}")"

# R1.1-R1.6: one job per bundle, without debuginfo, bwrap first on Linux.
build_if="$(job_if "${fork_release}" build)"
expect workflow-build-after-cheap-checks contains "${build_if}" \
  "!cancelled() && needs.preflight.result == 'success' && needs.draft-precheck.result != 'failure'"
expect workflow-build-skipped-for-prebuild contains "${build_if}" "needs.preflight.outputs.prebuilt_run_id == ''"
# shellcheck disable=SC2016 # ${{ }} is workflow syntax
expect workflow-build-bundle-binaries grep -qxF '          BINARIES: ${{ matrix.binaries }}' <<<"${build_job}"
expect workflow-build-no-debuginfo grep -qxF '      CARGO_PROFILE_RELEASE_DEBUG: "0"' <<<"${build_job}"
# shellcheck disable=SC2016 # ${TARGET} is workflow shell text
expect workflow-build-timings grep -qF 'cargo build --locked --target "${TARGET}" --release --timings' <<<"${build_job}"
expect workflow-build-bwrap-linux-only test "$(step_if "${build_job}" 'Build bwrap and export digest')" = "runner.os == 'Linux'"
expect workflow-build-bwrap-first in_order "${build_job}" 'name: Build bwrap and export digest' 'name: Build release binaries'
expect workflow-build-part-after-cargo in_order "${build_job}" 'name: Build release binaries' 'release-package.sh part'
# shellcheck disable=SC2016 # ${{ }} is workflow syntax
expect workflow-build-uploads-part grep -qxF \
  '          name: codex-parts-${{ needs.preflight.outputs.version }}-${{ matrix.target }}-${{ matrix.bundle }}' <<<"${build_job}"
expect workflow-build-uploads-timings grep -qF 'path: codex-rs/target/cargo-timings/' <<<"${build_job}"

# R3.1: a successful push run of fork-ci replaces the ci job.
ci_if="$(job_if "${fork_release}" ci)"
expect workflow-ci-skipped-when-reused contains "${ci_if}" "needs.preflight.outputs.ci_run_url == ''"
expect workflow-ci-never-on-push contains "${ci_if}" "github.event_name != 'push'"
draft_if="$(job_if "${fork_release}" draft)"
expect workflow-draft-accepts-reused-ci contains "${draft_if}" \
  "(needs.ci.result == 'success' || needs.preflight.outputs.ci_run_url != '' || !inputs.require_ci)"
expect workflow-draft-never-on-push contains "${draft_if}" "github.event_name != 'push'"

# R2.4, R4.3: verify assembles from this run's or the prebuild run's parts.
expect workflow-verify-after-build-or-prebuild contains "$(job_if "${fork_release}" verify)" \
  "(needs.build.result == 'success' || (needs.build.result == 'skipped' && needs.preflight.outputs.prebuilt_run_id != ''))"
expect workflow-verify-reads-actions grep -qxF '      actions: read' <<<"${verify_job}"
expect workflow-verify-no-write lacks "${verify_job}" '^ +[a-z-]+: write$'
# shellcheck disable=SC2016 # ${{ }} is workflow syntax
expect workflow-verify-downloads-prebuild grep -qxF '          run-id: ${{ needs.preflight.outputs.prebuilt_run_id }}' <<<"${verify_job}"
expect workflow-verify-assembles-then-verifies in_order "${verify_job}" 'release-package.sh assemble' 'verify-release.sh --version'
# shellcheck disable=SC2016 # ${{ }} is workflow syntax
expect workflow-verify-uploads-packages grep -qxF '          name: codex-${{ needs.preflight.outputs.version }}-packages' <<<"${verify_job}"
# shellcheck disable=SC2016 # ${{ }} is workflow syntax
expect workflow-draft-downloads-verified-only test \
  "$(grep -E '^ +(name|pattern): codex-' <<<"$(job_block "${fork_release}" draft)" | sed -E 's/^ +//' | tr '\n' ' ')" = \
  'name: codex-${{ needs.preflight.outputs.version }}-packages name: codex-${{ needs.preflight.outputs.version }}-SHA256SUMS '

# Third-party actions: full SHA plus a comment with the full version.
pins_ok=1
while IFS= read -r line; do
  if [[ ! "${line}" =~ uses:\ [A-Za-z0-9_.-]+/[A-Za-z0-9_./-]+@[0-9a-f]{40}\ \#\ v?[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "    bad pin: ${line}" >&2
    pins_ok=0
  fi
done < <(grep -hE '^\s*(- )?uses: [^.]' "${fork_ci}" "${fork_release}" "${setup_rust}" "${rust_toolchain}")
expect workflow-pins-carry-full-versions test "${pins_ok}" -eq 1

# The Rust toolchain comes from codex-rs/rust-toolchain.toml: only fork-rust-toolchain installs
# one, and it passes the channel that rust-toolchain.sh reads from that file.
toolchain_script="${repo_root}/scripts/fork/rust-toolchain.sh"
expect workflow-toolchain-only-from-pin bash -c \
  "! grep -l 'dtolnay/rust-toolchain' '${fork_ci}' '${fork_release}' '${setup_rust}' | grep -q ."
expect workflow-toolchain-action-uses-pin grep -qE '^ +toolchain: \$\{\{ steps\.pin\.outputs\.channel \}\}$' "${rust_toolchain}"
expect workflow-toolchain-action-runs-script grep -q 'bash scripts/fork/rust-toolchain.sh' "${rust_toolchain}"
expect workflow-setup-rust-uses-toolchain-action grep -qE '^ +- uses: \./\.github/actions/fork-rust-toolchain$' "${setup_rust}"
expect workflow-release-uses-toolchain-action grep -qE '^ +- uses: \./\.github/actions/fork-rust-toolchain$' "${fork_release}"
expect toolchain-script-reads-repo-pin test -n "$("${toolchain_script}")"
mkdir -p "${work}/toolchain"
printf '[toolchain]\nchannel = "1.95.0"\ncomponents = ["clippy"]\n' >"${work}/toolchain/plain.toml"
printf '[toolchain]\r\nchannel = "1.96.1"  # pinned\r\n' >"${work}/toolchain/crlf.toml"
printf "[other]\nchannel = \"9.9.9\"\n\n[ toolchain ]\nchannel = 'nightly-2025-09-18'\n" >"${work}/toolchain/tables.toml"
printf '[toolchain]\ncomponents = ["clippy"]\n' >"${work}/toolchain/missing.toml"
printf '[toolchain]\nchannel = "1.95.0; rm -rf /"\n' >"${work}/toolchain/bad.toml"
check toolchain-plain ok '^1\.95\.0$' "${toolchain_script}" "${work}/toolchain/plain.toml"
check toolchain-crlf-comment ok '^1\.96\.1$' "${toolchain_script}" "${work}/toolchain/crlf.toml"
check toolchain-table-only ok '^nightly-2025-09-18$' "${toolchain_script}" "${work}/toolchain/tables.toml"
check toolchain-missing-channel fail 'no valid \[toolchain\] channel' "${toolchain_script}" "${work}/toolchain/missing.toml"
check toolchain-bad-channel fail 'no valid \[toolchain\] channel' "${toolchain_script}" "${work}/toolchain/bad.toml"
check toolchain-missing-file fail 'no toolchain file' "${toolchain_script}" "${work}/toolchain/absent.toml"

# Release machinery covers every local action and workflow the fork uses.
machinery="$("${release}" machinery)"
covered=1
while IFS= read -r local_use; do
  path="${local_use#./}"
  if ! grep -qE "^${path%/}(/|$)" <<<"${machinery}" &&
    ! awk -v p="${path}/" 'index(p, $0) == 1 { found = 1 } END { exit !found }' <<<"${machinery}"; then
    echo "    not in release machinery: ${path}" >&2
    covered=0
  fi
done < <(cat "${fork_ci}" "${fork_release}" "${setup_rust}" "${rust_toolchain}" "${repo_root}"/.github/actions/setup-rusty-v8/action.yml \
  "${repo_root}"/.github/actions/setup-msvc-env/action.yml "${repo_root}"/.github/actions/check-clean-worktree/action.yml |
  grep -oE 'uses: \./[A-Za-z0-9_./-]+' | sed 's/^uses: //' | sort -u)
expect machinery-covers-local-actions test "${covered}" -eq 1
expect machinery-covers-scripts grep -qx 'scripts/fork/' <<<"${machinery}"
# shellcheck disable=SC2016 # awk fields, not shell expansions
expect machinery-covers-release-package \
  awk -v p=scripts/fork/release-package.sh 'index(p, $0) == 1 { found = 1 } END { exit !found }' <<<"${machinery}"

echo "release.sh self-test: ${passed} passed, ${failures} failed"
[[ ${failures} -eq 0 ]]
