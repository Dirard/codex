package protocol

import "context"

type Sender interface {
	Call(ctx context.Context, method string, params any, result any, metadata MethodMetadata) error
}

type RawClient struct{ sender Sender }

func NewRawClient(sender Sender) RawClient { return RawClient{sender: sender} }

func (c RawClient) ThreadStart(ctx context.Context, params ThreadStartParams) (ThreadStartResponse, error) {
	var result ThreadStartResponse
	err := c.sender.Call(ctx, "thread/start", params, &result, MethodMetadataByMethod["thread/start"])
	return result, err
}

func (c RawClient) ThreadResume(ctx context.Context, params ThreadResumeParams) (ThreadResumeResponse, error) {
	var result ThreadResumeResponse
	err := c.sender.Call(ctx, "thread/resume", params, &result, MethodMetadataByMethod["thread/resume"])
	return result, err
}

func (c RawClient) ThreadFork(ctx context.Context, params ThreadForkParams) (ThreadForkResponse, error) {
	var result ThreadForkResponse
	err := c.sender.Call(ctx, "thread/fork", params, &result, MethodMetadataByMethod["thread/fork"])
	return result, err
}

func (c RawClient) ThreadArchive(ctx context.Context, params ThreadArchiveParams) (ThreadArchiveResponse, error) {
	var result ThreadArchiveResponse
	err := c.sender.Call(ctx, "thread/archive", params, &result, MethodMetadataByMethod["thread/archive"])
	return result, err
}

func (c RawClient) ThreadDelete(ctx context.Context, params ThreadDeleteParams) (ThreadDeleteResponse, error) {
	var result ThreadDeleteResponse
	err := c.sender.Call(ctx, "thread/delete", params, &result, MethodMetadataByMethod["thread/delete"])
	return result, err
}

func (c RawClient) ThreadUnsubscribe(ctx context.Context, params ThreadUnsubscribeParams) (ThreadUnsubscribeResponse, error) {
	var result ThreadUnsubscribeResponse
	err := c.sender.Call(ctx, "thread/unsubscribe", params, &result, MethodMetadataByMethod["thread/unsubscribe"])
	return result, err
}

func (c RawClient) ThreadIncrementElicitation(ctx context.Context, params ThreadIncrementElicitationParams) (ThreadIncrementElicitationResponse, error) {
	var result ThreadIncrementElicitationResponse
	err := c.sender.Call(ctx, "thread/increment_elicitation", params, &result, MethodMetadataByMethod["thread/increment_elicitation"])
	return result, err
}

func (c RawClient) ThreadDecrementElicitation(ctx context.Context, params ThreadDecrementElicitationParams) (ThreadDecrementElicitationResponse, error) {
	var result ThreadDecrementElicitationResponse
	err := c.sender.Call(ctx, "thread/decrement_elicitation", params, &result, MethodMetadataByMethod["thread/decrement_elicitation"])
	return result, err
}

func (c RawClient) ThreadNameSet(ctx context.Context, params ThreadSetNameParams) (ThreadSetNameResponse, error) {
	var result ThreadSetNameResponse
	err := c.sender.Call(ctx, "thread/name/set", params, &result, MethodMetadataByMethod["thread/name/set"])
	return result, err
}

func (c RawClient) ThreadGoalSet(ctx context.Context, params ThreadGoalSetParams) (ThreadGoalSetResponse, error) {
	var result ThreadGoalSetResponse
	err := c.sender.Call(ctx, "thread/goal/set", params, &result, MethodMetadataByMethod["thread/goal/set"])
	return result, err
}

func (c RawClient) ThreadGoalGet(ctx context.Context, params ThreadGoalGetParams) (ThreadGoalGetResponse, error) {
	var result ThreadGoalGetResponse
	err := c.sender.Call(ctx, "thread/goal/get", params, &result, MethodMetadataByMethod["thread/goal/get"])
	return result, err
}

func (c RawClient) ThreadGoalClear(ctx context.Context, params ThreadGoalClearParams) (ThreadGoalClearResponse, error) {
	var result ThreadGoalClearResponse
	err := c.sender.Call(ctx, "thread/goal/clear", params, &result, MethodMetadataByMethod["thread/goal/clear"])
	return result, err
}

func (c RawClient) ThreadMetadataUpdate(ctx context.Context, params ThreadMetadataUpdateParams) (ThreadMetadataUpdateResponse, error) {
	var result ThreadMetadataUpdateResponse
	err := c.sender.Call(ctx, "thread/metadata/update", params, &result, MethodMetadataByMethod["thread/metadata/update"])
	return result, err
}

func (c RawClient) ThreadSettingsUpdate(ctx context.Context, params ThreadSettingsUpdateParams) (ThreadSettingsUpdateResponse, error) {
	var result ThreadSettingsUpdateResponse
	err := c.sender.Call(ctx, "thread/settings/update", params, &result, MethodMetadataByMethod["thread/settings/update"])
	return result, err
}

