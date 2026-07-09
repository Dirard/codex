# Current Protocol Inventory

## Client Methods By Resource Owner

### Accounts

serverNotifications=account/login/completed,account/rateLimits/updated,account/updated
serverHandlers=account/chatgptAuthTokens/refresh(chatgpt-token-refresh),attestation/generate(attestation-generate)

- `account/login/start` status=implemented-stage4 raw=AccountLoginStart wrapper=Accounts.StartChatGPTLogin / Accounts.StartDeviceCodeLogin / Accounts.LoginWithAPIKey / LoginHandle file=accounts.go signature= convention=handle-start callsite=login, err := client.Accounts.StartDeviceCodeLogin(ctx); login, err = client.Accounts.StartChatGPTLogin(ctx); err = client.Accounts.LoginWithAPIKey(ctx, codex.APIKey("test-key")); result, err := login.Wait(ctx) unitTest=workflows_test.go safeIntegration=auth-bound account workflows use isolated test CODEX_HOME and mocked SDK/app-server JSON-RPC fixtures; live auth/Responses proof deferred to Stage 7 docs=examples/login_account exception= review=typed handle workflow plus API-key helper with redacted secret handling; raw params only through generated raw protocol APIs
- `account/login/cancel` status=implemented-stage4 raw=AccountLoginCancel wrapper=LoginHandle.Cancel file=accounts.go signature= convention=handle-followup callsite=login.Cancel(ctx) unitTest=workflows_test.go safeIntegration=auth-bound account workflows use isolated test CODEX_HOME and mocked SDK/app-server JSON-RPC fixtures; live auth/Responses proof deferred to Stage 7 docs=examples/login_account exception= review=typed handle workflow
- `account/logout` status=implemented-stage4 raw=AccountLogout wrapper=Accounts.Logout file=accounts.go signature= convention=thin callsite=client.Accounts.Logout(ctx) unitTest=workflows_test.go safeIntegration=auth-bound account workflows use isolated test CODEX_HOME and mocked SDK/app-server JSON-RPC fixtures; live auth/Responses proof deferred to Stage 7 docs=examples/login_account exception= review=SDK-public thin wrapper
- `account/rateLimits/read` status=implemented-stage4 raw=AccountRateLimitsRead wrapper=Accounts.RateLimits file=accounts.go signature= convention=thin callsite=client.Accounts.RateLimits(ctx) unitTest=workflows_test.go safeIntegration=auth-bound account workflows use isolated test CODEX_HOME and mocked SDK/app-server JSON-RPC fixtures; live auth/Responses proof deferred to Stage 7 docs=examples/login_account exception= review=SDK-public thin wrapper
- `account/rateLimitResetCredit/consume` status=implemented-stage5e raw=AccountRateLimitResetCreditConsume wrapper=Accounts.ConsumeRateLimitResetCredit file=accounts.go signature= convention=thin callsite=client.Accounts.ConsumeRateLimitResetCredit(ctx, protocol.ConsumeAccountRateLimitResetCreditParams{IDempotencyKey: "reset-credit-idempotency-key"}) unitTest=accounts_test.go safeIntegration=auth-bound account workflows use isolated test CODEX_HOME and mocked SDK/app-server JSON-RPC fixtures; live auth/Responses proof deferred to Stage 7 docs=README accounts exception= review=SDK-public thin wrapper
- `account/usage/read` status=implemented-stage4 raw=AccountUsageRead wrapper=Accounts.Usage file=accounts.go signature= convention=thin callsite=client.Accounts.Usage(ctx) unitTest=workflows_test.go safeIntegration=auth-bound account workflows use isolated test CODEX_HOME and mocked SDK/app-server JSON-RPC fixtures; live auth/Responses proof deferred to Stage 7 docs=examples/login_account exception= review=SDK-public thin wrapper
- `account/workspaceMessages/read` status=implemented-stage5e raw=AccountWorkspaceMessagesRead wrapper=Accounts.ReadWorkspaceMessages file=accounts.go signature= convention=thin callsite=client.Accounts.ReadWorkspaceMessages(ctx) unitTest=accounts_test.go safeIntegration=auth-bound account workflows use isolated test CODEX_HOME and mocked SDK/app-server JSON-RPC fixtures; live auth/Responses proof deferred to Stage 7 docs=README accounts exception= review=SDK-public thin wrapper
- `account/sendAddCreditsNudgeEmail` status=implemented-stage5e raw=AccountSendAddCreditsNudgeEmail wrapper=Accounts.SendAddCreditsNudgeEmail file=accounts.go signature= convention=thin callsite=client.Accounts.SendAddCreditsNudgeEmail(ctx, protocol.SendAddCreditsNudgeEmailParams{CreditType: protocol.AddCreditsNudgeCreditTypeCredits}) unitTest=accounts_test.go safeIntegration=auth-bound account workflows use isolated test CODEX_HOME and mocked SDK/app-server JSON-RPC fixtures; live auth/Responses proof deferred to Stage 7 docs=README accounts exception= review=SDK-public thin wrapper
- `account/read` status=implemented-stage4 raw=AccountRead wrapper=Accounts.Read file=accounts.go signature=Read(ctx context.Context, refreshToken bool) convention=thin callsite=client.Accounts.Read(ctx, false) unitTest=workflows_test.go safeIntegration=auth-bound account workflows use isolated test CODEX_HOME and mocked SDK/app-server JSON-RPC fixtures; live auth/Responses proof deferred to Stage 7 docs=examples/login_account exception= review=SDK-public thin wrapper

### Apps

serverNotifications=app/list/updated
serverHandlers=

- `app/list` status=implemented-stage5d raw=AppList wrapper=Apps.List file=apps.go signature= convention=thin callsite=client.Apps.List(ctx, protocol.AppsListParams{}) unitTest=apps_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper

### CollaborationModes

serverNotifications=
serverHandlers=

- `collaborationMode/list` status=implemented-stage5e raw=CollaborationModeList wrapper=CollaborationModes.List file=collaboration_modes.go signature= convention=thin callsite=client.CollaborationModes.List(ctx, protocol.CollaborationModeListParams{}) unitTest=collaboration_modes_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5E has package tests only docs=examples/resources exception= review=SDK-public thin wrapper

### Commands

serverNotifications=command/exec/outputDelta
serverHandlers=

- `command/exec` status=implemented-stage5c raw=CommandExec wrapper=Commands.Exec file=commands.go signature= convention=handle-start callsite=cmd, err := client.Commands.Exec(ctx, codex.CommandExecOptions{Command: []string{"echo", "ok"}}); stream, err := cmd.Stream(ctx); result, err := cmd.Wait(ctx) unitTest=commands_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=typed handle workflow; SDK generates/injects process identity; raw params only through generated raw protocol APIs
- `command/exec/write` status=implemented-stage5c raw=CommandExecWrite wrapper=CommandHandle.Write / CloseStdin file=commands.go signature= convention=handle-followup callsite=cmd.Write(ctx, []byte("input")); cmd.CloseStdin(ctx) unitTest=commands_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=typed handle workflow; handle injects process identity
- `command/exec/terminate` status=implemented-stage5c raw=CommandExecTerminate wrapper=CommandHandle.Terminate file=commands.go signature= convention=handle-followup callsite=cmd.Terminate(ctx) unitTest=commands_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=typed handle workflow; handle injects process identity
- `command/exec/resize` status=implemented-stage5c raw=CommandExecResize wrapper=CommandHandle.Resize file=commands.go signature= convention=handle-followup callsite=cmd.Resize(ctx, codex.TerminalSize{Rows: 24, Cols: 80}) unitTest=commands_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=typed handle workflow; handle injects process identity

### Config

serverNotifications=configWarning
serverHandlers=

