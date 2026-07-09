# Stage 5F: Search, Memory, Feedback, Platform, Experimental

> Execute this substage as its own commit and fresh blind review.

## Scope

- `sdk/go/fuzzy_file_search.go`
- `sdk/go/memory.go`
- `sdk/go/feedback.go`
- `sdk/go/windows_sandbox.go`
- `sdk/go/experimental_features.go`
- `sdk/go/permission_profiles.go`
- focused tests for each resource

## Tasks

- [ ] Implement every SDK-public matrix row for `FuzzyFileSearch`, `Memory`, `Feedback`, `WindowsSandbox`, `ExperimentalFeatures`, and `PermissionProfiles`.
- [ ] Fuzzy file search session wrappers must own session identity and inject it into follow-up generated params.
- [ ] `WindowsSandbox` must compile on every supported OS and return typed unsupported-platform status/errors on unsupported runtimes.
- [ ] Experimental resources must honor `ProtocolModeStable` pre-write rejection for method-level and field-level experimental gates.
- [ ] Feedback uploads and other side-effecting methods must not retry after write unless generated retry metadata explicitly allows it.
- [ ] Record Stage 6 docs/example owners for search, memory, feedback, Windows sandbox, experimental features, and permission profiles.

## Verification

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go test ./... -run 'Test(FuzzyFileSearch|Memory|Feedback|WindowsSandbox|ExperimentalFeatures|PermissionProfiles|ResourceCoverage|ResourceCallsites)'
go test ./...
```

## Review Gate

- Commit this substage separately.
- Fresh blind engineering/product review must confirm full matrix coverage for remaining resources before Stage 6 starts.