func (c RawClient) ThreadMemoryModeSet(ctx context.Context, params ThreadMemoryModeSetParams) (ThreadMemoryModeSetResponse, error) {
	var result ThreadMemoryModeSetResponse
	err := c.sender.Call(ctx, "thread/memoryMode/set", params, &result, MethodMetadataByMethod["thread/memoryMode/set"])
	return result, err
}

func (c RawClient) MemoryReset(ctx context.Context) (MemoryResetResponse, error) {
	var result MemoryResetResponse
	err := c.sender.Call(ctx, "memory/reset", nil, &result, MethodMetadataByMethod["memory/reset"])
	return result, err
}

func (c RawClient) ThreadUnarchive(ctx context.Context, params ThreadUnarchiveParams) (ThreadUnarchiveResponse, error) {
	var result ThreadUnarchiveResponse
	err := c.sender.Call(ctx, "thread/unarchive", params, &result, MethodMetadataByMethod["thread/unarchive"])
	return result, err
}

func (c RawClient) ThreadCompactStart(ctx context.Context, params ThreadCompactStartParams) (ThreadCompactStartResponse, error) {
	var result ThreadCompactStartResponse
	err := c.sender.Call(ctx, "thread/compact/start", params, &result, MethodMetadataByMethod["thread/compact/start"])
	return result, err
}

func (c RawClient) ThreadShellCommand(ctx context.Context, params ThreadShellCommandParams) (ThreadShellCommandResponse, error) {
	var result ThreadShellCommandResponse
	err := c.sender.Call(ctx, "thread/shellCommand", params, &result, MethodMetadataByMethod["thread/shellCommand"])
	return result, err
}

func (c RawClient) ThreadApproveGuardianDeniedAction(ctx context.Context, params ThreadApproveGuardianDeniedActionParams) (ThreadApproveGuardianDeniedActionResponse, error) {
	var result ThreadApproveGuardianDeniedActionResponse
	err := c.sender.Call(ctx, "thread/approveGuardianDeniedAction", params, &result, MethodMetadataByMethod["thread/approveGuardianDeniedAction"])
	return result, err
}

func (c RawClient) ThreadBackgroundTerminalsClean(ctx context.Context, params ThreadBackgroundTerminalsCleanParams) (ThreadBackgroundTerminalsCleanResponse, error) {
	var result ThreadBackgroundTerminalsCleanResponse
	err := c.sender.Call(ctx, "thread/backgroundTerminals/clean", params, &result, MethodMetadataByMethod["thread/backgroundTerminals/clean"])
	return result, err
}

func (c RawClient) ThreadBackgroundTerminalsList(ctx context.Context, params ThreadBackgroundTerminalsListParams) (ThreadBackgroundTerminalsListResponse, error) {
	var result ThreadBackgroundTerminalsListResponse
	err := c.sender.Call(ctx, "thread/backgroundTerminals/list", params, &result, MethodMetadataByMethod["thread/backgroundTerminals/list"])
	return result, err
}

func (c RawClient) ThreadBackgroundTerminalsTerminate(ctx context.Context, params ThreadBackgroundTerminalsTerminateParams) (ThreadBackgroundTerminalsTerminateResponse, error) {
	var result ThreadBackgroundTerminalsTerminateResponse
	err := c.sender.Call(ctx, "thread/backgroundTerminals/terminate", params, &result, MethodMetadataByMethod["thread/backgroundTerminals/terminate"])
	return result, err
}

func (c RawClient) ThreadRollback(ctx context.Context, params ThreadRollbackParams) (ThreadRollbackResponse, error) {
	var result ThreadRollbackResponse
	err := c.sender.Call(ctx, "thread/rollback", params, &result, MethodMetadataByMethod["thread/rollback"])
	return result, err
}

func (c RawClient) ThreadList(ctx context.Context, params ThreadListParams) (ThreadListResponse, error) {
	var result ThreadListResponse
	err := c.sender.Call(ctx, "thread/list", params, &result, MethodMetadataByMethod["thread/list"])
	return result, err
}

func (c RawClient) ThreadSearch(ctx context.Context, params ThreadSearchParams) (ThreadSearchResponse, error) {
	var result ThreadSearchResponse
	err := c.sender.Call(ctx, "thread/search", params, &result, MethodMetadataByMethod["thread/search"])
	return result, err
}

func (c RawClient) ThreadLoadedList(ctx context.Context, params ThreadLoadedListParams) (ThreadLoadedListResponse, error) {
	var result ThreadLoadedListResponse
	err := c.sender.Call(ctx, "thread/loaded/list", params, &result, MethodMetadataByMethod["thread/loaded/list"])
	return result, err
}

func (c RawClient) ThreadRead(ctx context.Context, params ThreadReadParams) (ThreadReadResponse, error) {
	var result ThreadReadResponse
	err := c.sender.Call(ctx, "thread/read", params, &result, MethodMetadataByMethod["thread/read"])
	return result, err
}

func (c RawClient) ThreadTurnsList(ctx context.Context, params ThreadTurnsListParams) (ThreadTurnsListResponse, error) {
	var result ThreadTurnsListResponse
	err := c.sender.Call(ctx, "thread/turns/list", params, &result, MethodMetadataByMethod["thread/turns/list"])
	return result, err
}

