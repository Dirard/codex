# Stage 5E: Account, Review, Model, Environment, Control

> Execute this substage as its own commit and fresh blind review.

## Scope

- `sdk/go/accounts.go`
- `sdk/go/reviews.go`
- `sdk/go/models.go`
- `sdk/go/environments.go`
- `sdk/go/remote_control.go`
- `sdk/go/collaboration_modes.go`
- `sdk/go/external_agents.go`
- focused tests for each resource

## Tasks

- [ ] Implement every SDK-public matrix row for `Accounts`, `Reviews`, `Models`, `Environments`, `RemoteControl`, `CollaborationModes`, and `ExternalAgents`.
- [ ] Account/auth tests must use isolated `CODEX_HOME` and mocked auth/Responses endpoints only.
- [ ] Implement account login helpers, including `Accounts.LoginWithAPIKey(ctx, key APIKey)` or the reviewed equivalent, plus device-code handle workflow, redacted secret tests, and docs/example owner rows.
- [ ] Implement review start as the `ReviewHandle` workflow from Stage 4 with `Events`, `Wait`, ownership of `reviewThreadId` and `turn.id` from `ReviewStartResponse`, ordinary review turn lifecycle routing such as `turn/completed`, review result collection from the review turn stream/result, README/example owner, and integration coverage or a matrix-backed not-applicable reason. Do not use `item/autoApprovalReview/*` as `review/start` lifecycle events; those are approval/Guardian item notifications.
- [ ] Implement remote-control pairing as `RemoteControlPairingHandle`. `remoteControl/pairing/start` must own pairing code/session data returned by start, provide `Status`, `Wait`, and `Close`/cleanup when supported, route `remoteControl/status/changed`, and avoid a thin raw params-only wrapper.
- [ ] Reviews, environments, remote control, collaboration mode, and external agent tests must use safe mock fixtures or explicit unsupported/not-applicable matrix reasons.

## Verification

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go test ./... -run 'Test(Account|Review|Model|Environment|RemoteControl|Collaboration|ExternalAgent|ResourceCoverage|ResourceCallsites)'
go test ./...
```

## Review Gate

- Commit this substage separately.
- Fresh blind review must check auth secrecy, handle ownership, and unsupported/not-applicable reasons.
