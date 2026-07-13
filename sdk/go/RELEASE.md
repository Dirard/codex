# Codex Go SDK Release Validation

The Go SDK module lives at:

```text
github.com/openai/codex/sdk/go
```

For v0 and v1 releases, consumers import the root package and protocol package without a semantic import suffix:

```go
import (
    codex "github.com/openai/codex/sdk/go"
    _ "github.com/openai/codex/sdk/go/protocol"
)
```

Future v2 and later releases must use the matching semantic import path:

```go
import (
    codex "github.com/openai/codex/sdk/go/v2"
    _ "github.com/openai/codex/sdk/go/v2/protocol"
)
```

## Tags

Release tags use the submodule prefix:

```text
sdk/go/vX.Y.Z
```

The Go module version queried by consumers strips the `sdk/go/` prefix. For example, the VCS tag `sdk/go/v1.2.3` is consumed as:

```bash
go get github.com/openai/codex/sdk/go@v1.2.3
```

## Non-Publishing Validation

Run the validation workflow before publishing a Go SDK tag:

```bash
gh workflow run go-sdk-release-readiness.yml -f checkout_ref="$(git rev-parse HEAD)"
```

The workflow is validation-only. It does not publish artifacts, push persistent tags, or require secrets. For normal CI it creates temporary synthetic `sdk/go/v0.*`, `sdk/go/v1.*`, annotated `sdk/go/v1.*`, and rewritten-tree `sdk/go/v2.*` tags inside a local bare repository, then verifies external consumers can resolve the module and its `/protocol` subpackage through Git.

## Linux-only production sign-off

Linux production readiness requires both a successful manual `go-sdk-release-readiness` run and a successful `rust-release` run for the same reviewed commit. The module run must include the synthetic v0, v1, annotated v1, and v2 tag lanes described above.

Download the published `go-sdk-linux-release-readiness.json` and `codex-package_SHA256SUMS` assets from the matching `rust-v*` release, then run the Linux branch of the Stage 7 validator. The metadata must set `linuxReleaseReady` to `true`, cover both Linux targets, match the public checksums, and contain successful real app-server and sandbox smoke evidence for each package archive.

Linux-only sign-off does not require aggregate cross-platform `sdk.yml` evidence or the absence of readiness markers from Windows/macOS producer lanes. Those lanes remain separate release gates when their platforms are in scope. Non-publishing fixture evidence can validate archive shape, but it cannot establish Linux shipping readiness.

## Bad Tags

Do not delete, overwrite, force-push, or retag an already published Go module version in place. If a bad `sdk/go/vX.Y.Z` tag was published, block the release, publish a higher patch version after the fix, and add a `retract` directive or release note when downstream users need an explicit superseding version.