func (c RawClient) ThreadItemsList(ctx context.Context, params ThreadItemsListParams) (ThreadItemsListResponse, error) {
	var result ThreadItemsListResponse
	err := c.sender.Call(ctx, "thread/items/list", params, &result, MethodMetadataByMethod["thread/items/list"])
	return result, err
}

func (c RawClient) ThreadInjectItems(ctx context.Context, params ThreadInjectItemsParams) (ThreadInjectItemsResponse, error) {
	var result ThreadInjectItemsResponse
	err := c.sender.Call(ctx, "thread/inject_items", params, &result, MethodMetadataByMethod["thread/inject_items"])
	return result, err
}

func (c RawClient) SkillsList(ctx context.Context, params SkillsListParams) (SkillsListResponse, error) {
	var result SkillsListResponse
	err := c.sender.Call(ctx, "skills/list", params, &result, MethodMetadataByMethod["skills/list"])
	return result, err
}

func (c RawClient) SkillsExtraRootsSet(ctx context.Context, params SkillsExtraRootsSetParams) (SkillsExtraRootsSetResponse, error) {
	var result SkillsExtraRootsSetResponse
	err := c.sender.Call(ctx, "skills/extraRoots/set", params, &result, MethodMetadataByMethod["skills/extraRoots/set"])
	return result, err
}

func (c RawClient) HooksList(ctx context.Context, params HooksListParams) (HooksListResponse, error) {
	var result HooksListResponse
	err := c.sender.Call(ctx, "hooks/list", params, &result, MethodMetadataByMethod["hooks/list"])
	return result, err
}

func (c RawClient) MarketplaceAdd(ctx context.Context, params MarketplaceAddParams) (MarketplaceAddResponse, error) {
	var result MarketplaceAddResponse
	err := c.sender.Call(ctx, "marketplace/add", params, &result, MethodMetadataByMethod["marketplace/add"])
	return result, err
}

func (c RawClient) MarketplaceRemove(ctx context.Context, params MarketplaceRemoveParams) (MarketplaceRemoveResponse, error) {
	var result MarketplaceRemoveResponse
	err := c.sender.Call(ctx, "marketplace/remove", params, &result, MethodMetadataByMethod["marketplace/remove"])
	return result, err
}

func (c RawClient) MarketplaceUpgrade(ctx context.Context, params MarketplaceUpgradeParams) (MarketplaceUpgradeResponse, error) {
	var result MarketplaceUpgradeResponse
	err := c.sender.Call(ctx, "marketplace/upgrade", params, &result, MethodMetadataByMethod["marketplace/upgrade"])
	return result, err
}

func (c RawClient) PluginList(ctx context.Context, params PluginListParams) (PluginListResponse, error) {
	var result PluginListResponse
	err := c.sender.Call(ctx, "plugin/list", params, &result, MethodMetadataByMethod["plugin/list"])
	return result, err
}

func (c RawClient) PluginInstalled(ctx context.Context, params PluginInstalledParams) (PluginInstalledResponse, error) {
	var result PluginInstalledResponse
	err := c.sender.Call(ctx, "plugin/installed", params, &result, MethodMetadataByMethod["plugin/installed"])
	return result, err
}

func (c RawClient) PluginRead(ctx context.Context, params PluginReadParams) (PluginReadResponse, error) {
	var result PluginReadResponse
	err := c.sender.Call(ctx, "plugin/read", params, &result, MethodMetadataByMethod["plugin/read"])
	return result, err
}

func (c RawClient) PluginSkillRead(ctx context.Context, params PluginSkillReadParams) (PluginSkillReadResponse, error) {
	var result PluginSkillReadResponse
	err := c.sender.Call(ctx, "plugin/skill/read", params, &result, MethodMetadataByMethod["plugin/skill/read"])
	return result, err
}

func (c RawClient) PluginShareSave(ctx context.Context, params PluginShareSaveParams) (PluginShareSaveResponse, error) {
	var result PluginShareSaveResponse
	err := c.sender.Call(ctx, "plugin/share/save", params, &result, MethodMetadataByMethod["plugin/share/save"])
	return result, err
}

func (c RawClient) PluginShareUpdateTargets(ctx context.Context, params PluginShareUpdateTargetsParams) (PluginShareUpdateTargetsResponse, error) {
	var result PluginShareUpdateTargetsResponse
	err := c.sender.Call(ctx, "plugin/share/updateTargets", params, &result, MethodMetadataByMethod["plugin/share/updateTargets"])
	return result, err
}

func (c RawClient) PluginShareList(ctx context.Context, params PluginShareListParams) (PluginShareListResponse, error) {
	var result PluginShareListResponse
	err := c.sender.Call(ctx, "plugin/share/list", params, &result, MethodMetadataByMethod["plugin/share/list"])
	return result, err
}

func (c RawClient) PluginShareCheckout(ctx context.Context, params PluginShareCheckoutParams) (PluginShareCheckoutResponse, error) {
	var result PluginShareCheckoutResponse
	err := c.sender.Call(ctx, "plugin/share/checkout", params, &result, MethodMetadataByMethod["plugin/share/checkout"])
	return result, err
}

