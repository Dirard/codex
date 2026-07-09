# Stage 5G: Package Source Hermeticity Prerequisite

## Purpose

Establish release-owned, no-network package resource inputs before Stage 6 attempts to build a Go SDK runtime layout. This stage exists because the current package builder obtains managed `rg`, patched zsh, and the `.tar.zst` writer through DotSlash manifests or PATH fallbacks at package-build time. Stage 6 must not claim hermetic release-shaped staging until this stage has made those resources available as explicit same-checkout build inputs or has blocked the release-shaped staging plan.

## Files

- Modify: `scripts/codex_package/README.md`
- Modify: `scripts/codex_package/ripgrep.py`
- Modify: `scripts/codex_package/zsh.py`
- Modify: `scripts/codex_package/archive.py`
- Modify: `scripts/codex_package/dotslash.py` only if the resolver contract changes.
- Modify: `scripts/codex_package/targets.py` only if target-to-resource mapping changes.
- Modify: `scripts/codex_package/layout.py` only if install-layout placement changes.
- Modify: `.github/scripts/build-codex-package-archive.sh` only if the package-builder CLI contract changes.
- Modify: `.github/workflows/zstd` only if it remains a package source manifest; Go SDK archive validation must not execute it implicitly.
- Modify: `.github/workflows/rust-release.yml` so Linux/macOS release package assembly consumes the same materialized helper source without ad hoc `curl` or DotSlash fetches.
- Modify: `.github/workflows/rust-release-windows.yml` so Windows release package assembly consumes the same materialized helper source without installing or invoking DotSlash in the package-archive job.
- Modify: `.github/dotslash-config.json` only when package archive names or in-archive paths intentionally change; otherwise add tests proving it still matches the shipping release archive names and paths used by `publish-dotslash`.
- Modify: `scripts/codex_package/rg` and `scripts/codex_package/codex-zsh` only if the pinned manifests remain the package source of truth and their digest/target metadata changes.
- Create/modify: a focused package-source test file under `scripts/codex_package/` or `sdk/go/` only for this package-source contract.

## Tasks

- [ ] Define a single release-owned helper source contract for managed `rg`, patched zsh, and archive `zstd` that both the existing package builder and Stage 6's Bazel/runtime-layout collector can consume.
- [ ] The contract must materialize concrete helper payloads without DotSlash/package-cache/network access during Go SDK CI or Stage 7. Acceptable implementations include checked same-checkout build outputs, Bazel-materialized runfiles whose inputs are already available before the Go SDK CI job starts, or another reviewed release-owned source that can be proven with network disabled.
- [ ] `scripts/codex_package/rg` and `scripts/codex_package/codex-zsh` may remain pinned metadata/source manifests, but they must not be staging-time fetchers for the Go SDK runtime layout. If they remain in the release path, add tests proving the Stage 6 collector consumes already-materialized payloads rather than calling DotSlash.
- [ ] Update the real release workflows to consume the same Stage 5G materialized helper contract. `rust-release.yml` must not download the packaged zsh manifest with an ad hoc `curl` step for package assembly, and `rust-release-windows.yml` must not install or invoke DotSlash for package assembly. If a release workflow still needs a network fetch, explicit artifact download, or DotSlash manifest resolution, this stage blocks Stage 6 release-readiness claims until that shipping path is aligned.
- [ ] Make `zstd` hermetic for every archive validation lane and record the selected contract in the Stage 5G review notes. Acceptable outcomes are either a fail-fast prerequisite that requires a preinstalled `zstd` before `--release-package-archive` reaches package assembly, or a reviewed no-network `zstd` input supplied by this stage through an explicit `--zstd-source`/`-ZstdSource` staging-script argument. Go SDK CI and Stage 7 must never satisfy archive creation by prepending `.github/workflows` to `PATH`, invoking `.github/workflows/zstd`, running DotSlash, reading an ambient DotSlash/package cache, or fetching the upstream zstd archive at validation time.
- [ ] The app-server test fixture `codex-rs/app-server/tests/suite/zsh` must not be used as the release-shaped zsh source. It may be cited only as a test-only fixture.
- [ ] Add a no-network package-source test that starts with an empty DotSlash/package cache and fails if resolving `rg`, zsh, or archive `zstd` for the Go SDK runtime layout performs a fetch, reads from an ambient cache, prepends `.github/workflows` to `PATH`, or silently omits the helper.
- [ ] Add release-workflow assertions proving `.github/workflows/rust-release.yml` and `.github/workflows/rust-release-windows.yml` consume the same materialized helper inputs used by Stage 6 and no longer use ad hoc zsh `curl`, `facebook/install-dotslash`, package-assembly DotSlash resolution for `rg` or zsh, or `.github/workflows/zstd`/DotSlash fallback for archive validation. Stage 7 must rerun these assertions directly or rerun the exact owning test command even if the tests do not live under `sdk/go`.
- [ ] Add a DotSlash release-output parity assertion, for example `test_dotslash_release_archive_config_parity`, that reads `.github/dotslash-config.json`, the `publish-dotslash` job in `.github/workflows/rust-release.yml`, and the release artifact naming/layout contract for every entry published by that config. It must fail if any claimed Linux/macOS/Windows `codex`, `codex-app-server`, `codex-responses-api-proxy`, Linux `bwrap`, Windows `codex-command-runner`, or Windows `codex-windows-sandbox-setup` artifact filename no longer matches the configured regex, if the configured `path` is absent from the corresponding archive or compressed helper payload, or if `publish-dotslash` stops using `.github/dotslash-config.json`. Do not narrow the parity gate to only the package archives; helper-output regex/path drift is release-blocking because those helpers are part of the shipped runtime and DotSlash distribution story.
- [ ] Add a package-layout parity test that compares helper placement with `scripts/codex_package/layout.py`: managed `rg` must land where `InstallContext::rg_command()` resolves it, and non-Windows zsh must land at `codex-resources/zsh/bin/zsh`.
- [ ] If this stage cannot establish a release-owned no-network source or explicit fail-fast prerequisite for `rg`, zsh, and archive `zstd` across both Go SDK staging and the real release workflows, mark Stage 6 runtime staging and release-readiness claims blocked. Do not weaken Stage 6 by allowing hidden DotSlash fetches, ambient caches, placeholder directories, or release-parity claims against missing helper payloads.

## Verification

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
python3 -m unittest scripts.codex_package.test_package_sources
python3 -m unittest discover scripts/codex_package
```

The final stage review must quote these exact owner commands and the passing output. `scripts.codex_package.test_package_sources` owns the Stage 5G hermeticity, workflow producer/consumer, `zstd`, DotSlash parity, and package-layout assertions; do not substitute a `go test -run ...` placeholder unless matching Go tests are actually implemented and reviewed.

## Commit

```bash
git add scripts/codex_package .github/scripts/build-codex-package-archive.sh
git add .github/workflows/rust-release.yml .github/workflows/rust-release-windows.yml
git commit -m "build(go-sdk): make package helper sources hermetic"
```

## Stage Review

Fresh blind engineering and product review are mandatory. Review must confirm this stage creates a real no-network helper source for Stage 6 and the shipping release workflows, or explicitly blocks Stage 6 runtime staging and release-readiness claims.