- `config/mcpServer/reload` status=implemented-stage5c raw=ConfigMcpServerReload wrapper=Config.ReloadMCPServers file=config_resource.go signature= convention=thin callsite=client.Config.ReloadMCPServers(ctx) unitTest=config_resource_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `config/read` status=implemented-stage5c raw=ConfigRead wrapper=Config.Read file=config_resource.go signature= convention=thin callsite=client.Config.Read(ctx, protocol.ConfigReadParams{}) unitTest=config_resource_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `config/value/write` status=implemented-stage5c raw=ConfigValueWrite wrapper=Config.WriteValue file=config_resource.go signature= convention=thin callsite=client.Config.WriteValue(ctx, protocol.ConfigValueWriteParams{}) unitTest=config_resource_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `config/batchWrite` status=implemented-stage5c raw=ConfigBatchWrite wrapper=Config.BatchWrite file=config_resource.go signature= convention=thin callsite=client.Config.BatchWrite(ctx, protocol.ConfigBatchWriteParams{}) unitTest=config_resource_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `configRequirements/read` status=implemented-stage5c raw=ConfigRequirementsRead wrapper=Config.ReadRequirements file=config_resource.go signature= convention=thin callsite=client.Config.ReadRequirements(ctx) unitTest=config_resource_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=README config exception= review=SDK-public thin wrapper

### Environments

serverNotifications=
serverHandlers=

- `environment/add` status=implemented-stage5e raw=EnvironmentAdd wrapper=Environments.Add file=environments.go signature= convention=thin callsite=client.Environments.Add(ctx, protocol.EnvironmentAddParams{EnvironmentID: "env-1", ExecServerURL: "http://127.0.0.1:9876"}) unitTest=environments_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5E has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `environment/info` status=implemented-stage5e raw=EnvironmentInfo wrapper=Environments.Info file=environments.go signature= convention=thin callsite=client.Environments.Info(ctx, protocol.EnvironmentInfoParams{EnvironmentID: "env-1"}) unitTest=environments_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5E has package tests only docs=examples/resources exception= review=SDK-public thin wrapper

### ExperimentalFeatures

serverNotifications=
serverHandlers=

- `experimentalFeature/list` status=implemented-stage5f raw=ExperimentalFeatureList wrapper=ExperimentalFeatures.List file=experimental_features.go signature= convention=thin callsite=client.ExperimentalFeatures.List(ctx, protocol.ExperimentalFeatureListParams{ThreadID: protocol.Some("thread-1")}) unitTest=experimental_features_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5F has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `experimentalFeature/enablement/set` status=implemented-stage5f raw=ExperimentalFeatureEnablementSet wrapper=ExperimentalFeatures.SetEnablement file=experimental_features.go signature= convention=thin callsite=client.ExperimentalFeatures.SetEnablement(ctx, protocol.ExperimentalFeatureEnablementSetParams{Enablement: map[string]bool{"feature": true}}) unitTest=experimental_features_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5F has package tests only docs=examples/resources exception= review=SDK-public thin wrapper

### ExternalAgents

serverNotifications=externalAgentConfig/import/completed,externalAgentConfig/import/progress
serverHandlers=

- `externalAgentConfig/detect` status=implemented-stage5e raw=ExternalAgentConfigDetect wrapper=ExternalAgents.DetectConfig file=external_agents.go signature= convention=thin callsite=client.ExternalAgents.DetectConfig(ctx, protocol.ExternalAgentConfigDetectParams{Cwds: protocol.Some([]string{"/repo"}), IncludeHome: protocol.SomeNonNull(true)}) unitTest=external_agents_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5E has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `externalAgentConfig/import` status=implemented-stage5e raw=ExternalAgentConfigImport wrapper=ExternalAgents.ImportConfig file=external_agents.go signature= convention=thin callsite=client.ExternalAgents.ImportConfig(ctx, protocol.ExternalAgentConfigImportParams{MigrationItems: []protocol.ExternalAgentConfigMigrationItem{}, Source: protocol.Some("codex")}) unitTest=external_agents_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5E has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `externalAgentConfig/import/readHistories` status=implemented-stage5e raw=ExternalAgentConfigImportReadHistories wrapper=ExternalAgents.ReadImportHistories file=external_agents.go signature= convention=thin callsite=client.ExternalAgents.ReadImportHistories(ctx) unitTest=external_agents_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5E has package tests only docs=examples/resources exception= review=SDK-public thin wrapper

### Feedback

serverNotifications=
serverHandlers=

- `feedback/upload` status=implemented-stage5f raw=FeedbackUpload wrapper=Feedback.Upload file=feedback.go signature= convention=thin callsite=client.Feedback.Upload(ctx, protocol.FeedbackUploadParams{Classification: "bug", ThreadID: protocol.Some("thread-1")}) unitTest=feedback_test.go safeIntegration=feedback upload is side-effecting/auth-bound; current Stage 5F has mocked package tests only and live proof deferred to Stage 7 docs=examples/resources exception= review=SDK-public thin wrapper; upload uses generated neverRetryAfterWrite metadata

### FileSystem

serverNotifications=fs/changed
serverHandlers=

- `fs/readFile` status=implemented-stage5c raw=FsReadFile wrapper=FileSystem.ReadFile file=filesystem.go signature= convention=thin callsite=client.FileSystem.ReadFile(ctx, protocol.FsReadFileParams{}) unitTest=filesystem_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `fs/writeFile` status=implemented-stage5c raw=FsWriteFile wrapper=FileSystem.WriteFile file=filesystem.go signature= convention=thin callsite=client.FileSystem.WriteFile(ctx, protocol.FsWriteFileParams{}) unitTest=filesystem_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `fs/createDirectory` status=implemented-stage5c raw=FsCreateDirectory wrapper=FileSystem.CreateDirectory file=filesystem.go signature= convention=thin callsite=client.FileSystem.CreateDirectory(ctx, protocol.FsCreateDirectoryParams{}) unitTest=filesystem_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `fs/getMetadata` status=implemented-stage5c raw=FsGetMetadata wrapper=FileSystem.GetMetadata file=filesystem.go signature= convention=thin callsite=client.FileSystem.GetMetadata(ctx, protocol.FsGetMetadataParams{}) unitTest=filesystem_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `fs/readDirectory` status=implemented-stage5c raw=FsReadDirectory wrapper=FileSystem.ReadDirectory file=filesystem.go signature= convention=thin callsite=client.FileSystem.ReadDirectory(ctx, protocol.FsReadDirectoryParams{}) unitTest=filesystem_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `fs/remove` status=implemented-stage5c raw=FsRemove wrapper=FileSystem.Remove file=filesystem.go signature= convention=thin callsite=client.FileSystem.Remove(ctx, protocol.FsRemoveParams{}) unitTest=filesystem_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `fs/copy` status=implemented-stage5c raw=FsCopy wrapper=FileSystem.Copy file=filesystem.go signature= convention=thin callsite=client.FileSystem.Copy(ctx, protocol.FsCopyParams{}) unitTest=filesystem_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `fs/watch` status=implemented-stage5c raw=FsWatch wrapper=FileSystem.Watch file=filesystem.go signature= convention=handle-start callsite=watch, start, err := client.FileSystem.Watch(ctx, codex.FileSystemWatchOptions{Path: "/repo"}) unitTest=filesystem_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=typed handle workflow; SDK generates/injects watch identity; raw params only through generated raw protocol APIs
- `fs/unwatch` status=implemented-stage5c raw=FsUnwatch wrapper=FileSystemWatchHandle.Close file=filesystem.go signature= convention=handle-followup callsite=watch.Close(ctx) unitTest=filesystem_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=typed handle workflow

### FuzzyFileSearch

serverNotifications=fuzzyFileSearch/sessionCompleted,fuzzyFileSearch/sessionUpdated
serverHandlers=