func (c RawClient) PluginShareDelete(ctx context.Context, params PluginShareDeleteParams) (PluginShareDeleteResponse, error) {
	var result PluginShareDeleteResponse
	err := c.sender.Call(ctx, "plugin/share/delete", params, &result, MethodMetadataByMethod["plugin/share/delete"])
	return result, err
}

func (c RawClient) AppList(ctx context.Context, params AppsListParams) (AppsListResponse, error) {
	var result AppsListResponse
	err := c.sender.Call(ctx, "app/list", params, &result, MethodMetadataByMethod["app/list"])
	return result, err
}

func (c RawClient) FsReadFile(ctx context.Context, params FsReadFileParams) (FsReadFileResponse, error) {
	var result FsReadFileResponse
	err := c.sender.Call(ctx, "fs/readFile", params, &result, MethodMetadataByMethod["fs/readFile"])
	return result, err
}

func (c RawClient) FsWriteFile(ctx context.Context, params FsWriteFileParams) (FsWriteFileResponse, error) {
	var result FsWriteFileResponse
	err := c.sender.Call(ctx, "fs/writeFile", params, &result, MethodMetadataByMethod["fs/writeFile"])
	return result, err
}

func (c RawClient) FsCreateDirectory(ctx context.Context, params FsCreateDirectoryParams) (FsCreateDirectoryResponse, error) {
	var result FsCreateDirectoryResponse
	err := c.sender.Call(ctx, "fs/createDirectory", params, &result, MethodMetadataByMethod["fs/createDirectory"])
	return result, err
}

func (c RawClient) FsGetMetadata(ctx context.Context, params FsGetMetadataParams) (FsGetMetadataResponse, error) {
	var result FsGetMetadataResponse
	err := c.sender.Call(ctx, "fs/getMetadata", params, &result, MethodMetadataByMethod["fs/getMetadata"])
	return result, err
}

func (c RawClient) FsReadDirectory(ctx context.Context, params FsReadDirectoryParams) (FsReadDirectoryResponse, error) {
	var result FsReadDirectoryResponse
	err := c.sender.Call(ctx, "fs/readDirectory", params, &result, MethodMetadataByMethod["fs/readDirectory"])
	return result, err
}

func (c RawClient) FsRemove(ctx context.Context, params FsRemoveParams) (FsRemoveResponse, error) {
	var result FsRemoveResponse
	err := c.sender.Call(ctx, "fs/remove", params, &result, MethodMetadataByMethod["fs/remove"])
	return result, err
}

func (c RawClient) FsCopy(ctx context.Context, params FsCopyParams) (FsCopyResponse, error) {
	var result FsCopyResponse
	err := c.sender.Call(ctx, "fs/copy", params, &result, MethodMetadataByMethod["fs/copy"])
	return result, err
}

func (c RawClient) FsWatch(ctx context.Context, params FsWatchParams) (FsWatchResponse, error) {
	var result FsWatchResponse
	err := c.sender.Call(ctx, "fs/watch", params, &result, MethodMetadataByMethod["fs/watch"])
	return result, err
}

func (c RawClient) FsUnwatch(ctx context.Context, params FsUnwatchParams) (FsUnwatchResponse, error) {
	var result FsUnwatchResponse
	err := c.sender.Call(ctx, "fs/unwatch", params, &result, MethodMetadataByMethod["fs/unwatch"])
	return result, err
}

func (c RawClient) SkillsConfigWrite(ctx context.Context, params SkillsConfigWriteParams) (SkillsConfigWriteResponse, error) {
	var result SkillsConfigWriteResponse
	err := c.sender.Call(ctx, "skills/config/write", params, &result, MethodMetadataByMethod["skills/config/write"])
	return result, err
}

func (c RawClient) PluginInstall(ctx context.Context, params PluginInstallParams) (PluginInstallResponse, error) {
	var result PluginInstallResponse
	err := c.sender.Call(ctx, "plugin/install", params, &result, MethodMetadataByMethod["plugin/install"])
	return result, err
}

func (c RawClient) PluginUninstall(ctx context.Context, params PluginUninstallParams) (PluginUninstallResponse, error) {
	var result PluginUninstallResponse
	err := c.sender.Call(ctx, "plugin/uninstall", params, &result, MethodMetadataByMethod["plugin/uninstall"])
	return result, err
}

func (c RawClient) TurnStart(ctx context.Context, params TurnStartParams) (TurnStartResponse, error) {
	var result TurnStartResponse
	err := c.sender.Call(ctx, "turn/start", params, &result, MethodMetadataByMethod["turn/start"])
	return result, err
}

func (c RawClient) TurnSteer(ctx context.Context, params TurnSteerParams) (TurnSteerResponse, error) {
	var result TurnSteerResponse
	err := c.sender.Call(ctx, "turn/steer", params, &result, MethodMetadataByMethod["turn/steer"])
	return result, err
}

func (c RawClient) TurnInterrupt(ctx context.Context, params TurnInterruptParams) (TurnInterruptResponse, error) {
	var result TurnInterruptResponse
	err := c.sender.Call(ctx, "turn/interrupt", params, &result, MethodMetadataByMethod["turn/interrupt"])
	return result, err
}

