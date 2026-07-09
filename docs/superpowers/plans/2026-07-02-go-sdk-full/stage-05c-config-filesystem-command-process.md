# Stage 5C: Config, FileSystem, Commands, Processes

> Execute this substage as its own commit and fresh blind review.

## Scope

- `sdk/go/config_resource.go`
- `sdk/go/filesystem.go`
- `sdk/go/commands.go`
- `sdk/go/processes.go`
- focused tests: `config_resource_test.go`, `filesystem_test.go`, `commands_test.go`, `processes_test.go`

## Tasks

- [ ] Implement current matrix-backed config surfaces: `config/read`, `config/value/write`, `config/batchWrite`, and `configRequirements/read`, with config-specific snake_case payload preservation. Do not add a `config/list` wrapper unless the manifest exposes it; if a list workflow is required, stop and route it as a separate reviewed protocol change.
- [ ] Implement current matrix-backed filesystem surfaces: `fs/readFile`, `fs/writeFile`, `fs/createDirectory`, `fs/getMetadata`, `fs/readDirectory`, `fs/remove`, `fs/copy`, `fs/watch`, and `fs/unwatch`, with watch handle cleanup and lifecycle routing. File search belongs to the separate `FuzzyFileSearch` resource in Stage 5F and must not be duplicated under `FileSystem` unless the manifest moves it.
- [ ] Implement command exec streaming with completion by JSON-RPC response after output delta notifications.
- [ ] Implement process spawn/write/terminate/kill streaming with cleanup by `process/exited`.
- [ ] Add handle-identity tests for filesystem watch, command, and process follow-up wrappers. Public handle options must not require callers to re-supply owned identity; conflicting advanced params must fail before send.
- [ ] Cover response, notification stream, cleanup, unsupported operations, cancellation, queue overflow, and terminal-notification cleanup for every matrix row in this bundle.
- [ ] Record Stage 6 docs/example owners for config, filesystem, command, and process resources.

## Verification

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go test ./... -run 'Test(Config|FileSystem|Command|Process|ResourceCoverage|ResourceCallsites)'
go test ./...
```

## Review Gate

- Commit this substage separately.
- Fresh blind engineering review must specifically check lifecycle cleanup and bounded buffering.
