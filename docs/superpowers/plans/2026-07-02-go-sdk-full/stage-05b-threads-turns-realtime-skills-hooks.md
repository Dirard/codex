# Stage 5B: Threads, Turns, Realtime, Skills, Hooks

> Execute this substage as its own commit and fresh blind review.

## Scope

- `sdk/go/thread.go`
- `sdk/go/turn.go`
- `sdk/go/realtime.go`
- `sdk/go/skills.go`
- `sdk/go/hooks.go`
- focused tests: `thread_test.go`, `turn_test.go`, `realtime_test.go`, `skills_test.go`, `hooks_test.go`

## Tasks

- [ ] Implement every SDK-public `Threads` and `Turns` matrix row, including start, resume, fork, read, list, archive/unarchive, settings update, `thread/settings/updated` notification routing, memory-related thread operations, Guardian-style operations, `turn/start`, `turn/steer`, `turn/interrupt`, thread-owned turn listing via `thread/turns/list`, and notification-backed handles when present. Generated matrix rows are the authority for exact current method names; do not invent `thread/settings/read`, `turn/read`, or `turn/list` wrappers unless the manifest exposes them. If product scope requires a settings read row and the matrix still lacks it, stop and route that as a separate reviewed protocol change before implementing Go SDK surface.
- [ ] Implement `Realtime` rows and notifications from the matrix, including audio/text/speech/voices workflows, start/stop handles, streaming events, and unsupported-platform/resource errors.
- [ ] Until every realtime notification has a Codex-owned realtime session identity, enforce one active realtime session per thread. A second start for the same thread must return a typed conflict error before sending, and tests must prove thread-scoped notifications cannot cross-deliver.
- [ ] Implement every SDK-public `Skills` and `Hooks` row exposed by the manifest.
- [ ] Add handle-identity tests for thread, turn, and realtime follow-up methods. Public handle options must not require callers to re-supply identity already owned by the handle.
- [ ] Record Stage 6 docs/example owner rows for thread lifecycle, turn controls, realtime, skills, and hooks; do not create docs in this substage.

## Verification

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go test ./... -run 'Test(Threads|Turns|Realtime|Skills|Hooks|ResourceCoverage|ResourceCallsites)'
go test ./...
```

## Review Gate

- Commit this substage separately.
- Fresh blind product review must confirm this is not raw-only protocol exposure.