func (c RawClient) ThreadRealtimeStart(ctx context.Context, params ThreadRealtimeStartParams) (ThreadRealtimeStartResponse, error) {
	var result ThreadRealtimeStartResponse
	err := c.sender.Call(ctx, "thread/realtime/start", params, &result, MethodMetadataByMethod["thread/realtime/start"])
	return result, err
}

func (c RawClient) ThreadRealtimeAppendAudio(ctx context.Context, params ThreadRealtimeAppendAudioParams) (ThreadRealtimeAppendAudioResponse, error) {
	var result ThreadRealtimeAppendAudioResponse
	err := c.sender.Call(ctx, "thread/realtime/appendAudio", params, &result, MethodMetadataByMethod["thread/realtime/appendAudio"])
	return result, err
}

func (c RawClient) ThreadRealtimeAppendText(ctx context.Context, params ThreadRealtimeAppendTextParams) (ThreadRealtimeAppendTextResponse, error) {
	var result ThreadRealtimeAppendTextResponse
	err := c.sender.Call(ctx, "thread/realtime/appendText", params, &result, MethodMetadataByMethod["thread/realtime/appendText"])
	return result, err
}

func (c RawClient) ThreadRealtimeAppendSpeech(ctx context.Context, params ThreadRealtimeAppendSpeechParams) (ThreadRealtimeAppendSpeechResponse, error) {
	var result ThreadRealtimeAppendSpeechResponse
	err := c.sender.Call(ctx, "thread/realtime/appendSpeech", params, &result, MethodMetadataByMethod["thread/realtime/appendSpeech"])
	return result, err
}

func (c RawClient) ThreadRealtimeStop(ctx context.Context, params ThreadRealtimeStopParams) (ThreadRealtimeStopResponse, error) {
	var result ThreadRealtimeStopResponse
	err := c.sender.Call(ctx, "thread/realtime/stop", params, &result, MethodMetadataByMethod["thread/realtime/stop"])
	return result, err
}

func (c RawClient) ThreadRealtimeListVoices(ctx context.Context, params ThreadRealtimeListVoicesParams) (ThreadRealtimeListVoicesResponse, error) {
	var result ThreadRealtimeListVoicesResponse
	err := c.sender.Call(ctx, "thread/realtime/listVoices", params, &result, MethodMetadataByMethod["thread/realtime/listVoices"])
	return result, err
}

func (c RawClient) ReviewStart(ctx context.Context, params ReviewStartParams) (ReviewStartResponse, error) {
	var result ReviewStartResponse
	err := c.sender.Call(ctx, "review/start", params, &result, MethodMetadataByMethod["review/start"])
	return result, err
}

func (c RawClient) ModelList(ctx context.Context, params ModelListParams) (ModelListResponse, error) {
	var result ModelListResponse
	err := c.sender.Call(ctx, "model/list", params, &result, MethodMetadataByMethod["model/list"])
	return result, err
}

func (c RawClient) ModelProviderCapabilitiesRead(ctx context.Context, params ModelProviderCapabilitiesReadParams) (ModelProviderCapabilitiesReadResponse, error) {
	var result ModelProviderCapabilitiesReadResponse
	err := c.sender.Call(ctx, "modelProvider/capabilities/read", params, &result, MethodMetadataByMethod["modelProvider/capabilities/read"])
	return result, err
}

func (c RawClient) ExperimentalFeatureList(ctx context.Context, params ExperimentalFeatureListParams) (ExperimentalFeatureListResponse, error) {
	var result ExperimentalFeatureListResponse
	err := c.sender.Call(ctx, "experimentalFeature/list", params, &result, MethodMetadataByMethod["experimentalFeature/list"])
	return result, err
}

func (c RawClient) PermissionProfileList(ctx context.Context, params PermissionProfileListParams) (PermissionProfileListResponse, error) {
	var result PermissionProfileListResponse
	err := c.sender.Call(ctx, "permissionProfile/list", params, &result, MethodMetadataByMethod["permissionProfile/list"])
	return result, err
}

func (c RawClient) ExperimentalFeatureEnablementSet(ctx context.Context, params ExperimentalFeatureEnablementSetParams) (ExperimentalFeatureEnablementSetResponse, error) {
	var result ExperimentalFeatureEnablementSetResponse
	err := c.sender.Call(ctx, "experimentalFeature/enablement/set", params, &result, MethodMetadataByMethod["experimentalFeature/enablement/set"])
	return result, err
}

func (c RawClient) RemoteControlEnable(ctx context.Context, params NullableRemoteControlEnableParams) (RemoteControlEnableResponse, error) {
	var result RemoteControlEnableResponse
	err := c.sender.Call(ctx, "remoteControl/enable", params, &result, MethodMetadataByMethod["remoteControl/enable"])
	return result, err
}

func (c RawClient) RemoteControlDisable(ctx context.Context, params NullableRemoteControlDisableParams) (RemoteControlDisableResponse, error) {
	var result RemoteControlDisableResponse
	err := c.sender.Call(ctx, "remoteControl/disable", params, &result, MethodMetadataByMethod["remoteControl/disable"])
	return result, err
}