- `fuzzyFileSearch` status=implemented-stage5f raw=FuzzyFileSearch wrapper=FuzzyFileSearch.Search file=fuzzy_file_search.go signature= convention=thin callsite=client.FuzzyFileSearch.Search(ctx, protocol.FuzzyFileSearchParams{Query: "main.go", Roots: []string{"/repo"}}) unitTest=fuzzy_file_search_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5F has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `fuzzyFileSearch/sessionStart` status=implemented-stage5f raw=FuzzyFileSearchSessionStart wrapper=FuzzyFileSearch.StartSession file=fuzzy_file_search.go signature= convention=handle-start callsite=session, err := client.FuzzyFileSearch.StartSession(ctx, codex.FuzzySearchSessionOptions{Roots: []string{"."}}) unitTest=fuzzy_file_search_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5F has package tests only docs=examples/resources exception= review=typed handle workflow; SDK generates/injects session identity; raw params only through generated raw protocol APIs
- `fuzzyFileSearch/sessionUpdate` status=implemented-stage5f raw=FuzzyFileSearchSessionUpdate wrapper=FuzzySearchSession.Update file=fuzzy_file_search.go signature= convention=handle-followup callsite=session.Update(ctx, codex.FuzzySearchUpdate{Query: "main.go"}) unitTest=fuzzy_file_search_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5F has package tests only docs=examples/resources exception= review=typed handle workflow; handle injects session identity
- `fuzzyFileSearch/sessionStop` status=implemented-stage5f raw=FuzzyFileSearchSessionStop wrapper=FuzzySearchSession.Close file=fuzzy_file_search.go signature= convention=handle-followup callsite=session.Close(ctx) unitTest=fuzzy_file_search_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5F has package tests only docs=examples/resources exception= review=typed handle workflow

### Hooks

serverNotifications=hook/completed,hook/started
serverHandlers=

- `hooks/list` status=implemented-stage5b raw=HooksList wrapper=Hooks.List file=hooks.go signature= convention=thin callsite=client.Hooks.List(ctx, protocol.HooksListParams{}) unitTest=hooks_test.go safeIntegration=integration_app_server_test.go docs=examples/skills_hooks exception= review=SDK-public thin wrapper

### MCP

serverNotifications=mcpServer/oauthLogin/completed,mcpServer/startupStatus/updated
serverHandlers=mcpServer/elicitation/request(mcp-elicitation)

- `mcpServer/oauth/login` status=implemented-stage4 raw=McpServerOauthLogin wrapper=MCP.OAuthLogin / MCPOAuthHandle file=mcp.go signature= convention=handle-start callsite=oauth, err := client.MCP.OAuthLogin(ctx, codex.MCPOAuthLoginOptions{Name: "github"}); result, err := oauth.Wait(ctx) unitTest=workflows_test.go safeIntegration=integration_app_server_test.go docs=examples/resources exception= review=typed MCP OAuth handle workflow routed by terminal completion notification; raw params only through generated raw protocol APIs
- `mcpServer/resource/read` status=implemented-stage5d raw=McpServerResourceRead wrapper=MCP.ReadResource file=mcp.go signature= convention=thin callsite=client.MCP.ReadResource(ctx, protocol.McpResourceReadParams{Server: "github", Uri: "file:///README.md"}) unitTest=mcp_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `mcpServer/tool/call` status=implemented-stage5d raw=McpServerToolCall wrapper=MCP.CallTool file=mcp.go signature= convention=thin callsite=client.MCP.CallTool(ctx, protocol.McpServerToolCallParams{Server: "github", ThreadID: "thread-id", Tool: "search"}) unitTest=mcp_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `mcpServerStatus/list` status=implemented-stage5d raw=McpServerStatusList wrapper=MCP.ListStatus file=mcp.go signature= convention=thin callsite=client.MCP.ListStatus(ctx, protocol.ListMcpServerStatusParams{}) unitTest=mcp_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper

### Marketplace

serverNotifications=
serverHandlers=

- `marketplace/add` status=implemented-stage5d raw=MarketplaceAdd wrapper=Marketplace.Add file=marketplace.go signature= convention=thin callsite=client.Marketplace.Add(ctx, protocol.MarketplaceAddParams{Source: "https://example.test/marketplace.git"}) unitTest=marketplace_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `marketplace/remove` status=implemented-stage5d raw=MarketplaceRemove wrapper=Marketplace.Remove file=marketplace.go signature= convention=thin callsite=client.Marketplace.Remove(ctx, protocol.MarketplaceRemoveParams{MarketplaceName: "default"}) unitTest=marketplace_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `marketplace/upgrade` status=implemented-stage5d raw=MarketplaceUpgrade wrapper=Marketplace.Upgrade file=marketplace.go signature= convention=thin callsite=client.Marketplace.Upgrade(ctx, protocol.MarketplaceUpgradeParams{}) unitTest=marketplace_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper

### Memory

serverNotifications=
serverHandlers=

- `memory/reset` status=implemented-stage5f raw=MemoryReset wrapper=Memory.Reset file=memory.go signature= convention=thin callsite=client.Memory.Reset(ctx) unitTest=memory_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5F has package tests only docs=examples/resources exception= review=SDK-public thin wrapper

### Models

serverNotifications=model/rerouted,model/safetyBuffering/updated,model/verification
serverHandlers=

- `model/list` status=implemented-stage5e raw=ModelList wrapper=Models.List file=models.go signature= convention=thin callsite=client.Models.List(ctx, protocol.ModelListParams{}) unitTest=models_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5E has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `modelProvider/capabilities/read` status=implemented-stage5e raw=ModelProviderCapabilitiesRead wrapper=Models.ReadProviderCapabilities file=models.go signature= convention=thin callsite=client.Models.ReadProviderCapabilities(ctx, protocol.ModelProviderCapabilitiesReadParams{}) unitTest=models_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5E has package tests only docs=examples/resources exception= review=SDK-public thin wrapper

### PermissionProfiles

serverNotifications=
serverHandlers=

- `permissionProfile/list` status=implemented-stage5f raw=PermissionProfileList wrapper=PermissionProfiles.List file=permission_profiles.go signature= convention=thin callsite=client.PermissionProfiles.List(ctx, protocol.PermissionProfileListParams{Cwd: protocol.Some("/repo")}) unitTest=permission_profiles_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5F has package tests only docs=examples/resources exception= review=SDK-public thin wrapper

### Plugins

serverNotifications=
serverHandlers=

