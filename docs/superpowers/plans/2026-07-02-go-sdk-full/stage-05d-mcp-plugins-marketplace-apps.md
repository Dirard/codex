# Stage 5D: MCP, Plugins, Marketplace, Apps

> Execute this substage as its own commit and fresh blind review.

## Scope

- `sdk/go/mcp.go`
- `sdk/go/plugins.go`
- `sdk/go/marketplace.go`
- `sdk/go/apps.go`
- focused tests: `mcp_test.go`, `plugins_test.go`, `marketplace_test.go`, `apps_test.go`

## Tasks

- [ ] Implement every SDK-public MCP matrix row, including status, structured content, tool calls, and elicitation workflows exposed by the manifest.
- [ ] Implement MCP OAuth as the `MCPOAuthHandle` workflow defined in Stage 4. `mcpServer/oauth/login` must require a handle callsite, terminal `mcpServer/oauthLogin/completed` routing test, docs/example owner, and integration coverage or a manifest-backed not-applicable reason.
- [ ] Implement every SDK-public plugin row, including read-only and mutation workflows exposed by the manifest.
- [ ] Implement every SDK-public marketplace row, including read-only and mutation workflows exposed by the manifest.
- [ ] Implement Apps only for current manifest surface, currently app listing and `app/list/updated`; do not invent install/update/share APIs.
- [ ] Tests must cover each SDK-public matrix row and notification path in this bundle.
- [ ] Record Stage 6 docs/example owners for MCP, plugins, marketplace, and apps.

## Verification

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go test ./... -run 'Test(MCP|Plugin|Marketplace|Apps|ResourceCoverage|ResourceCallsites)'
go test ./...
```

## Review Gate

- Commit this substage separately.
- Product review must confirm Apps scope matches current protocol and does not promise unsupported app workflows.