func (c RawClient) RemoteControlStatusRead(ctx context.Context) (RemoteControlStatusReadResponse, error) {
	var result RemoteControlStatusReadResponse
	err := c.sender.Call(ctx, "remoteControl/status/read", nil, &result, MethodMetadataByMethod["remoteControl/status/read"])
	return result, err
}

func (c RawClient) RemoteControlPairingStart(ctx context.Context, params RemoteControlPairingStartParams) (RemoteControlPairingStartResponse, error) {
	var result RemoteControlPairingStartResponse
	err := c.sender.Call(ctx, "remoteControl/pairing/start", params, &result, MethodMetadataByMethod["remoteControl/pairing/start"])
	return result, err
}

func (c RawClient) RemoteControlPairingStatus(ctx context.Context, params RemoteControlPairingStatusParams) (RemoteControlPairingStatusResponse, error) {
	var result RemoteControlPairingStatusResponse
	err := c.sender.Call(ctx, "remoteControl/pairing/status", params, &result, MethodMetadataByMethod["remoteControl/pairing/status"])
	return result, err
}

func (c RawClient) RemoteControlClientList(ctx context.Context, params RemoteControlClientsListParams) (RemoteControlClientsListResponse, error) {
	var result RemoteControlClientsListResponse
	err := c.sender.Call(ctx, "remoteControl/client/list", params, &result, MethodMetadataByMethod["remoteControl/client/list"])
	return result, err
}

func (c RawClient) RemoteControlClientRevoke(ctx context.Context, params RemoteControlClientsRevokeParams) (RemoteControlClientsRevokeResponse, error) {
	var result RemoteControlClientsRevokeResponse
	err := c.sender.Call(ctx, "remoteControl/client/revoke", params, &result, MethodMetadataByMethod["remoteControl/client/revoke"])
	return result, err
}

func (c RawClient) CollaborationModeList(ctx context.Context, params CollaborationModeListParams) (CollaborationModeListResponse, error) {
	var result CollaborationModeListResponse
	err := c.sender.Call(ctx, "collaborationMode/list", params, &result, MethodMetadataByMethod["collaborationMode/list"])
	return result, err
}

func (c RawClient) EnvironmentAdd(ctx context.Context, params EnvironmentAddParams) (EnvironmentAddResponse, error) {
	var result EnvironmentAddResponse
	err := c.sender.Call(ctx, "environment/add", params, &result, MethodMetadataByMethod["environment/add"])
	return result, err
}

func (c RawClient) EnvironmentInfo(ctx context.Context, params EnvironmentInfoParams) (EnvironmentInfoResponse, error) {
	var result EnvironmentInfoResponse
	err := c.sender.Call(ctx, "environment/info", params, &result, MethodMetadataByMethod["environment/info"])
	return result, err
}

func (c RawClient) McpServerOauthLogin(ctx context.Context, params McpServerOauthLoginParams) (McpServerOauthLoginResponse, error) {
	var result McpServerOauthLoginResponse
	err := c.sender.Call(ctx, "mcpServer/oauth/login", params, &result, MethodMetadataByMethod["mcpServer/oauth/login"])
	return result, err
}

func (c RawClient) ConfigMcpServerReload(ctx context.Context) (McpServerRefreshResponse, error) {
	var result McpServerRefreshResponse
	err := c.sender.Call(ctx, "config/mcpServer/reload", nil, &result, MethodMetadataByMethod["config/mcpServer/reload"])
	return result, err
}

func (c RawClient) McpServerStatusList(ctx context.Context, params ListMcpServerStatusParams) (ListMcpServerStatusResponse, error) {
	var result ListMcpServerStatusResponse
	err := c.sender.Call(ctx, "mcpServerStatus/list", params, &result, MethodMetadataByMethod["mcpServerStatus/list"])
	return result, err
}

func (c RawClient) McpServerResourceRead(ctx context.Context, params McpResourceReadParams) (McpResourceReadResponse, error) {
	var result McpResourceReadResponse
	err := c.sender.Call(ctx, "mcpServer/resource/read", params, &result, MethodMetadataByMethod["mcpServer/resource/read"])
	return result, err
}

func (c RawClient) McpServerToolCall(ctx context.Context, params McpServerToolCallParams) (McpServerToolCallResponse, error) {
	var result McpServerToolCallResponse
	err := c.sender.Call(ctx, "mcpServer/tool/call", params, &result, MethodMetadataByMethod["mcpServer/tool/call"])
	return result, err
}

func (c RawClient) WindowsSandboxSetupStart(ctx context.Context, params WindowsSandboxSetupStartParams) (WindowsSandboxSetupStartResponse, error) {
	var result WindowsSandboxSetupStartResponse
	err := c.sender.Call(ctx, "windowsSandbox/setupStart", params, &result, MethodMetadataByMethod["windowsSandbox/setupStart"])
	return result, err
}

