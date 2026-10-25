#!/usr/bin/env bash
# Self-test for scripts/fork/release.sh.
#
#   scripts/fork/test-release.sh
#
# Runs preflight against this checkout (HEAD and codex-rs/Cargo.toml) and draft
# against a small fake dist. Every GitHub call goes to a stub `gh` placed first
# on PATH, so the test is hermetic and never changes GitHub.

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
release="${repo_root}/scripts/fork/release.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/test-release.XXXXXX")"
trap 'rm -rf "${work}"' EXIT

head="$(git -C "${repo_root}" rev-parse HEAD)"
version="$(awk '
  /^\[workspace\.package\]/ { section = 1; next }
  /^\[/ { section = 0 }
  section && $1 == "version" { gsub(/"/, "", $3); print $3; exit }
' "${repo_root}/codex-rs/Cargo.toml")"
other_sha="$(printf '%040d' 0 | tr 0 a)"
repo="example/codex"

# Stub gh: answers the calls release.sh makes with the post---jq output, using
# files under $FAKE_GH_STATE. Unknown calls fail loudly.
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
case "${args}" in
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
  *" api --paginate "*"/releases?per_page=100 "*)
    if [[ -f "${state}/releases" ]]; then cat "${state}/releases"; fi
    ;;
  " release create "*)
    shift 3 # release create TAG
    target="" assets=()
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --target) target="$2"; shift 2 ;;
        --repo | --title | --notes-file) shift 2 ;;
        --draft) shift ;;
        *) assets+=("$1"); shift ;;
      esac
    done
    printf '%s\n' "${assets[@]}" >"${state}/assets"
    printf '4242\ttrue\t%s\thttps://github.com/example/codex/releases/tag/untagged-1\n' "${target}" >"${state}/releases"
    ;;
  *"/releases/4242 "*)
    while IFS= read -r asset; do
      hash="$(sha256sum "${asset}" | cut -d' ' -f1)"
      if [[ -f "${state}/corrupt-digest" ]]; then hash="0${hash:1}"; fi
      printf '%s\t%s\tsha256:%s\tuploaded\n' "$(basename "${asset}")" "$(stat -c %s "${asset}")" "${hash}"
    done <"${state}/assets"
    ;;
  *)
    echo "stub gh: unexpected call:${args}" >&2
    exit 99
    ;;
esac
GH
chmod 0755 "${work}/bin/gh"
export PATH="${work}/bin:${PATH}"

passed=0
failures=0

# new_state: fresh stub state; prints its directory.
new_state() {
  local dir
  dir="$(mktemp -d "${work}/state.XXXXXX")"
  : >"${dir}/calls"
  printf '%s\n' "${dir}"
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
if grep -qx "sha=${head}" "${work}/github-output" &&
  grep -q '^matrix=\[{"runner":"ubuntu-24.04","target":"x86_64-unknown-linux-gnu"}' "${work}/github-output"; then
  echo "ok   preflight-output-file"
  passed=$((passed + 1))
else
  echo "FAIL preflight-output-file" >&2
  failures=$((failures + 1))
fi
check preflight-partial-matrix ok '^full_matrix=false$' \
  "${release}" preflight --version "${version}" --sha "${head}" \
  --matrix '[{"runner":"ubuntu-24.04-arm","target":"aarch64-unknown-linux-gnu"}]'
check preflight-unknown-target fail 'build_matrix must be' \
  "${release}" preflight --version "${version}" --sha "${head}" \
  --matrix '[{"runner":"ubuntu-24.04","target":"riscv64gc-unknown-linux-gnu"}]'
check preflight-duplicate-target fail 'build_matrix must be' \
  "${release}" preflight --version "${version}" --sha "${head}" \
  --matrix '[{"runner":"a","target":"x86_64-apple-darwin"},{"runner":"b","target":"x86_64-apple-darwin"}]'

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

FAKE_GH_STATE="$(new_state)"
check draft-dry-run ok 'gh release create rust-v' "${draft[@]}" --dist "${dist}" --dry-run
if grep -q 'release create' "${FAKE_GH_STATE}/calls"; then
  echo "FAIL draft-dry-run-no-mutation: dry run created a release" >&2
  failures=$((failures + 1))
else
  echo "ok   draft-dry-run-no-mutation"
  passed=$((passed + 1))
fi

FAKE_GH_STATE="$(new_state)"
check draft-create ok 'Publication stays manual' \
  "${draft[@]}" --dist "${dist}" --run-url https://github.com/example/codex/actions/runs/1
if grep -qE "^gh release create rust-v${version} --repo ${repo} --draft --target ${head} " "${FAKE_GH_STATE}/calls" &&
  [[ "$(wc -l <"${FAKE_GH_STATE}/assets")" -eq 6 ]] &&
  grep -q '/SHA256SUMS$' "${FAKE_GH_STATE}/assets" &&
  ! grep -qE 'git tag|-F draft=false' "${FAKE_GH_STATE}/calls"; then
  echo "ok   draft-create-arguments"
  passed=$((passed + 1))
else
  echo "FAIL draft-create-arguments" >&2
  sed 's/^/    /' "${FAKE_GH_STATE}/calls" >&2
  failures=$((failures + 1))
fi

FAKE_GH_STATE="$(new_state)"
printf '77\ttrue\t%s\thttps://github.com/example/codex/releases/tag/untagged-0\n' "${head}" >"${FAKE_GH_STATE}/releases"
check draft-existing-release fail "a release for rust-v${version} already exists" "${draft[@]}" --dist "${dist}"

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

echo "release.sh self-test: ${passed} passed, ${failures} failed"
[[ ${failures} -eq 0 ]]