- `plugin/list` status=implemented-stage5d raw=PluginList wrapper=Plugins.List file=plugins.go signature= convention=thin callsite=client.Plugins.List(ctx, protocol.PluginListParams{}) unitTest=plugins_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `plugin/installed` status=implemented-stage5d raw=PluginInstalled wrapper=Plugins.Installed file=plugins.go signature= convention=thin callsite=client.Plugins.Installed(ctx, protocol.PluginInstalledParams{}) unitTest=plugins_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `plugin/read` status=implemented-stage5d raw=PluginRead wrapper=Plugins.Read file=plugins.go signature= convention=thin callsite=client.Plugins.Read(ctx, protocol.PluginReadParams{PluginName: "plugin"}) unitTest=plugins_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `plugin/skill/read` status=implemented-stage5d raw=PluginSkillRead wrapper=Plugins.ReadSkill file=plugins.go signature= convention=thin callsite=client.Plugins.ReadSkill(ctx, protocol.PluginSkillReadParams{RemoteMarketplaceName: "marketplace", RemotePluginID: "plugin-id", SkillName: "skill"}) unitTest=plugins_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/skills_hooks exception= review=SDK-public thin wrapper
- `plugin/share/save` status=implemented-stage5d raw=PluginShareSave wrapper=Plugins.SaveShare file=plugins.go signature= convention=thin callsite=client.Plugins.SaveShare(ctx, protocol.PluginShareSaveParams{PluginPath: "/repo/plugin"}) unitTest=plugins_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `plugin/share/updateTargets` status=implemented-stage5d raw=PluginShareUpdateTargets wrapper=Plugins.UpdateShareTargets file=plugins.go signature= convention=thin callsite=client.Plugins.UpdateShareTargets(ctx, protocol.PluginShareUpdateTargetsParams{Discoverability: protocol.PluginShareUpdateDiscoverabilityPrivate, RemotePluginID: "plugin-id", ShareTargets: []protocol.PluginShareTarget{}}) unitTest=plugins_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `plugin/share/list` status=implemented-stage5d raw=PluginShareList wrapper=Plugins.ListShares file=plugins.go signature= convention=thin callsite=client.Plugins.ListShares(ctx, protocol.PluginShareListParams{}) unitTest=plugins_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `plugin/share/checkout` status=implemented-stage5d raw=PluginShareCheckout wrapper=Plugins.CheckoutShare file=plugins.go signature= convention=thin callsite=client.Plugins.CheckoutShare(ctx, protocol.PluginShareCheckoutParams{RemotePluginID: "plugin-id"}) unitTest=plugins_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `plugin/share/delete` status=implemented-stage5d raw=PluginShareDelete wrapper=Plugins.DeleteShare file=plugins.go signature= convention=thin callsite=client.Plugins.DeleteShare(ctx, protocol.PluginShareDeleteParams{RemotePluginID: "plugin-id"}) unitTest=plugins_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `plugin/install` status=implemented-stage5d raw=PluginInstall wrapper=Plugins.Install file=plugins.go signature= convention=thin callsite=client.Plugins.Install(ctx, protocol.PluginInstallParams{PluginName: "plugin"}) unitTest=plugins_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper
- `plugin/uninstall` status=implemented-stage5d raw=PluginUninstall wrapper=Plugins.Uninstall file=plugins.go signature= convention=thin callsite=client.Plugins.Uninstall(ctx, protocol.PluginUninstallParams{PluginID: "plugin-id"}) unitTest=plugins_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5D has package tests only docs=examples/resources exception= review=SDK-public thin wrapper

### Processes

serverNotifications=process/exited,process/outputDelta
serverHandlers=

- `process/spawn` status=implemented-stage5c raw=ProcessSpawn wrapper=Processes.Spawn / process handle file=processes.go signature= convention=handle-start callsite=proc, start, err := client.Processes.Spawn(ctx, codex.ProcessSpawnOptions{Command: []string{"echo", "ok"}, CWD: "/repo"}) unitTest=processes_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=typed handle workflow; SDK generates/injects process handle identity; raw params only through generated raw protocol APIs
- `process/writeStdin` status=implemented-stage5c raw=ProcessWriteStdin wrapper=ProcessHandle.WriteStdin / CloseStdin file=processes.go signature= convention=handle-followup callsite=proc.WriteStdin(ctx, []byte("input")); proc.CloseStdin(ctx) unitTest=processes_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=typed handle workflow; handle injects process identity
- `process/kill` status=implemented-stage5c raw=ProcessKill wrapper=ProcessHandle.Kill file=processes.go signature= convention=handle-followup callsite=proc.Kill(ctx) unitTest=processes_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=typed handle workflow; handle injects process identity
- `process/resizePty` status=implemented-stage5c raw=ProcessResizePty wrapper=ProcessHandle.ResizePTY file=processes.go signature= convention=handle-followup callsite=proc.ResizePTY(ctx, codex.TerminalSize{Rows: 24, Cols: 80}) unitTest=processes_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5C has package tests only docs=examples/resources exception= review=typed handle workflow; handle injects process identity

### Realtime

serverNotifications=thread/realtime/closed,thread/realtime/error,thread/realtime/itemAdded,thread/realtime/outputAudio/delta,thread/realtime/sdp,thread/realtime/started,thread/realtime/transcript/delta,thread/realtime/transcript/done
serverHandlers=

- `thread/realtime/start` status=implemented-stage5b raw=ThreadRealtimeStart wrapper=Realtime.Start / realtime handle file=realtime.go signature= convention=handle-start callsite=session, start, err := client.Realtime.Start(ctx, codex.RealtimeStartOptions{ThreadID: thread.ID()}) unitTest=realtime_test.go safeIntegration=integration_app_server_test.go docs=examples/resources exception= review=typed handle workflow; SDK owns realtime session handle identity where the manifest marks it client-supplied; raw params only through generated raw protocol APIs
- `thread/realtime/appendAudio` status=implemented-stage5b raw=ThreadRealtimeAppendAudio wrapper=RealtimeSession.AppendAudio file=realtime.go signature= convention=handle-followup callsite=session.AppendAudio(ctx, codex.AudioChunk{Data: audio}) unitTest=realtime_test.go safeIntegration=integration_app_server_test.go docs=examples/resources exception= review=typed handle workflow; handle injects thread identity while current protocol lacks follow-up session identity
- `thread/realtime/appendText` status=implemented-stage5b raw=ThreadRealtimeAppendText wrapper=RealtimeSession.AppendText file=realtime.go signature= convention=handle-followup callsite=session.AppendText(ctx, "hello") unitTest=realtime_test.go safeIntegration=integration_app_server_test.go docs=examples/resources exception= review=typed handle workflow; handle injects thread identity while current protocol lacks follow-up session identity
- `thread/realtime/appendSpeech` status=implemented-stage5b raw=ThreadRealtimeAppendSpeech wrapper=RealtimeSession.AppendSpeech file=realtime.go signature= convention=handle-followup callsite=session.AppendSpeech(ctx, codex.SpeechInput{Text: "hello"}) unitTest=realtime_test.go safeIntegration=integration_app_server_test.go docs=examples/resources exception= review=typed handle workflow; handle injects thread identity while current protocol lacks follow-up session identity
- `thread/realtime/stop` status=implemented-stage5b raw=ThreadRealtimeStop wrapper=RealtimeSession.Stop file=realtime.go signature= convention=handle-followup callsite=session.Stop(ctx) unitTest=realtime_test.go safeIntegration=integration_app_server_test.go docs=examples/resources exception= review=typed handle workflow
- `thread/realtime/listVoices` status=implemented-stage5b raw=ThreadRealtimeListVoices wrapper=Realtime.ListVoices file=realtime.go signature= convention=thin callsite=client.Realtime.ListVoices(ctx, protocol.ThreadRealtimeListVoicesParams{}) unitTest=realtime_test.go safeIntegration=integration_app_server_test.go docs=examples/resources exception= review=SDK-public thin wrapper

### RemoteControl

serverNotifications=remoteControl/status/changed
serverHandlers=