func (c RawClient) WindowsSandboxReadiness(ctx context.Context) (WindowsSandboxReadinessResponse, error) {
	var result WindowsSandboxReadinessResponse
	err := c.sender.Call(ctx, "windowsSandbox/readiness", nil, &result, MethodMetadataByMethod["windowsSandbox/readiness"])
	return result, err
}

func (c RawClient) AccountLoginStart(ctx context.Context, params LoginAccountParams) (LoginAccountResponse, error) {
	var result LoginAccountResponse
	err := c.sender.Call(ctx, "account/login/start", params, &result, MethodMetadataByMethod["account/login/start"])
	return result, err
}

func (c RawClient) AccountLoginCancel(ctx context.Context, params CancelLoginAccountParams) (CancelLoginAccountResponse, error) {
	var result CancelLoginAccountResponse
	err := c.sender.Call(ctx, "account/login/cancel", params, &result, MethodMetadataByMethod["account/login/cancel"])
	return result, err
}

func (c RawClient) AccountLogout(ctx context.Context) (LogoutAccountResponse, error) {
	var result LogoutAccountResponse
	err := c.sender.Call(ctx, "account/logout", nil, &result, MethodMetadataByMethod["account/logout"])
	return result, err
}

func (c RawClient) AccountRateLimitsRead(ctx context.Context) (GetAccountRateLimitsResponse, error) {
	var result GetAccountRateLimitsResponse
	err := c.sender.Call(ctx, "account/rateLimits/read", nil, &result, MethodMetadataByMethod["account/rateLimits/read"])
	return result, err
}

func (c RawClient) AccountRateLimitResetCreditConsume(ctx context.Context, params ConsumeAccountRateLimitResetCreditParams) (ConsumeAccountRateLimitResetCreditResponse, error) {
	var result ConsumeAccountRateLimitResetCreditResponse
	err := c.sender.Call(ctx, "account/rateLimitResetCredit/consume", params, &result, MethodMetadataByMethod["account/rateLimitResetCredit/consume"])
	return result, err
}

func (c RawClient) AccountUsageRead(ctx context.Context) (GetAccountTokenUsageResponse, error) {
	var result GetAccountTokenUsageResponse
	err := c.sender.Call(ctx, "account/usage/read", nil, &result, MethodMetadataByMethod["account/usage/read"])
	return result, err
}

func (c RawClient) AccountWorkspaceMessagesRead(ctx context.Context) (GetWorkspaceMessagesResponse, error) {
	var result GetWorkspaceMessagesResponse
	err := c.sender.Call(ctx, "account/workspaceMessages/read", nil, &result, MethodMetadataByMethod["account/workspaceMessages/read"])
	return result, err
}

func (c RawClient) AccountSendAddCreditsNudgeEmail(ctx context.Context, params SendAddCreditsNudgeEmailParams) (SendAddCreditsNudgeEmailResponse, error) {
	var result SendAddCreditsNudgeEmailResponse
	err := c.sender.Call(ctx, "account/sendAddCreditsNudgeEmail", params, &result, MethodMetadataByMethod["account/sendAddCreditsNudgeEmail"])
	return result, err
}

func (c RawClient) FeedbackUpload(ctx context.Context, params FeedbackUploadParams) (FeedbackUploadResponse, error) {
	var result FeedbackUploadResponse
	err := c.sender.Call(ctx, "feedback/upload", params, &result, MethodMetadataByMethod["feedback/upload"])
	return result, err
}

func (c RawClient) CommandExec(ctx context.Context, params CommandExecParams) (CommandExecResponse, error) {
	var result CommandExecResponse
	err := c.sender.Call(ctx, "command/exec", params, &result, MethodMetadataByMethod["command/exec"])
	return result, err
}

func (c RawClient) CommandExecWrite(ctx context.Context, params CommandExecWriteParams) (CommandExecWriteResponse, error) {
	var result CommandExecWriteResponse
	err := c.sender.Call(ctx, "command/exec/write", params, &result, MethodMetadataByMethod["command/exec/write"])
	return result, err
}

func (c RawClient) CommandExecTerminate(ctx context.Context, params CommandExecTerminateParams) (CommandExecTerminateResponse, error) {
	var result CommandExecTerminateResponse
	err := c.sender.Call(ctx, "command/exec/terminate", params, &result, MethodMetadataByMethod["command/exec/terminate"])
	return result, err
}

func (c RawClient) CommandExecResize(ctx context.Context, params CommandExecResizeParams) (CommandExecResizeResponse, error) {
	var result CommandExecResizeResponse
	err := c.sender.Call(ctx, "command/exec/resize", params, &result, MethodMetadataByMethod["command/exec/resize"])
	return result, err
}

func (c RawClient) ProcessSpawn(ctx context.Context, params ProcessSpawnParams) (ProcessSpawnResponse, error) {
	var result ProcessSpawnResponse
	err := c.sender.Call(ctx, "process/spawn", params, &result, MethodMetadataByMethod["process/spawn"])
	return result, err
}

func (c RawClient) ProcessWriteStdin(ctx context.Context, params ProcessWriteStdinParams) (ProcessWriteStdinResponse, error) {
	var result ProcessWriteStdinResponse
	err := c.sender.Call(ctx, "process/writeStdin", params, &result, MethodMetadataByMethod["process/writeStdin"])
	return result, err
}

