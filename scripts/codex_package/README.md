# Codex package builder

This package contains the implementation behind `scripts/build_codex_package.py`.
The top-level script is the stable executable entry point; these modules keep the
package-building logic split by responsibility.

Run the builder through `just`:

```bash
just assemble-codex-package --help
just assemble-codex-package --variant codex-app-server
just assemble-codex-package --target x86_64-unknown-linux-gnu
```

The builder creates a canonical Codex package directory:

```text
.
├── codex-package.json
├── bin
│   ├── <entrypoint>[.exe]
│   └── codex-code-mode-host[.exe]
├── codex-resources
│   ├── bwrap                             # Linux only
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
`codex` and `codex-app-server`. The `--package-version` flag sets the version in
`codex-package.json`; it defaults to `[workspace.package].version` in
`codex-rs/Cargo.toml`.

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

On Linux and macOS, the builder strips symbols from the **package copies** of
source-built release-profile entrypoint and code-mode host binaries. Cargo outputs
retain their symbols for debugging. The default `--strip auto` also preserves
development/profiling builds and prebuilt inputs
byte-for-byte, including signatures. Use `--strip all` to also strip prebuilt
entrypoint/host copies, or `--strip none` to keep symbols in every package copy.
Strip before production signing; `--strip all` is not appropriate for inputs
whose release signatures must be preserved. Windows MSVC symbols are separate
PDB files and are not included in the package.

macOS uses `xcrun strip -S -x`, matching the release pipeline and preserving
executable ad-hoc signatures. Linux uses `llvm-strip`, a target-prefixed GNU
strip, or native GNU strip when the host architecture matches. For cross builds,
install `llvm-strip` or pass `--strip-tool /path/to/target-strip`. Missing tools
or strip failures fail the build; use `--strip none` to deliberately opt out.
Third-party resources are copied unchanged. In particular, never strip `bwrap`
after its integrity digest has been embedded in Codex.

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

## Stage 5G hermetic package-source contract

Release-shaped package assembly and Go SDK runtime staging must use
`--require-materialized-helper-sources`. In that mode the builder requires
explicit `--rg-bin`. Linux targets also require explicit `--bwrap-bin`, and Windows targets require
explicit `--codex-command-runner-bin` and `--codex-windows-sandbox-setup-bin`.
It does not call DotSlash, read the package cache, discover helpers from `PATH`,
source-build helper payloads, or fetch helper archives from the network.

The release wrapper `.github/scripts/build-codex-package-archive.sh` always uses
that strict mode. Linux and Windows release jobs run
`python3 -m codex_package.materialize_helpers` before package assembly, while
macOS release jobs consume their verified signed resources directly. Package
assembly then consumes only already materialized helper payloads:

```text
${CODEX_PACKAGE_HELPER_ROOT}/<target>/rg[.exe]
${CODEX_PACKAGE_HELPER_ROOT}/<target>/bwrap     # Linux targets
${CODEX_PACKAGE_HELPER_ROOT}/<target>/codex-command-runner.exe
${CODEX_PACKAGE_HELPER_ROOT}/<target>/codex-windows-sandbox-setup.exe
```

The workflow-owned materializer uses the pinned manifest metadata for managed
`rg`, downloads that provider archive into the helper root, verifies its declared
size and SHA-256 digest, extracts the configured payload
path, and writes `${CODEX_PACKAGE_HELPER_ROOT}/<target>/codex-package-helpers.json`.
That manifest records the target, generator, relative helper paths, byte sizes,
and SHA-256 digests for every concrete payload. Linux `bwrap` and Windows
sandbox helper executables are copied from the same release binaries that the
workflow already builds, signs, and verifies.

Stage 6 Go SDK runtime staging may consume that helper root only as a
pre-produced artifact. The consumer must make the helper root available before
the network-disabled staging/test segment, run:

```bash
PYTHONPATH="$GITHUB_WORKSPACE/scripts" python3 -m codex_package.materialize_helpers \
  --target "$TARGET" \
  --output-root "$CODEX_PACKAGE_HELPER_ROOT" \
  --verify-only
```

and then stage from the verified files. After this verification step, Go SDK
runtime staging must not call the materializer, DotSlash, package-cache lookup,
PATH discovery, or any network fetch for `rg`, `bwrap`, or Windows sandbox
helpers.

`zstd` is a fail-fast prerequisite for `.tar.zst` archive creation and release
artifact compression. The package builder no longer falls back to
`.github/workflows/zstd` or DotSlash for archive validation.

This makes the shipping package assembly path reviewed and explicit instead of
depending on an out-of-band environment variable. It also gives Stage 6 a
reviewed helper-root artifact contract for runtime staging. Full Go SDK release
readiness still requires the later Stage 6 CI wiring and Stage 7 downloaded
GitHub Actions evidence; local manifest verification alone is only a preflight.