- `remoteControl/enable` status=implemented-stage5e raw=RemoteControlEnable wrapper=RemoteControl.Enable file=remote_control.go signature= convention=thin callsite=client.RemoteControl.Enable(ctx, protocol.NullableRemoteControlEnableParams{}) unitTest=remote_control_test.go safeIntegration=paired remote-control service/session unavailable in hermetic app-server fixture; current Stage 5E has package tests only docs=examples/resources exception= review=public wrapper, live workflow requires paired CI environment
- `remoteControl/disable` status=implemented-stage5e raw=RemoteControlDisable wrapper=RemoteControl.Disable file=remote_control.go signature= convention=thin callsite=client.RemoteControl.Disable(ctx, protocol.NullableRemoteControlDisableParams{}) unitTest=remote_control_test.go safeIntegration=paired remote-control service/session unavailable in hermetic app-server fixture; current Stage 5E has package tests only docs=examples/resources exception= review=public wrapper, live workflow requires paired CI environment
- `remoteControl/status/read` status=implemented-stage5e raw=RemoteControlStatusRead wrapper=RemoteControl.ReadStatus file=remote_control.go signature= convention=thin callsite=client.RemoteControl.ReadStatus(ctx) unitTest=remote_control_test.go safeIntegration=paired remote-control service/session unavailable in hermetic app-server fixture; current Stage 5E has package tests only docs=examples/resources exception= review=public wrapper, live workflow requires paired CI environment
- `remoteControl/pairing/start` status=implemented-stage5e raw=RemoteControlPairingStart wrapper=RemoteControl.StartPairing / RemoteControlPairingHandle file=remote_control.go signature= convention=handle-start callsite=pairing, start, err := client.RemoteControl.StartPairing(ctx, codex.RemoteControlPairingOptions{ManualCode: true}) unitTest=remote_control_test.go safeIntegration=paired remote-control service/session unavailable in hermetic app-server fixture; current Stage 5E has package tests only docs=examples/resources exception= review=typed pairing handle workflow; handle owns pairing code/session data; raw params only through generated raw protocol APIs
- `remoteControl/pairing/status` status=implemented-stage5e raw=RemoteControlPairingStatus wrapper=RemoteControlPairingHandle.Status / RemoteControl.PairingStatus file=remote_control.go signature= convention=handle-followup callsite=status, err := pairing.Status(ctx); status, err = client.RemoteControl.PairingStatus(ctx, pairing.ID()) unitTest=remote_control_test.go safeIntegration=paired remote-control service/session unavailable in hermetic app-server fixture; current Stage 5E has package tests only docs=examples/resources exception= review=typed pairing handle workflow; handle injects pairing identity when protocol exposes one, otherwise root method accepts the start-returned pairing code/session token without exposing raw params
- `remoteControl/client/list` status=implemented-stage5e raw=RemoteControlClientList wrapper=RemoteControl.ListClients file=remote_control.go signature= convention=thin callsite=client.RemoteControl.ListClients(ctx, protocol.RemoteControlClientsListParams{EnvironmentID: "env-1"}) unitTest=remote_control_test.go safeIntegration=paired remote-control service/session unavailable in hermetic app-server fixture; current Stage 5E has package tests only docs=examples/resources exception= review=public wrapper, live workflow requires paired CI environment
- `remoteControl/client/revoke` status=implemented-stage5e raw=RemoteControlClientRevoke wrapper=RemoteControl.RevokeClient file=remote_control.go signature= convention=thin callsite=client.RemoteControl.RevokeClient(ctx, protocol.RemoteControlClientsRevokeParams{ClientID: "client-1", EnvironmentID: "env-1"}) unitTest=remote_control_test.go safeIntegration=paired remote-control service/session unavailable in hermetic app-server fixture; current Stage 5E has package tests only docs=examples/resources exception= review=public wrapper, live workflow requires paired CI environment

### Reviews

serverNotifications=error,guardianWarning,item/agentMessage/delta,item/commandExecution/outputDelta,item/commandExecution/terminalInteraction,item/completed,item/fileChange/outputDelta,item/fileChange/patchUpdated,item/mcpToolCall/progress,item/plan/delta,item/reasoning/summaryPartAdded,item/reasoning/summaryTextDelta,item/reasoning/textDelta,item/started,rawResponseItem/completed,serverRequest/resolved,turn/completed,turn/diff/updated,turn/moderationMetadata,turn/plan/updated,turn/started,warning
serverHandlers=

- `review/start` status=implemented-stage4 raw=ReviewStart wrapper=Reviews.Start / ReviewHandle file=reviews.go signature= convention=handle-start callsite=review, err := client.Reviews.Start(ctx, codex.ReviewStartOptions{ThreadID: thread.ID(), Target: codex.UncommittedChangesReviewTarget()}); result, err := review.Wait(ctx) unitTest=workflows_test.go safeIntegration=Stage 7 live app-server integration proof pending; current Stage 5E has package tests only docs=examples/reviews exception= review=typed review handle workflow owning reviewThreadId and turn.id from ReviewStartResponse, routing ordinary review turn lifecycle/result events; raw params only through generated raw protocol APIs

### Skills

serverNotifications=skills/changed
serverHandlers=

- `skills/list` status=implemented-stage5b raw=SkillsList wrapper=Skills.List file=skills.go signature= convention=thin callsite=client.Skills.List(ctx, protocol.SkillsListParams{}) unitTest=skills_test.go safeIntegration=integration_app_server_test.go docs=examples/skills_hooks exception= review=SDK-public thin wrapper
- `skills/extraRoots/set` status=implemented-stage5b raw=SkillsExtraRootsSet wrapper=Skills.SetExtraRoots file=skills.go signature= convention=thin callsite=client.Skills.SetExtraRoots(ctx, protocol.SkillsExtraRootsSetParams{}) unitTest=skills_test.go safeIntegration=integration_app_server_test.go docs=examples/skills_hooks exception= review=SDK-public thin wrapper
- `skills/config/write` status=implemented-stage5b raw=SkillsConfigWrite wrapper=Skills.WriteConfig file=skills.go signature= convention=thin callsite=client.Skills.WriteConfig(ctx, protocol.SkillsConfigWriteParams{}) unitTest=skills_test.go safeIntegration=integration_app_server_test.go docs=examples/skills_hooks exception= review=SDK-public thin wrapper

### Threads

serverNotifications=deprecationNotice,error,guardianWarning,serverRequest/resolved,thread/archived,thread/closed,thread/compacted,thread/deleted,thread/goal/cleared,thread/goal/updated,thread/name/updated,thread/realtime/closed,thread/realtime/error,thread/realtime/itemAdded,thread/realtime/outputAudio/delta,thread/realtime/sdp,thread/realtime/started,thread/realtime/transcript/delta,thread/realtime/transcript/done,thread/settings/updated,thread/started,thread/status/changed,thread/tokenUsage/updated,thread/unarchived,warning
serverHandlers=currentTime/read(current-time-read)

