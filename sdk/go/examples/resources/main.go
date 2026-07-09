package main

import (
	"context"
	"errors"

	codex "github.com/openai/codex/sdk/go"
	"github.com/openai/codex/sdk/go/protocol"
)

func resources(ctx context.Context, client *codex.Client) error {
	// codex-go-sdk-resource:Apps
	// codex-go-sdk-docs:app/list
	if _, err := client.Apps.List(ctx, protocol.AppsListParams{}); err != nil {
		return err
	}
	// codex-go-sdk-resource:Commands
	// codex-go-sdk-docs:command/exec
	// codex-go-sdk-docs:command/exec/write
	// codex-go-sdk-docs:command/exec/terminate
	// codex-go-sdk-docs:command/exec/resize
	cmd, err := client.Commands.Exec(ctx, codex.CommandExecOptions{Command: []string{"echo", "ok"}})
	if err != nil {
		return err
	}
	if _, err := cmd.Stream(ctx); err != nil {
		return err
	}
	if err := cmd.Write(ctx, []byte("input")); err != nil {
		return err
	}
	if err := cmd.CloseStdin(ctx); err != nil {
		return err
	}
	if err := cmd.Resize(ctx, codex.TerminalSize{Rows: 24, Cols: 80}); err != nil {
		return err
	}
	if err := cmd.Terminate(ctx); err != nil {
		return err
	}
	if _, err := cmd.Wait(ctx); err != nil {
		return err
	}
	// codex-go-sdk-resource:Config
	// codex-go-sdk-docs:config/mcpServer/reload
	// codex-go-sdk-docs:config/read
	// codex-go-sdk-docs:config/value/write
	// codex-go-sdk-docs:config/batchWrite
	if _, err := client.Config.Read(ctx, protocol.ConfigReadParams{}); err != nil {
		return err
	}
	if _, err := client.Config.ReadRequirements(ctx); err != nil {
		return err
	}
	_, _ = client.Config.ReloadMCPServers(ctx)
	_, _ = client.Config.WriteValue(ctx, protocol.ConfigValueWriteParams{})
	_, _ = client.Config.BatchWrite(ctx, protocol.ConfigBatchWriteParams{})
	// codex-go-sdk-resource:FileSystem
	// codex-go-sdk-docs:fs/readFile
	// codex-go-sdk-docs:fs/writeFile
	// codex-go-sdk-docs:fs/createDirectory
	// codex-go-sdk-docs:fs/getMetadata
	// codex-go-sdk-docs:fs/readDirectory
	// codex-go-sdk-docs:fs/remove
	// codex-go-sdk-docs:fs/copy
	// codex-go-sdk-docs:fs/watch
	// codex-go-sdk-docs:fs/unwatch
	watch, _, err := client.FileSystem.Watch(ctx, codex.FileSystemWatchOptions{Path: "/tmp"})
	if err != nil {
		return err
	}
	_ = watch.Close(ctx)
	_, _ = client.FileSystem.ReadFile(ctx, protocol.FsReadFileParams{})
	_, _ = client.FileSystem.WriteFile(ctx, protocol.FsWriteFileParams{})
	_, _ = client.FileSystem.CreateDirectory(ctx, protocol.FsCreateDirectoryParams{})
	_, _ = client.FileSystem.GetMetadata(ctx, protocol.FsGetMetadataParams{})
	_, _ = client.FileSystem.ReadDirectory(ctx, protocol.FsReadDirectoryParams{})
	_, _ = client.FileSystem.Copy(ctx, protocol.FsCopyParams{})
	_, _ = client.FileSystem.Remove(ctx, protocol.FsRemoveParams{})
	// codex-go-sdk-resource:FuzzyFileSearch
	// codex-go-sdk-docs:fuzzyFileSearch
	// codex-go-sdk-docs:fuzzyFileSearch/sessionStart
	// codex-go-sdk-docs:fuzzyFileSearch/sessionUpdate
	// codex-go-sdk-docs:fuzzyFileSearch/sessionStop
	if _, err := client.FuzzyFileSearch.Search(ctx, protocol.FuzzyFileSearchParams{Query: "main.go"}); err != nil {
		return err
	}
	{
		session, err := client.FuzzyFileSearch.StartSession(ctx, codex.FuzzySearchSessionOptions{Roots: []string{"."}})
		if err != nil {
			return err
		}
		if err := session.Update(ctx, codex.FuzzySearchUpdate{Query: "README.md"}); err != nil {
			return err
		}
		if err := session.Close(ctx); err != nil {
			return err
		}
	}
	// codex-go-sdk-resource:MCP
	// codex-go-sdk-docs:mcpServer/oauth/login
	// codex-go-sdk-docs:mcpServerStatus/list
	// codex-go-sdk-docs:mcpServer/resource/read
	// codex-go-sdk-docs:mcpServer/tool/call
	if _, err := client.MCP.ListStatus(ctx, protocol.ListMcpServerStatusParams{}); err != nil {
		return err
	}
	oauth, err := client.MCP.OAuthLogin(ctx, codex.MCPOAuthLoginOptions{Name: "github"})
	if err != nil {
		return err
	}
	_, _ = oauth.Wait(ctx)
	_, _ = client.MCP.ReadResource(ctx, protocol.McpResourceReadParams{})
	_, _ = client.MCP.CallTool(ctx, protocol.McpServerToolCallParams{})
	// codex-go-sdk-resource:Memory
	// codex-go-sdk-docs:memory/reset
	if _, err := client.Memory.Reset(ctx); err != nil {
		return err
	}
	// codex-go-sdk-resource:WindowsSandbox
	// codex-go-sdk-docs:windowsSandbox/readiness
	readiness, err := client.WindowsSandbox.Readiness(ctx)
	if err != nil {
		return err
	}
	if readiness.Status == codex.WindowsSandboxReadinessUnsupportedPlatform {
		_ = readiness.PlatformOS
	}
	// codex-go-sdk-resource:Processes
	// codex-go-sdk-docs:process/spawn
	// codex-go-sdk-docs:process/writeStdin
	// codex-go-sdk-docs:process/kill
	// codex-go-sdk-docs:process/resizePty
	proc, _, err := client.Processes.Spawn(ctx, codex.ProcessSpawnOptions{Command: []string{"echo", "ok"}, CWD: "/tmp"})
	if err != nil {
		return err
	}
	if _, err := proc.Stream(ctx); err != nil {
		return err
	}
	if err := proc.WriteStdin(ctx, []byte("input")); err != nil {
		return err
	}
	if err := proc.CloseStdin(ctx); err != nil {
		return err
	}
	if err := proc.ResizePTY(ctx, codex.TerminalSize{Rows: 24, Cols: 80}); err != nil {
		return err
	}
	if err := proc.Kill(ctx); err != nil {
		return err
	}
	// codex-go-sdk-resource:Realtime
	// codex-go-sdk-docs:thread/realtime/start
	// codex-go-sdk-docs:thread/realtime/listVoices
	// codex-go-sdk-docs:thread/realtime/appendAudio
	// codex-go-sdk-docs:thread/realtime/appendText
	// codex-go-sdk-docs:thread/realtime/appendSpeech
	// codex-go-sdk-docs:thread/realtime/stop
	session, _, err := client.Realtime.Start(ctx, codex.RealtimeStartOptions{ThreadID: "thread-id"})
	if err != nil {
		return err
	}
	_, _ = client.Realtime.ListVoices(ctx, protocol.ThreadRealtimeListVoicesParams{})
	_ = session.AppendAudio(ctx, codex.AudioChunk{})
	_ = session.AppendText(ctx, "hello")
	_ = session.AppendSpeech(ctx, codex.SpeechInput{Text: "hello"})
	_ = session.Stop(ctx)
	// codex-go-sdk-resource:ExperimentalFeatures
	// codex-go-sdk-docs:experimentalFeature/list
	// codex-go-sdk-docs:experimentalFeature/enablement/set
	_, _ = client.ExperimentalFeatures.List(ctx, protocol.ExperimentalFeatureListParams{ThreadID: protocol.Some("thread-id")})
	_, _ = client.ExperimentalFeatures.SetEnablement(ctx, protocol.ExperimentalFeatureEnablementSetParams{Enablement: map[string]bool{"feature": true}})
	// codex-go-sdk-resource:PermissionProfiles
	// codex-go-sdk-docs:permissionProfile/list
	_, _ = client.PermissionProfiles.List(ctx, protocol.PermissionProfileListParams{})
	// codex-go-sdk-resource:Feedback
	// codex-go-sdk-docs:feedback/upload
	_, _ = client.Feedback.Upload(ctx, protocol.FeedbackUploadParams{Classification: "bug"})
	// codex-go-sdk-resource:Models
	// codex-go-sdk-docs:model/list
	// codex-go-sdk-docs:modelProvider/capabilities/read
	_, _ = client.Models.List(ctx, protocol.ModelListParams{})
	_, _ = client.Models.ReadProviderCapabilities(ctx, protocol.ModelProviderCapabilitiesReadParams{})
	// codex-go-sdk-resource:Environments
	// codex-go-sdk-docs:environment/add
	// codex-go-sdk-docs:environment/info
	_, _ = client.Environments.Info(ctx, protocol.EnvironmentInfoParams{})
	_, _ = client.Environments.Add(ctx, protocol.EnvironmentAddParams{})
	// codex-go-sdk-resource:RemoteControl
	// codex-go-sdk-docs:remoteControl/enable
	// codex-go-sdk-docs:remoteControl/disable
	// codex-go-sdk-docs:remoteControl/status/read
	// codex-go-sdk-docs:remoteControl/pairing/start
	// codex-go-sdk-docs:remoteControl/pairing/status
	// codex-go-sdk-docs:remoteControl/client/list
	// codex-go-sdk-docs:remoteControl/client/revoke
	_, _ = client.RemoteControl.ReadStatus(ctx)
	_, _ = client.RemoteControl.Enable(ctx, protocol.NullableRemoteControlEnableParams{})
	_, _ = client.RemoteControl.Disable(ctx, protocol.NullableRemoteControlDisableParams{})
	pairing, _, err := client.RemoteControl.StartPairing(ctx, codex.RemoteControlPairingOptions{ManualCode: true})
	if err != nil {
		return err
	}
	_, _ = pairing.Status(ctx)
	_, _ = client.RemoteControl.PairingStatus(ctx, pairing.ID())
	_, _ = client.RemoteControl.ListClients(ctx, protocol.RemoteControlClientsListParams{})
	_, _ = client.RemoteControl.RevokeClient(ctx, protocol.RemoteControlClientsRevokeParams{})
	// codex-go-sdk-resource:CollaborationModes
	// codex-go-sdk-docs:collaborationMode/list
	_, _ = client.CollaborationModes.List(ctx, protocol.CollaborationModeListParams{})
	// codex-go-sdk-resource:ExternalAgents
	// codex-go-sdk-docs:externalAgentConfig/detect
	// codex-go-sdk-docs:externalAgentConfig/import
	// codex-go-sdk-docs:externalAgentConfig/import/readHistories
	_, _ = client.ExternalAgents.DetectConfig(ctx, protocol.ExternalAgentConfigDetectParams{
		Cwds:        protocol.Some([]string{"/repo"}),
		IncludeHome: protocol.SomeNonNull(true),
	})
	_, _ = client.ExternalAgents.ImportConfig(ctx, protocol.ExternalAgentConfigImportParams{})
	_, _ = client.ExternalAgents.ReadImportHistories(ctx)
	// codex-go-sdk-resource:Marketplace
	// codex-go-sdk-docs:marketplace/add
	// codex-go-sdk-docs:marketplace/remove
	// codex-go-sdk-docs:marketplace/upgrade
	_, _ = client.Marketplace.Add(ctx, protocol.MarketplaceAddParams{})
	_, _ = client.Marketplace.Remove(ctx, protocol.MarketplaceRemoveParams{})
	_, _ = client.Marketplace.Upgrade(ctx, protocol.MarketplaceUpgradeParams{})
	// codex-go-sdk-resource:Plugins
	// codex-go-sdk-docs:plugin/list
	// codex-go-sdk-docs:plugin/read
	// codex-go-sdk-docs:plugin/installed
	// codex-go-sdk-docs:plugin/share/list
	// codex-go-sdk-docs:plugin/share/save
	// codex-go-sdk-docs:plugin/share/updateTargets
	// codex-go-sdk-docs:plugin/share/checkout
	// codex-go-sdk-docs:plugin/share/delete
	// codex-go-sdk-docs:plugin/install
	// codex-go-sdk-docs:plugin/uninstall
	_, _ = client.Plugins.List(ctx, protocol.PluginListParams{})
	_, _ = client.Plugins.Read(ctx, protocol.PluginReadParams{})
	_, _ = client.Plugins.Installed(ctx, protocol.PluginInstalledParams{})
	_, _ = client.Plugins.ListShares(ctx, protocol.PluginShareListParams{})
	_, _ = client.Plugins.SaveShare(ctx, protocol.PluginShareSaveParams{})
	_, _ = client.Plugins.UpdateShareTargets(ctx, protocol.PluginShareUpdateTargetsParams{})
	_, _ = client.Plugins.CheckoutShare(ctx, protocol.PluginShareCheckoutParams{})
	_, _ = client.Plugins.DeleteShare(ctx, protocol.PluginShareDeleteParams{})
	_, _ = client.Plugins.Install(ctx, protocol.PluginInstallParams{})
	_, _ = client.Plugins.Uninstall(ctx, protocol.PluginUninstallParams{})
	// codex-go-sdk-resource:Threads
	// codex-go-sdk-docs:thread/resume
	// codex-go-sdk-docs:thread/fork
	// codex-go-sdk-docs:thread/archive
	// codex-go-sdk-docs:thread/delete
	// codex-go-sdk-docs:thread/unarchive
	// codex-go-sdk-docs:thread/list
	// codex-go-sdk-docs:thread/search
	thread, err := client.Threads.Resume(ctx, codex.ThreadResumeOptions{ThreadID: "thread-id"})
	if err != nil {
		return err
	}
	_ = thread
	_, _ = client.Threads.Fork(ctx, codex.ThreadForkOptions{ThreadID: "thread-id"})
	_, _ = client.Threads.Archive(ctx, protocol.ThreadArchiveParams{})
	_, _ = client.Threads.Delete(ctx, protocol.ThreadDeleteParams{})
	_, _ = client.Threads.Unarchive(ctx, protocol.ThreadUnarchiveParams{})
	_, _ = client.Threads.List(ctx, protocol.ThreadListParams{})
	_, _ = client.Threads.Search(ctx, protocol.ThreadSearchParams{})
	// codex-go-sdk-resource:WindowsSandbox
	// codex-go-sdk-docs:windowsSandbox/setupStart
	if _, err := client.WindowsSandbox.SetupStart(ctx, protocol.WindowsSandboxSetupStartParams{}); err != nil {
		var unsupported *codex.UnsupportedPlatformError
		if !errors.As(err, &unsupported) {
			return err
		}
		_ = unsupported.PlatformOS
	}
	return nil
}

func main() {}
