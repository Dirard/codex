# Codex package builder

This package contains the implementation behind `scripts/build_codex_package.py`.
The top-level script is the stable executable entry point; these modules keep the
package-building logic split by responsibility.

The builder creates a canonical Codex package directory:

```text
.
├── codex-package.json
├── bin
│   ├── <entrypoint>[.exe]
│   └── codex-code-mode-host[.exe]
├── codex-resources
│   ├── bwrap                             # Linux only
│   ├── zsh/bin/zsh                       # supported Unix targets only
│   ├── codex-command-runner.exe          # Windows only
│   └── codex-windows-sandbox-setup.exe   # Windows only
└── codex-path
    └── rg[.exe]
```

The package directory is the primary artifact. Archive formats such as
`.tar.gz`, `.tar.zst`, and `.zip` are serializations of that directory.

If `--target` is omitted, the builder uses the release target for the current
host platform. On Linux, that default is a musl target to match Codex release
artifacts; pass a GNU Linux target explicitly for native glibc local builds. If
`--package-dir` is omitted, the builder creates a new temporary directory and
prints its path after the package is built.

The `--variant` flag selects the package entrypoint. Supported variants are
`codex` and `codex-app-server`. The `version` field in `codex-package.json` is
read from `[workspace.package].version` in `codex-rs/Cargo.toml`.

## Source-built artifacts

Artifacts built from this repository are built by the package builder in one
grouped `cargo build` command per package when they are needed and no prebuilt
override was provided:

- all targets: the selected entrypoint, unless `--entrypoint-bin` is provided
- all targets: `codex-code-mode-host`, unless `--code-mode-host-bin` is provided
- Linux targets: `bwrap`, unless `--bwrap-bin` is provided
- Windows targets: `codex-command-runner` and `codex-windows-sandbox-setup`,
  unless the corresponding prebuilt helper flags are provided

The default cargo profile is `dev-small` because local iteration should favor
fast, small builds. Release jobs should pass `--cargo-profile release` and an
explicit target. Release jobs that already built and signed/notarized the
entrypoint should pass `--entrypoint-bin` so the package contains that exact
binary instead of rebuilding it.

Release jobs should likewise pass `--code-mode-host-bin` so the package contains
the signed host executable beside the signed entrypoint.

Release jobs that already built package resource binaries should also pass the
corresponding resource flags: `--bwrap-bin` for Linux packages, and
`--codex-command-runner-bin` plus `--codex-windows-sandbox-setup-bin` for
Windows packages. This keeps package archive creation as a pure staging step
after signing instead of rebuilding resources.

When the builder source-builds an entrypoint for a Darwin or Linux target, it
downloads and verifies the matching Codex-built V8 release pair before invoking
Cargo and sets `RUSTY_V8_ARCHIVE` plus `RUSTY_V8_SRC_BINDING_PATH` for that
build. Windows targets keep Cargo's release-build MSVC artifact path. Explicit
overrides remain authoritative when both variables are already set. Set
`V8_FROM_SOURCE=1` to leave the build with the `v8` crate source-build path.

`rg` is not built from this repository, so the default local builder path can
fetch it from the DotSlash manifest at `scripts/codex_package/rg`. Downloaded
archives are cached under `$TMPDIR/codex-package/<target>-rg` and are reused only
after the recorded size and SHA-256 digest have been verified. Pass `--rg-bin`
to use a local ripgrep executable instead.

The patched zsh fork used by `shell_zsh_fork` is fetched from the DotSlash
manifest at `scripts/codex_package/codex-zsh` in that default local path when
the selected target has a matching prebuilt artifact. Downloaded archives are
cached under `$TMPDIR/codex-package/<target>-zsh` and installed at
`codex-resources/zsh/bin/zsh`. Pass `--zsh-manifest` to use a different DotSlash
manifest, such as the manifest published with a standalone zsh artifact release,
or pass `--zsh-bin` to use an already materialized patched zsh executable.

## Stage 5G hermetic package-source contract

Release-shaped package assembly and Go SDK runtime staging must use
`--require-materialized-helper-sources`. In that mode the builder requires
explicit `--rg-bin` and, for non-Windows targets, explicit `--zsh-bin` inputs.
Linux targets also require explicit `--bwrap-bin`, and Windows targets require
explicit `--codex-command-runner-bin` and `--codex-windows-sandbox-setup-bin`.
It does not call DotSlash, read the package cache, discover helpers from `PATH`,
source-build helper payloads, or fetch helper archives from the network.

The release wrapper `.github/scripts/build-codex-package-archive.sh` always uses
that strict mode. The shipping release workflows run
`python3 -m codex_package.materialize_helpers` before package assembly, set
`CODEX_PACKAGE_HELPER_ROOT`, and then consume only already materialized helper
payloads:

```text
${CODEX_PACKAGE_HELPER_ROOT}/<target>/rg[.exe]
${CODEX_PACKAGE_HELPER_ROOT}/<target>/zsh       # non-Windows targets
${CODEX_PACKAGE_HELPER_ROOT}/<target>/bwrap     # Linux targets
${CODEX_PACKAGE_HELPER_ROOT}/<target>/codex-command-runner.exe
${CODEX_PACKAGE_HELPER_ROOT}/<target>/codex-windows-sandbox-setup.exe
```

The workflow-owned materializer uses the pinned manifest metadata for managed
`rg` and patched zsh, downloads those provider archives into the helper root,
verifies their declared size and SHA-256 digest, and extracts the configured
payload path. Linux `bwrap` and Windows sandbox helper executables are copied
from the same release binaries that the workflow already builds, signs, and
verifies.

`zstd` is a fail-fast prerequisite for `.tar.zst` archive creation and release
artifact compression. The package builder no longer falls back to
`.github/workflows/zstd` or DotSlash for archive validation.

This makes the shipping package assembly path reviewed and explicit instead of
depending on an out-of-band environment variable. Stage 6 runtime staging and
release-readiness claims still remain blocked until the same helper payloads are
available before Go SDK CI and release-readiness runs begin with network
disabled.