- `thread/start` status=implemented-stage4 raw=ThreadStart wrapper=Threads.Start file=thread.go signature= convention=high-level callsite=client.Threads.Start(ctx, codex.ThreadStartOptions{CWD: "/repo", Permissions: "workspace-write"}) unitTest=workflows_test.go safeIntegration=integration_app_server_test.go docs=examples/run exception= review=high-level ergonomic workflow using root SDK options plus raw generated method
- `thread/resume` status=implemented-stage5b raw=ThreadResume wrapper=Threads.Resume file=thread.go signature= convention=high-level callsite=client.Threads.Resume(ctx, codex.ThreadResumeOptions{ThreadID: "thread-id"}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=examples/resources exception= review=high-level ergonomic workflow using root SDK options plus raw generated method
- `thread/fork` status=implemented-stage5b raw=ThreadFork wrapper=Threads.Fork file=thread.go signature= convention=high-level callsite=client.Threads.Fork(ctx, codex.ThreadForkOptions{ThreadID: "thread-id"}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=examples/resources exception= review=high-level ergonomic workflow using root SDK options plus raw generated method
- `thread/archive` status=implemented-stage5b raw=ThreadArchive wrapper=Threads.Archive file=thread.go signature= convention=thin callsite=client.Threads.Archive(ctx, protocol.ThreadArchiveParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=examples/resources exception= review=SDK-public thin wrapper
- `thread/delete` status=implemented-stage5b raw=ThreadDelete wrapper=Threads.Delete file=thread.go signature= convention=thin callsite=client.Threads.Delete(ctx, protocol.ThreadDeleteParams{}) unitTest=thread_test.go safeIntegration=destructive thread mutation requires isolated CODEX_HOME fixture to avoid user data loss docs=examples/resources exception= review=SDK-public thin wrapper
- `thread/unsubscribe` status=implemented-stage5b raw=ThreadUnsubscribe wrapper=Thread.Unsubscribe file=thread.go signature= convention=handle-followup callsite=thread.Unsubscribe(ctx) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=typed handle workflow; handle injects thread identity
- `thread/name/set` status=implemented-stage5b raw=ThreadNameSet wrapper=Threads.SetName file=thread.go signature= convention=thin callsite=client.Threads.SetName(ctx, protocol.ThreadSetNameParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/goal/set` status=implemented-stage5b raw=ThreadGoalSet wrapper=Threads.SetGoal file=thread.go signature= convention=thin callsite=client.Threads.SetGoal(ctx, protocol.ThreadGoalSetParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/goal/get` status=implemented-stage5b raw=ThreadGoalGet wrapper=Threads.GetGoal file=thread.go signature= convention=thin callsite=client.Threads.GetGoal(ctx, protocol.ThreadGoalGetParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/goal/clear` status=implemented-stage5b raw=ThreadGoalClear wrapper=Threads.ClearGoal file=thread.go signature= convention=thin callsite=client.Threads.ClearGoal(ctx, protocol.ThreadGoalClearParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/metadata/update` status=implemented-stage5b raw=ThreadMetadataUpdate wrapper=Threads.UpdateMetadata file=thread.go signature= convention=thin callsite=client.Threads.UpdateMetadata(ctx, protocol.ThreadMetadataUpdateParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/unarchive` status=implemented-stage5b raw=ThreadUnarchive wrapper=Threads.Unarchive file=thread.go signature= convention=thin callsite=client.Threads.Unarchive(ctx, protocol.ThreadUnarchiveParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=examples/resources exception= review=SDK-public thin wrapper
- `thread/compact/start` status=implemented-stage5b raw=ThreadCompactStart wrapper=Threads.StartCompaction file=thread.go signature= convention=thin callsite=client.Threads.StartCompaction(ctx, protocol.ThreadCompactStartParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/shellCommand` status=implemented-stage5b raw=ThreadShellCommand wrapper=Threads.ShellCommand file=thread.go signature= convention=thin callsite=client.Threads.ShellCommand(ctx, protocol.ThreadShellCommandParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/approveGuardianDeniedAction` status=implemented-stage5b raw=ThreadApproveGuardianDeniedAction wrapper=Threads.ApproveGuardianDeniedAction file=thread.go signature= convention=thin callsite=client.Threads.ApproveGuardianDeniedAction(ctx, protocol.ThreadApproveGuardianDeniedActionParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/rollback` status=implemented-stage5b raw=ThreadRollback wrapper=Threads.Rollback file=thread.go signature= convention=thin callsite=client.Threads.Rollback(ctx, protocol.ThreadRollbackParams{}) unitTest=thread_test.go safeIntegration=destructive thread mutation requires isolated CODEX_HOME fixture to avoid user data loss docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/list` status=implemented-stage5b raw=ThreadList wrapper=Threads.List file=thread.go signature= convention=thin callsite=client.Threads.List(ctx, protocol.ThreadListParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=examples/resources exception= review=SDK-public thin wrapper
- `thread/loaded/list` status=implemented-stage5b raw=ThreadLoadedList wrapper=Threads.ListLoaded file=thread.go signature= convention=thin callsite=client.Threads.ListLoaded(ctx, protocol.ThreadLoadedListParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/read` status=implemented-stage5b raw=ThreadRead wrapper=Threads.Read file=thread.go signature= convention=thin callsite=client.Threads.Read(ctx, protocol.ThreadReadParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=examples/raw_protocol exception= review=SDK-public thin wrapper
- `thread/inject_items` status=implemented-stage5b raw=ThreadInjectItems wrapper=Threads.InjectItems file=thread.go signature= convention=thin callsite=client.Threads.InjectItems(ctx, protocol.ThreadInjectItemsParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/increment_elicitation` status=implemented-stage5b raw=ThreadIncrementElicitation wrapper=Thread.IncrementElicitation / Threads.IncrementElicitation file=thread.go signature= convention=handle-followup callsite=thread.IncrementElicitation(ctx); client.Threads.IncrementElicitation(ctx, thread.ID()) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=typed handle workflow; handle method injects thread identity, root resource method accepts thread identity without exposing raw params
- `thread/decrement_elicitation` status=implemented-stage5b raw=ThreadDecrementElicitation wrapper=Thread.DecrementElicitation / Threads.DecrementElicitation file=thread.go signature= convention=handle-followup callsite=thread.DecrementElicitation(ctx); client.Threads.DecrementElicitation(ctx, thread.ID()) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=typed handle workflow; handle method injects thread identity, root resource method accepts thread identity without exposing raw params
- `thread/settings/update` status=implemented-stage5b raw=ThreadSettingsUpdate wrapper=Threads.UpdateSettings file=thread.go signature= convention=thin callsite=client.Threads.UpdateSettings(ctx, protocol.ThreadSettingsUpdateParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/memoryMode/set` status=implemented-stage5b raw=ThreadMemoryModeSet wrapper=Threads.SetMemoryMode file=thread.go signature= convention=thin callsite=client.Threads.SetMemoryMode(ctx, protocol.ThreadMemoryModeSetParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/backgroundTerminals/clean` status=implemented-stage5b raw=ThreadBackgroundTerminalsClean wrapper=Threads.CleanBackgroundTerminals file=thread.go signature= convention=thin callsite=client.Threads.CleanBackgroundTerminals(ctx, protocol.ThreadBackgroundTerminalsCleanParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/backgroundTerminals/list` status=implemented-stage5b raw=ThreadBackgroundTerminalsList wrapper=Threads.ListBackgroundTerminals file=thread.go signature= convention=thin callsite=client.Threads.ListBackgroundTerminals(ctx, protocol.ThreadBackgroundTerminalsListParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/backgroundTerminals/terminate` status=implemented-stage5b raw=ThreadBackgroundTerminalsTerminate wrapper=Threads.TerminateBackgroundTerminal file=thread.go signature= convention=thin callsite=client.Threads.TerminateBackgroundTerminal(ctx, protocol.ThreadBackgroundTerminalsTerminateParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/search` status=implemented-stage5b raw=ThreadSearch wrapper=Threads.Search file=thread.go signature= convention=thin callsite=client.Threads.Search(ctx, protocol.ThreadSearchParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=examples/resources exception= review=SDK-public thin wrapper
- `thread/turns/list` status=implemented-stage5b raw=ThreadTurnsList wrapper=Threads.ListTurns file=thread.go signature= convention=thin callsite=client.Threads.ListTurns(ctx, protocol.ThreadTurnsListParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper
- `thread/items/list` status=implemented-stage5b raw=ThreadItemsList wrapper=Threads.ListItems file=thread.go signature= convention=thin callsite=client.Threads.ListItems(ctx, protocol.ThreadItemsListParams{}) unitTest=thread_test.go safeIntegration=integration_app_server_test.go docs=README thread lifecycle exception= review=SDK-public thin wrapper

### Turns

serverNotifications=error,guardianWarning,item/agentMessage/delta,item/autoApprovalReview/completed,item/autoApprovalReview/started,item/commandExecution/outputDelta,item/commandExecution/terminalInteraction,item/completed,item/fileChange/outputDelta,item/fileChange/patchUpdated,item/mcpToolCall/progress,item/plan/delta,item/reasoning/summaryPartAdded,item/reasoning/summaryTextDelta,item/reasoning/textDelta,item/started,rawResponseItem/completed,serverRequest/resolved,turn/completed,turn/diff/updated,turn/moderationMetadata,turn/plan/updated,turn/started,warning
serverHandlers=item/commandExecution/requestApproval(command-execution-approval),item/fileChange/requestApproval(file-change-approval),item/permissions/requestApproval(permission-approval),item/tool/call(dynamic-tool-call),item/tool/requestUserInput(tool-user-input)

- `turn/start` status=implemented-stage4 raw=TurnStart wrapper=Thread.Run / Thread.Turn / TurnHandle.Stream file=turn.go signature= convention=high-level callsite=thread.Run(ctx, codex.Text("inspect this repo"), codex.TurnOptions{Model: "gpt-5.4"}); turn, err := thread.Turn(ctx, codex.Text("continue"), codex.TurnOptions{}); stream, err := turn.Stream(ctx) unitTest=workflows_test.go safeIntegration=integration_app_server_test.go docs=examples/run exception= review=high-level ergonomic workflow using root SDK input/options plus raw generated method
- `turn/steer` status=implemented-stage4 raw=TurnSteer wrapper=TurnHandle.Steer file=turn.go signature= convention=handle-followup callsite=turn.Steer(ctx, codex.Text("steer toward tests")) unitTest=workflows_test.go safeIntegration=integration_app_server_test.go docs=examples/streaming exception= review=typed handle workflow using root SDK input helpers; raw params only through generated raw protocol APIs
- `turn/interrupt` status=implemented-stage4 raw=TurnInterrupt wrapper=TurnHandle.Interrupt file=turn.go signature= convention=handle-followup callsite=turn.Interrupt(ctx) unitTest=workflows_test.go safeIntegration=integration_app_server_test.go docs=examples/streaming exception= review=typed handle workflow

### WindowsSandbox

serverNotifications=windows/worldWritableWarning,windowsSandbox/setupCompleted
serverHandlers=

- `windowsSandbox/setupStart` status=implemented-stage5f raw=WindowsSandboxSetupStart wrapper=WindowsSandbox.SetupStart file=windows_sandbox.go signature= convention=thin callsite=client.WindowsSandbox.SetupStart(ctx, protocol.WindowsSandboxSetupStartParams{Mode: protocol.WindowsSandboxSetupModeUnelevated}) unitTest=windows_sandbox_test.go safeIntegration=Windows sandbox is Windows-runtime-bound; current Stage 5F has package tests for unsupported runtimes and mocked Windows-runtime JSON-RPC only docs=examples/resources exception= review=public wrapper with typed unsupported-platform error on non-Windows app-server runtimes
- `windowsSandbox/readiness` status=implemented-stage5f raw=WindowsSandboxReadiness wrapper=WindowsSandbox.Readiness file=windows_sandbox.go signature= convention=thin callsite=client.WindowsSandbox.Readiness(ctx) unitTest=windows_sandbox_test.go safeIntegration=Windows sandbox is Windows-runtime-bound; current Stage 5F has package tests for unsupported runtimes and mocked Windows-runtime JSON-RPC only docs=examples/resources exception= review=public wrapper with typed unsupported-platform readiness status on non-Windows app-server runtimes

### compatibility

serverNotifications=
serverHandlers=

- `getConversationSummary` status=generated-only raw=GetConversationSummary wrapper= file= signature= convention=compatibility-only callsite= unitTest=protocol_test.go safeIntegration= docs=internal manifest exception only; no public docs/examples exception=internal compatibility dispatch/decode only; no public Raw() method review=deprecated v1 compatibility surface
- `gitDiffToRemote` status=generated-only raw=GitDiffToRemote wrapper= file= signature= convention=compatibility-only callsite= unitTest=protocol_test.go safeIntegration= docs=internal manifest exception only; no public docs/examples exception=internal compatibility dispatch/decode only; no public Raw() method review=deprecated v1 compatibility surface
- `getAuthStatus` status=generated-only raw=GetAuthStatus wrapper= file= signature= convention=compatibility-only callsite= unitTest=protocol_test.go safeIntegration= docs=internal manifest exception only; no public docs/examples exception=internal compatibility dispatch/decode only; no public Raw() method review=deprecated in favor of account/read

### handshake

serverNotifications=
serverHandlers=

- `initialize` status=generated-only raw=Initialize wrapper= file= signature= convention=handshake-only callsite= unitTest=compatibility_test.go safeIntegration= docs=internal handshake manifest exception only; no public raw/resource docs exception=no public Raw().Initialize or resource wrapper review=NewClient owns initialize and generated initialized

### internal test only

serverNotifications=
serverHandlers=

- `mock/experimentalMethod` status=generated-only raw=MockExperimentalMethod wrapper= file=protocol_test.go signature= convention=internal-test-only callsite= unitTest=protocol_test.go safeIntegration=internal test only docs=none; test-only manifest exception exception=internal test-only manifest exception; no public API/docs/raw method review=internal test-only manifest exception; no public API/docs/raw method


## Server Requests

- `account/chatgptAuthTokens/refresh` handler=ServerHandlers.ChatGPTTokenRefresh visibility=sdk-public capability=chatgpt-token-refresh unitTest=handlers_test.go docs=README server handlers exception= review=public handler
- `applyPatchApproval` handler=internal compatibility dispatch/decode only; no public handler field visibility=compatibility-only capability=legacy-apply-patch-approval unitTest=handlers_test.go docs=internal manifest exception only; no public docs/examples exception=internal compatibility dispatch/decode only; no public handler field review=deprecated v1 compatibility request
- `attestation/generate` handler=ServerHandlers.Attestation visibility=sdk-public capability=attestation-generate unitTest=handlers_test.go docs=README server handlers exception= review=public handler
- `execCommandApproval` handler=internal compatibility dispatch/decode only; no public handler field visibility=compatibility-only capability=legacy-exec-command-approval unitTest=handlers_test.go docs=internal manifest exception only; no public docs/examples exception=internal compatibility dispatch/decode only; no public handler field review=deprecated v1 compatibility request
- `item/commandExecution/requestApproval` handler=ServerHandlers.Approvals visibility=sdk-public capability=command-execution-approval unitTest=handlers_test.go docs=examples/server_handlers exception= review=public handler
- `item/fileChange/requestApproval` handler=ServerHandlers.Approvals visibility=sdk-public capability=file-change-approval unitTest=handlers_test.go docs=examples/server_handlers exception= review=public handler
- `item/tool/requestUserInput` handler=ServerHandlers.UserInput visibility=sdk-public capability=tool-user-input unitTest=handlers_test.go docs=examples/server_handlers exception= review=public handler
- `item/permissions/requestApproval` handler=ServerHandlers.Permissions visibility=sdk-public capability=permission-approval unitTest=handlers_test.go docs=examples/server_handlers exception= review=public handler
- `item/tool/call` handler=ServerHandlers.DynamicTools visibility=sdk-public capability=dynamic-tool-call unitTest=handlers_test.go docs=examples/server_handlers exception= review=public handler
- `mcpServer/elicitation/request` handler=ServerHandlers.MCPElicitation visibility=sdk-public capability=mcp-elicitation unitTest=handlers_test.go docs=examples/server_handlers exception= review=public handler
- `currentTime/read` handler=ServerHandlers.CurrentTime visibility=experimental-public capability=current-time-read unitTest=handlers_test.go docs=README experimental server handlers exception= review=experimental public handler; default behavior may return typed unsupported error when unset

## Server Notifications

- `error` payload=ErrorNotification visibility=public routing=routed routeDomains=error
- `thread/started` payload=ThreadStartedNotification visibility=public routing=routed routeDomains=thread
- `thread/status/changed` payload=ThreadStatusChangedNotification visibility=public routing=routed routeDomains=thread
- `thread/archived` payload=ThreadArchivedNotification visibility=public routing=routed routeDomains=thread
- `thread/deleted` payload=ThreadDeletedNotification visibility=public routing=routed routeDomains=thread
- `thread/unarchived` payload=ThreadUnarchivedNotification visibility=public routing=routed routeDomains=thread
- `thread/closed` payload=ThreadClosedNotification visibility=public routing=routed routeDomains=thread
- `skills/changed` payload=SkillsChangedNotification visibility=public routing=globalOnly routeDomains=
- `thread/name/updated` payload=ThreadNameUpdatedNotification visibility=public routing=routed routeDomains=thread
- `thread/goal/updated` payload=ThreadGoalUpdatedNotification visibility=public routing=routed routeDomains=thread
- `thread/goal/cleared` payload=ThreadGoalClearedNotification visibility=public routing=routed routeDomains=thread
- `thread/settings/updated` payload=ThreadSettingsUpdatedNotification visibility=public routing=routed routeDomains=thread
- `thread/tokenUsage/updated` payload=ThreadTokenUsageUpdatedNotification visibility=public routing=routed routeDomains=thread
- `turn/started` payload=TurnStartedNotification visibility=public routing=routed routeDomains=turn
- `hook/started` payload=HookStartedNotification visibility=public routing=routed routeDomains=hook
- `turn/completed` payload=TurnCompletedNotification visibility=public routing=routed routeDomains=turn
- `hook/completed` payload=HookCompletedNotification visibility=public routing=routed routeDomains=hook
- `turn/diff/updated` payload=TurnDiffUpdatedNotification visibility=public routing=routed routeDomains=turn
- `turn/plan/updated` payload=TurnPlanUpdatedNotification visibility=public routing=routed routeDomains=turn
- `item/started` payload=ItemStartedNotification visibility=public routing=routed routeDomains=item
- `item/autoApprovalReview/started` payload=ItemGuardianApprovalReviewStartedNotification visibility=public routing=routed routeDomains=item
- `item/autoApprovalReview/completed` payload=ItemGuardianApprovalReviewCompletedNotification visibility=public routing=routed routeDomains=item
- `item/completed` payload=ItemCompletedNotification visibility=public routing=routed routeDomains=item
- `rawResponseItem/completed` payload=RawResponseItemCompletedNotification visibility=generatedOnly routing=routed routeDomains=rawResponseItem
- `item/agentMessage/delta` payload=AgentMessageDeltaNotification visibility=public routing=routed routeDomains=item
- `item/plan/delta` payload=PlanDeltaNotification visibility=public routing=routed routeDomains=item
- `command/exec/outputDelta` payload=CommandExecOutputDeltaNotification visibility=public routing=routed routeDomains=command
- `process/outputDelta` payload=ProcessOutputDeltaNotification visibility=public routing=routed routeDomains=process
- `process/exited` payload=ProcessExitedNotification visibility=public routing=routed routeDomains=process
- `item/commandExecution/outputDelta` payload=CommandExecutionOutputDeltaNotification visibility=public routing=routed routeDomains=item
- `item/commandExecution/terminalInteraction` payload=TerminalInteractionNotification visibility=public routing=routed routeDomains=item
- `item/fileChange/outputDelta` payload=FileChangeOutputDeltaNotification visibility=public routing=routed routeDomains=item
- `item/fileChange/patchUpdated` payload=FileChangePatchUpdatedNotification visibility=public routing=routed routeDomains=item
- `serverRequest/resolved` payload=ServerRequestResolvedNotification visibility=public routing=routed routeDomains=serverRequest
- `item/mcpToolCall/progress` payload=McpToolCallProgressNotification visibility=public routing=routed routeDomains=item
- `mcpServer/oauthLogin/completed` payload=McpServerOauthLoginCompletedNotification visibility=public routing=routed routeDomains=mcpServer
- `mcpServer/startupStatus/updated` payload=McpServerStatusUpdatedNotification visibility=public routing=routed routeDomains=mcpServer
- `account/updated` payload=AccountUpdatedNotification visibility=public routing=globalOnly routeDomains=
- `account/rateLimits/updated` payload=AccountRateLimitsUpdatedNotification visibility=public routing=globalOnly routeDomains=
- `app/list/updated` payload=AppListUpdatedNotification visibility=public routing=globalOnly routeDomains=
- `remoteControl/status/changed` payload=RemoteControlStatusChangedNotification visibility=public routing=routed routeDomains=remoteControl
- `externalAgentConfig/import/progress` payload=ExternalAgentConfigImportProgressNotification visibility=public routing=routed routeDomains=externalAgentConfig
- `externalAgentConfig/import/completed` payload=ExternalAgentConfigImportCompletedNotification visibility=public routing=routed routeDomains=externalAgentConfig
- `fs/changed` payload=FsChangedNotification visibility=public routing=routed routeDomains=fs
- `item/reasoning/summaryTextDelta` payload=ReasoningSummaryTextDeltaNotification visibility=public routing=routed routeDomains=item
- `item/reasoning/summaryPartAdded` payload=ReasoningSummaryPartAddedNotification visibility=public routing=routed routeDomains=item
- `item/reasoning/textDelta` payload=ReasoningTextDeltaNotification visibility=public routing=routed routeDomains=item
- `thread/compacted` payload=ContextCompactedNotification visibility=public routing=routed routeDomains=thread
- `model/rerouted` payload=ModelReroutedNotification visibility=public routing=routed routeDomains=model
- `model/verification` payload=ModelVerificationNotification visibility=public routing=routed routeDomains=model
- `turn/moderationMetadata` payload=TurnModerationMetadataNotification visibility=public routing=routed routeDomains=turn
- `model/safetyBuffering/updated` payload=ModelSafetyBufferingUpdatedNotification visibility=public routing=routed routeDomains=model
- `warning` payload=WarningNotification visibility=public routing=routedWithGlobalFallback routeDomains=warning
- `guardianWarning` payload=GuardianWarningNotification visibility=public routing=routed routeDomains=guardianWarning
- `deprecationNotice` payload=DeprecationNoticeNotification visibility=public routing=globalOnly routeDomains=
- `configWarning` payload=ConfigWarningNotification visibility=public routing=globalOnly routeDomains=
- `fuzzyFileSearch/sessionUpdated` payload=FuzzyFileSearchSessionUpdatedNotification visibility=public routing=routed routeDomains=fuzzyFileSearch
- `fuzzyFileSearch/sessionCompleted` payload=FuzzyFileSearchSessionCompletedNotification visibility=public routing=routed routeDomains=fuzzyFileSearch
- `thread/realtime/started` payload=ThreadRealtimeStartedNotification visibility=public routing=routed routeDomains=thread
- `thread/realtime/itemAdded` payload=ThreadRealtimeItemAddedNotification visibility=public routing=routed routeDomains=thread
- `thread/realtime/transcript/delta` payload=ThreadRealtimeTranscriptDeltaNotification visibility=public routing=routed routeDomains=thread
- `thread/realtime/transcript/done` payload=ThreadRealtimeTranscriptDoneNotification visibility=public routing=routed routeDomains=thread
- `thread/realtime/outputAudio/delta` payload=ThreadRealtimeOutputAudioDeltaNotification visibility=public routing=routed routeDomains=thread
- `thread/realtime/sdp` payload=ThreadRealtimeSdpNotification visibility=public routing=routed routeDomains=thread
- `thread/realtime/error` payload=ThreadRealtimeErrorNotification visibility=public routing=routed routeDomains=thread
- `thread/realtime/closed` payload=ThreadRealtimeClosedNotification visibility=public routing=routed routeDomains=thread
- `windows/worldWritableWarning` payload=WindowsWorldWritableWarningNotification visibility=public routing=globalOnly routeDomains=
- `windowsSandbox/setupCompleted` payload=WindowsSandboxSetupCompletedNotification visibility=public routing=globalOnly routeDomains=
- `account/login/completed` payload=AccountLoginCompletedNotification visibility=public routing=routedWithGlobalFallback routeDomains=account

## Client Notifications

- `initialized` payload= visibility=public