func (c RawClient) ProcessKill(ctx context.Context, params ProcessKillParams) (ProcessKillResponse, error) {
	var result ProcessKillResponse
	err := c.sender.Call(ctx, "process/kill", params, &result, MethodMetadataByMethod["process/kill"])
	return result, err
}

func (c RawClient) ProcessResizePty(ctx context.Context, params ProcessResizePtyParams) (ProcessResizePtyResponse, error) {
	var result ProcessResizePtyResponse
	err := c.sender.Call(ctx, "process/resizePty", params, &result, MethodMetadataByMethod["process/resizePty"])
	return result, err
}

func (c RawClient) ConfigRead(ctx context.Context, params ConfigReadParams) (ConfigReadResponse, error) {
	var result ConfigReadResponse
	err := c.sender.Call(ctx, "config/read", params, &result, MethodMetadataByMethod["config/read"])
	return result, err
}

func (c RawClient) ExternalAgentConfigDetect(ctx context.Context, params ExternalAgentConfigDetectParams) (ExternalAgentConfigDetectResponse, error) {
	var result ExternalAgentConfigDetectResponse
	err := c.sender.Call(ctx, "externalAgentConfig/detect", params, &result, MethodMetadataByMethod["externalAgentConfig/detect"])
	return result, err
}

func (c RawClient) ExternalAgentConfigImport(ctx context.Context, params ExternalAgentConfigImportParams) (ExternalAgentConfigImportResponse, error) {
	var result ExternalAgentConfigImportResponse
	err := c.sender.Call(ctx, "externalAgentConfig/import", params, &result, MethodMetadataByMethod["externalAgentConfig/import"])
	return result, err
}

func (c RawClient) ExternalAgentConfigImportReadHistories(ctx context.Context) (ExternalAgentConfigImportHistoriesReadResponse, error) {
	var result ExternalAgentConfigImportHistoriesReadResponse
	err := c.sender.Call(ctx, "externalAgentConfig/import/readHistories", nil, &result, MethodMetadataByMethod["externalAgentConfig/import/readHistories"])
	return result, err
}

func (c RawClient) ConfigValueWrite(ctx context.Context, params ConfigValueWriteParams) (ConfigWriteResponse, error) {
	var result ConfigWriteResponse
	err := c.sender.Call(ctx, "config/value/write", params, &result, MethodMetadataByMethod["config/value/write"])
	return result, err
}

func (c RawClient) ConfigBatchWrite(ctx context.Context, params ConfigBatchWriteParams) (ConfigWriteResponse, error) {
	var result ConfigWriteResponse
	err := c.sender.Call(ctx, "config/batchWrite", params, &result, MethodMetadataByMethod["config/batchWrite"])
	return result, err
}

func (c RawClient) ConfigRequirementsRead(ctx context.Context) (ConfigRequirementsReadResponse, error) {
	var result ConfigRequirementsReadResponse
	err := c.sender.Call(ctx, "configRequirements/read", nil, &result, MethodMetadataByMethod["configRequirements/read"])
	return result, err
}

func (c RawClient) AccountRead(ctx context.Context, params GetAccountParams) (GetAccountResponse, error) {
	var result GetAccountResponse
	err := c.sender.Call(ctx, "account/read", params, &result, MethodMetadataByMethod["account/read"])
	return result, err
}

func (c RawClient) FuzzyFileSearch(ctx context.Context, params FuzzyFileSearchParams) (FuzzyFileSearchResponse, error) {
	var result FuzzyFileSearchResponse
	err := c.sender.Call(ctx, "fuzzyFileSearch", params, &result, MethodMetadataByMethod["fuzzyFileSearch"])
	return result, err
}

func (c RawClient) FuzzyFileSearchSessionStart(ctx context.Context, params FuzzyFileSearchSessionStartParams) (FuzzyFileSearchSessionStartResponse, error) {
	var result FuzzyFileSearchSessionStartResponse
	err := c.sender.Call(ctx, "fuzzyFileSearch/sessionStart", params, &result, MethodMetadataByMethod["fuzzyFileSearch/sessionStart"])
	return result, err
}

func (c RawClient) FuzzyFileSearchSessionUpdate(ctx context.Context, params FuzzyFileSearchSessionUpdateParams) (FuzzyFileSearchSessionUpdateResponse, error) {
	var result FuzzyFileSearchSessionUpdateResponse
	err := c.sender.Call(ctx, "fuzzyFileSearch/sessionUpdate", params, &result, MethodMetadataByMethod["fuzzyFileSearch/sessionUpdate"])
	return result, err
}

func (c RawClient) FuzzyFileSearchSessionStop(ctx context.Context, params FuzzyFileSearchSessionStopParams) (FuzzyFileSearchSessionStopResponse, error) {
	var result FuzzyFileSearchSessionStopResponse
	err := c.sender.Call(ctx, "fuzzyFileSearch/sessionStop", params, &result, MethodMetadataByMethod["fuzzyFileSearch/sessionStop"])
	return result, err
}
